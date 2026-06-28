//! Encrypted classified AI thread storage.
//!
//! Thread data is persisted as CEF-encrypted JSON files under
//! `.classified/.iris-ai/sessions/<uuid>.cef`. A thread index is stored at
//! `.classified/.iris-ai/index.cef`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::crypto::classified_io;
use crate::crypto::vault_key::{VaultKey, VAULT_KEY};
use crate::error::{AppError, AppResult};

const AI_DIR_NAME: &str = ".iris-ai";
const SESSIONS_DIR_NAME: &str = "sessions";
const INDEX_FILE_NAME: &str = "index.cef";

/// A complete classified AI thread with all messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedAiThread {
    pub version: u32,
    pub thread_id: String,
    pub document_path: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<ClassifiedAiMessage>,
    pub evidence_packets: Vec<serde_json::Value>,
    pub token_usage: Option<serde_json::Value>,
}

/// A single message within a classified AI thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedAiMessage {
    pub seq: i64,
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_parts: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<serde_json::Value>,
    pub created_at: String,
}

/// Summary of a classified AI thread (for listing).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifiedAiThreadSummary {
    pub thread_id: String,
    pub document_path: String,
    pub title: String,
    pub message_count: u32,
    pub created_at: String,
    pub updated_at: String,
}

/// In-memory cache for decrypted thread index.
static THREAD_INDEX_CACHE: RwLock<Option<Vec<ClassifiedAiThreadSummary>>> = RwLock::new(None);

fn vault_key_read() -> AppResult<std::sync::RwLockReadGuard<'static, VaultKey>> {
    VAULT_KEY
        .get()
        .ok_or_else(|| AppError::msg("保险库未初始化"))?
        .read()
        .map_err(|e| AppError::msg(format!("VAULT_KEY lock error: {e}")))
}

fn require_unlocked() -> AppResult<std::sync::RwLockReadGuard<'static, VaultKey>> {
    let vk = vault_key_read()?;
    if !vk.is_unlocked() {
        return Err(AppError::msg("保险库未解锁"));
    }
    Ok(vk)
}

/// Resolve the `.classified/.iris-ai` directory path.
fn ai_dir(vault: &Path) -> PathBuf {
    vault.join(".classified").join(AI_DIR_NAME)
}

/// Resolve the sessions subdirectory.
fn sessions_dir(vault: &Path) -> PathBuf {
    ai_dir(vault).join(SESSIONS_DIR_NAME)
}

/// Resolve the index file path.
fn index_path(vault: &Path) -> PathBuf {
    ai_dir(vault).join(INDEX_FILE_NAME)
}

/// Resolve the file path for a specific thread.
fn thread_file_path(vault: &Path, thread_id: &str) -> PathBuf {
    sessions_dir(vault).join(format!("{thread_id}.cef"))
}

fn validate_thread_id(thread_id: &str) -> AppResult<Uuid> {
    let parsed =
        Uuid::parse_str(thread_id).map_err(|_| AppError::msg("涉密 AI thread_id 必须是 UUID"))?;
    if parsed.to_string() != thread_id.to_ascii_lowercase() {
        return Err(AppError::msg("涉密 AI thread_id 必须使用标准 UUID 文件名"));
    }
    Ok(parsed)
}

/// Ensure the `.iris-ai/sessions` directory structure exists.
fn ensure_ai_dirs(vault: &Path) -> AppResult<()> {
    let dir = sessions_dir(vault);
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(())
}

/// Load and decrypt the thread index from disk.
fn load_index(vault: &Path, key: &[u8; 32]) -> AppResult<Vec<ClassifiedAiThreadSummary>> {
    let path = index_path(vault);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read(&path)?;
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let plaintext = classified_io::decrypt_cef(&raw, key)?;
    let index: Vec<ClassifiedAiThreadSummary> = serde_json::from_slice(&plaintext)?;
    Ok(index)
}

/// Encrypt and save the thread index to disk.
fn save_index(vault: &Path, key: &[u8; 32], index: &[ClassifiedAiThreadSummary]) -> AppResult<()> {
    ensure_ai_dirs(vault)?;
    let plaintext = serde_json::to_vec(index)?;
    let encrypted = classified_io::encrypt_cef(&plaintext, key)?;
    fs::write(index_path(vault), &encrypted)?;
    Ok(())
}

/// Update the in-memory cache after a mutation.
fn update_cache(index: &[ClassifiedAiThreadSummary]) {
    if let Ok(mut cache) = THREAD_INDEX_CACHE.write() {
        *cache = Some(index.to_vec());
    }
}

