use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::app::AppState;
use crate::crypto::classified_io;
use crate::crypto::vault_key::VAULT_KEY;
use crate::embedding::scheduler::EmbeddingScheduler;
use crate::error::{AppError, AppResult};
use crate::indexer::scan::{content_hash, index_file_from_content, FileEntry};
use crate::storage::atomic_write::{
    atomic_create, atomic_write, move_file_no_replace_locked, with_vault_move_lock,
};
use crate::storage::note_title::title_from_path;
use crate::storage::paths::{
    has_reserved_path_root, is_classified_note_path, read_file_lossy, resolve_vault_path,
};

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
    /// Present for operations affecting additional files. Never infer these from indexes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<FileOperationReceipt>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileOperationReceipt {
    pub previous_path: String,
    pub applied_paths: Vec<String>,
    pub pending_paths: Vec<String>,
    pub recovery_versions: Vec<(String, i64)>,
}

/// The single persistence path for Markdown note bodies.
pub(crate) struct NoteWriteService;

impl NoteWriteService {
    /// Reject a locked target before creating recovery material.
    pub(crate) fn ensure_unlocked(state: &AppState, path: &str) -> AppResult<()> {
        if is_note_locked(&state.db, path)? {
            return Err(AppError::msg("note_locked"));
        }
        Ok(())
    }
    /// Atomically persist a note body, then best-effort refresh its derived index.
    pub(crate) fn write(state: &AppState, path: &str, content: &str) -> AppResult<FileWriteResult> {
        with_vault_move_lock(|| Self::write_body(state, path, content, false))
    }

    /// Atomically create a note body without replacing an existing Markdown file.
    pub(crate) fn create(
        state: &AppState,
        path: &str,
        content: &str,
    ) -> AppResult<FileWriteResult> {
        with_vault_move_lock(|| Self::write_body(state, path, content, true))
    }

    /// No-overwrite create for a caller already holding the vault operation guard.
    pub(crate) fn create_under_move_lock(
        state: &AppState,
        path: &str,
        content: &str,
    ) -> AppResult<FileWriteResult> {
        Self::write_body(state, path, content, true)
    }

    /// Persist a note body when the caller already holds [`with_vault_move_lock`].
    ///
    /// `VAULT_MOVE_LOCK` is not reentrant; rename/trash cascades must use this
    /// instead of [`Self::write`] to avoid deadlocking the coordinator.
    ///
    /// Background edits obey the same document lock as ordinary saves.
    pub(crate) fn write_under_move_lock(
        state: &AppState,
        path: &str,
        content: &str,
    ) -> AppResult<FileWriteResult> {
        Self::write_body(state, path, content, false)
    }

    /// Move an existing plain Markdown file into the vault, then refresh its derived index.
    ///
    /// This is the persistence boundary for workflows such as recycle-bin
    /// restore: once the filesystem move succeeds, index failures are reported
    /// as degraded and queued for repair rather than negating the Markdown fact.
    pub(crate) fn adopt(state: &AppState, source: &Path, path: &str) -> AppResult<FileWriteResult> {
        if is_classified_note_path(path) {
            return Err(AppError::msg(
                "classified notes cannot be adopted through the plain Markdown service",
            ));
        }

        let vault = state.vault_path()?;
        let (absolute, content) = with_vault_move_lock(|| {
            ensure_note_parent(&vault, path)?;
            let absolute = resolve_vault_path(&vault, path)?;
            let content = read_file_lossy(source)?;
            move_file_no_replace_locked(source, &absolute)?;
            Ok((absolute, content))
        })?;
        let hash = content_hash(&content);
        state.storage.write_guard.mark(path, &hash);

        Self::refresh_index_after_persist(state, vault, path, &absolute, &content, hash)
    }

    /// Queue a best-effort repair for derived state after an already-persisted move.
    pub(crate) fn schedule_index_repair(state: &AppState, path: &str) {
        let Ok(vault) = state.vault_path() else {
            tracing::warn!(
                result_code = "note_index_repair_vault_unavailable",
                "derived index repair could not resolve the active vault"
            );
            return;
        };
        schedule_index_repair_task(
            Arc::clone(&state.db),
            vault,
            path.to_string(),
            state.embedding_scheduler(),
        );
    }

