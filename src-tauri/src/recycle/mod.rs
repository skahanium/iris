//! Soft-delete notes (current `.md` + all version snapshots) into `.iris/trash/`.

mod cleanup;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{Duration, Utc};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::AppState;
use crate::error::AppError;
use crate::error::AppResult;
use crate::indexer::scan::{index_file, remove_file_index};
use crate::storage::atomic_write::with_vault_move_lock;
use crate::storage::note_title::title_from_path;
use crate::storage::note_write::{FileWriteIndexStatus, FileWriteResult, NoteWriteService};
use crate::storage::paths::resolve_vault_path;
use crate::version::VersionEntry;

/// Days before trashed items are permanently removed.
pub const RECYCLE_RETENTION_DAYS: i64 = 15;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashVersionMeta {
    /// Exact durable row frozen before moving the note. Absent in old bundles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_id: Option<i64>,
    pub version_no: String,
    pub label: Option<String>,
    pub content_hash: String,
    pub storage_path: String,
    pub word_count: i64,
    pub is_finalized: bool,
    pub kind: String,
    pub created_at: String,
    /// File name under `versions/` inside the trash bundle.
    pub trash_file: String,
    /// The convenience copy is unreadable; durable metadata and objects remain owned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unreadable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashManifest {
    pub original_path: String,
    pub title: String,
    pub deleted_at: String,
    pub expires_at: String,
    pub versions: Vec<TrashVersionMeta>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecycleBinItem {
    pub id: String,
    pub original_path: String,
    pub title: String,
    pub deleted_at: String,
    pub expires_at: String,
    pub version_count: usize,
}

fn load_manifest(vault: &Path, trash_rel: &str) -> AppResult<TrashManifest> {
    let path = vault.join(trash_rel).join("manifest.json");
    let raw = fs::read_to_string(path)?;
    serde_json::from_str(&raw).map_err(|e| AppError::msg(format!("Invalid trash manifest: {e}")))
}

fn trash_root(vault: &Path) -> PathBuf {
    vault.join(".iris").join("trash")
}

fn versions_root(vault: &Path) -> PathBuf {
    vault.join(".iris").join("versions")
}

fn lookup_file_id(conn: &rusqlite::Connection, path: &str) -> AppResult<Option<i64>> {
    conn.query_row(
        "SELECT id FROM files WHERE path = ?1 ORDER BY id DESC LIMIT 1",
        [path],
        |r| r.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn in_immediate_transaction<T>(
    conn: &rusqlite::Connection,
    operation: impl FnOnce(&rusqlite::Connection) -> AppResult<T>,
) -> AppResult<T> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    match operation(conn) {
        Ok(value) => match conn.execute_batch("COMMIT") {
            Ok(()) => Ok(value),
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(error.into())
            }
        },
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn validate_file_index_ownership(
    conn: &rusqlite::Connection,
    path: &str,
    expected_ids: &[i64],
) -> AppResult<()> {
    let mut statement = conn.prepare("SELECT id FROM files WHERE path = ?1 ORDER BY id")?;
    let actual_ids = statement
        .query_map([path], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if actual_ids != expected_ids {
        return Err(AppError::msg("discard_file_ownership_mismatch"));
    }
    Ok(())
}

/// Permanently remove a note, all version blobs, and index rows (no recycle).
pub fn discard_document(state: &AppState, path: &str) -> AppResult<()> {
    with_vault_move_lock(|| {
        let vault = state.vault_path()?;
        ensure_cleanup_recovered(&state.db, &vault)?;
        let abs = crate::storage::paths::validate_user_note_relative_path(&vault, path)?;
        if abs.exists() && !abs.is_file() {
            return Err(AppError::msg("discard target is not a regular file"));
        }
        let (rows, file_ids, legacy_paths) = state.db.with_read_conn(|conn| {
            let rows = crate::version::repository::active_rows(conn, &vault, path)?
                .into_iter()
                .map(|(_, ownership)| ownership)
                .collect::<Vec<_>>();
            let legacy_paths =
                crate::version::repository::unshared_non_cas_storage_paths(conn, &vault, &rows)?;
            let mut statement = conn.prepare("SELECT id FROM files WHERE path = ?1")?;
            let file_ids = statement
                .query_map([path], |row| row.get::<_, i64>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok((rows, file_ids, legacy_paths))
        })?;
        let mut candidates = legacy_paths
            .iter()
            .cloned()
            .map(|relative_path| cleanup::Candidate {
                root: cleanup::CandidateRoot::Versions,
                relative_path,
            })
            .collect::<Vec<_>>();
        if abs.is_file() {
            candidates.push(cleanup::Candidate {
                root: cleanup::CandidateRoot::Vault,
                relative_path: path.to_string(),
            });
        }
        let version_ids = rows.iter().map(|row| row.id).collect::<Vec<_>>();
        let mut checkpoint = cleanup::prepare(
            &vault,
            cleanup::CleanupKind::Discard {
                note_path: path.to_string(),
                file_ids: file_ids.clone(),
                version_ids,
            },
            rows.clone(),
            candidates,
        )?;
        let database_result = state.db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                crate::version::repository::delete_cleanup_rows(
                    conn,
                    &vault,
                    &rows,
                    &legacy_paths,
                )?;
                validate_file_index_ownership(conn, path, &file_ids)?;
                remove_file_index(conn, path)?;
                Ok(())
            })
        });
        if let Err(error) = database_result {
            return match cleanup::rollback(&mut checkpoint) {
                Ok(()) => Err(error),
                Err(rollback_error) => Err(AppError::msg(format!(
                    "discard database failed and rollback is pending: {error}; {rollback_error}"
                ))),
            };
        }
        cleanup::finish(&state.db, &checkpoint)?;
        state.storage.write_guard.mark_removed(path);
        Ok(())
    })
}

/// Move current note + all versions/finalized snapshots into recycle bin.
pub fn trash_document(state: &AppState, path: &str) -> AppResult<()> {
    let receipt = trash_with_receipt(state, path)?;
    if let Some(error) = &receipt.metadata_error {
        return Err(AppError::msg(format!(
            "{error}: document_moved_to_recycle=true; trash_id={}",
            receipt.trash_id
        )));
    }
    Ok(())
}

/// Result of a recoverable deletion, independent of Agent/Run metadata.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrashReceipt {
    pub path: String,
    pub trash_id: String,
    pub content_hash: String,
    pub metadata_pending: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_error: Option<String>,
}

/// Trash via the shared file-operation guard.
pub(crate) fn trash_with_receipt(state: &AppState, path: &str) -> AppResult<TrashReceipt> {
    with_vault_move_lock(|| trash_locked(state, path))
}

