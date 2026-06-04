use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::error::{AppError, AppResult};

const CRYPT_MAGIC: &[u8; 4] = b"CASE";
const ENC_KEY_LEN: usize = 32;

/// 对象类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectType {
    Blob,
    Tree,
    Commit,
}

/// Tree 条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeEntry {
    pub name: String,
    pub object_hash: String,
    pub object_type: ObjectType,
    pub mode: String,
}

/// Tree 对象（目录树）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeObject {
    pub hash: String,
    pub entries: Vec<TreeEntry>,
    pub ref_count: u32,
    pub created_at: DateTime<Utc>,
}

/// 提交元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitMetadata {
    pub file_id: i64,
    pub version_no: String,
    pub label: Option<String>,
    pub kind: String,
    pub word_count: i64,
    pub is_finalized: bool,
}

/// Commit 对象（版本提交）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitObject {
    pub hash: String,
    pub tree_hash: String,
    pub parent_hash: Option<String>,
    pub author: String,
    pub message: String,
    pub metadata: CommitMetadata,
    pub created_at: DateTime<Utc>,
}

/// CAS 对象存储
#[derive(Clone)]
pub struct CasObjectStore {
    base_path: PathBuf,
    enc_key: OnceLock<Option<[u8; ENC_KEY_LEN]>>,
}

impl CasObjectStore {
    /// 创建新的 CAS 存储实例
    pub fn new(base_path: PathBuf) -> AppResult<Self> {
        let objects_dir = base_path.join("objects");
        let refs_dir = base_path.join("refs");

        fs::create_dir_all(&objects_dir)?;
        fs::create_dir_all(&refs_dir)?;

        Ok(Self {
            base_path,
            enc_key: OnceLock::new(),
        })
    }

    /// Enable AES-256-GCM encryption for all future blob writes.
    /// Existing plaintext blobs remain readable via header detection.
    /// Call once during initialization; idempotent after the first call.
    pub fn enable_encryption(&self, key: [u8; ENC_KEY_LEN]) {
        let _ = self.enc_key.set(Some(key));
    }

    /// Get the encryption key if configured.
    fn enc_key(&self) -> Option<[u8; ENC_KEY_LEN]> {
        self.enc_key.get().copied().flatten()
    }

    /// 获取对象文件路径
    pub fn object_path(&self, hash: &str) -> AppResult<PathBuf> {
        if hash.len() < 2 {
            return Err(AppError::msg(format!("Invalid hash: {}", hash)));
        }
        let (prefix, suffix) = hash.split_at(2);
        Ok(self.base_path.join("objects").join(prefix).join(suffix))
    }

    /// Atomic write: write to temp file then rename to final path.
    fn atomic_write_raw(&self, target: &std::path::Path, data: &[u8]) -> AppResult<()> {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = target.with_extension("tmp");
        fs::write(&tmp, data)?;
        if let Err(e) = fs::rename(&tmp, target) {
            let _ = fs::remove_file(&tmp);
            return Err(e.into());
        }
        Ok(())
    }

    fn prepare_on_disk(&self, content: &[u8]) -> AppResult<Vec<u8>> {
        if let Some(key) = self.enc_key() {
            let encrypted = super::encryption::encrypt_blob(content, &key)?;
            let mut buf = Vec::with_capacity(CRYPT_MAGIC.len() + encrypted.len());
            buf.extend_from_slice(CRYPT_MAGIC);
            buf.extend_from_slice(&encrypted);
            Ok(buf)
        } else {
            Ok(content.to_vec())
        }
    }

    /// 存储 blob 对象。如果启用了加密，写入前加密内容。
    pub fn store_blob(&self, content: &[u8]) -> AppResult<String> {
        let hash = super::hash::content_hash(content);
        let path = self.object_path(&hash)?;

        if path.exists() {
            return Ok(hash);
        }

        let on_disk = self.prepare_on_disk(content)?;
        self.atomic_write_raw(&path, &on_disk)?;
        Ok(hash)
    }

    /// 读取 blob 内容。自动检测并解密加密的 blob。
    pub fn read_blob(&self, hash: &str) -> AppResult<Vec<u8>> {
        let path = self.object_path(hash)?;
        if !path.exists() {
            return Err(AppError::msg(format!("Object not found: {}", hash)));
        }
        let raw = fs::read(&path)?;

        if raw.len() >= CRYPT_MAGIC.len() && &raw[..CRYPT_MAGIC.len()] == CRYPT_MAGIC {
            let enc_data = &raw[CRYPT_MAGIC.len()..];
            let key = self.enc_key().ok_or_else(|| {
                AppError::msg("encrypted CAS blob detected but no encryption key configured")
            })?;
            super::encryption::decrypt_blob(enc_data, &key)
        } else {
            Ok(raw)
        }
    }

    /// 读取 blob 内容为字符串
    pub fn read_blob_content(&self, hash: &str) -> AppResult<String> {
        let content = self.read_blob(hash)?;
        String::from_utf8(content).map_err(|e| AppError::msg(format!("Invalid UTF-8: {}", e)))
    }

    /// 存储 tree 对象
    pub fn store_tree(&self, tree: &TreeObject) -> AppResult<String> {
        let content = serde_json::to_vec(tree)?;
        let hash = super::hash::content_hash(&content);
        let path = self.object_path(&hash)?;

        if path.exists() {
            return Ok(hash);
        }

        let on_disk = self.prepare_on_disk(&content)?;
        self.atomic_write_raw(&path, &on_disk)?;
        Ok(hash)
    }

    /// 读取 tree 对象
    pub fn read_tree(&self, hash: &str) -> AppResult<TreeObject> {
        let content = self.read_blob(hash)?;
        serde_json::from_slice(&content)
            .map_err(|e| AppError::msg(format!("Invalid tree object: {}", e)))
    }

    /// 存储 commit 对象
    pub fn store_commit(&self, commit: &CommitObject) -> AppResult<String> {
        let content = serde_json::to_vec(commit)?;
        let hash = super::hash::content_hash(&content);
        let path = self.object_path(&hash)?;

        if path.exists() {
            return Ok(hash);
        }

        let on_disk = self.prepare_on_disk(&content)?;
        self.atomic_write_raw(&path, &on_disk)?;
        Ok(hash)
    }

    /// 读取 commit 对象
    pub fn read_commit(&self, hash: &str) -> AppResult<CommitObject> {
        let content = self.read_blob(hash)?;
        serde_json::from_slice(&content)
            .map_err(|e| AppError::msg(format!("Invalid commit object: {}", e)))
    }

    /// 更新引用
    pub fn update_ref(&self, ref_name: &str, hash: &str) -> AppResult<()> {
        let ref_path = self.base_path.join("refs").join(ref_name);
        self.atomic_write_raw(&ref_path, hash.as_bytes())
    }

    /// 读取引用
    pub fn read_ref(&self, ref_name: &str) -> AppResult<Option<String>> {
        let ref_path = self.base_path.join("refs").join(ref_name);
        if !ref_path.exists() {
            return Ok(None);
        }
        let hash = fs::read_to_string(ref_path)?;
        Ok(Some(hash.trim().to_string()))
    }

    /// 获取基础路径
    pub fn base_path(&self) -> &std::path::Path {
        &self.base_path
    }

    /// 写入文件内容（写入CAS）
    pub fn write_content(&self, content: &str) -> AppResult<String> {
        let hash = super::hash::content_hash_str(content);
        self.store_blob(content.as_bytes())?;
        Ok(hash)
    }
}
