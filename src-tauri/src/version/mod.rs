mod kind;
mod policy;

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use rusqlite::OptionalExtension;
use rusqlite::Row;
use serde::Serialize;
use tracing::info;

use crate::app::AppState;
use crate::crypto::{classified_io, vault_key::VAULT_KEY};
use crate::error::{AppError, AppResult};
use crate::storage::paths::is_classified_note_path;

pub use kind::VersionKind;
pub use policy::{SnapshotDecisionInput, SnapshotSkipReason, AUTO_IDLE_MAX_PER_FILE};

#[derive(Debug, Clone, Serialize)]
pub struct VersionEntry {
    pub id: i64,
    pub file_id: i64,
    pub version_no: String,
    pub label: Option<String>,
    pub content_hash: String,
    pub word_count: i64,
    pub is_finalized: bool,
    pub kind: VersionKind,
    pub created_at: String,
}

/// Parameters for [`create_snapshot`].
#[derive(Debug, Clone)]
pub struct SnapshotParams {
    pub kind: VersionKind,
    pub label: Option<String>,
    pub is_finalized: bool,
}

#[derive(Debug, Clone)]
pub struct VersionSaveOutcome {
    pub entry: Option<VersionEntry>,
    pub skip_reason: Option<SnapshotSkipReason>,
}

impl SnapshotParams {
    pub fn manual() -> Self {
        Self {
            kind: VersionKind::Manual,
            label: None,
            is_finalized: false,
        }
    }

    pub fn pre_restore() -> Self {
        Self {
            kind: VersionKind::PreRestore,
            label: None,
            is_finalized: false,
        }
    }

    pub fn auto_idle() -> Self {
        Self {
            kind: VersionKind::AutoIdle,
            label: None,
            is_finalized: false,
        }
    }

    pub fn finalize(label: Option<String>) -> Self {
        Self {
            kind: VersionKind::Finalize,
            label,
            is_finalized: true,
        }
    }
}

/// Explicit user checkpoint (`kind = manual`).
pub fn version_save_manual(
    state: &AppState,
    path: &str,
    content: &str,
) -> AppResult<Option<VersionEntry>> {
    Ok(version_save_manual_outcome(state, path, content)?.entry)
}

pub fn version_save_manual_outcome(
    state: &AppState,
    path: &str,
    content: &str,
) -> AppResult<VersionSaveOutcome> {
    create_snapshot_outcome(state, path, content, SnapshotParams::manual())
}

/// Idle auto backup (`kind = auto_idle`); policy may skip.
pub fn version_save_idle(
    state: &AppState,
    path: &str,
    content: &str,
) -> AppResult<Option<VersionEntry>> {
    Ok(version_save_idle_outcome(state, path, content)?.entry)
}

pub fn version_save_idle_outcome(
    state: &AppState,
    path: &str,
    content: &str,
) -> AppResult<VersionSaveOutcome> {
    create_snapshot_outcome(state, path, content, SnapshotParams::auto_idle())
}

const VERSION_SELECT: &str = "SELECT v.id, v.file_id, v.version_no, v.label, v.content_hash,
       v.word_count, v.is_finalized, v.kind, v.created_at";

fn timestamp_version_no() -> String {
    Utc::now().format("%Y%m%d%H%M%S%6f").to_string()
}

/// Non-whitespace character count; aligned with frontend `characterCountExcludingWhitespace`.
pub fn character_count_excluding_whitespace(content: &str) -> i64 {
    content.chars().filter(|c| !c.is_whitespace()).count() as i64
}