/// Caller must hold the common file-operation guard.
pub(crate) fn trash_locked(state: &AppState, path: &str) -> AppResult<TrashReceipt> {
    crate::storage::note_write::NoteWriteService::ensure_unlocked(state, path)?;
    let vault = state.vault_path()?;
    let absolute = crate::storage::paths::validate_user_note_relative_path(&vault, path)?;
    let body = fs::read_to_string(&absolute)?;
    let body_hash = crate::cas::hash::content_hash_str(&body);
    let trash_id = Uuid::new_v4().to_string();
    let bundle_dir = trash_root(&vault).join(&trash_id);
    let versions_dir = bundle_dir.join("versions");

    let (title, mut version_metas) = state.db.with_conn(|conn| {
        let file_id = lookup_file_id(conn, path)?;
        let stored_title: Option<String> = if file_id.is_some() {
            conn.query_row(
                "SELECT title FROM files WHERE path = ?1 ORDER BY id DESC LIMIT 1",
                [path],
                |r| r.get(0),
            )
            .optional()?
        } else {
            None
        };
        let title = stored_title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| title_from_path(path));

        let mut metas = Vec::new();
        for (entry, ownership) in crate::version::repository::active_rows(conn, &vault, path)? {
            metas.push(TrashVersionMeta {
                version_id: Some(entry.id),
                version_no: entry.version_no.clone(),
                label: entry.label.clone(),
                content_hash: entry.content_hash.clone(),
                storage_path: ownership.storage_path,
                word_count: entry.word_count,
                is_finalized: entry.is_finalized,
                kind: entry.kind.as_str().to_string(),
                created_at: entry.created_at.clone(),
                trash_file: format!("{}.md", entry.version_no),
                unreadable: None,
            });
        }
        Ok((title, metas))
    })?;

    {
        fs::create_dir_all(&versions_dir)?;
        for meta in &mut version_metas {
            let dest = versions_dir.join(&meta.trash_file);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            if crate::version::is_cas_storage_path(&meta.storage_path)
                || meta.storage_path.starts_with("dif:")
            {
                match crate::version::read_version_content(state, &vault, &meta.storage_path) {
                    Ok(content) => {
                        crate::storage::atomic_write::atomic_write(&dest, content.as_bytes())?
                    }
                    Err(error) => {
                        // Keep the durable row and original CAS references even
                        // when the optional readable bundle copy is unavailable.
                        meta.unreadable = Some(true);
                        tracing::warn!(
                            result_code = "recycle_trash_skip_unreadable_version",
                            path = %path,
                            version_no = %meta.version_no,
                            error = %error,
                            "skipping unreadable version snapshot while trashing document"
                        );
                    }
                }
            } else {
                let src = versions_root(&vault).join(&meta.storage_path);
                if src.is_file() {
                    fs::copy(&src, &dest)?;
                }
            }
        }
    }
    let deleted_at = Utc::now();
    let expires_at = deleted_at + Duration::days(RECYCLE_RETENTION_DAYS);
    let manifest = TrashManifest {
        original_path: path.to_string(),
        title: title.clone(),
        deleted_at: deleted_at.to_rfc3339(),
        expires_at: expires_at.to_rfc3339(),
        versions: version_metas,
    };
    crate::storage::atomic_write::atomic_write(
        &bundle_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?.as_bytes(),
    )?;

    crate::storage::atomic_write::move_file_no_replace_locked(
        &absolute,
        &bundle_dir.join("document.md"),
    )?;
    state.storage.write_guard.mark_removed(path);

    let trash_rel = format!(".iris/trash/{trash_id}");
    enum MetadataOutcome {
        Committed,
        OwnershipMismatch,
    }
    let version_ids = manifest
        .versions
        .iter()
        .filter_map(|version| version.version_id)
        .collect::<Vec<_>>();
    let metadata_result = state.db.with_conn(|conn| {
        conn.execute_batch("BEGIN IMMEDIATE")?;
        match crate::version::repository::archive_rows(
            conn,
            &vault,
            path,
            &trash_id,
            &version_ids,
        ) {
            Ok(()) => {}
            Err(crate::version::repository::ArchiveRowsError::OwnershipMismatch) => {
                conn.execute_batch("ROLLBACK")?;
                return Ok(MetadataOutcome::OwnershipMismatch);
            }
            Err(crate::version::repository::ArchiveRowsError::Database(error)) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(error.into());
            }
        }
        let result = (|| {
            conn.execute(
                "INSERT INTO recycle_bin (id, original_path, title, deleted_at, expires_at, trash_rel_dir)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    trash_id,
                    path,
                    title,
                    manifest.deleted_at,
                    manifest.expires_at,
                    trash_rel,
                ],
            )?;
            remove_file_index(conn, path)
        })();
        match result {
            Ok(()) => match conn.execute_batch("COMMIT") {
                Ok(()) => Ok(MetadataOutcome::Committed),
                Err(error) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    Err(error.into())
                }
            },
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    });
    let (metadata_pending, metadata_error) = match metadata_result {
        Ok(MetadataOutcome::Committed) => (false, None),
        Ok(MetadataOutcome::OwnershipMismatch) => (
            true,
            Some("recycled_version_ownership_mismatch".to_string()),
        ),
        Err(_) => (true, None),
    };

    Ok(TrashReceipt {
        path: path.into(),
        trash_id,
        content_hash: body_hash,
        metadata_pending,
        metadata_error,
    })
}

fn purge_bundle(vault: &Path, trash_rel_dir: &str) -> AppResult<u64> {
    let dir = vault.join(trash_rel_dir);
    let mut size = 0u64;
    if dir.exists() {
        size = dir_size(&dir);
        fs::remove_dir_all(dir)?;
    }
    Ok(size)
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_file() {
                total += meta.len();
            } else if meta.is_dir() {
                total += dir_size(&entry.path());
            }
        }
    }
    total
}

/// Remove expired recycle entries using the given DB connection and vault path.
/// Returns (purged_count, bytes_freed).
pub(crate) fn purge_expired_items(
    db: &crate::storage::db::Database,
    vault: &Path,
) -> AppResult<(usize, u64)> {
    ensure_cleanup_recovered(db, vault)?;
    let now = Utc::now().to_rfc3339();
    let expired: Vec<(String, String)> = db.with_read_conn(|conn| {
        let mut stmt =
            conn.prepare("SELECT id, trash_rel_dir FROM recycle_bin WHERE expires_at <= ?1")?;
        let rows = stmt.query_map([&now], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.flatten().collect())
    })?;

    let mut count = 0usize;
    let mut freed = 0u64;
    for (id, trash_rel) in expired {
        if !vault.join(&trash_rel).join("manifest.json").is_file() {
            continue;
        }
        freed = freed.saturating_add(purge_one(db, vault, &id, &trash_rel)?);
        count += 1;
    }
    Ok((count, freed))
}

/// Remove recycle entries whose retention period has ended.
pub fn purge_expired(state: &AppState) -> AppResult<usize> {
    with_vault_move_lock(|| {
        let vault = state.vault_path()?;
        let (count, _) = purge_expired_items(&state.db, &vault)?;
        Ok(count)
    })
}

pub fn list_recycle(state: &AppState) -> AppResult<Vec<RecycleBinItem>> {
    let vault = state.vault_path()?;
    let unresolved = with_vault_move_lock(|| Ok(cleanup::recover_pending(&state.db, &vault)))?;
    if !unresolved.is_empty() {
        tracing::warn!(
            result_code = "recycle_cleanup_recovery_required",
            checkpoint_count = unresolved.len(),
            "recycle cleanup checkpoints still require recovery"
        );
    }
    reconcile_durable_trash(state, &vault)?;
    resume_deferred_restores(state, &vault);
    let rows: Vec<(String, String, String, String, String, String)> =
        state.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, original_path, title, deleted_at, expires_at, trash_rel_dir
                 FROM recycle_bin ORDER BY deleted_at DESC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })?;
            Ok(rows.flatten().collect())
        })?;

    Ok(rows
        .into_iter()
        .filter(|(_, _, _, _, _, trash_rel)| vault.join(trash_rel).join("manifest.json").is_file())
        .map(
            |(id, original_path, title, deleted_at, expires_at, trash_rel)| {
                let version_count = load_manifest(&vault, &trash_rel)
                    .map(|m| m.versions.len())
                    .unwrap_or(0);
                RecycleBinItem {
                    id,
                    original_path,
                    title,
                    deleted_at,
                    expires_at,
                    version_count,
                }
            },
        )
        .collect())
}

// Rebuild missing bookkeeping only for completed body moves. Prepared bundles
// without document.md never authorize replay of a deletion or a restoration.
fn reconcile_durable_trash(state: &AppState, vault: &Path) -> AppResult<()> {
    with_vault_move_lock(|| {
        let root = trash_root(vault);
        if !root.is_dir() {
            return Ok(());
        }
        for item in fs::read_dir(root)? {
            let item = item?;
            if !item.file_type()?.is_dir() {
                continue;
            }
            let id = item.file_name().to_string_lossy().to_string();
            if Uuid::parse_str(&id).is_err() || !item.path().join("document.md").is_file() {
                continue;
            }
            let relative = format!(".iris/trash/{id}");
            let Ok(manifest) = load_manifest(vault, &relative) else {
                continue;
            };
            let Ok(original) = crate::storage::paths::validate_user_note_relative_path(
                vault,
                &manifest.original_path,
            ) else {
                continue;
            };
            state.db.with_conn(|conn| {
                in_immediate_transaction(conn, |conn| {
                    crate::version::repository::archive_rows(conn, vault, &manifest.original_path, &id,
                        &manifest.versions.iter().filter_map(|v| v.version_id).collect::<Vec<_>>())
                        .map_err(AppError::from)?;
                    conn.execute("INSERT OR IGNORE INTO recycle_bin (id,original_path,title,deleted_at,expires_at,trash_rel_dir) VALUES (?1,?2,?3,?4,?5,?6)",
                        rusqlite::params![id, manifest.original_path, manifest.title, manifest.deleted_at, manifest.expires_at, relative])?;
                    if !original.exists() { remove_file_index(conn, &manifest.original_path)?; }
                    Ok(())
                })
            })?;
        }
        Ok(())
    })
}