    fn write_body(
        state: &AppState,
        path: &str,
        content: &str,
        reject_existing: bool,
    ) -> AppResult<FileWriteResult> {
        if is_note_locked(&state.db, path)? {
            return Err(AppError::msg("note_locked"));
        }

        let vault = state.vault_path()?;
        let payload = encode_payload(path, content)?;
        // Callers must hold the vault move lock so a concurrent rename/trash
        // cannot move the target between path resolution and the durable write.
        ensure_note_parent(&vault, path)?;
        let absolute = resolve_vault_path(&vault, path)?;
        if reject_existing {
            atomic_create(&absolute, &payload)?;
        } else {
            atomic_write(&absolute, &payload)?;
        }

        let hash = content_hash(content);
        state.storage.write_guard.mark(path, &hash);

        Self::refresh_index_after_persist(state, vault, path, &absolute, content, hash)
    }

    fn refresh_index_after_persist(
        state: &AppState,
        vault: PathBuf,
        path: &str,
        absolute: &Path,
        content: &str,
        hash: String,
    ) -> AppResult<FileWriteResult> {
        if is_classified_note_path(path) {
            return Ok(FileWriteResult {
                entry: fallback_entry(path, content),
                content_hash: hash,
                index_status: FileWriteIndexStatus::Synced,
                operation: None,
            });
        }

        match state
            .db
            .with_conn(|conn| index_file_from_content(conn, &vault, absolute, content, &hash))
        {
            Ok(entry) => {
                state.embedding_scheduler().notify_index_committed();
                Ok(FileWriteResult {
                    entry,
                    content_hash: hash,
                    index_status: FileWriteIndexStatus::Synced,
                    operation: None,
                })
            }
            Err(_) => {
                Self::schedule_index_repair(state, path);
                tracing::warn!(
                    result_code = "note_index_degraded",
                    "note markdown persisted while derived index refresh failed"
                );
                Ok(FileWriteResult {
                    entry: fallback_entry(path, content),
                    content_hash: hash,
                    index_status: FileWriteIndexStatus::Degraded,
                    operation: None,
                })
            }
        }
    }
}

fn ensure_note_parent(vault: &Path, path: &str) -> AppResult<()> {
    // Validate every existing ancestor before any mkdir. In particular, never
    // create directories through a symlink and reject only after the side effect.
    let mut parent = vault.canonicalize()?;
    let relative_parent = Path::new(path).parent().unwrap_or_else(|| Path::new(""));
    let mut missing = Vec::new();
    for component in relative_parent.components() {
        match component {
            Component::Normal(part) => {
                parent.push(part);
                match std::fs::symlink_metadata(&parent) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        return Err(AppError::msg("note_path_alias_not_allowed"))
                    }
                    Ok(metadata) if !metadata.is_dir() => {
                        return Err(AppError::msg("note_parent_not_directory"))
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        missing.push(parent.clone())
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::msg("Path traversal is not allowed"));
            }
        }
    }
    // Also reject a final-component symlink; its canonical path could bypass
    // lock identity or enter metadata despite a harmless-looking lexical name.
    let file_name = Path::new(path)
        .file_name()
        .ok_or_else(|| AppError::msg("Invalid path"))?;
    if std::fs::symlink_metadata(parent.join(file_name))
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(AppError::msg("note_path_alias_not_allowed"));
    }
    for directory in missing {
        std::fs::create_dir(directory)?;
    }
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

/// Returns true when `files.is_locked` is set for `path`. Missing rows are unlocked.
fn is_note_locked(db: &crate::storage::db::Database, path: &str) -> AppResult<bool> {
    db.with_read_conn(|conn| {
        let mut stmt = conn.prepare("SELECT is_locked FROM files WHERE path = ?1")?;
        match stmt.query_row([path], |row| row.get::<_, i64>(0)) {
            Ok(v) => Ok(v != 0),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(e) => Err(e.into()),
        }
    })
}

pub(crate) fn noop_write_receipt(path: &str, content: &str) -> FileWriteResult {
    let hash = content_hash(content);
    FileWriteResult {
        entry: fallback_entry(path, content),
        content_hash: hash,
        index_status: FileWriteIndexStatus::Synced,
        operation: None,
    }
}

fn schedule_index_repair_task(
    db: Arc<crate::storage::db::Database>,
    vault: std::path::PathBuf,
    path: String,
    scheduler: Arc<EmbeddingScheduler>,
) {
    #[cfg(not(test))]
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let result = (|| -> AppResult<()> {
            let absolute = resolve_vault_path(&vault, &path)?;
            db.with_conn(|conn| {
                crate::indexer::scan::index_file(conn, &vault, &absolute)?;
                Ok(())
            })?;
            scheduler.notify_index_committed();
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
        let _ = (db, vault, path, scheduler);
    }
}