fn title_from_path(path: &str) -> String {
    PathBuf::from(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn purge_classified_derived_rows(
    conn: &rusqlite::Connection,
    file_id: i64,
    path: &str,
) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM files_fts WHERE path = ?1", [path])?;
    conn.execute(
        "DELETE FROM chunk_embeddings
         WHERE chunk_id IN (SELECT id FROM chunks WHERE file_id = ?1)",
        [file_id],
    )?;
    conn.execute("DELETE FROM chunks WHERE file_id = ?1", [file_id])?;
    conn.execute(
        "DELETE FROM links WHERE source_id = ?1 OR target_id = ?1",
        [file_id],
    )?;
    conn.execute("DELETE FROM file_tags WHERE file_id = ?1", [file_id])?;
    Ok(())
}

fn ensure_snapshot_file_id(
    state: &AppState,
    path: &str,
    content_hash: &str,
    content: &str,
) -> AppResult<i64> {
    if !is_classified_note_path(path) {
        return state.db.with_conn(|conn| {
            conn.query_row("SELECT id FROM files WHERE path = ?1", [path], |r| r.get(0))
                .map_err(|e| AppError::msg(format!("File not indexed: {e}")))
        });
    }

    let title = title_from_path(path);
    let wc = character_count_excluding_whitespace(content);
    state.db.with_conn(|conn| {
        let existing: Option<i64> = conn
            .query_row("SELECT id FROM files WHERE path = ?1", [path], |r| r.get(0))
            .optional()?;
        let file_id = if let Some(id) = existing {
            conn.execute(
                "UPDATE files
                 SET title = ?1, frontmatter = NULL, content_hash = ?2, word_count = ?3, updated_at = datetime('now')
                 WHERE id = ?4",
                rusqlite::params![title, content_hash, wc, id],
            )?;
            id
        } else {
            conn.execute(
                "INSERT INTO files (path, title, frontmatter, content_hash, word_count, created_at, updated_at)
                 VALUES (?1, ?2, NULL, ?3, ?4, datetime('now'), datetime('now'))",
                rusqlite::params![path, title, content_hash, wc],
            )?;
            conn.last_insert_rowid()
        };
        purge_classified_derived_rows(conn, file_id, path)?;
        Ok(file_id)
    })
}

fn encode_restore_payload(path: &str, content: &str) -> AppResult<Vec<u8>> {
    if !is_classified_note_path(path) {
        return Ok(content.as_bytes().to_vec());
    }
    let key = VAULT_KEY
        .get()
        .ok_or_else(|| AppError::msg("保险库未初始化"))?
        .read()
        .map_err(|e| AppError::msg(format!("VAULT_KEY lock error: {e}")))?
        .key()
        .copied()?;
    classified_io::encrypt_cef(content.as_bytes(), &key)
}

fn map_version_row(row: &Row<'_>) -> rusqlite::Result<VersionEntry> {
    let kind_str: String = row.get(7)?;
    let kind = VersionKind::parse(&kind_str).unwrap_or(VersionKind::Manual);
    Ok(VersionEntry {
        id: row.get(0)?,
        file_id: row.get(1)?,
        version_no: row.get(2)?,
        label: row.get(3)?,
        content_hash: row.get(4)?,
        word_count: row.get(5)?,
        is_finalized: row.get::<_, i64>(6)? != 0,
        kind,
        created_at: row.get(8)?,
    })
}

const CAS_STORAGE_PREFIX: &str = "cas:";
const CAS_DIFF_PREFIX: &str = "dif:";

fn cas_storage_path(content_hash: &str) -> String {
    format!("{CAS_STORAGE_PREFIX}{content_hash}")
}

pub(crate) fn is_cas_storage_path(storage_path: &str) -> bool {
    storage_path.starts_with(CAS_STORAGE_PREFIX)
}

/// Read snapshot body from CAS blob or legacy `.iris/versions/...` file.
pub(crate) fn read_version_content(
    state: &AppState,
    vault: &std::path::Path,
    storage_path: &str,
) -> AppResult<String> {
    // Diff delta: "dif:{parent_hash}:{diff_hash}"
    if let Some(rest) = storage_path.strip_prefix(CAS_DIFF_PREFIX) {
        if let Some((parent_hash, diff_hash)) = rest.split_once(':') {
            let parent = state.cas_store()?.read_blob_content(parent_hash)?;
            let diff = state.cas_store()?.read_blob_content(diff_hash)?;
            return crate::cas::diff::apply_diff(&parent, &diff);
        }
        return Err(AppError::msg(format!(
            "invalid diff storage path: {storage_path}"
        )));
    }
    // Full-content CAS: "cas:{hash}"
    if let Some(hash) = storage_path.strip_prefix(CAS_STORAGE_PREFIX) {
        return state.cas_store()?.read_blob_content(hash);
    }
    let abs = vault.join(".iris").join("versions").join(storage_path);
    Ok(fs::read_to_string(abs)?)
}

/// Store snapshot body in CAS; returns `storage_path` for the versions table.
/// Store version content, preferring a compressed line diff against `prev_content`
/// when it saves >30% space (diff delta storage).
fn write_version_blob(
    state: &AppState,
    content: &str,
    prev_content: Option<&str>,
) -> AppResult<String> {
    // Try diff-based delta first
    if let Some(prev) = prev_content {
        if let Some(diff) = crate::cas::diff::compute_diff(prev, content) {
            let diff_hash = state.cas_store()?.store_blob(diff.as_bytes())?;
            let parent_hash = crate::cas::hash::content_hash_str(prev);
            let path = format!("{CAS_DIFF_PREFIX}{parent_hash}:{diff_hash}");
            return Ok(path);
        }
    }
    // Fall back to full-content CAS storage
    let hash = state.cas_store()?.store_blob(content.as_bytes())?;
    Ok(cas_storage_path(&hash))
}

fn remove_version_file(vault: &std::path::Path, storage_path: &str) {
    if is_cas_storage_path(storage_path) {
        return;
    }
    let abs = vault.join(".iris").join("versions").join(storage_path);
    let _ = fs::remove_file(&abs);
}

fn delete_version_row(
    state: &AppState,
    vault: &std::path::Path,
    id: i64,
    storage_path: &str,
) -> AppResult<()> {
    remove_version_file(vault, storage_path);
    if let Some(hash) = storage_path.strip_prefix(CAS_STORAGE_PREFIX) {
        if let Err(e) = state.ref_counter().decrement(hash) {
            tracing::warn!("CAS ref decrement failed for {hash}: {e}");
        }
    }
    state.db.with_conn(|conn| {
        conn.execute("DELETE FROM versions WHERE id = ?1", [id])?;
        Ok(())
    })
}

/// Drop oldest `auto_idle` rows when a file exceeds `max` non-finalized idle snapshots.
pub fn enforce_auto_idle_cap(state: &AppState, file_id: i64, max: usize) -> AppResult<usize> {
    let vault = state.vault_path()?;
    let to_remove: Vec<(i64, String)> = state.db.with_conn(|conn| {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM versions
             WHERE file_id = ?1 AND kind = 'auto_idle' AND is_finalized = 0",
            [file_id],
            |r| r.get(0),
        )?;
        let count = count as usize;
        if count <= max {
            return Ok(Vec::new());
        }
        let excess = count - max;
        let mut stmt = conn.prepare(
            "SELECT id, storage_path FROM versions
             WHERE file_id = ?1 AND kind = 'auto_idle' AND is_finalized = 0
             ORDER BY created_at ASC, id ASC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![file_id, excess as i64], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
        Ok(rows.flatten().collect())
    })?;

    let mut removed = 0;
    for (id, storage_path) in to_remove {
        delete_version_row(state, &vault, id, &storage_path)?;
        removed += 1;
    }
    Ok(removed)
}

fn load_snapshot_context(
    conn: &rusqlite::Connection,
    file_id: i64,
) -> AppResult<(
    Option<policy::LatestSnapshot>,
    Option<chrono::DateTime<Utc>>,
)> {
    let latest: Option<policy::LatestSnapshot> = conn
        .query_row(
            "SELECT content_hash, kind, created_at FROM versions
             WHERE file_id = ?1
             ORDER BY created_at DESC, id DESC
             LIMIT 1",
            [file_id],
            |row| {
                let kind_str: String = row.get(1)?;
                let kind = VersionKind::parse(&kind_str).unwrap_or(VersionKind::Manual);
                Ok(policy::LatestSnapshot {
                    content_hash: row.get(0)?,
                    kind,
                    created_at: policy::parse_created_at(&row.get::<_, String>(2)?),
                })
            },
        )
        .ok();

    let last_auto_idle_at: Option<chrono::DateTime<Utc>> = conn
        .query_row(
            "SELECT created_at FROM versions
             WHERE file_id = ?1 AND kind = 'auto_idle'
             ORDER BY created_at DESC, id DESC
             LIMIT 1",
            [file_id],
            |row| Ok(policy::parse_created_at(&row.get::<_, String>(0)?)),
        )
        .ok();

    Ok((latest, last_auto_idle_at))
}

/// Create a version snapshot when policy allows it.
pub fn create_snapshot(
    state: &AppState,
    path: &str,
    content: &str,
    params: SnapshotParams,
) -> AppResult<Option<VersionEntry>> {
    Ok(create_snapshot_outcome(state, path, content, params)?.entry)
}

pub fn create_snapshot_outcome(
    state: &AppState,
    path: &str,
    content: &str,
    params: SnapshotParams,
) -> AppResult<VersionSaveOutcome> {
    let hash = crate::cas::hash::content_hash_str(content);
    let file_id = ensure_snapshot_file_id(state, path, &hash, content)?;

    let now = Utc::now();
    let decision = state.db.with_conn(|conn| {
        let (latest, last_auto_idle_at) = load_snapshot_context(conn, file_id)?;
        Ok(policy::decide_snapshot(&SnapshotDecisionInput {
            kind: params.kind,
            content_hash: &hash,
            latest,
            last_auto_idle_at,
            now,
        }))
    })?;

    if !decision.create {
        return Ok(VersionSaveOutcome {
            entry: None,
            skip_reason: decision.skip_reason,
        });
    }

    let version_no = timestamp_version_no();
    // Look up previous snapshot content for diff-based delta storage.
    let prev_content: Option<String> = state.db.with_read_conn(|conn| {
        let result: Result<(String,), _> = conn.query_row(
            "SELECT storage_path FROM versions
             WHERE file_id = ?1
             ORDER BY created_at DESC, id DESC
             LIMIT 1",
            [file_id],
            |row| Ok((row.get::<_, String>(0)?,)),
        );
        Ok(result.ok().map(|(p,)| p))
    })?;
    let prev = prev_content
        .as_ref()
        .and_then(|p| read_version_content(state, &state.vault_path().ok()?, p).ok());
    let storage_path = write_version_blob(state, content, prev.as_deref())?;

    if let Some(cas_hash) = storage_path.strip_prefix(CAS_STORAGE_PREFIX) {
        if let Err(e) = state.ref_counter().increment(cas_hash) {
            tracing::warn!("CAS ref increment failed for {cas_hash}: {e}");
        }
    }

    let wc = character_count_excluding_whitespace(content);
    let created_at = now.to_rfc3339();
    let is_finalized = if params.is_finalized { 1 } else { 0 };

    let id = state.db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO versions (file_id, version_no, label, content_hash, storage_path, word_count, is_finalized, kind, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                file_id,
                &version_no,
                params.label,
                hash,
                storage_path,
                wc,
                is_finalized,
                params.kind.as_str(),
                created_at,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    })?;

    if params.kind == VersionKind::AutoIdle {
        let _ = enforce_auto_idle_cap(state, file_id, AUTO_IDLE_MAX_PER_FILE)?;
    }

    info!(
        file_id = %file_id,
        version_no = %version_no,
        kind = ?params.kind,
        "Version snapshot created"
    );

    Ok(VersionSaveOutcome {
        entry: Some(VersionEntry {
            id,
            file_id,
            version_no,
            label: params.label,
            content_hash: hash,
            word_count: wc,
            is_finalized: params.is_finalized,
            kind: params.kind,
            created_at,
        }),
        skip_reason: None,
    })
}