fn resume_deferred_restores(state: &AppState, vault: &Path) {
    let pending: Vec<(String, String, String)> = match state.db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, trash_rel_dir, original_path
             FROM recycle_bin ORDER BY deleted_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        Ok(rows.flatten().collect())
    }) {
        Ok(pending) => pending,
        Err(_) => {
            tracing::warn!(
                result_code = "recycle_restore_resume_lookup_degraded",
                "deferred recycle restore lookup failed"
            );
            return;
        }
    };

    for (id, trash_rel, original_path) in pending {
        let bundle_dir = vault.join(&trash_rel);
        let document = bundle_dir.join("document.md");
        let Ok(destination) = resolve_vault_path(vault, &original_path) else {
            continue;
        };
        if document.is_file() || !destination.is_file() {
            continue;
        }

        let Ok(manifest) = load_manifest(vault, &trash_rel) else {
            tracing::warn!(
                result_code = "recycle_restore_resume_manifest_degraded",
                "deferred recycle restore manifest could not be loaded"
            );
            continue;
        };
        if manifest.original_path != original_path {
            tracing::warn!(
                result_code = "recycle_restore_resume_manifest_mismatch",
                "deferred recycle restore manifest did not match metadata"
            );
            continue;
        }

        let entry = match state
            .db
            .with_conn(|conn| index_file(conn, vault, &destination))
        {
            Ok(entry) => {
                state.embedding_scheduler().notify_index_committed();
                entry
            }
            Err(_) => {
                tracing::warn!(
                    result_code = "recycle_restore_resume_index_degraded",
                    "deferred recycle restore index repair is still pending"
                );
                continue;
            }
        };

        if finalize_restore(state, vault, &id, &trash_rel, &manifest, entry.id).is_err() {
            tracing::warn!(
                result_code = "recycle_restore_resume_versions_degraded",
                "deferred recycle restore version recovery is still pending"
            );
        }
    }
}

fn finalize_restore(
    state: &AppState,
    vault: &Path,
    id: &str,
    trash_rel: &str,
    manifest: &TrashManifest,
    file_id: i64,
) -> AppResult<()> {
    let bundle_dir = vault.join(trash_rel);
    for v in &manifest.versions {
        if let Some(version_id) = v.version_id {
            state.db.with_conn(|conn| {
                in_immediate_transaction(conn, |conn| {
                    crate::version::repository::restore_archived_row(
                        conn,
                        vault,
                        &manifest.original_path,
                        id,
                        version_id,
                        file_id,
                    )
                })
            })?;
            continue;
        }
        let src = bundle_dir.join("versions").join(&v.trash_file);
        let new_storage = if v.unreadable == Some(true) {
            v.storage_path.clone()
        } else {
            format!("recycled/{id}/{}.md", v.version_no)
        };
        let dest_version = versions_root(vault).join(&new_storage);
        if v.unreadable != Some(true) {
            if let Some(parent) = dest_version.parent() {
                fs::create_dir_all(parent)?;
            }
            if src.is_file() {
                crate::storage::atomic_write::atomic_write(&dest_version, &fs::read(&src)?)?;
            } else if !dest_version.is_file() {
                return Err(AppError::msg("recycled version snapshot is missing"));
            }
        }
        state.db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                crate::version::repository::import_recycled_row(
                    conn,
                    vault,
                    &manifest.original_path,
                    &VersionEntry {
                        id: 0,
                        file_id,
                        version_no: v.version_no.clone(),
                        label: v.label.clone(),
                        content_hash: v.content_hash.clone(),
                        word_count: v.word_count,
                        is_finalized: v.is_finalized,
                        kind: crate::version::VersionKind::parse(&v.kind)
                            .unwrap_or(crate::version::VersionKind::Manual),
                        created_at: v.created_at.clone(),
                        is_legacy_unscoped: false,
                    },
                    &new_storage,
                )
            })
        })?;
    }

    purge_bundle(vault, trash_rel)?;
    state.db.with_conn(|conn| {
        conn.execute("DELETE FROM recycle_bin WHERE id = ?1", [id])?;
        Ok(())
    })
}

/// Restore a trashed note (body + all version snapshots) to its original path.
///
/// The Markdown move is authoritative. If its derived index cannot be
/// refreshed, the receipt remains successful with a degraded index status;
/// version snapshots stay in the recycle bundle until a later repair can bind
/// them to the regenerated file row.
pub(crate) fn restore_document(state: &Arc<AppState>, id: &str) -> AppResult<FileWriteResult> {
    with_vault_move_lock(|| restore_document_locked(state, id))
}

