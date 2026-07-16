//! Soft-delete notes (current `.md` + all version snapshots) into `.iris/trash/`.

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
use crate::storage::note_write::{FileWriteIndexStatus, FileWriteResult, NoteWriteService};
use crate::storage::paths::resolve_vault_path;
use crate::version::VersionEntry;

/// Days before trashed items are permanently removed.
pub const RECYCLE_RETENTION_DAYS: i64 = 15;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashVersionMeta {
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

fn storage_path_for(file_id: i64, version_no: &str) -> String {
    format!("{file_id}/{version_no}.md")
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

struct VersionRow {
    entry: VersionEntry,
    storage_path: String,
}

fn load_versions_for_file(conn: &rusqlite::Connection, file_id: i64) -> AppResult<Vec<VersionRow>> {
    use crate::version::VersionKind;
    use rusqlite::Row;

    let mut stmt = conn.prepare(
        "SELECT id, file_id, version_no, label, content_hash, word_count, is_finalized, kind, created_at, storage_path
         FROM versions WHERE file_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([file_id], |row: &Row<'_>| {
        let kind_str: String = row.get(7)?;
        let kind = VersionKind::parse(&kind_str).unwrap_or(VersionKind::Manual);
        Ok(VersionRow {
            entry: VersionEntry {
                id: row.get(0)?,
                file_id: row.get(1)?,
                version_no: row.get(2)?,
                label: row.get(3)?,
                content_hash: row.get(4)?,
                word_count: row.get(5)?,
                is_finalized: row.get::<_, i64>(6)? != 0,
                kind,
                created_at: row.get(8)?,
            },
            storage_path: row.get(9)?,
        })
    })?;
    Ok(rows.flatten().collect())
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

fn title_from_path(path: &str) -> String {
    path.trim_end_matches(".md")
        .split('/')
        .next_back()
        .unwrap_or(path)
        .to_string()
}

/// Permanently remove a note, all version blobs, and index rows (no recycle).
pub fn discard_document(state: &AppState, path: &str) -> AppResult<()> {
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, path)?;

    let cas_hashes = state.db.with_conn(|conn| {
        let mut cas_hashes = Vec::new();
        if let Some(file_id) = lookup_file_id(conn, path)? {
            for v in load_versions_for_file(conn, file_id)? {
                if let Some(hash) = v.storage_path.strip_prefix("cas:") {
                    cas_hashes.push(hash.to_string());
                    continue;
                }
                let src = versions_root(&vault).join(&v.storage_path);
                if src.is_file() {
                    let _ = fs::remove_file(src);
                }
            }
        }
        remove_file_index(conn, path)?;
        Ok(cas_hashes)
    })?;

    for hash in cas_hashes {
        if let Err(error) = state.ref_counter().decrement(&hash) {
            tracing::warn!("CAS ref decrement failed while permanently discarding {path}: {error}");
        }
    }

    if abs.is_file() {
        fs::remove_file(abs)?;
    }
    Ok(())
}

/// Move current note + all versions/finalized snapshots into recycle bin.
pub fn trash_document(state: &AppState, path: &str) -> AppResult<()> {
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, path)?;
    let trash_id = Uuid::new_v4().to_string();
    let bundle_dir = trash_root(&vault).join(&trash_id);
    let versions_dir = bundle_dir.join("versions");
    fs::create_dir_all(&versions_dir)?;

    let (title, version_metas) = state.db.with_conn(|conn| {
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
        if let Some(fid) = file_id {
            for v in load_versions_for_file(conn, fid)? {
                let trash_file = format!("{}.md", v.entry.version_no);
                let dest = versions_dir.join(&trash_file);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                if crate::version::is_cas_storage_path(&v.storage_path) {
                    let content =
                        crate::version::read_version_content(state, &vault, &v.storage_path)?;
                    fs::write(&dest, content)?;
                } else {
                    let src = versions_root(&vault).join(&v.storage_path);
                    if src.is_file() {
                        fs::rename(&src, &dest)?;
                    }
                }
                metas.push(TrashVersionMeta {
                    version_no: v.entry.version_no.clone(),
                    label: v.entry.label.clone(),
                    content_hash: v.entry.content_hash.clone(),
                    storage_path: v.storage_path.clone(),
                    word_count: v.entry.word_count,
                    is_finalized: v.entry.is_finalized,
                    kind: v.entry.kind.as_str().to_string(),
                    created_at: v.entry.created_at.clone(),
                    trash_file,
                });
            }
        }
        Ok((title, metas))
    })?;

    if abs.is_file() {
        fs::rename(&abs, bundle_dir.join("document.md"))?;
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
    fs::write(
        bundle_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    let trash_rel = format!(".iris/trash/{trash_id}");
    state.db.with_conn(|conn| {
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
    })?;

    Ok(())
}

pub fn purge_bundle(vault: &Path, trash_rel_dir: &str) -> AppResult<u64> {
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
pub fn purge_expired_items(
    db: &crate::storage::db::Database,
    vault: &Path,
) -> AppResult<(usize, u64)> {
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
        freed += purge_bundle(vault, &trash_rel)?;
        db.with_conn(|conn| {
            conn.execute("DELETE FROM recycle_bin WHERE id = ?1", [&id])?;
            Ok(())
        })?;
        count += 1;
    }
    Ok((count, freed))
}

/// Remove recycle entries whose retention period has ended.
pub fn purge_expired(state: &AppState) -> AppResult<usize> {
    let vault = state.vault_path()?;
    let (count, _) = purge_expired_items(&state.db, &vault)?;
    Ok(count)
}

pub fn list_recycle(state: &AppState) -> AppResult<Vec<RecycleBinItem>> {
    let vault = state.vault_path()?;
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
        let src = bundle_dir.join("versions").join(&v.trash_file);
        let new_storage = storage_path_for(file_id, &v.version_no);
        let dest_version = versions_root(vault).join(&new_storage);
        if let Some(parent) = dest_version.parent() {
            fs::create_dir_all(parent)?;
        }
        if src.is_file() {
            fs::rename(&src, &dest_version)?;
        } else if !dest_version.is_file() {
            return Err(AppError::msg("recycled version snapshot is missing"));
        }
        state.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO versions (file_id, version_no, label, content_hash, storage_path, word_count, is_finalized, kind, created_at)
                 SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9
                 WHERE NOT EXISTS (
                    SELECT 1 FROM versions WHERE file_id = ?1 AND version_no = ?2
                 )",
                rusqlite::params![
                    file_id,
                    v.version_no,
                    v.label,
                    v.content_hash,
                    new_storage,
                    v.word_count,
                    if v.is_finalized { 1 } else { 0 },
                    v.kind,
                    v.created_at,
                ],
            )?;
            Ok(())
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
    let receipt = NoteWriteService::adopt(state, &doc, &manifest.original_path)?;

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
    let vault = state.vault_path()?;
    let trash_rel: String = state.db.with_conn(|conn| {
        conn.query_row(
            "SELECT trash_rel_dir FROM recycle_bin WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|_| AppError::msg("回收站中找不到该条目"))
    })?;
    purge_bundle(&vault, &trash_rel)?;
    state.db.with_conn(|conn| {
        conn.execute("DELETE FROM recycle_bin WHERE id = ?1", [id])?;
        Ok(())
    })
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
}
