use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use rusqlite::Connection;
use serde::Serialize;

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::paths::relative_path;

#[derive(Debug, Clone, Serialize)]
pub struct VersionEntry {
    pub id: i64,
    pub file_id: i64,
    pub version_no: String,
    pub label: Option<String>,
    pub content_hash: String,
    pub word_count: i64,
    pub is_finalized: bool,
    pub created_at: String,
}

fn versions_dir(vault: &std::path::Path, file_id: i64) -> PathBuf {
    vault.join(".iris").join("versions").join(file_id.to_string())
}

fn ensure_versions_dir(vault: &std::path::Path, file_id: i64) -> AppResult<PathBuf> {
    let dir = versions_dir(vault, file_id);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn timestamp_version_no() -> String {
    Utc::now().format("%Y%m%d%H%M%S%3f").to_string()
}

/// Create a version snapshot of the current file content.
pub fn create_snapshot(
    state: &Arc<AppState>,
    path: &str,
    content: &str,
) -> AppResult<VersionEntry> {
    let vault = state.vault_path()?;
    let abs = crate::storage::paths::resolve_vault_path(&vault, path)?;

    let hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        hex::encode(hasher.finalize())
    };

    // Check if we already have a snapshot with this hash (skip duplicates)
    let existing: Option<VersionEntry> = state.db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, file_id, version_no, label, content_hash, word_count, is_finalized, created_at
             FROM versions WHERE content_hash = ?1
             AND file_id = (SELECT id FROM files WHERE path = ?2)
             LIMIT 1",
        )?;
        let rows: Vec<VersionEntry> = stmt
            .query_map(rusqlite::params![hash, path], |row| {
                Ok(VersionEntry {
                    id: row.get(0)?,
                    file_id: row.get(1)?,
                    version_no: row.get(2)?,
                    label: row.get(3)?,
                    content_hash: row.get(4)?,
                    word_count: row.get(5)?,
                    is_finalized: row.get::<_, i64>(6)? != 0,
                    created_at: row.get(7)?,
                })
            })?
            .flatten()
            .collect();
        Ok(rows.into_iter().next())
    })?;

    if let Some(entry) = existing {
        return Ok(entry);
    }

    // Get file_id
    let file_id: i64 = state.db.with_conn(|conn| {
        conn.query_row("SELECT id FROM files WHERE path = ?1", [path], |r| {
            r.get(0)
        })
        .map_err(|e| AppError::msg(format!("File not indexed: {e}")))
    })?;

    let version_no = timestamp_version_no();
    let dir = ensure_versions_dir(&vault, file_id)?;
    let storage_path = format!("{}/{}.md", file_id, version_no);
    let abs_storage = dir.join(format!("{}.md", version_no));

    fs::write(&abs_storage, content)?;

    let rel = relative_path(&vault, &abs)?;
    let wc = content.split_whitespace().count() as i64;
    let now = Utc::now().to_rfc3339();

    let entry = VersionEntry {
        id: 0,
        file_id,
        version_no: version_no.clone(),
        label: None,
        content_hash: hash,
        word_count: wc,
        is_finalized: false,
        created_at: now.clone(),
    };

    state.db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO versions (file_id, version_no, label, content_hash, storage_path, word_count, is_finalized, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)",
            rusqlite::params![
                entry.file_id,
                version_no,
                entry.label,
                entry.content_hash,
                rel,
                entry.word_count,
                now,
            ],
        )?;
        Ok(())
    })?;

    Ok(entry)
}

pub fn version_list(state: &Arc<AppState>, path: &str) -> AppResult<Vec<VersionEntry>> {
    state.db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT v.id, v.file_id, v.version_no, v.label, v.content_hash, v.word_count, v.is_finalized, v.created_at
             FROM versions v JOIN files f ON f.id = v.file_id
             WHERE f.path = ?1
             ORDER BY v.created_at DESC",
        )?;
        let rows = stmt.query_map([path], |row| {
            Ok(VersionEntry {
                id: row.get(0)?,
                file_id: row.get(1)?,
                version_no: row.get(2)?,
                label: row.get(3)?,
                content_hash: row.get(4)?,
                word_count: row.get(5)?,
                is_finalized: row.get::<_, i64>(6)? != 0,
                created_at: row.get(7)?,
            })
        })?;
        Ok(rows.flatten().collect())
    })
}

