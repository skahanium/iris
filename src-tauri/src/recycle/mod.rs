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
pub fn discard_document(state: &Arc<AppState>, path: &str) -> AppResult<()> {
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, path)?;

    state.db.with_conn(|conn| {
        if let Some(file_id) = lookup_file_id(conn, path)? {
            for v in load_versions_for_file(conn, file_id)? {
                let src = versions_root(&vault).join(&v.storage_path);
                if src.is_file() {
                    let _ = fs::remove_file(src);
                }
            }
        }
        remove_file_index(conn, path)
    })?;

    if abs.is_file() {
        fs::remove_file(abs)?;
    }
    Ok(())
}

/// Move current note + all versions/finalized snapshots into recycle bin.
pub fn trash_document(state: &Arc<AppState>, path: &str) -> AppResult<()> {
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
                let src = versions_root(&vault).join(&v.storage_path);
                if src.is_file() {
                    let dest = versions_dir.join(&trash_file);
                    if let Some(parent) = dest.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::rename(&src, &dest)?;
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

fn purge_bundle(vault: &Path, trash_rel_dir: &str) -> AppResult<()> {
    let dir = vault.join(trash_rel_dir);
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    Ok(())
}

/// Remove recycle entries whose retention period has ended.
pub fn purge_expired(state: &Arc<AppState>) -> AppResult<usize> {
    let vault = state.vault_path()?;
    let now = Utc::now().to_rfc3339();
    let expired: Vec<(String, String)> = state.db.with_conn(|conn| {
        let mut stmt =
            conn.prepare("SELECT id, trash_rel_dir FROM recycle_bin WHERE expires_at <= ?1")?;
        let rows = stmt.query_map([&now], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.flatten().collect())
    })?;

    let mut count = 0usize;
    for (id, trash_rel) in expired {
        purge_bundle(&vault, &trash_rel)?;
        state.db.with_conn(|conn| {
            conn.execute("DELETE FROM recycle_bin WHERE id = ?1", [&id])?;
            Ok(())
        })?;
        count += 1;
    }
    Ok(count)
}

pub fn list_recycle(state: &Arc<AppState>) -> AppResult<Vec<RecycleBinItem>> {
    let vault = state.vault_path()?;
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

/// Restore a trashed note (body + all version snapshots) to its original path.
pub fn restore_document(state: &Arc<AppState>, id: &str) -> AppResult<String> {
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
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    let doc = bundle_dir.join("document.md");
    if doc.is_file() {
        fs::rename(&doc, &dest)?;
    } else {
        fs::write(&dest, "")?;
    }

    let file_entry = state.db.with_conn(|conn| index_file(conn, &vault, &dest))?;
    let file_id = file_entry.id;

    for v in &manifest.versions {
        let src = bundle_dir.join("versions").join(&v.trash_file);
        let new_storage = storage_path_for(file_id, &v.version_no);
        let dest_version = versions_root(&vault).join(&new_storage);
        if let Some(parent) = dest_version.parent() {
            fs::create_dir_all(parent)?;
        }
        if src.is_file() {
            fs::rename(&src, &dest_version)?;
        }
        state.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO versions (file_id, version_no, label, content_hash, storage_path, word_count, is_finalized, kind, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
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

    purge_bundle(&vault, &trash_rel)?;
    state.db.with_conn(|conn| {
        conn.execute("DELETE FROM recycle_bin WHERE id = ?1", [id])?;
        Ok(())
    })?;

    Ok(manifest.original_path)
}

/// Permanently delete a recycle entry before its expiry.
pub fn purge_recycle_item(state: &Arc<AppState>, id: &str) -> AppResult<()> {
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
    use tempfile::tempdir;

    fn setup() -> (tempfile::TempDir, Arc<AppState>) {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let data = dir.path().join("data");
        let state = Arc::new(AppState::new(data).unwrap());
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
        let path = restore_document(&state, &id).unwrap();
        assert_eq!(path, "restore-me.md");
        assert!(note.is_file());
        assert!(list_recycle(&state).unwrap().is_empty());
        let versions = crate::version::version_list(&state, "restore-me.md").unwrap();
        assert!(!versions.is_empty());
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
}