fn restore_document_locked(state: &Arc<AppState>, id: &str) -> AppResult<FileWriteResult> {
    let vault = state.vault_path()?;
    let (trash_rel, original_path): (String, String) = state.db.with_conn(|conn| {
        conn.query_row(
            "SELECT trash_rel_dir, original_path FROM recycle_bin WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| AppError::msg("回收站中找不到该条目"))
    })?;

    let bundle_dir = vault.join(&trash_rel);
    let manifest = load_manifest(&vault, &trash_rel)?;

    if manifest.original_path != original_path {
        return Err(AppError::msg("回收站条目元数据不一致"));
    }

    let dest = resolve_vault_path(&vault, &manifest.original_path)?;
    if dest.exists() {
        return Err(AppError::msg(format!(
            "无法恢复：「{}」已存在，请先处理冲突后再试。",
            manifest.original_path
        )));
    }
    let doc = bundle_dir.join("document.md");
    if !doc.is_file() {
        return Err(AppError::msg(
            "回收站中的文档文件已损坏（document.md 缺失），无法恢复",
        ));
    }
    let receipt = NoteWriteService::adopt_under_move_lock(state, &doc, &manifest.original_path)?;

    if receipt.index_status == FileWriteIndexStatus::Degraded {
        if manifest.versions.is_empty() {
            purge_bundle(&vault, &trash_rel)?;
            state.db.with_conn(|conn| {
                conn.execute("DELETE FROM recycle_bin WHERE id = ?1", [id])?;
                Ok(())
            })?;
        } else {
            tracing::warn!(
                result_code = "recycle_restore_versions_pending_index_repair",
                "recycle restore persisted Markdown while version metadata awaits index repair"
            );
        }
        return Ok(receipt);
    }

    finalize_restore(state, &vault, id, &trash_rel, &manifest, receipt.entry.id)?;

    Ok(receipt)
}

/// Permanently delete a recycle entry before its expiry.
pub fn purge_recycle_item(state: &AppState, id: &str) -> AppResult<()> {
    with_vault_move_lock(|| {
        let vault = state.vault_path()?;
        ensure_cleanup_recovered(&state.db, &vault)?;
        let trash_rel: String = state.db.with_conn(|conn| {
            conn.query_row(
                "SELECT trash_rel_dir FROM recycle_bin WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .map_err(|_| AppError::msg("回收站中找不到该条目"))
        })?;
        cleanup::validate_bundle_manifest(&vault, id, &trash_rel)?;
        load_manifest(&vault, &trash_rel)?;
        purge_one(&state.db, &vault, id, &trash_rel).map(|_| ())
    })
}

fn purge_one(
    db: &crate::storage::db::Database,
    vault: &Path,
    id: &str,
    trash_rel: &str,
) -> AppResult<u64> {
    let (rows, legacy_paths) = db.with_read_conn(|conn| {
        let rows = crate::version::repository::archived_rows(conn, vault, id)?;
        let paths = crate::version::repository::unshared_non_cas_storage_paths(conn, vault, &rows)?;
        Ok((rows, paths))
    })?;
    let mut checkpoint = cleanup::prepare(
        vault,
        cleanup::CleanupKind::Purge {
            recycle_id: id.to_string(),
            trash_rel_dir: trash_rel.to_string(),
        },
        rows.clone(),
        legacy_paths
            .iter()
            .cloned()
            .map(|relative_path| cleanup::Candidate {
                root: cleanup::CandidateRoot::Versions,
                relative_path,
            })
            .collect(),
    )?;
    let database_result = db.with_conn(|conn| {
        in_immediate_transaction(conn, |conn| {
            crate::version::repository::delete_cleanup_rows(conn, vault, &rows, &legacy_paths)?;
            if conn.execute("DELETE FROM recycle_bin WHERE id = ?1", [id])? != 1 {
                return Err(AppError::msg("recycle_purge_identity_mismatch"));
            }
            Ok(())
        })
    });
    if let Err(error) = database_result {
        return match cleanup::rollback(&mut checkpoint) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(AppError::msg(format!(
                "recycle purge database failed and rollback is pending: {error}; {rollback_error}"
            ))),
        };
    }
    cleanup::finish(db, &checkpoint)
}

fn ensure_cleanup_recovered(db: &crate::storage::db::Database, vault: &Path) -> AppResult<()> {
    let unresolved = cleanup::recover_pending(db, vault);
    if unresolved.is_empty() {
        Ok(())
    } else {
        Err(AppError::msg("recycle_cleanup_recovery_required"))
    }
}

pub(crate) fn recover_cleanup_checkpoints(
    db: &crate::storage::db::Database,
    vault: &Path,
) -> Vec<String> {
    cleanup::recover_pending(db, vault)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::scan::{index_file, scan_vault};
    use crate::version::version_save_manual;
    use std::fs;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup() -> (tempfile::TempDir, Arc<AppState>) {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let data = dir.path().join("data");
        let state = AppState::new(data).unwrap();
        state.set_vault(vault).unwrap();
        (dir, state)
    }

    fn restore_legacy_bundle(
        state: &Arc<AppState>,
        path: &str,
        expires_at: &str,
    ) -> (String, PathBuf) {
        let vault = state.vault_path().unwrap();
        let id = Uuid::new_v4().to_string();
        let trash_rel = format!(".iris/trash/{id}");
        let bundle = vault.join(&trash_rel);
        fs::create_dir_all(bundle.join("versions")).unwrap();
        fs::write(bundle.join("document.md"), "restored body").unwrap();
        fs::write(bundle.join("versions/legacy-1.md"), "legacy snapshot").unwrap();
        let manifest = TrashManifest {
            original_path: path.to_string(),
            title: "Legacy".to_string(),
            deleted_at: "2026-01-01T00:00:00Z".to_string(),
            expires_at: expires_at.to_string(),
            versions: vec![TrashVersionMeta {
                version_id: None,
                version_no: "legacy-1".to_string(),
                label: None,
                content_hash: crate::cas::hash::content_hash_str("legacy snapshot"),
                storage_path: "old/legacy-1.md".to_string(),
                word_count: 2,
                is_finalized: false,
                kind: "manual".to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                trash_file: "legacy-1.md".to_string(),
                unreadable: None,
            }],
        };
        fs::write(
            bundle.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        state
            .db
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO recycle_bin
                     (id, original_path, title, deleted_at, expires_at, trash_rel_dir)
                     VALUES (?1, ?2, 'Legacy', ?3, ?4, ?5)",
                    rusqlite::params![
                        id,
                        path,
                        manifest.deleted_at,
                        manifest.expires_at,
                        trash_rel
                    ],
                )?;
                Ok(())
            })
            .unwrap();
        restore_document(state, &id).unwrap();
        let storage: String = state
            .db
            .with_read_conn(|conn| {
                conn.query_row(
                    "SELECT storage_path FROM versions
                     WHERE vault_path = ?1 AND note_path = ?2 AND version_no = 'legacy-1'",
                    rusqlite::params![vault.to_string_lossy(), path],
                    |row| row.get(0),
                )
                .map_err(Into::into)
            })
            .unwrap();
        let file = versions_root(&vault).join(storage);
        assert_eq!(fs::read_to_string(&file).unwrap(), "legacy snapshot");
        (id, file)
    }

    fn install_delete_failure(state: &AppState, target: &str) {
        let sql = match target {
            "versions" => {
                "CREATE TRIGGER fail_purge_versions BEFORE DELETE ON versions
                 BEGIN SELECT RAISE(ABORT, 'simulated version purge failure'); END;"
            }
            "recycle_bin" => {
                "CREATE TRIGGER fail_purge_recycle BEFORE DELETE ON recycle_bin
                 BEGIN SELECT RAISE(ABORT, 'simulated recycle purge failure'); END;"
            }
            _ => unreachable!(),
        };
        state
            .db
            .with_conn(|conn| {
                conn.execute_batch(sql)?;
                Ok(())
            })
            .unwrap();
    }

    fn remove_delete_failure(state: &AppState, target: &str) {
        let sql = match target {
            "versions" => "DROP TRIGGER fail_purge_versions;",
            "recycle_bin" => "DROP TRIGGER fail_purge_recycle;",
            _ => unreachable!(),
        };
        state
            .db
            .with_conn(|conn| {
                conn.execute_batch(sql)?;
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn trash_moves_document_and_versions() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        let note = vault.join("note.md");
        fs::write(&note, "# Note\n\nBody.").unwrap();
        state.db.with_conn(|conn| scan_vault(conn, &vault)).unwrap();
        version_save_manual(&state, "note.md", "# Note\n\nBody v2.").unwrap();
        trash_document(&state, "note.md").unwrap();
        assert!(!note.exists());
        state
            .db
            .with_conn(|conn| {
                let file_rows: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM files WHERE path = 'note.md'",
                    [],
                    |r| r.get(0),
                )?;
                let chunk_rows: i64 =
                    conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
                let fts_rows: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM files_fts WHERE path = 'note.md'",
                    [],
                    |r| r.get(0),
                )?;
                assert_eq!(file_rows, 0);
                assert_eq!(chunk_rows, 0);
                assert_eq!(fts_rows, 0);
                Ok::<_, crate::error::AppError>(())
            })
            .unwrap();
        let items = list_recycle(&state).unwrap();
        assert_eq!(items.len(), 1);
        let bundle = trash_root(&vault).join(&items[0].id);
        assert!(bundle.join("document.md").is_file());
        assert!(bundle.join("manifest.json").is_file());
        let manifest: TrashManifest =
            serde_json::from_str(&fs::read_to_string(bundle.join("manifest.json")).unwrap())
                .unwrap();
        assert!(!manifest.versions.is_empty());
        assert!(crate::version::version_list(&state, "note.md")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn interrupted_trash_metadata_is_recovered_from_the_prepared_manifest() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        fs::write(vault.join("interrupted.md"), "recover me\n").unwrap();
        state.db.with_conn(|conn| {
            index_file(conn, &vault, &vault.join("interrupted.md"))?;
            conn.execute_batch("CREATE TRIGGER fail_trash BEFORE INSERT ON recycle_bin BEGIN SELECT RAISE(ABORT, 'fault'); END;")?;
            Ok(())
        }).unwrap();
        let _ = trash_document(&state, "interrupted.md");
        state
            .db
            .with_conn(|conn| {
                conn.execute_batch("DROP TRIGGER fail_trash;")?;
                Ok(())
            })
            .unwrap();
        let items = list_recycle(&state).unwrap();
        assert_eq!(
            items.len(),
            1,
            "a durable trash bundle cannot disappear merely because bookkeeping failed"
        );
        restore_document(&state, &items[0].id).unwrap();
        assert_eq!(
            fs::read_to_string(vault.join("interrupted.md")).unwrap(),
            "recover me\n"
        );
    }

    #[test]
    fn restore_roundtrip_restores_body_and_versions() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        let note = vault.join("restore-me.md");
        fs::write(&note, "# Restore\n\nBody.").unwrap();
        state.db.with_conn(|conn| scan_vault(conn, &vault)).unwrap();
        version_save_manual(&state, "restore-me.md", "# Restore\n\nBody v2.").unwrap();
        trash_document(&state, "restore-me.md").unwrap();
        let id = list_recycle(&state).unwrap()[0].id.clone();
        let receipt = restore_document(&state, &id).unwrap();
        assert_eq!(receipt.entry.path, "restore-me.md");
        assert_eq!(receipt.index_status, FileWriteIndexStatus::Synced);
        assert!(note.is_file());
        assert!(list_recycle(&state).unwrap().is_empty());
        let versions = crate::version::version_list(&state, "restore-me.md").unwrap();
        assert!(!versions.is_empty());
    }

    #[test]
    fn restore_reports_degraded_after_markdown_is_recovered_when_indexing_fails() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        let note = vault.join("restore-degraded.md");
        fs::write(&note, "# Restore\n\nBody.").unwrap();
        state.db.with_conn(|conn| scan_vault(conn, &vault)).unwrap();
        trash_document(&state, "restore-degraded.md").unwrap();
        let id = list_recycle(&state).unwrap()[0].id.clone();
        state
            .db
            .with_conn(|conn| {
                conn.execute_batch(
                    "CREATE TRIGGER fail_restore_index
                     BEFORE INSERT ON files
                     WHEN NEW.path = 'restore-degraded.md'
                     BEGIN
                       SELECT RAISE(ABORT, 'simulated index failure');
                     END;",
                )?;
                Ok(())
            })
            .unwrap();

        let receipt = restore_document(&state, &id).unwrap();

        assert_eq!(
            receipt.index_status,
            crate::storage::note_write::FileWriteIndexStatus::Degraded
        );
        assert_eq!(receipt.entry.path, "restore-degraded.md");
        assert_eq!(fs::read_to_string(note).unwrap(), "# Restore\n\nBody.");
        assert!(list_recycle(&state).unwrap().is_empty());
    }

    #[test]
    fn deferred_restore_keeps_versions_until_index_repair_can_bind_them() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        let note = vault.join("restore-versions.md");
        fs::write(&note, "# Restore\n\nBody.").unwrap();
        state.db.with_conn(|conn| scan_vault(conn, &vault)).unwrap();
        version_save_manual(&state, "restore-versions.md", "# Restore\n\nSnapshot").unwrap();
        trash_document(&state, "restore-versions.md").unwrap();
        let id = list_recycle(&state).unwrap()[0].id.clone();
        state
            .db
            .with_conn(|conn| {
                conn.execute_batch(
                    "CREATE TRIGGER fail_restore_versions_index
                     BEFORE INSERT ON files
                     WHEN NEW.path = 'restore-versions.md'
                     BEGIN
                       SELECT RAISE(ABORT, 'simulated index failure');
                     END;",
                )?;
                Ok(())
            })
            .unwrap();

        let receipt = restore_document(&state, &id).unwrap();

        assert_eq!(receipt.index_status, FileWriteIndexStatus::Degraded);
        assert_eq!(fs::read_to_string(&note).unwrap(), "# Restore\n\nBody.");
        assert_eq!(list_recycle(&state).unwrap().len(), 1);

        state
            .db
            .with_conn(|conn| {
                conn.execute_batch("DROP TRIGGER fail_restore_versions_index;")?;
                Ok(())
            })
            .unwrap();

        assert!(list_recycle(&state).unwrap().is_empty());
        let versions = crate::version::version_list(&state, "restore-versions.md").unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(
            crate::version::version_preview(&state, versions[0].id).unwrap(),
            "# Restore\n\nSnapshot"
        );
    }

    #[test]
    fn legacy_bundle_restore_resumes_without_duplicating_partially_imported_versions() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        let id = Uuid::new_v4().to_string();
        let trash_rel = format!(".iris/trash/{id}");
        let bundle = vault.join(&trash_rel);
        fs::create_dir_all(bundle.join("versions")).unwrap();
        fs::write(bundle.join("document.md"), "restored body").unwrap();
        let versions = ["legacy-1", "legacy-2"]
            .into_iter()
            .map(|version_no| TrashVersionMeta {
                version_id: None,
                version_no: version_no.to_string(),
                label: None,
                content_hash: crate::cas::hash::content_hash_str(version_no),
                storage_path: format!("old/{version_no}.md"),
                word_count: 1,
                is_finalized: false,
                kind: "manual".to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                trash_file: format!("{version_no}.md"),
                unreadable: None,
            })
            .collect::<Vec<_>>();
        for version in &versions {
            fs::write(
                bundle.join("versions").join(&version.trash_file),
                &version.version_no,
            )
            .unwrap();
        }
        let manifest = TrashManifest {
            original_path: "legacy.md".to_string(),
            title: "Legacy".to_string(),
            deleted_at: "2026-01-01T00:00:00Z".to_string(),
            expires_at: "2099-01-01T00:00:00Z".to_string(),
            versions,
        };
        fs::write(
            bundle.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        state
            .db
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO recycle_bin
                 (id, original_path, title, deleted_at, expires_at, trash_rel_dir)
                 VALUES (?1, 'legacy.md', 'Legacy', ?2, ?3, ?4)",
                    rusqlite::params![id, manifest.deleted_at, manifest.expires_at, trash_rel],
                )?;
                conn.execute_batch(
                    "CREATE TRIGGER fail_second_legacy_import
                 BEFORE INSERT ON versions WHEN NEW.version_no = 'legacy-2'
                 BEGIN SELECT RAISE(ABORT, 'fault'); END;",
                )?;
                Ok(())
            })
            .unwrap();

        assert!(restore_document(&state, &id).is_err());
        assert_eq!(
            fs::read_to_string(vault.join("legacy.md")).unwrap(),
            "restored body"
        );
        state
            .db
            .with_conn(|conn| {
                conn.execute_batch("DROP TRIGGER fail_second_legacy_import")?;
                Ok(())
            })
            .unwrap();

        assert!(list_recycle(&state).unwrap().is_empty());
        let restored = crate::version::version_list(&state, "legacy.md").unwrap();
        assert_eq!(restored.len(), 2);
        assert_eq!(
            restored
                .iter()
                .filter(|entry| entry.version_no == "legacy-1")
                .count(),
            1,
        );
    }

    #[test]
    fn trash_and_restore_preserves_cas_version_blob() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        let note = vault.join("cas-note.md");
        fs::write(&note, "# CAS\n\nv1.").unwrap();
        state.db.with_conn(|conn| scan_vault(conn, &vault)).unwrap();

        let snapshot_body = "# CAS\n\nv2 unique snapshot body.";
        let entry = version_save_manual(&state, "cas-note.md", snapshot_body)
            .unwrap()
            .expect("manual snapshot");

        let storage_path: String = state
            .db
            .with_conn(|conn| {
                let path: String = conn.query_row(
                    "SELECT storage_path FROM versions WHERE id = ?1",
                    [entry.id],
                    |r| r.get(0),
                )?;
                assert!(crate::version::is_cas_storage_path(&path));
                Ok(path)
            })
            .unwrap();

        trash_document(&state, "cas-note.md").unwrap();
        let items = list_recycle(&state).unwrap();
        assert_eq!(items.len(), 1);
        let bundle = trash_root(&vault).join(&items[0].id);
        let manifest: TrashManifest =
            serde_json::from_str(&fs::read_to_string(bundle.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest.versions.len(), 1);
        let trash_file = &manifest.versions[0].trash_file;
        let trash_body = fs::read_to_string(bundle.join("versions").join(trash_file)).unwrap();
        assert_eq!(trash_body, snapshot_body);

        let id = items[0].id.clone();
        restore_document(&state, &id).unwrap();

        let versions = crate::version::version_list(&state, "cas-note.md").unwrap();
        assert_eq!(versions.len(), 1);
        let preview = crate::version::version_preview(&state, versions[0].id).unwrap();
        assert_eq!(preview, snapshot_body);

        let restored_storage: String = state
            .db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT storage_path FROM versions WHERE id = ?1",
                    [versions[0].id],
                    |r| r.get(0),
                )
                .map_err(Into::into)
            })
            .unwrap();
        assert!(
            crate::version::read_version_content(&state, &vault, &restored_storage).unwrap()
                == snapshot_body
        );
        assert!(crate::version::is_cas_storage_path(&storage_path));
    }

    #[test]
    fn trash_restore_materializes_diff_version_for_preview() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        let note = vault.join("diff-note.md");
        fs::write(&note, "# Diff\n\nCurrent body").unwrap();
        state.db.with_conn(|conn| scan_vault(conn, &vault)).unwrap();
        // The legacy delta format only represents newline-free endings exactly.
        // Exercise real delta materialization, not the full-blob fallback.
        let base = format!("{}\nline-a", "shared-prefix-".repeat(80));
        let expected = format!("{}\nline-b", "shared-prefix-".repeat(80));
        version_save_manual(&state, "diff-note.md", &base)
            .unwrap()
            .expect("base snapshot");
        let delta = version_save_manual(&state, "diff-note.md", &expected)
            .unwrap()
            .expect("delta snapshot");
        let storage_path: String = state
            .db
            .with_read_conn(|conn| {
                conn.query_row(
                    "SELECT storage_path FROM versions WHERE id = ?1",
                    [delta.id],
                    |row| row.get(0),
                )
                .map_err(Into::into)
            })
            .unwrap();
        assert!(
            storage_path.starts_with("dif:"),
            "precondition: {storage_path}"
        );
        let expected_preview = crate::version::version_preview(&state, delta.id).unwrap();
        assert_eq!(expected_preview, expected);

        trash_document(&state, "diff-note.md").expect("trash diff-backed note");
        let item = list_recycle(&state).unwrap().remove(0);
        restore_document(&state, &item.id).expect("restore diff-backed note");

        let versions = crate::version::version_list(&state, "diff-note.md").unwrap();
        let restored = versions
            .iter()
            .find(|entry| entry.version_no == delta.version_no)
            .expect("restored delta version");
        assert_eq!(
            crate::version::version_preview(&state, restored.id).unwrap(),
            expected_preview
        );
    }

    #[test]
    fn trash_preserves_unreadable_version_ownership_through_restore() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        let note = vault.join("mixed.md");
        fs::write(&note, "# Mixed\n\nCurrent body").unwrap();
        state.db.with_conn(|conn| scan_vault(conn, &vault)).unwrap();

        // 一个正常可读版本。
        let readable_body = "# Mixed\n\nReadable snapshot";
        version_save_manual(&state, "mixed.md", readable_body).unwrap();

        // 一个不可读版本：blob 用错误密钥加密后手动写入对象存储，并插入版本行。
        let unreadable_body = b"# Mixed\n\nUnreadable snapshot";
        let unreadable_hash = crate::cas::hash::content_hash(unreadable_body);
        let blob_path = state
            .cas_store()
            .unwrap()
            .object_path(&unreadable_hash)
            .unwrap();
        let mut buf = b"CASE".to_vec();
        buf.extend_from_slice(
            &crate::cas::encryption::encrypt_blob(unreadable_body, &[0xEE; 32]).unwrap(),
        );
        std::fs::create_dir_all(blob_path.parent().unwrap()).unwrap();
        std::fs::write(&blob_path, &buf).unwrap();
        let file_id: i64 = state
            .db
            .with_conn(|conn| {
                conn.query_row("SELECT id FROM files WHERE path = 'mixed.md'", [], |r| {
                    r.get(0)
                })
                .map_err(Into::into)
            })
            .unwrap();
        state
            .db
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO versions (file_id, version_no, label, content_hash, word_count, is_finalized, kind, created_at, storage_path, vault_path, note_path)
                     VALUES (?1, 'unreadable-1', NULL, ?2, 0, 0, 'manual', '2026-01-01T00:00:00+00:00', ?3, ?4, 'mixed.md')",
                    rusqlite::params![file_id, unreadable_hash, format!("cas:{unreadable_hash}"), vault.to_string_lossy()],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(
            crate::version::version_list(&state, "mixed.md")
                .unwrap()
                .len()
                == 2,
            "precondition: two version rows"
        );

        // 删除必须成功：不可读版本被跳过而非阻断整个删除。
        trash_document(&state, "mixed.md").unwrap();
        assert!(!note.exists());

        let items = list_recycle(&state).unwrap();
        let bundle = trash_root(&vault).join(&items[0].id);
        let manifest: TrashManifest =
            serde_json::from_str(&fs::read_to_string(bundle.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest.versions.len(), 2);
        let unreadable_meta = manifest
            .versions
            .iter()
            .find(|v| v.version_no == "unreadable-1")
            .expect("unreadable version recorded in manifest");
        assert_eq!(
            unreadable_meta.unreadable,
            Some(true),
            "skipped version must be marked unreadable in manifest"
        );
        let readable_meta = manifest
            .versions
            .iter()
            .find(|v| v.version_no != "unreadable-1")
            .expect("readable version recorded");
        assert_ne!(readable_meta.unreadable, Some(true));
        let copied = fs::read_dir(bundle.join("versions"))
            .unwrap()
            .filter_map(Result::ok)
            .count();
        assert_eq!(
            copied, 1,
            "only the readable snapshot may be copied into the bundle"
        );

        // Unreadability never removes metadata or original recovery material.
        restore_document(&state, &items[0].id).unwrap();
        let versions = crate::version::version_list(&state, "mixed.md").unwrap();
        assert_eq!(versions.len(), 2);
        assert!(blob_path.is_file());
        let readable = versions
            .iter()
            .find(|v| v.version_no == readable_meta.version_no)
            .unwrap();
        assert_eq!(
            crate::version::version_preview(&state, readable.id).unwrap(),
            readable_body
        );
    }

    #[test]
    fn discard_removes_without_recycle_row() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        let note = vault.join("blank.md");
        fs::write(&note, "---\ntitle: \"\"\n---\n\n").unwrap();
        state
            .db
            .with_conn(|conn| index_file(conn, &vault, &note))
            .unwrap();
        discard_document(&state, "blank.md").unwrap();
        assert!(!note.exists());
        assert!(list_recycle(&state).unwrap().is_empty());
    }

    #[test]
    fn discard_releases_cas_reference_for_permanent_cleanup() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        let note = vault.join("discard-cas.md");
        fs::write(&note, "# Discard\n\nCurrent body").unwrap();
        state.db.with_conn(|conn| scan_vault(conn, &vault)).unwrap();
        let snapshot = version_save_manual(&state, "discard-cas.md", "# Discard\n\nSnapshot")
            .unwrap()
            .expect("snapshot");
        let storage_path: String = state
            .db
            .with_read_conn(|conn| {
                conn.query_row(
                    "SELECT storage_path FROM versions WHERE id = ?1",
                    [snapshot.id],
                    |row| row.get(0),
                )
                .map_err(Into::into)
            })
            .unwrap();
        let hash = storage_path.strip_prefix("cas:").expect("CAS snapshot");
        assert_eq!(state.ref_counter().get_count(hash).unwrap(), 1);

        discard_document(&state, "discard-cas.md").unwrap();

        assert_eq!(state.ref_counter().get_count(hash).unwrap(), 0);
    }

    #[test]
    fn discard_rejects_a_non_file_before_removing_history_ownership() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        fs::create_dir(vault.join("broken.md")).unwrap();
        state
            .db
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO versions
                 (file_id, version_no, content_hash, storage_path, kind, created_at,
                  vault_path, note_path)
                 VALUES (0, 'keep-1', 'hash', 'legacy/keep.md', 'manual', datetime('now'),
                         ?1, 'broken.md')",
                    [vault.to_string_lossy()],
                )?;
                Ok(())
            })
            .unwrap();

        assert!(discard_document(&state, "broken.md").is_err());
        let remaining = state
            .db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM versions WHERE version_no = 'keep-1'",
                    [],
                    |row| row.get::<_, i64>(0),
                )?)
            })
            .unwrap();
        assert_eq!(remaining, 1);
        assert!(vault.join("broken.md").is_dir());
    }

    #[test]
    fn reconcile_never_archives_history_from_a_recreated_same_path_note() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        fs::write(vault.join("same.md"), "old body").unwrap();
        let old = version_save_manual(&state, "same.md", "old snapshot")
            .unwrap()
            .unwrap();
        trash_document(&state, "same.md").unwrap();
        fs::write(vault.join("same.md"), "new body").unwrap();
        let new = version_save_manual(&state, "same.md", "new snapshot")
            .unwrap()
            .unwrap();

        assert_eq!(list_recycle(&state).unwrap().len(), 1);
        assert_eq!(list_recycle(&state).unwrap().len(), 1);
        let active = crate::version::version_list(&state, "same.md").unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, new.id);
        state
            .db
            .with_read_conn(|conn| {
                let old_owner: Option<String> = conn.query_row(
                    "SELECT recycle_id FROM versions WHERE id = ?1",
                    [old.id],
                    |row| row.get(0),
                )?;
                assert!(old_owner.is_some());
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn manual_purge_db_failures_keep_bundle_discoverable_and_retryable() {
        for target in ["versions", "recycle_bin"] {
            let (_dir, state) = setup();
            let vault = state.vault_path().unwrap();
            fs::write(vault.join("purge.md"), "body").unwrap();
            version_save_manual(&state, "purge.md", "snapshot")
                .unwrap()
                .unwrap();
            trash_document(&state, "purge.md").unwrap();
            let id = list_recycle(&state).unwrap()[0].id.clone();
            install_delete_failure(&state, target);

            assert!(purge_recycle_item(&state, &id).is_err());
            let items = list_recycle(&state).unwrap();
            assert_eq!(items.len(), 1, "failed {target} delete hid the bundle");
            assert_eq!(items[0].id, id);
            assert!(trash_root(&vault).join(&id).join("document.md").is_file());

            remove_delete_failure(&state, target);
            purge_recycle_item(&state, &id).unwrap();
            assert!(list_recycle(&state).unwrap().is_empty());
            assert!(!trash_root(&vault).join(&id).exists());
        }
    }

    #[test]
    fn manual_purge_database_failure_still_allows_full_restore() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        fs::write(vault.join("restore-after-purge.md"), "current body").unwrap();
        version_save_manual(&state, "restore-after-purge.md", "historical body")
            .unwrap()
            .unwrap();
        trash_document(&state, "restore-after-purge.md").unwrap();
        let id = list_recycle(&state).unwrap()[0].id.clone();
        install_delete_failure(&state, "recycle_bin");
        assert!(purge_recycle_item(&state, &id).is_err());
        remove_delete_failure(&state, "recycle_bin");

        restore_document(&state, &id).unwrap();

        assert_eq!(
            fs::read_to_string(vault.join("restore-after-purge.md")).unwrap(),
            "current body"
        );
        let versions = crate::version::version_list(&state, "restore-after-purge.md").unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(
            crate::version::version_preview(&state, versions[0].id).unwrap(),
            "historical body"
        );
    }

    #[test]
    fn expired_purge_db_failures_keep_bundle_discoverable_and_retryable() {
        for target in ["versions", "recycle_bin"] {
            let (_dir, state) = setup();
            let vault = state.vault_path().unwrap();
            fs::write(vault.join("expired.md"), "body").unwrap();
            version_save_manual(&state, "expired.md", "snapshot")
                .unwrap()
                .unwrap();
            trash_document(&state, "expired.md").unwrap();
            let id = list_recycle(&state).unwrap()[0].id.clone();
            state
                .db
                .with_conn(|conn| {
                    conn.execute(
                        "UPDATE recycle_bin SET expires_at = '2020-01-01T00:00:00Z'
                         WHERE id = ?1",
                        [&id],
                    )?;
                    Ok(())
                })
                .unwrap();
            install_delete_failure(&state, target);

            assert!(purge_expired_items(&state.db, &vault).is_err());
            let items = list_recycle(&state).unwrap();
            assert_eq!(items.len(), 1, "failed {target} delete hid the bundle");
            assert_eq!(items[0].id, id);

            remove_delete_failure(&state, target);
            let (count, _) = purge_expired_items(&state.db, &vault).unwrap();
            assert_eq!(count, 1);
            assert!(list_recycle(&state).unwrap().is_empty());
            assert!(!trash_root(&vault).join(&id).exists());
        }
    }

    #[test]
    fn legacy_restore_then_discard_removes_materialized_snapshot() {
        let (_dir, state) = setup();
        let (_id, version_file) =
            restore_legacy_bundle(&state, "legacy-discard.md", "2099-01-01T00:00:00Z");

        discard_document(&state, "legacy-discard.md").unwrap();

        assert!(!version_file.exists());
        let count = state
            .db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM versions WHERE note_path = 'legacy-discard.md'",
                    [],
                    |row| row.get::<_, i64>(0),
                )?)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn legacy_discard_database_failure_restores_note_snapshot_and_identity() {
        let (_dir, state) = setup();
        let (_id, version_file) =
            restore_legacy_bundle(&state, "legacy-discard-fail.md", "2099-01-01T00:00:00Z");
        let note = state.vault_path().unwrap().join("legacy-discard-fail.md");
        install_delete_failure(&state, "versions");

        assert!(discard_document(&state, "legacy-discard-fail.md").is_err());
        assert_eq!(fs::read_to_string(&note).unwrap(), "restored body");
        assert_eq!(
            fs::read_to_string(&version_file).unwrap(),
            "legacy snapshot"
        );
        assert_eq!(
            crate::version::version_list(&state, "legacy-discard-fail.md")
                .unwrap()
                .len(),
            1
        );

        remove_delete_failure(&state, "versions");
        discard_document(&state, "legacy-discard-fail.md").unwrap();
        assert!(!note.exists());
        assert!(!version_file.exists());
    }

    #[test]
    fn legacy_restore_then_trash_and_purge_removes_materialized_snapshot() {
        let (_dir, state) = setup();
        let (_restored_id, version_file) =
            restore_legacy_bundle(&state, "legacy-purge.md", "2099-01-01T00:00:00Z");
        trash_document(&state, "legacy-purge.md").unwrap();
        let id = list_recycle(&state).unwrap()[0].id.clone();

        purge_recycle_item(&state, &id).unwrap();

        assert!(!version_file.exists());
        assert!(!trash_root(&state.vault_path().unwrap()).join(id).exists());
    }

    #[test]
    fn purge_releases_cas_ownership_without_mixing_it_into_legacy_file_cleanup() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        fs::write(vault.join("purge-cas.md"), "body").unwrap();
        let version = version_save_manual(&state, "purge-cas.md", "cas snapshot")
            .unwrap()
            .unwrap();
        let storage: String = state
            .db
            .with_read_conn(|conn| {
                conn.query_row(
                    "SELECT storage_path FROM versions WHERE id = ?1",
                    [version.id],
                    |row| row.get(0),
                )
                .map_err(Into::into)
            })
            .unwrap();
        let hash = storage.strip_prefix("cas:").unwrap();
        assert_eq!(state.ref_counter().get_count(hash).unwrap(), 1);
        trash_document(&state, "purge-cas.md").unwrap();
        let id = list_recycle(&state).unwrap()[0].id.clone();

        purge_recycle_item(&state, &id).unwrap();

        assert_eq!(state.ref_counter().get_count(hash).unwrap(), 0);
        let rows = state
            .db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM versions WHERE id = ?1",
                    [version.id],
                    |row| row.get::<_, i64>(0),
                )?)
            })
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn legacy_purge_database_failure_restores_snapshot_and_bundle_for_retry() {
        let (_dir, state) = setup();
        let (_restored_id, version_file) =
            restore_legacy_bundle(&state, "legacy-purge-fail.md", "2099-01-01T00:00:00Z");
        trash_document(&state, "legacy-purge-fail.md").unwrap();
        let id = list_recycle(&state).unwrap()[0].id.clone();
        install_delete_failure(&state, "versions");

        assert!(purge_recycle_item(&state, &id).is_err());
        assert_eq!(
            fs::read_to_string(&version_file).unwrap(),
            "legacy snapshot"
        );
        assert_eq!(list_recycle(&state).unwrap()[0].id, id);
        assert!(trash_root(&state.vault_path().unwrap())
            .join(&id)
            .join("document.md")
            .is_file());

        remove_delete_failure(&state, "versions");
        purge_recycle_item(&state, &id).unwrap();
        assert!(!version_file.exists());
        assert!(list_recycle(&state).unwrap().is_empty());
    }

    #[test]
    fn discard_rejects_legacy_storage_outside_version_root_before_changing_state() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        let note = vault.join("bounded.md");
        let outside = vault.join("outside.md");
        fs::write(&note, "body").unwrap();
        fs::write(&outside, "outside").unwrap();
        state
            .db
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO versions
                     (file_id, version_no, content_hash, storage_path, kind, created_at,
                      vault_path, note_path)
                     VALUES (0, 'outside', 'hash', '../../outside.md', 'manual', datetime('now'),
                             ?1, 'bounded.md')",
                    [vault.to_string_lossy()],
                )?;
                Ok(())
            })
            .unwrap();

        assert!(discard_document(&state, "bounded.md").is_err());
        assert_eq!(fs::read_to_string(&note).unwrap(), "body");
        assert_eq!(fs::read_to_string(&outside).unwrap(), "outside");
        let count = state
            .db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM versions WHERE version_no = 'outside'",
                    [],
                    |row| row.get::<_, i64>(0),
                )?)
            })
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn discard_keeps_non_cas_file_referenced_by_another_history_owner() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        fs::write(vault.join("owner-a.md"), "body a").unwrap();
        fs::create_dir_all(versions_root(&vault).join("shared")).unwrap();
        let shared = versions_root(&vault).join("shared/snapshot.md");
        fs::write(&shared, "shared body").unwrap();
        state
            .db
            .with_conn(|conn| {
                for (version_no, note_path) in [("a", "owner-a.md"), ("b", "owner-b.md")] {
                    conn.execute(
                        "INSERT INTO versions
                         (file_id, version_no, content_hash, storage_path, kind, created_at,
                          vault_path, note_path)
                         VALUES (0, ?1, 'hash', 'shared/snapshot.md', 'manual', datetime('now'),
                                 ?2, ?3)",
                        rusqlite::params![version_no, vault.to_string_lossy(), note_path],
                    )?;
                }
                Ok(())
            })
            .unwrap();

        discard_document(&state, "owner-a.md").unwrap();

        assert_eq!(fs::read_to_string(&shared).unwrap(), "shared body");
        let owner_b = state
            .db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM versions WHERE note_path = 'owner-b.md'",
                    [],
                    |row| row.get::<_, i64>(0),
                )?)
            })
            .unwrap();
        assert_eq!(owner_b, 1);
    }

    #[cfg(unix)]
    #[test]
    fn discard_rejects_symlinked_legacy_storage_before_changing_state() {
        use std::os::unix::fs::symlink;

        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        let note = vault.join("symlinked.md");
        let outside = vault.join("outside-versions");
        fs::write(&note, "body").unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("snapshot.md"), "outside snapshot").unwrap();
        fs::create_dir_all(versions_root(&vault)).unwrap();
        symlink(&outside, versions_root(&vault).join("linked")).unwrap();
        state
            .db
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO versions
                     (file_id, version_no, content_hash, storage_path, kind, created_at,
                      vault_path, note_path)
                     VALUES (0, 'symlink', 'hash', 'linked/snapshot.md', 'manual', datetime('now'),
                             ?1, 'symlinked.md')",
                    [vault.to_string_lossy()],
                )?;
                Ok(())
            })
            .unwrap();

        assert!(discard_document(&state, "symlinked.md").is_err());
        assert_eq!(fs::read_to_string(note).unwrap(), "body");
        assert_eq!(
            fs::read_to_string(outside.join("snapshot.md")).unwrap(),
            "outside snapshot"
        );
    }

    #[test]
    fn ordinary_trash_reports_ownership_mismatch_after_preserving_the_bundle() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        let note = vault.join("trash-mismatch.md");
        fs::write(&note, "body").unwrap();
        version_save_manual(&state, "trash-mismatch.md", "snapshot")
            .unwrap()
            .unwrap();
        state
            .db
            .with_conn(|conn| {
                conn.execute_batch(
                    "CREATE TRIGGER remove_version_during_archive
                     BEFORE UPDATE OF recycle_id ON versions
                     WHEN OLD.note_path = 'trash-mismatch.md'
                     BEGIN DELETE FROM versions WHERE id = OLD.id; END;",
                )?;
                Ok(())
            })
            .unwrap();

        let error = trash_document(&state, "trash-mismatch.md").unwrap_err();

        assert!(error
            .to_string()
            .contains("recycled_version_ownership_mismatch"));
        assert!(
            !note.exists(),
            "the authoritative body move already happened"
        );
        let bundles = fs::read_dir(trash_root(&vault))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| Uuid::parse_str(&entry.file_name().to_string_lossy()).is_ok())
            .collect::<Vec<_>>();
        assert_eq!(bundles.len(), 1);
        assert!(bundles[0].path().join("document.md").is_file());
        assert!(bundles[0].path().join("manifest.json").is_file());
        let (versions, recycle_rows) = state
            .db
            .with_read_conn(|conn| {
                Ok((
                    conn.query_row("SELECT COUNT(*) FROM versions", [], |row| {
                        row.get::<_, i64>(0)
                    })?,
                    conn.query_row("SELECT COUNT(*) FROM recycle_bin", [], |row| {
                        row.get::<_, i64>(0)
                    })?,
                ))
            })
            .unwrap();
        assert_eq!(versions, 1, "the failed archive transaction must roll back");
        assert_eq!(recycle_rows, 0);
    }

    #[test]
    fn trash_receipt_exposes_ownership_mismatch_and_recovery_identity() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        fs::write(vault.join("receipt-mismatch.md"), "body").unwrap();
        version_save_manual(&state, "receipt-mismatch.md", "snapshot")
            .unwrap()
            .unwrap();
        state
            .db
            .with_conn(|conn| {
                conn.execute_batch(
                    "CREATE TRIGGER remove_receipt_version_during_archive
                     BEFORE UPDATE OF recycle_id ON versions
                     WHEN OLD.note_path = 'receipt-mismatch.md'
                     BEGIN DELETE FROM versions WHERE id = OLD.id; END;",
                )?;
                Ok(())
            })
            .unwrap();

        let receipt = trash_with_receipt(&state, "receipt-mismatch.md").unwrap();
        let serialized = serde_json::to_value(&receipt).unwrap();

        assert_eq!(
            serialized["metadataError"],
            "recycled_version_ownership_mismatch"
        );
        assert_eq!(serialized["metadataPending"], true);
        assert!(Uuid::parse_str(serialized["trashId"].as_str().unwrap()).is_ok());
        assert!(trash_root(&vault)
            .join(serialized["trashId"].as_str().unwrap())
            .join("document.md")
            .is_file());
    }

    #[test]
    fn discard_fails_closed_when_a_row_is_reassigned_after_staging() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        let note = vault.join("staged-reassign.md");
        let relative = "recycled/staged-reassign.md";
        let snapshot = versions_root(&vault).join(relative);
        fs::create_dir_all(snapshot.parent().unwrap()).unwrap();
        fs::write(&note, "body").unwrap();
        fs::write(&snapshot, "snapshot").unwrap();
        state
            .db
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO versions
                     (file_id, version_no, content_hash, storage_path, kind, created_at,
                      vault_path, note_path)
                     VALUES (0, 'staged-reassign', 'hash', ?1, 'manual', datetime('now'),
                             ?2, 'staged-reassign.md')",
                    rusqlite::params![relative, vault.to_string_lossy()],
                )?;
                conn.execute_batch(
                    "CREATE TRIGGER reassign_version_during_discard
                     BEFORE DELETE ON versions
                     WHEN OLD.note_path = 'staged-reassign.md'
                     BEGIN
                       UPDATE versions SET note_path = 'new-owner.md' WHERE id = OLD.id;
                       SELECT RAISE(IGNORE);
                     END;",
                )?;
                Ok(())
            })
            .unwrap();

        assert!(discard_document(&state, "staged-reassign.md").is_err());
        assert_eq!(fs::read_to_string(&note).unwrap(), "body");
        assert_eq!(fs::read_to_string(&snapshot).unwrap(), "snapshot");
        let owner: String = state
            .db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT note_path FROM versions WHERE version_no = 'staged-reassign'",
                    [],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(owner, "staged-reassign.md");
    }

    #[test]
    fn purge_fails_closed_when_a_new_storage_owner_appears_after_staging() {
        let (_dir, state) = setup();
        let (_restored_id, version_file) =
            restore_legacy_bundle(&state, "purge-new-owner.md", "2099-01-01T00:00:00Z");
        trash_document(&state, "purge-new-owner.md").unwrap();
        let id = list_recycle(&state).unwrap()[0].id.clone();
        state
            .db
            .with_conn(|conn| {
                conn.execute_batch(
                    "CREATE TRIGGER add_owner_during_purge
                     BEFORE DELETE ON versions
                     WHEN OLD.note_path = 'purge-new-owner.md'
                     BEGIN
                       INSERT INTO versions
                         (file_id, version_no, content_hash, storage_path, kind, created_at,
                          vault_path, note_path)
                       VALUES
                         (0, 'injected-owner', OLD.content_hash, OLD.storage_path, 'manual',
                          datetime('now'), OLD.vault_path, 'new-owner.md');
                     END;",
                )?;
                Ok(())
            })
            .unwrap();

        assert!(purge_recycle_item(&state, &id).is_err());
        assert_eq!(
            fs::read_to_string(&version_file).unwrap(),
            "legacy snapshot"
        );
        assert_eq!(list_recycle(&state).unwrap()[0].id, id);
        let injected: i64 = state
            .db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM versions WHERE version_no = 'injected-owner'",
                    [],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(injected, 0, "the conflicting transaction must roll back");
    }
}