pub fn version_preview(state: &Arc<AppState>, version_id: i64) -> AppResult<String> {
    let (file_id, storage_path): (i64, String) = state.db.with_conn(|conn| {
        conn.query_row(
            "SELECT file_id, storage_path FROM versions WHERE id = ?1",
            [version_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    })?;

    let vault = state.vault_path()?;
    let abs = vault.join(".iris").join("versions").join(&storage_path);
    Ok(fs::read_to_string(&abs)?)
}

pub fn version_restore(state: &Arc<AppState>, version_id: i64, current_content: &str) -> AppResult<String> {
    let (file_id, storage_path, path): (i64, String, String) = state.db.with_conn(|conn| {
        conn.query_row(
            "SELECT v.file_id, v.storage_path, f.path
             FROM versions v JOIN files f ON f.id = v.file_id
             WHERE v.id = ?1",
            [version_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
    })?;

    // Snapshot current state before restoring (protect pre-restore state)
    let _ = create_snapshot(state, &path, current_content);

    let vault = state.vault_path()?;
    let abs = vault.join(".iris").join("versions").join(&storage_path);
    let content = fs::read_to_string(&abs)?;
    let abs_note = crate::storage::paths::resolve_vault_path(&vault, &path)?;

    // Atomic write
    let tmp = abs_note.with_extension("md.tmp");
    fs::write(&tmp, &content)?;
    fs::rename(&tmp, &abs_note)?;

    // Re-index
    state.db.with_conn(|conn| {
        crate::indexer::scan::index_file(conn, &vault, &abs_note)
    })?;

    Ok(content)
}

pub fn version_delete(state: &Arc<AppState>, version_id: i64) -> AppResult<()> {
    let (file_id, storage_path): (i64, String) = state.db.with_conn(|conn| {
        conn.query_row(
            "SELECT file_id, storage_path FROM versions WHERE id = ?1",
            [version_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    })?;

    let vault = state.vault_path()?;
    let abs = vault.join(".iris").join("versions").join(&storage_path);
    let _ = fs::remove_file(&abs);

    state.db.with_conn(|conn| {
        conn.execute("DELETE FROM versions WHERE id = ?1", [version_id])?;
        Ok(())
    })
}

pub fn version_finalize(state: &Arc<AppState>, version_id: i64, label: Option<String>) -> AppResult<()> {
    state.db.with_conn(|conn| {
        conn.execute(
            "UPDATE versions SET is_finalized = 1, label = ?1 WHERE id = ?2",
            rusqlite::params![label, version_id],
        )?;
        Ok(())
    })
}

pub fn version_cleanup(state: &Arc<AppState>) -> AppResult<usize> {
    let vault = state.vault_path()?;
    let cutoff = Utc::now()
        .checked_sub_signed(chrono::Duration::days(7))
        .unwrap_or(Utc::now())
        .to_rfc3339();

    let stale: Vec<(i64, String)> = state.db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, storage_path FROM versions
             WHERE is_finalized = 0 AND created_at < ?1",
        )?;
        let rows = stmt.query_map([&cutoff], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        Ok(rows.flatten().collect())
    })?;

    let mut cleaned = 0;
    for (id, storage_path) in &stale {
        let abs = vault.join(".iris").join("versions").join(storage_path);
        let _ = fs::remove_file(&abs);
        state.db.with_conn(|conn| {
            conn.execute("DELETE FROM versions WHERE id = ?1", [*id])?;
            Ok(())
        })?;
        cleaned += 1;
    }

    Ok(cleaned)
}