/// List classified AI threads, optionally filtered by document path.
pub fn classified_ai_thread_list(
    vault: &Path,
    document_path: Option<String>,
) -> AppResult<Vec<ClassifiedAiThreadSummary>> {
    let vk = require_unlocked()?;
    let key = vk.key()?;

    // Try cache first
    if let Ok(cache) = THREAD_INDEX_CACHE.read() {
        if let Some(ref index) = *cache {
            return Ok(filter_index(index, document_path.as_deref()));
        }
    }

    let index = load_index(vault, key)?;
    let filtered = filter_index(&index, document_path.as_deref());

    // Populate cache
    update_cache(&index);

    Ok(filtered)
}

fn filter_index(
    index: &[ClassifiedAiThreadSummary],
    document_path: Option<&str>,
) -> Vec<ClassifiedAiThreadSummary> {
    match document_path {
        Some(path) => index
            .iter()
            .filter(|t| t.document_path == path)
            .cloned()
            .collect(),
        None => index.to_vec(),
    }
}

/// Load and decrypt a classified AI thread by id.
pub fn classified_ai_thread_load(vault: &Path, thread_id: String) -> AppResult<ClassifiedAiThread> {
    let vk = require_unlocked()?;
    let key = vk.key()?;
    validate_thread_id(&thread_id)?;

    let path = thread_file_path(vault, &thread_id);
    if !path.exists() {
        return Err(AppError::msg(format!("线程不存在: {thread_id}")));
    }

    let raw = fs::read(&path)?;
    let plaintext = classified_io::decrypt_cef(&raw, key)?;
    let thread: ClassifiedAiThread = serde_json::from_slice(&plaintext)?;
    Ok(thread)
}

/// Encrypt and save a classified AI thread.
pub fn classified_ai_thread_save(vault: &Path, thread: ClassifiedAiThread) -> AppResult<()> {
    let vk = require_unlocked()?;
    let key = vk.key()?;
    validate_thread_id(&thread.thread_id)?;

    ensure_ai_dirs(vault)?;

    // Save thread file
    let plaintext = serde_json::to_vec(&thread)?;
    let encrypted = classified_io::encrypt_cef(&plaintext, key)?;
    let path = thread_file_path(vault, &thread.thread_id);
    fs::write(&path, &encrypted)?;

    // Update index
    let mut index = load_index(vault, key)?;
    let summary = ClassifiedAiThreadSummary {
        thread_id: thread.thread_id.clone(),
        document_path: thread.document_path.clone(),
        title: thread
            .title
            .clone()
            .unwrap_or_else(|| derive_thread_title(&thread)),
        message_count: thread.messages.len() as u32,
        created_at: thread.created_at.clone(),
        updated_at: thread.updated_at.clone(),
    };

    if let Some(existing) = index.iter_mut().find(|t| t.thread_id == thread.thread_id) {
        *existing = summary;
    } else {
        index.push(summary);
    }

    // Sort by updated_at descending
    index.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    save_index(vault, key, &index)?;
    update_cache(&index);

    Ok(())
}

/// Delete a classified AI thread.
pub fn classified_ai_thread_delete(vault: &Path, thread_id: String) -> AppResult<()> {
    let _vk = require_unlocked()?;
    let key = _vk.key()?;
    validate_thread_id(&thread_id)?;

    // Remove thread file
    let path = thread_file_path(vault, &thread_id);
    if path.exists() {
        fs::remove_file(&path)?;
    }

    // Remove from index
    let mut index = load_index(vault, key)?;
    index.retain(|t| t.thread_id != thread_id);
    save_index(vault, key, &index)?;
    update_cache(&index);

    Ok(())
}

/// Clear the in-memory thread index cache.
pub fn classified_ai_cache_clear() -> AppResult<()> {
    if let Ok(mut cache) = THREAD_INDEX_CACHE.write() {
        *cache = None;
    }
    crate::ai_runtime::classified_retrieval::clear_classified_index();
    Ok(())
}