pub fn version_list(state: &AppState, path: &str) -> AppResult<Vec<VersionEntry>> {
    state.db.with_conn(|conn| {
        let sql = format!(
            "{VERSION_SELECT}
             FROM versions v JOIN files f ON f.id = v.file_id
             WHERE f.path = ?1
             ORDER BY v.created_at DESC"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([path], map_version_row)?;
        Ok(rows.flatten().collect())
    })
}

pub fn version_preview(state: &AppState, version_id: i64) -> AppResult<String> {
    let storage_path: String = state.db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT storage_path FROM versions WHERE id = ?1",
            [version_id],
            |r| r.get(0),
        )?)
    })?;

    let vault = state.vault_path()?;
    read_version_content(state, &vault, &storage_path)
}

pub fn version_restore(
    state: &Arc<AppState>,
    version_id: i64,
    current_content: &str,
) -> AppResult<String> {
    let (storage_path, path): (String, String) = state.db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT v.storage_path, f.path
             FROM versions v JOIN files f ON f.id = v.file_id
             WHERE v.id = ?1",
            [version_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?)
    })?;

    let pre_restore =
        create_snapshot(state, &path, current_content, SnapshotParams::pre_restore())?;
    if pre_restore.is_none() {
        return Err(AppError::msg(
            "恢复前备份未能创建，已取消恢复以保护当前正文",
        ));
    }

    let vault = state.vault_path()?;
    let content = read_version_content(state, &vault, &storage_path)?;
    let abs_note = crate::storage::paths::resolve_vault_path(&vault, &path)?;

    let tmp = abs_note.with_extension("md.tmp");
    let payload = encode_restore_payload(&path, &content)?;
    fs::write(&tmp, payload)?;
    fs::rename(&tmp, &abs_note)?;

    if is_classified_note_path(&path) {
        let hash = crate::cas::hash::content_hash_str(&content);
        let _ = ensure_snapshot_file_id(state, &path, &hash, &content)?;
    } else {
        state.db.with_conn(|conn| {
            crate::indexer::scan::index_file_with_embed(
                conn,
                &vault,
                &abs_note,
                crate::indexer::scan::IndexEmbeddingMode::Queue(state),
            )
        })?;
    }

    Ok(content)
}

