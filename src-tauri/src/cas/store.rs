use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use super::encryption::{CasKeyRing, KEY_LEN};
use crate::error::{AppError, AppError::CasUnreadable, AppResult};

/// 版本化 blob 头（v2）：`CAS2` + 1 字节密钥版本 + nonce + 密文 + tag。
const CRYPT_MAGIC_V2: &[u8; 4] = b"CAS2";
/// 旧版 blob 头（v0）：`CASE` + nonce + 密文 + tag，读取时按版本 0 解密。
const CRYPT_MAGIC_LEGACY: &[u8; 4] = b"CASE";
const VERSION_HEADER_LEN: usize = CRYPT_MAGIC_V2.len() + 1;

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
    enc_ring: OnceLock<Option<CasKeyRing>>,
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
            enc_ring: OnceLock::new(),
        })
    }

    /// 启用单密钥 AES-256-GCM 加密（等价于单版本环，历史行为；测试与显式单密钥场景使用）。
    pub fn enable_encryption(&self, key: [u8; KEY_LEN]) {
        let ring = CasKeyRing::from_keys(vec![key]).expect("single-key ring always valid");
        let _ = self.enc_ring.set(Some(ring));
    }

    /// 启用版本化密钥环。写入用当前版本，读取按 blob 头版本取对应密钥。
    pub fn enable_encryption_ring(&self, ring: CasKeyRing) {
        let _ = self.enc_ring.set(Some(ring));
    }

    fn enc_ring(&self) -> Option<CasKeyRing> {
        self.enc_ring.get().cloned().flatten()
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
        let ring = self.enc_ring().ok_or_else(|| {
            AppError::msg("CAS encryption key is unavailable; refusing to write plaintext object")
        })?;
        let encrypted = super::encryption::encrypt_blob(content, &ring.current_key())?;
        let mut buf = Vec::with_capacity(VERSION_HEADER_LEN + encrypted.len());
        buf.extend_from_slice(CRYPT_MAGIC_V2);
        buf.push(ring.current_version());
        buf.extend_from_slice(&encrypted);
        Ok(buf)
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

    /// 读取 blob 内容。自动检测并解密加密的 blob；无法解密的 blob 返回 [`AppError::CasUnreadable`]。
    pub fn read_blob(&self, hash: &str) -> AppResult<Vec<u8>> {
        let path = self.object_path(hash)?;
        if !path.exists() {
            return Err(AppError::msg(format!("Object not found: {}", hash)));
        }
        let raw = fs::read(&path)?;

        if raw.starts_with(CRYPT_MAGIC_V2) {
            if raw.len() < VERSION_HEADER_LEN + super::encryption::NONCE_LEN {
                return Err(CasUnreadable("CAS blob header is truncated".into()));
            }
            let version = raw[CRYPT_MAGIC_V2.len()];
            let ring = self.enc_ring().ok_or_else(|| {
                CasUnreadable("encrypted CAS blob detected but no encryption key configured".into())
            })?;
            let key = ring.key_for(version).ok_or_else(|| {
                CasUnreadable(format!(
                    "CAS blob uses encryption key version {version} which is not in the key ring"
                ))
            })?;
            super::encryption::decrypt_blob(&raw[VERSION_HEADER_LEN..], &key)
                .map_err(|e| CasUnreadable(format!("CAS decryption failed: {e}")))
        } else if raw.starts_with(CRYPT_MAGIC_LEGACY) {
            let ring = self.enc_ring().ok_or_else(|| {
                CasUnreadable("encrypted CAS blob detected but no encryption key configured".into())
            })?;
            let key = ring.key_for(0).ok_or_else(|| {
                CasUnreadable(
                    "legacy CAS blob requires the version-0 key which is unavailable".into(),
                )
            })?;
            super::encryption::decrypt_blob(&raw[CRYPT_MAGIC_LEGACY.len()..], &key)
                .map_err(|e| CasUnreadable(format!("CAS decryption failed: {e}")))
        } else {
            // 历史明文 blob：头检测无法识别时按原样返回。
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

    /// Lists complete content-addressed object hashes currently present on disk.
    ///
    /// Invalid or partially-written object names are ignored so an interrupted
    /// write cannot be mistaken for a recoverable document snapshot.
    pub fn list_object_hashes(&self) -> AppResult<Vec<String>> {
        let objects = self.base_path.join("objects");
        let mut hashes = Vec::new();
        if !objects.exists() {
            return Ok(hashes);
        }

        for prefix in fs::read_dir(&objects)? {
            let prefix = prefix?;
            if !prefix.file_type()?.is_dir() {
                continue;
            }
            let Some(prefix) = prefix.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            for object in fs::read_dir(self.base_path.join("objects").join(&prefix))? {
                let object = object?;
                if !object.file_type()?.is_file() {
                    continue;
                }
                let Some(suffix) = object.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                let hash = format!("{prefix}{suffix}");
                if hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    hashes.push(hash);
                }
            }
        }
        hashes.sort();
        Ok(hashes)
    }

    /// 写入文件内容（写入CAS）
    pub fn write_content(&self, content: &str) -> AppResult<String> {
        let hash = super::hash::content_hash_str(content);
        self.store_blob(content.as_bytes())?;
        Ok(hash)
    }
}
