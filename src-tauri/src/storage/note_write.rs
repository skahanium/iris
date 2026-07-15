use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::app::AppState;
use crate::crypto::classified_io;
use crate::crypto::vault_key::VAULT_KEY;
use crate::error::{AppError, AppResult};
use crate::indexer::scan::{content_hash, index_file_from_content, FileEntry, IndexEmbeddingMode};
use crate::storage::atomic_write::atomic_write;
use crate::storage::paths::{has_reserved_path_root, is_classified_note_path, resolve_vault_path};

/// Whether derived SQLite indexes match the persisted Markdown body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FileWriteIndexStatus {
    Synced,
    Degraded,
}

/// Receipt separating authoritative Markdown persistence from derived indexing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileWriteResult {
    pub entry: FileEntry,
    pub content_hash: String,
    pub index_status: FileWriteIndexStatus,
}

/// The single persistence path for Markdown note bodies.
pub(crate) struct NoteWriteService;

impl NoteWriteService {
    /// Atomically persist a note body, then best-effort refresh its derived index.
    pub(crate) fn write(
        state: &AppState,
        path: &str,
        content: &str,
        embedding_mode: IndexEmbeddingMode<'_>,
    ) -> AppResult<FileWriteResult> {
        Self::write_inner(state, path, content, embedding_mode, false)
    }

    /// Atomically create a note body without replacing an existing Markdown file.
    pub(crate) fn create(
        state: &AppState,
        path: &str,
        content: &str,
        embedding_mode: IndexEmbeddingMode<'_>,
    ) -> AppResult<FileWriteResult> {
        Self::write_inner(state, path, content, embedding_mode, true)
    }

    fn write_inner(
        state: &AppState,
        path: &str,
        content: &str,
        embedding_mode: IndexEmbeddingMode<'_>,
        reject_existing: bool,
    ) -> AppResult<FileWriteResult> {
        let vault = state.vault_path()?;
        ensure_note_parent(&vault, path)?;
        let absolute = resolve_vault_path(&vault, path)?;
        if reject_existing && absolute.exists() {
            return Err(AppError::msg("File already exists"));
        }
        let payload = encode_payload(path, content)?;
        atomic_write(&absolute, &payload)?;

        let hash = content_hash(content);
        state.storage.write_guard.mark(path, &hash);

        if is_classified_note_path(path) {
            return Ok(FileWriteResult {
                entry: fallback_entry(path, content),
                content_hash: hash,
                index_status: FileWriteIndexStatus::Synced,
            });
        }

        match state.db.with_conn(|conn| {
            index_file_from_content(conn, &vault, &absolute, content, &hash, embedding_mode)
        }) {
            Ok(entry) => Ok(FileWriteResult {
                entry,
                content_hash: hash,
                index_status: FileWriteIndexStatus::Synced,
            }),
            Err(_) => {
                schedule_index_repair(
                    Arc::clone(&state.db),
                    vault,
                    path.to_string(),
                    match embedding_mode {
                        IndexEmbeddingMode::Queue(state) => Some(Arc::clone(state)),
                        IndexEmbeddingMode::Skip | IndexEmbeddingMode::Sync => None,
                    },
                );
                tracing::warn!(
                    result_code = "note_index_degraded",
                    "note markdown persisted while derived index refresh failed"
                );
                Ok(FileWriteResult {
                    entry: fallback_entry(path, content),
                    content_hash: hash,
                    index_status: FileWriteIndexStatus::Degraded,
                })
            }
        }
    }
}

fn ensure_note_parent(vault: &Path, path: &str) -> AppResult<()> {
    let mut parent = PathBuf::from(vault);
    let relative_parent = Path::new(path).parent().unwrap_or_else(|| Path::new(""));
    for component in relative_parent.components() {
        match component {
            Component::Normal(part) => parent.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::msg("Path traversal is not allowed"));
            }
        }
    }
    std::fs::create_dir_all(parent)?;
    Ok(())
}

fn encode_payload(path: &str, content: &str) -> AppResult<Vec<u8>> {
    if is_classified_note_path(path) {
        let vault_key = VAULT_KEY
            .get()
            .ok_or_else(|| AppError::msg("保险库未初始化"))?
            .read()
            .map_err(|_| AppError::msg("保险库密钥不可用"))?;
        return classified_io::encrypt_cef(content.as_bytes(), vault_key.key()?);
    }
    if has_reserved_path_root(path) {
        return Err(AppError::msg("Invalid classified path casing"));
    }
    Ok(content.as_bytes().to_vec())
}

fn fallback_entry(path: &str, content: &str) -> FileEntry {
    FileEntry {
        id: 0,
        path: path.to_string(),
        title: title_from_path(path),
        updated_at: chrono::Utc::now().to_rfc3339(),
        word_count: content.split_whitespace().count() as i64,
    }
}

fn title_from_path(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn schedule_index_repair(
    db: Arc<crate::storage::db::Database>,
    vault: std::path::PathBuf,
    path: String,
    queue_state: Option<Arc<AppState>>,
) {
    #[cfg(not(test))]
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let result = (|| -> AppResult<()> {
            let absolute = resolve_vault_path(&vault, &path)?;
            db.with_conn(|conn| {
                crate::indexer::scan::index_file_with_embed(
                    conn,
                    &vault,
                    &absolute,
                    queue_state
                        .as_ref()
                        .map(IndexEmbeddingMode::Queue)
                        .unwrap_or(IndexEmbeddingMode::Skip),
                )
            })?;
            Ok(())
        })();
        if result.is_err() {
            tracing::warn!(
                result_code = "note_index_repair_failed",
                "low-priority note index repair failed"
            );
        }
    });

    #[cfg(test)]
    {
        let _ = (db, vault, path, queue_state);
    }
}