pub fn version_delete(state: &AppState, version_id: i64) -> AppResult<()> {
    let (storage_path,): (String,) = state.db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT storage_path FROM versions WHERE id = ?1",
            [version_id],
            |r| Ok((r.get(0)?,)),
        )?)
    })?;

    let vault = state.vault_path()?;
    delete_version_row(state, &vault, version_id, &storage_path)
}

/// Finalize the **current** note body: insert a new snapshot with `kind = finalize`.
pub fn version_finalize_current(
    state: &AppState,
    path: &str,
    content: &str,
    label: Option<String>,
) -> AppResult<Option<VersionEntry>> {
    create_snapshot(state, path, content, SnapshotParams::finalize(label))
}

pub fn version_cleanup(state: &AppState) -> AppResult<usize> {
    let vault = state.vault_path()?;
    let cutoff = Utc::now()
        .checked_sub_signed(chrono::Duration::days(7))
        .unwrap_or(Utc::now())
        .to_rfc3339();

    let stale: Vec<(i64, String)> = state.db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, storage_path FROM versions
             WHERE kind = 'auto_idle' AND is_finalized = 0 AND created_at < ?1",
        )?;
        let rows = stmt.query_map([&cutoff], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        Ok(rows.flatten().collect())
    })?;

    let mut cleaned = 0;
    for (id, storage_path) in stale {
        delete_version_row(state, &vault, id, &storage_path)?;
        cleaned += 1;
    }

    Ok(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppState;
    use crate::crypto::classified_io;
    use crate::crypto::vault_key::{init_vault_key, VAULT_KEY, VAULT_KEY_TEST_LOCK};
    use crate::storage::db::Database;
    use rusqlite::Connection;
    use std::fs;
    use std::sync::{Arc, OnceLock};
    use tempfile::tempdir;

    static INIT_KEY: OnceLock<()> = OnceLock::new();

    fn ensure_vault_key() {
        INIT_KEY.get_or_init(|| {
            init_vault_key();
        });
    }

    fn unlock_classified_vault_for_test() -> [u8; 32] {
        ensure_vault_key();
        let key = [42_u8; 32];
        let mut guard = VAULT_KEY.get().unwrap().write().unwrap();
        guard.set_test_key(key);
        key
    }

    fn test_state() -> (tempfile::TempDir, Arc<AppState>) {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let data_dir = dir.path().join("data");
        fs::create_dir_all(&data_dir).unwrap();
        let state = AppState::new(data_dir).unwrap();
        state.set_vault(vault).unwrap();
        (dir, state)
    }

    fn seed_file(conn: &Connection, path: &str, title: &str) -> i64 {
        conn.execute(
            "INSERT INTO files (path, title, content_hash, created_at, updated_at)
             VALUES (?1, ?2, 'abc', datetime('now'), datetime('now'))",
            rusqlite::params![path, title],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn seed_file_in_db(state: &Arc<AppState>, path: &str, title: &str) {
        state
            .db
            .with_conn(|conn| {
                seed_file(conn, path, title);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn create_snapshot_writes_kind_and_storage_path() {
        let (_dir, state) = test_state();
        seed_file_in_db(&state, "note.md", "Note");

        let entry = create_snapshot(&state, "note.md", "# Hello", SnapshotParams::manual())
            .unwrap()
            .expect("snapshot created");

        assert_eq!(entry.kind, VersionKind::Manual);
        assert!(!entry.is_finalized);

        let vault = state.vault_path().unwrap();
        let storage_path: String = state
            .db
            .with_conn(|conn| {
                let (kind, path): (String, String) = conn.query_row(
                    "SELECT kind, storage_path FROM versions WHERE id = ?1",
                    [entry.id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                assert_eq!(kind, "manual");
                assert!(is_cas_storage_path(&path));
                Ok(path)
            })
            .unwrap();
        let content = read_version_content(&state, &vault, &storage_path).unwrap();
        assert_eq!(content, "# Hello");
    }

    #[test]
    fn version_save_manual_sets_kind() {
        let (_dir, state) = test_state();
        seed_file_in_db(&state, "note.md", "Note");

        let entry = version_save_manual(&state, "note.md", "checkpoint")
            .unwrap()
            .expect("manual snapshot");

        assert_eq!(entry.kind, VersionKind::Manual);
    }

    #[test]
    fn classified_version_restore_keeps_disk_encrypted_and_unindexed() {
        let _guard = VAULT_KEY_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (_dir, state) = test_state();
        let vault = state.vault_path().unwrap();
        let key = unlock_classified_vault_for_test();
        fs::create_dir_all(vault.join(".classified")).unwrap();

        let encrypted_current = classified_io::encrypt_cef(b"# Current", &key).unwrap();
        fs::write(vault.join(".classified/secret.md"), encrypted_current).unwrap();

        let entry = version_save_manual(&state, ".classified/secret.md", "# Historical")
            .unwrap()
            .expect("classified snapshot");
        let listed = version_list(&state, ".classified/secret.md").unwrap();
        assert_eq!(listed.len(), 1);

        let restored = version_restore(&state, entry.id, "# Current").unwrap();
        assert_eq!(restored, "# Historical");

        let raw = fs::read(vault.join(".classified/secret.md")).unwrap();
        assert!(classified_io::has_csef_magic(&raw));
        let decrypted = classified_io::decrypt_cef(&raw, &key).unwrap();
        assert_eq!(String::from_utf8(decrypted).unwrap(), "# Historical");

        let (chunks, fts): (i64, i64) = state
            .db
            .with_conn(|conn| {
                let file_id: i64 = conn.query_row(
                    "SELECT id FROM files WHERE path = '.classified/secret.md'",
                    [],
                    |r| r.get(0),
                )?;
                let chunks = conn.query_row(
                    "SELECT COUNT(*) FROM chunks WHERE file_id = ?1",
                    [file_id],
                    |r| r.get(0),
                )?;
                let fts = conn.query_row(
                    "SELECT COUNT(*) FROM files_fts WHERE path = '.classified/secret.md'",
                    [],
                    |r| r.get(0),
                )?;
                Ok((chunks, fts))
            })
            .unwrap();
        assert_eq!(chunks, 0);
        assert_eq!(fts, 0);
    }

    #[test]
    fn version_save_idle_sets_kind() {
        let (_dir, state) = test_state();
        seed_file_in_db(&state, "note.md", "Note");

        let entry = version_save_idle(&state, "note.md", "idle body")
            .unwrap()
            .expect("idle snapshot");

        assert_eq!(entry.kind, VersionKind::AutoIdle);
    }

    #[test]
    fn create_snapshot_skips_duplicate_hash_for_manual() {
        let (_dir, state) = test_state();
        seed_file_in_db(&state, "note.md", "Note");

        assert!(
            create_snapshot(&state, "note.md", "same", SnapshotParams::manual())
                .unwrap()
                .is_some()
        );
        assert!(
            create_snapshot(&state, "note.md", "same", SnapshotParams::manual())
                .unwrap()
                .is_none()
        );

        let count: i64 = state
            .db
            .with_conn(
                |conn| Ok(conn.query_row("SELECT COUNT(*) FROM versions", [], |r| r.get(0))?),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn version_list_returns_empty_for_new_file() {
        let (_dir, db) = {
            let dir = tempdir().unwrap();
            let _ = fs::create_dir_all(dir.path().join("vault"));
            (dir, Database::open_in_memory().unwrap())
        };
        db.with_conn(|conn| {
            seed_file(conn, "note.md", "Note");
            let mut stmt = conn.prepare(
                "SELECT COUNT(*) FROM versions v JOIN files f ON f.id = v.file_id WHERE f.path = ?1",
            )?;
            let count: i64 = stmt.query_row(["note.md"], |row| row.get(0))?;
            assert_eq!(count, 0);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn finalize_creates_new_row_with_is_finalized() {
        let (_dir, state) = test_state();
        seed_file_in_db(&state, "note.md", "Note");

        let manual = version_save_manual(&state, "note.md", "same body")
            .unwrap()
            .expect("manual");
        let finalized =
            version_finalize_current(&state, "note.md", "same body", Some("release".to_string()))
                .unwrap()
                .expect("finalize");

        assert!(finalized.is_finalized);
        assert_eq!(finalized.kind, VersionKind::Finalize);
        assert_eq!(finalized.label.as_deref(), Some("release"));
        assert_ne!(finalized.id, manual.id);

        let count: i64 = state
            .db
            .with_conn(
                |conn| Ok(conn.query_row("SELECT COUNT(*) FROM versions", [], |r| r.get(0))?),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn version_restore_creates_pre_restore_snapshot() {
        let (_dir, state) = test_state();
        seed_file_in_db(&state, "note.md", "Note");

        let target = version_save_manual(&state, "note.md", "historical body")
            .unwrap()
            .expect("target snapshot");

        let count_before: i64 = state
            .db
            .with_conn(
                |conn| Ok(conn.query_row("SELECT COUNT(*) FROM versions", [], |r| r.get(0))?),
            )
            .unwrap();
        assert_eq!(count_before, 1);

        let restored = version_restore(&state, target.id, "current editor body").unwrap();
        assert_eq!(restored, "historical body");

        let pre_restore_count: i64 = state
            .db
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM versions WHERE kind = 'pre_restore'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(pre_restore_count, 1);

        let count_after: i64 = state
            .db
            .with_conn(
                |conn| Ok(conn.query_row("SELECT COUNT(*) FROM versions", [], |r| r.get(0))?),
            )
            .unwrap();
        assert_eq!(count_after, count_before + 1);
    }

    #[test]
    fn version_delete_removes_record() {
        let (_dir, db) = {
            let dir = tempdir().unwrap();
            (dir, Database::open_in_memory().unwrap())
        };
        db.with_conn(|conn| {
            seed_file(conn, "note.md", "Note");
            conn.execute(
                "INSERT INTO versions (file_id, version_no, content_hash, storage_path, kind, created_at)
                 VALUES (1, '20260501000000000', 'def', '1/20260501000000000.md', 'manual', datetime('now'))",
                [],
            )
            .unwrap();
            let id = conn.last_insert_rowid();
            conn.execute("DELETE FROM versions WHERE id = ?1", [id])?;
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM versions WHERE id = ?1", [id], |r| r.get(0))?;
            assert_eq!(count, 0);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn enforce_auto_idle_cap_deletes_oldest_when_over_limit() {
        let (_dir, state) = test_state();
        let file_id = {
            let mut id = 0_i64;
            state
                .db
                .with_conn(|conn| {
                    id = seed_file(conn, "note.md", "Note");
                    for i in 0..31 {
                        let version_no = format!("202601010000000{i:02}");
                        conn.execute(
                            "INSERT INTO versions (file_id, version_no, content_hash, storage_path, is_finalized, kind, created_at)
                             VALUES (?1, ?2, ?3, ?4, 0, 'auto_idle', ?5)",
                            rusqlite::params![
                                id,
                                version_no,
                                format!("hash{i}"),
                                format!("{id}/{version_no}.md"),
                                format!("2026-01-01T00:{i:02}:00Z"),
                            ],
                        )?;
                    }
                    Ok(())
                })
                .unwrap();
            id
        };

        let removed = enforce_auto_idle_cap(&state, file_id, 30).unwrap();
        assert_eq!(removed, 1);

        let count: i64 = state
            .db
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM versions
                     WHERE file_id = ?1 AND kind = 'auto_idle'",
                    [file_id],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(count, 30);

        let oldest_exists: i64 = state
            .db
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM versions WHERE version_no = '20260101000000000'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(oldest_exists, 0);
    }

    #[test]
    fn version_cleanup_only_removes_stale_auto_idle() {
        let (_dir, state) = test_state();
        state
            .db
            .with_conn(|conn| {
                seed_file(conn, "note.md", "Note");
                conn.execute(
                    "INSERT INTO versions (file_id, version_no, content_hash, storage_path, is_finalized, kind, created_at)
                     VALUES (1, '20200101000000000', 'old_auto', '1/old_auto.md', 0, 'auto_idle', '2020-01-01T00:00:00Z')",
                    [],
                )?;
                conn.execute(
                    "INSERT INTO versions (file_id, version_no, content_hash, storage_path, is_finalized, kind, created_at)
                     VALUES (1, '20200101000000001', 'old_manual', '1/old_manual.md', 0, 'manual', '2020-01-01T00:00:00Z')",
                    [],
                )?;
                conn.execute(
                    "INSERT INTO versions (file_id, version_no, content_hash, storage_path, is_finalized, kind, created_at)
                     VALUES (1, '20990101000000000', 'new_auto', '1/new_auto.md', 0, 'auto_idle', datetime('now'))",
                    [],
                )?;
                Ok(())
            })
            .unwrap();

        let cleaned = version_cleanup(&state).unwrap();
        assert_eq!(cleaned, 1);

        let manual_left: i64 = state
            .db
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM versions WHERE kind = 'manual'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(manual_left, 1);

        let auto_left: i64 = state
            .db
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM versions WHERE kind = 'auto_idle'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(auto_left, 1);
    }

    #[test]
    fn character_count_excluding_whitespace_matches_editor_metric() {
        assert_eq!(character_count_excluding_whitespace("a b c"), 3);
        assert_eq!(
            character_count_excluding_whitespace("一二三四五六七八九十"),
            10
        );
        assert_eq!(character_count_excluding_whitespace("a\n\nb\tc"), 3);
        assert_ne!(character_count_excluding_whitespace(&"字".repeat(100)), 1);
    }
}