/// Derive a display title from the first user message.
fn derive_thread_title(thread: &ClassifiedAiThread) -> String {
    let first_user = thread
        .messages
        .iter()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("新对话");
    let trimmed = first_user.trim();
    if trimmed.is_empty() {
        return "新对话".to_string();
    }
    let chars: String = trimmed.chars().take(40).collect();
    if trimmed.chars().count() > 40 {
        format!("{chars}…")
    } else {
        chars
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        for (i, item) in k.iter_mut().enumerate() {
            *item = i as u8;
        }
        k
    }

    #[test]
    fn thread_roundtrip_encrypt_decrypt() {
        let key = test_key();
        let thread = ClassifiedAiThread {
            version: 1,
            thread_id: "019f0871-0000-7000-8000-000000000001".into(),
            document_path: ".classified/secret.md".into(),
            title: Some("测试线程".into()),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            messages: vec![ClassifiedAiMessage {
                seq: 1,
                role: "user".into(),
                content: "你好".into(),
                content_parts: None,
                tool_calls: None,
                created_at: "2026-01-01T00:00:00Z".into(),
            }],
            evidence_packets: vec![],
            token_usage: None,
        };

        let plaintext = serde_json::to_vec(&thread).unwrap();
        let encrypted = classified_io::encrypt_cef(&plaintext, &key).unwrap();
        assert!(classified_io::has_csef_magic(&encrypted));

        let decrypted = classified_io::decrypt_cef(&encrypted, &key).unwrap();
        let restored: ClassifiedAiThread = serde_json::from_slice(&decrypted).unwrap();
        assert_eq!(restored.thread_id, "019f0871-0000-7000-8000-000000000001");
        assert_eq!(restored.messages.len(), 1);
        assert_eq!(restored.messages[0].content, "你好");
    }

    #[test]
    fn thread_summary_serialization_camel_case() {
        let summary = ClassifiedAiThreadSummary {
            thread_id: "abc".into(),
            document_path: ".classified/doc.md".into(),
            title: "标题".into(),
            message_count: 5,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-02T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("threadId"));
        assert!(json.contains("documentPath"));
        assert!(json.contains("messageCount"));
        assert!(json.contains("createdAt"));
        assert!(json.contains("updatedAt"));
    }

    #[test]
    fn validate_thread_id_rejects_path_traversal() {
        assert!(validate_thread_id("../secret").is_err());
        assert!(validate_thread_id(".classified/secret").is_err());
        assert!(validate_thread_id("not-a-uuid").is_err());
        assert!(validate_thread_id("019f0871-0000-7000-8000-000000000002").is_ok());
    }

    #[test]
    fn derive_thread_title_from_first_user_message() {
        let thread = ClassifiedAiThread {
            version: 1,
            thread_id: "t1".into(),
            document_path: "doc.md".into(),
            title: None,
            created_at: "".into(),
            updated_at: "".into(),
            messages: vec![
                ClassifiedAiMessage {
                    seq: 1,
                    role: "user".into(),
                    content: "请帮我分析这份报告的关键数据点".into(),
                    content_parts: None,
                    tool_calls: None,
                    created_at: "".into(),
                },
                ClassifiedAiMessage {
                    seq: 2,
                    role: "assistant".into(),
                    content: "好的，我来分析...".into(),
                    content_parts: None,
                    tool_calls: None,
                    created_at: "".into(),
                },
            ],
            evidence_packets: vec![],
            token_usage: None,
        };
        let title = derive_thread_title(&thread);
        assert_eq!(title, "请帮我分析这份报告的关键数据点");
    }

    #[test]
    fn derive_thread_title_truncates_long_messages() {
        let long_msg = "这是一段很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长的消息";
        let thread = ClassifiedAiThread {
            version: 1,
            thread_id: "t2".into(),
            document_path: "doc.md".into(),
            title: None,
            created_at: "".into(),
            updated_at: "".into(),
            messages: vec![ClassifiedAiMessage {
                seq: 1,
                role: "user".into(),
                content: long_msg.into(),
                content_parts: None,
                tool_calls: None,
                created_at: "".into(),
            }],
            evidence_packets: vec![],
            token_usage: None,
        };
        let title = derive_thread_title(&thread);
        assert!(title.ends_with('…'));
        assert!(title.chars().count() <= 41); // 40 chars + ellipsis
    }

    #[test]
    fn derive_thread_title_fallback_for_empty_messages() {
        let thread = ClassifiedAiThread {
            version: 1,
            thread_id: "t3".into(),
            document_path: "doc.md".into(),
            title: None,
            created_at: "".into(),
            updated_at: "".into(),
            messages: vec![],
            evidence_packets: vec![],
            token_usage: None,
        };
        assert_eq!(derive_thread_title(&thread), "新对话");
    }

    #[test]
    fn filter_index_by_document_path() {
        let index = vec![
            ClassifiedAiThreadSummary {
                thread_id: "a".into(),
                document_path: "doc1.md".into(),
                title: "A".into(),
                message_count: 1,
                created_at: "".into(),
                updated_at: "".into(),
            },
            ClassifiedAiThreadSummary {
                thread_id: "b".into(),
                document_path: "doc2.md".into(),
                title: "B".into(),
                message_count: 2,
                created_at: "".into(),
                updated_at: "".into(),
            },
            ClassifiedAiThreadSummary {
                thread_id: "c".into(),
                document_path: "doc1.md".into(),
                title: "C".into(),
                message_count: 3,
                created_at: "".into(),
                updated_at: "".into(),
            },
        ];

        let filtered = filter_index(&index, Some("doc1.md"));
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|t| t.document_path == "doc1.md"));

        let all = filter_index(&index, None);
        assert_eq!(all.len(), 3);
    }
}
