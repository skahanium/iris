//! Durable ownership operations consumed by note, recycle and recovery workflows.
use std::collections::HashSet;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{map_version_row, VersionEntry, VERSION_SELECT};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VersionOwnership {
    pub(crate) id: i64,
    pub(crate) vault_path: Option<String>,
    pub(crate) note_path: Option<String>,
    pub(crate) recycle_id: Option<String>,
    pub(crate) storage_path: String,
}

#[derive(Debug)]
pub(crate) enum ArchiveRowsError {
    OwnershipMismatch,
    Database(rusqlite::Error),
}

impl From<rusqlite::Error> for ArchiveRowsError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

impl From<ArchiveRowsError> for AppError {
    fn from(error: ArchiveRowsError) -> Self {
        match error {
            ArchiveRowsError::OwnershipMismatch => {
                AppError::msg("recycled_version_ownership_mismatch")
            }
            ArchiveRowsError::Database(error) => error.into(),
        }
    }
}

pub(crate) fn active_rows(
    conn: &Connection,
    vault: &Path,
    path: &str,
) -> AppResult<Vec<(VersionEntry, VersionOwnership)>> {
    let mut statement = conn.prepare(&format!(
        "{VERSION_SELECT}, v.storage_path, v.vault_path, v.note_path, v.recycle_id FROM versions v
         WHERE v.vault_path = ?1 AND v.note_path = ?2 AND v.recycle_id IS NULL
         ORDER BY v.created_at ASC, v.id ASC"
    ))?;
    let rows = statement.query_map(params![vault.to_string_lossy(), path], |row| {
        let entry = map_version_row(row)?;
        Ok((
            entry.clone(),
            VersionOwnership {
                id: entry.id,
                storage_path: row.get(10)?,
                vault_path: row.get(11)?,
                note_path: row.get(12)?,
                recycle_id: row.get(13)?,
            },
        ))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Archive only identities frozen in the durable trash manifest. Reconciliation
/// must never pick up snapshots created after a same-path note was recreated.
pub(crate) fn archive_rows(
    conn: &Connection,
    vault: &Path,
    path: &str,
    bundle: &str,
    ids: &[i64],
) -> Result<(), ArchiveRowsError> {
    for id in ids {
        let updated = conn.execute(
            "UPDATE versions SET recycle_id = ?1 WHERE id = ?2 AND vault_path = ?3
             AND note_path = ?4 AND recycle_id IS NULL",
            params![bundle, id, vault.to_string_lossy(), path],
        )?;
        if updated == 1 {
            continue;
        }
        let existing: Option<(Option<String>, Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT vault_path, note_path, recycle_id FROM versions WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        if !matches!(
            existing,
            Some((Some(ref row_vault), Some(ref row_path), Some(ref owner)))
                if row_vault == vault.to_string_lossy().as_ref()
                    && row_path == path
                    && owner == bundle
        ) {
            return Err(ArchiveRowsError::OwnershipMismatch);
        }
    }
    Ok(())
}

/// Return an exact bundle-owned row to active history. Already restored rows
/// are accepted only at the same durable identity, making resume idempotent.
pub(crate) fn restore_archived_row(
    conn: &Connection,
    vault: &Path,
    path: &str,
    bundle: &str,
    id: i64,
    file_id: i64,
) -> AppResult<()> {
    let ownership: Option<Option<String>> = conn
        .query_row(
            "SELECT recycle_id FROM versions WHERE id = ?1 AND vault_path = ?2 AND note_path = ?3",
            params![id, vault.to_string_lossy(), path],
            |row| row.get(0),
        )
        .optional()?;
    match ownership {
        Some(Some(owner)) if owner == bundle => {
            conn.execute(
                "UPDATE versions SET recycle_id = NULL, file_id = ?1 WHERE id = ?2",
                params![file_id, id],
            )?;
            Ok(())
        }
        Some(None) => Ok(()),
        _ => Err(AppError::msg("recycled_version_ownership_mismatch")),
    }
}

/// Import old bundle metadata without guessing ownership of unknown old rows.
/// The bundle-specific storage path is a durable idempotency discriminator.
pub(crate) fn import_recycled_row(
    conn: &Connection,
    vault: &Path,
    path: &str,
    entry: &VersionEntry,
    storage: &str,
) -> AppResult<()> {
    let existing: Option<(String, String)> = conn
        .query_row(
            "SELECT content_hash, storage_path FROM versions WHERE vault_path = ?1
         AND note_path = ?2 AND version_no = ?3 AND recycle_id IS NULL",
            params![vault.to_string_lossy(), path, entry.version_no],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((hash, previous_storage)) = existing {
        return if hash == entry.content_hash && previous_storage == storage {
            Ok(())
        } else {
            Err(AppError::msg("recycled_version_identity_conflict"))
        };
    }
    conn.execute(
        "INSERT INTO versions (file_id, version_no, label, content_hash, storage_path,
         word_count, is_finalized, kind, created_at, vault_path, note_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            entry.file_id,
            entry.version_no,
            entry.label,
            entry.content_hash,
            storage,
            entry.word_count,
            entry.is_finalized,
            entry.kind.as_str(),
            entry.created_at,
            vault.to_string_lossy(),
            path
        ],
    )?;
    super::increment_cas_refs_for_storage_path(conn, storage)
}

pub(crate) fn ensure_destination_available(
    conn: &Connection,
    vault: &Path,
    path: &str,
) -> AppResult<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM versions WHERE vault_path = ?1 AND note_path = ?2 AND recycle_id IS NULL)",
        params![vault.to_string_lossy(), path], |row| row.get(0),
    )?;
    if exists {
        return Err(AppError::msg("note_destination_history_conflict"));
    }
    Ok(())
}

pub(crate) fn active_paths(conn: &Connection, vault: &Path) -> AppResult<Vec<String>> {
    let mut statement = conn.prepare("SELECT DISTINCT note_path FROM versions WHERE vault_path = ?1 AND recycle_id IS NULL ORDER BY note_path")?;
    let rows = statement.query_map([vault.to_string_lossy()], |row| row.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub(crate) fn owned_hash(
    conn: &Connection,
    vault: &Path,
    path: &str,
    id: i64,
) -> AppResult<String> {
    Ok(conn.query_row(
        "SELECT content_hash FROM versions WHERE id = ?1 AND vault_path = ?2 AND note_path = ?3 AND recycle_id IS NULL",
        params![id, vault.to_string_lossy(), path], |row| row.get(0),
    )?)
}

/// Conservative global reference inventory also protects unknown legacy rows.
pub(crate) fn referenced_object_hashes(conn: &Connection) -> AppResult<HashSet<String>> {
    let mut statement = conn.prepare("SELECT storage_path FROM versions")?;
    let paths = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut hashes = HashSet::new();
    for path in paths {
        let path = path?;
        if let Some(hash) = path.strip_prefix("cas:") {
            hashes.insert(hash.into());
        } else if let Some((parent, diff)) = path
            .strip_prefix("dif:")
            .and_then(|rest| rest.split_once(':'))
        {
            hashes.insert(parent.into());
            hashes.insert(diff.into());
        }
    }
    Ok(hashes)
}

pub(crate) fn archived_rows(
    conn: &Connection,
    vault: &Path,
    bundle: &str,
) -> AppResult<Vec<VersionOwnership>> {
    let mut statement = conn.prepare(
        "SELECT id, vault_path, note_path, recycle_id, storage_path
         FROM versions WHERE vault_path = ?1 AND recycle_id = ?2 ORDER BY id",
    )?;
    let rows = statement
        .query_map(params![vault.to_string_lossy(), bundle], |row| {
            Ok(VersionOwnership {
                id: row.get(0)?,
                vault_path: row.get(1)?,
                note_path: row.get(2)?,
                recycle_id: row.get(3)?,
                storage_path: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub(crate) fn unshared_non_cas_storage_paths(
    conn: &Connection,
    vault: &Path,
    rows: &[VersionOwnership],
) -> AppResult<Vec<String>> {
    let mut unique = HashSet::new();
    let mut removable = Vec::new();
    for row in rows {
        let storage = &row.storage_path;
        if storage.starts_with("cas:") || storage.starts_with("dif:") || !unique.insert(storage) {
            continue;
        }
        let target_count = rows
            .iter()
            .filter(|candidate| candidate.storage_path == *storage)
            .count() as i64;
        let owner_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM versions
             WHERE storage_path = ?1 AND (vault_path = ?2 OR vault_path IS NULL)",
            params![storage, vault.to_string_lossy()],
            |row| row.get(0),
        )?;
        if owner_count == target_count {
            removable.push(storage.clone());
        }
    }
    Ok(removable)
}

fn storage_owners(
    conn: &Connection,
    vault: &Path,
    storage_path: &str,
) -> AppResult<Vec<VersionOwnership>> {
    let mut statement = conn.prepare(
        "SELECT id, vault_path, note_path, recycle_id, storage_path
         FROM versions
         WHERE storage_path = ?1 AND (vault_path = ?2 OR vault_path IS NULL)
         ORDER BY id",
    )?;
    let rows = statement.query_map(params![storage_path, vault.to_string_lossy()], |row| {
        Ok(VersionOwnership {
            id: row.get(0)?,
            vault_path: row.get(1)?,
            note_path: row.get(2)?,
            recycle_id: row.get(3)?,
            storage_path: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn validate_exact_row(conn: &Connection, owner: &VersionOwnership) -> AppResult<()> {
    let matches: bool = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM versions
           WHERE id = ?1 AND vault_path IS ?2 AND note_path IS ?3
             AND recycle_id IS ?4 AND storage_path = ?5
         )",
        params![
            owner.id,
            owner.vault_path.as_deref(),
            owner.note_path.as_deref(),
            owner.recycle_id.as_deref(),
            &owner.storage_path,
        ],
        |row| row.get(0),
    )?;
    if !matches {
        return Err(AppError::msg("version_cleanup_ownership_mismatch"));
    }
    Ok(())
}

fn validate_storage_owners(
    conn: &Connection,
    vault: &Path,
    storage_path: &str,
    expected: &[VersionOwnership],
) -> AppResult<()> {
    if storage_owners(conn, vault, storage_path)? != expected {
        return Err(AppError::msg("version_cleanup_storage_owner_conflict"));
    }
    Ok(())
}

pub(crate) fn delete_cleanup_rows(
    conn: &Connection,
    vault: &Path,
    rows: &[VersionOwnership],
    staged_storage_paths: &[String],
) -> AppResult<()> {
    let mut ids = HashSet::new();
    for row in rows {
        if !ids.insert(row.id) {
            return Err(AppError::msg("version_cleanup_duplicate_identity"));
        }
        validate_exact_row(conn, row)?;
    }
    for storage_path in staged_storage_paths {
        let mut expected = rows
            .iter()
            .filter(|row| row.storage_path == *storage_path)
            .cloned()
            .collect::<Vec<_>>();
        expected.sort_by_key(|row| row.id);
        if expected.is_empty() {
            return Err(AppError::msg("version_cleanup_storage_witness_missing"));
        }
        validate_storage_owners(conn, vault, storage_path, &expected)?;
    }
    for row in rows {
        let deleted = conn.execute(
            "DELETE FROM versions
             WHERE id = ?1 AND vault_path IS ?2 AND note_path IS ?3
               AND recycle_id IS ?4 AND storage_path = ?5",
            params![
                row.id,
                row.vault_path.as_deref(),
                row.note_path.as_deref(),
                row.recycle_id.as_deref(),
                &row.storage_path,
            ],
        )?;
        if deleted != 1 {
            return Err(AppError::msg("version_cleanup_ownership_mismatch"));
        }
        decrement_cas_refs(conn, &row.storage_path)?;
    }
    for storage_path in staged_storage_paths {
        validate_storage_owners(conn, vault, storage_path, &[])?;
    }
    Ok(())
}

pub(crate) fn ensure_cleanup_storage_unowned(
    conn: &Connection,
    vault: &Path,
    storage_paths: &[String],
) -> AppResult<()> {
    for storage_path in storage_paths {
        validate_storage_owners(conn, vault, storage_path, &[])?;
    }
    Ok(())
}

pub(crate) fn delete_row_on_conn(conn: &Connection, id: i64, storage: &str) -> AppResult<()> {
    if conn.execute("DELETE FROM versions WHERE id = ?1", [id])? == 0 {
        return Ok(());
    }
    decrement_cas_refs(conn, storage)
}

fn decrement_cas_refs(conn: &Connection, storage: &str) -> AppResult<()> {
    let hashes: Vec<&str> = if let Some(hash) = storage.strip_prefix("cas:") {
        vec![hash]
    } else if let Some((parent, diff)) = storage
        .strip_prefix("dif:")
        .and_then(|rest| rest.split_once(':'))
    {
        vec![parent, diff]
    } else {
        vec![]
    };
    for hash in hashes {
        conn.execute("UPDATE cas_refs SET ref_count = MAX(0, ref_count - 1), last_accessed_at = datetime('now') WHERE object_hash = ?1", [hash])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;
    use crate::version::VersionEntry;
    use tempfile::tempdir;

    #[test]
    fn archive_rows_only_accepts_exact_update_or_same_bundle_replay() {
        let directory = tempdir().unwrap();
        let vault = directory.path().join("vault");
        let foreign = directory.path().join("foreign");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::create_dir_all(&foreign).unwrap();
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            for (version_no, owner, row_vault, path) in [
                ("same", None, vault.as_path(), "note.md"),
                (
                    "other-owner",
                    Some("other-bundle"),
                    vault.as_path(),
                    "note.md",
                ),
                ("foreign", None, foreign.as_path(), "note.md"),
                ("other-path", None, vault.as_path(), "other.md"),
            ] {
                conn.execute(
                    "INSERT INTO versions
                     (file_id, version_no, content_hash, storage_path, kind, created_at,
                      vault_path, note_path, recycle_id)
                     VALUES (0, ?1, ?1, ?1, 'manual', datetime('now'), ?2, ?3, ?4)",
                    params![version_no, row_vault.to_string_lossy(), path, owner],
                )?;
            }

            let same = conn.query_row(
                "SELECT id FROM versions WHERE version_no = 'same'",
                [],
                |row| row.get::<_, i64>(0),
            )?;
            archive_rows(conn, &vault, "note.md", "bundle", &[same])?;
            archive_rows(conn, &vault, "note.md", "bundle", &[same])?;

            for version_no in ["other-owner", "foreign", "other-path"] {
                let id = conn.query_row(
                    "SELECT id FROM versions WHERE version_no = ?1",
                    [version_no],
                    |row| row.get::<_, i64>(0),
                )?;
                assert!(archive_rows(conn, &vault, "note.md", "bundle", &[id]).is_err());
            }
            assert!(archive_rows(conn, &vault, "note.md", "bundle", &[i64::MAX]).is_err());
            Ok(())
        })
        .unwrap();
    }

    fn sample_entry(version_no: &str, hash: &str) -> VersionEntry {
        VersionEntry {
            id: 0,
            file_id: 0,
            version_no: version_no.into(),
            label: None,
            content_hash: hash.into(),
            word_count: 1,
            is_finalized: false,
            kind: crate::version::VersionKind::Manual,
            created_at: "2026-09-07T00:00:00Z".into(),
            is_legacy_unscoped: false,
        }
    }

    #[test]
    fn import_restore_and_cleanup_surface_ownership_conflicts_directly() {
        let directory = tempdir().unwrap();
        let vault = directory.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            import_recycled_row(
                conn,
                &vault,
                "note.md",
                &sample_entry("v1", "hash-a"),
                "bundle/v1.md",
            )?;
            let conflict = import_recycled_row(
                conn,
                &vault,
                "note.md",
                &sample_entry("v1", "hash-b"),
                "bundle/v1-other.md",
            )
            .unwrap_err();
            assert!(
                conflict
                    .to_string()
                    .contains("recycled_version_identity_conflict"),
                "{conflict}"
            );

            let id: i64 = conn.query_row(
                "SELECT id FROM versions WHERE version_no = 'v1'",
                [],
                |row| row.get(0),
            )?;
            conn.execute(
                "UPDATE versions SET recycle_id = 'bundle' WHERE id = ?1",
                [id],
            )?;
            let restore =
                restore_archived_row(conn, &vault, "note.md", "other-bundle", id, 9).unwrap_err();
            assert!(
                restore
                    .to_string()
                    .contains("recycled_version_ownership_mismatch"),
                "{restore}"
            );

            let foreign = VersionOwnership {
                id,
                vault_path: Some(vault.to_string_lossy().into_owned()),
                note_path: Some("other.md".into()),
                recycle_id: Some("bundle".into()),
                storage_path: "bundle/v1.md".into(),
            };
            let cleanup = delete_cleanup_rows(conn, &vault, &[foreign], &["bundle/v1.md".into()])
                .unwrap_err();
            assert!(
                cleanup
                    .to_string()
                    .contains("version_cleanup_ownership_mismatch"),
                "{cleanup}"
            );

            let still_owned =
                ensure_cleanup_storage_unowned(conn, &vault, &["bundle/v1.md".into()]).unwrap_err();
            assert!(
                still_owned
                    .to_string()
                    .contains("version_cleanup_storage_owner_conflict"),
                "{still_owned}"
            );
            Ok(())
        })
        .unwrap();
    }
}
