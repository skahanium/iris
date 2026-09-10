mod kind;
mod policy;
pub(crate) mod repository;

use std::fs;
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use rusqlite::OptionalExtension;
use rusqlite::Row;
use serde::Serialize;
use tracing::info;

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::indexer::scan::{content_hash as index_content_hash, index_file_from_content};
use crate::storage::note_title::title_from_path;
use crate::storage::paths::is_classified_note_path;
use crate::storage::paths::normalized_relative;
use crate::storage::paths::resolve_vault_path;
use crate::storage::paths::vault_relative_from_absolute;

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
    pub is_legacy_unscoped: bool,
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

    pub fn pre_close() -> Self {
        Self {
            kind: VersionKind::PreClose,
            label: None,
            is_finalized: false,
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
       v.word_count, v.is_finalized, v.kind, v.created_at, v.vault_path IS NULL";

fn timestamp_version_no() -> String {
    Utc::now().format("%Y%m%d%H%M%S%6f").to_string()
}

/// Non-whitespace character count; aligned with frontend `characterCountExcludingWhitespace`.
pub fn character_count_excluding_whitespace(content: &str) -> i64 {
    content.chars().filter(|c| !c.is_whitespace()).count() as i64
}

fn purge_classified_derived_rows(
    conn: &rusqlite::Connection,
    file_id: i64,
    path: &str,
) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM files_fts WHERE path = ?1", [path])?;
    conn.execute("DELETE FROM files_metadata_fts WHERE path = ?1", [path])?;
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
    vault: &Path,
    path: &str,
    content_hash: &str,
    content: &str,
) -> AppResult<i64> {
    if !is_classified_note_path(path) {
        let indexed = state.db.with_read_conn(|conn| {
            conn.query_row("SELECT id FROM files WHERE path = ?1", [path], |r| r.get(0))
                .optional()
                .map_err(Into::into)
        })?;
        if let Some(file_id) = indexed {
            return Ok(file_id);
        }

        // Disk-first saves can succeed while derived indexing is temporarily
        // degraded. Versioning has the authoritative Markdown in hand, so
        // repair the missing file row before attaching the snapshot.
        let absolute = resolve_vault_path(vault, path)?;
        let index_hash = index_content_hash(content);
        let entry = state.db.with_conn(|conn| {
            index_file_from_content(conn, vault, &absolute, content, &index_hash)
        })?;
        return Ok(entry.id);
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
        is_legacy_unscoped: row.get(9)?,
    })
}

const CAS_STORAGE_PREFIX: &str = "cas:";
const CAS_DIFF_PREFIX: &str = "dif:";

fn increment_cas_refs_for_storage_path(
    conn: &rusqlite::Connection,
    storage_path: &str,
) -> AppResult<()> {
    use crate::cas::ref_counter::RefCounter;

    if let Some(cas_hash) = storage_path.strip_prefix(CAS_STORAGE_PREFIX) {
        RefCounter::increment_on_conn(conn, cas_hash)?;
    } else if let Some(rest) = storage_path.strip_prefix(CAS_DIFF_PREFIX) {
        if let Some((parent_hash, diff_hash)) = rest.split_once(':') {
            RefCounter::increment_on_conn(conn, parent_hash)?;
            RefCounter::increment_on_conn(conn, diff_hash)?;
        }
    }
    Ok(())
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
            let store = state.storage.cas_store(vault)?;
            let parent = store.read_blob_content(parent_hash)?;
            let diff = store.read_blob_content(diff_hash)?;
            return crate::cas::diff::apply_diff(&parent, &diff);
        }
        return Err(AppError::msg(format!(
            "invalid diff storage path: {storage_path}"
        )));
    }
    // Full-content CAS: "cas:{hash}"
    if let Some(hash) = storage_path.strip_prefix(CAS_STORAGE_PREFIX) {
        return state.storage.cas_store(vault)?.read_blob_content(hash);
    }
    let abs = vault.join(".iris").join("versions").join(storage_path);
    Ok(fs::read_to_string(abs)?)
}

/// Store snapshot body in CAS; returns `storage_path` for the versions table.
/// Store version content, preferring a compressed line diff against `prev_content`
/// when it saves >30% space (diff delta storage).
fn write_version_blob(
    state: &AppState,
    vault: &Path,
    content: &str,
    prev_content: Option<&str>,
) -> AppResult<String> {
    let store = state.storage.cas_store(vault)?;
    // Try diff-based delta first
    if let Some(prev) = prev_content {
        if let Some(diff) = crate::cas::diff::compute_diff(prev, content) {
            // A previous snapshot may itself be stored as a delta. Its content
            // hash alone does not guarantee a full parent blob exists.
            let parent_hash = store.store_blob(prev.as_bytes())?;
            let diff_hash = store.store_blob(diff.as_bytes())?;
            let path = format!("{CAS_DIFF_PREFIX}{parent_hash}:{diff_hash}");
            return Ok(path);
        }
    }
    // Fall back to full-content CAS storage
    let hash = store.store_blob(content.as_bytes())?;
    Ok(cas_storage_path(&hash))
}

fn remove_version_file(vault: &std::path::Path, storage_path: &str) {
    if is_cas_storage_path(storage_path) || storage_path.starts_with(CAS_DIFF_PREFIX) {
        return;
    }
    let abs = vault.join(".iris").join("versions").join(storage_path);
    if let Err(error) = fs::remove_file(&abs) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                result_code = "version_legacy_file_cleanup_failed",
                error = %error,
                "version row was deleted but its legacy file could not be reclaimed"
            );
        }
    }
}

fn delete_version_row(
    state: &AppState,
    vault: &std::path::Path,
    id: i64,
    storage_path: &str,
) -> AppResult<()> {
    state.db.with_conn(|conn| {
        in_immediate_transaction(conn, |conn| {
            repository::delete_row_on_conn(conn, id, storage_path)
        })
    })?;
    remove_version_file(vault, storage_path);
    Ok(())
}

/// Drop oldest `auto_idle` rows when a file exceeds `max` non-finalized idle snapshots.
#[cfg(test)]
fn enforce_auto_idle_cap(state: &AppState, file_id: i64, max: usize) -> AppResult<usize> {
    let vault = state.vault_path()?;
    let path: String = state.db.with_read_conn(|conn| {
        Ok(
            conn.query_row("SELECT path FROM files WHERE id = ?1", [file_id], |r| {
                r.get(0)
            })?,
        )
    })?;
    enforce_auto_idle_cap_scoped(state, &vault, &path, max)
}

fn enforce_auto_idle_cap_scoped(
    state: &AppState,
    vault: &Path,
    path: &str,
    max: usize,
) -> AppResult<usize> {
    let to_remove: Vec<(i64, String)> = state.db.with_conn(|conn| {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM versions
             WHERE vault_path = ?1 AND note_path = ?2 AND recycle_id IS NULL
               AND kind = 'auto_idle' AND is_finalized = 0",
            rusqlite::params![vault.to_string_lossy(), path],
            |r| r.get(0),
        )?;
        let count = count as usize;
        if count <= max {
            return Ok(Vec::new());
        }
        let excess = count - max;
        let mut stmt = conn.prepare(
            "SELECT id, storage_path FROM versions
             WHERE vault_path = ?1 AND note_path = ?2 AND recycle_id IS NULL
               AND kind = 'auto_idle' AND is_finalized = 0
             ORDER BY created_at ASC, id ASC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![vault.to_string_lossy(), path, excess as i64],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(rows.flatten().collect())
    })?;

    let mut removed = 0;
    for (id, storage_path) in to_remove {
        delete_version_row(state, vault, id, &storage_path)?;
        removed += 1;
    }
    Ok(removed)
}

fn load_snapshot_context(
    conn: &rusqlite::Connection,
    vault: &Path,
    path: &str,
) -> AppResult<(
    Option<policy::LatestSnapshot>,
    Option<chrono::DateTime<Utc>>,
)> {
    let latest: Option<policy::LatestSnapshot> = conn
        .query_row(
            "SELECT content_hash, kind, created_at FROM versions
             WHERE vault_path = ?1 AND note_path = ?2 AND recycle_id IS NULL
             ORDER BY created_at DESC, id DESC
             LIMIT 1",
            rusqlite::params![vault.to_string_lossy(), path],
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
             WHERE vault_path = ?1 AND note_path = ?2 AND recycle_id IS NULL AND kind = 'auto_idle'
             ORDER BY created_at DESC, id DESC
             LIMIT 1",
            rusqlite::params![vault.to_string_lossy(), path],
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
    let vault = state.vault_path()?;
    create_snapshot_outcome_in_vault(state, &vault, path, content, params)
}

/// Serialize a queued snapshot against the vault captured by its caller.
pub(crate) fn create_snapshot_outcome_in_vault(
    state: &AppState,
    expected_vault: &Path,
    path: &str,
    content: &str,
    params: SnapshotParams,
) -> AppResult<VersionSaveOutcome> {
    crate::storage::atomic_write::with_vault_move_lock(|| {
        if state.vault_path()? != expected_vault {
            return Err(AppError::msg("version_vault_changed"));
        }
        create_snapshot_under_move_lock(state, path, content, params)
    })
}

/// Caller holds the vault operation lock for the entire note operation.
pub(crate) fn create_snapshot_under_move_lock(
    state: &AppState,
    path: &str,
    content: &str,
    params: SnapshotParams,
) -> AppResult<VersionSaveOutcome> {
    let vault = state.vault_path()?;
    let absolute = resolve_vault_path(&vault, path)?;
    let path = vault_relative_from_absolute(&vault, &absolute)?;
    let hash = crate::cas::hash::content_hash_str(content);
    let file_id = ensure_snapshot_file_id(state, &vault, &path, &hash, content).unwrap_or(0);

    let now = Utc::now();
    let decision = state.db.with_conn(|conn| {
        let (latest, last_auto_idle_at) = load_snapshot_context(conn, &vault, &path)?;
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
             WHERE vault_path = ?1 AND note_path = ?2 AND recycle_id IS NULL
             ORDER BY created_at DESC, id DESC
             LIMIT 1",
            rusqlite::params![vault.to_string_lossy(), path],
            |row| Ok((row.get::<_, String>(0)?,)),
        );
        Ok(result.ok().map(|(p,)| p))
    })?;
    let prev = prev_content
        .as_ref()
        .and_then(|p| read_version_content(state, &vault, p).ok());
    let storage_path = write_version_blob(state, &vault, content, prev.as_deref())?;

    let wc = character_count_excluding_whitespace(content);
    let created_at = now.to_rfc3339();
    let is_finalized = if params.is_finalized { 1 } else { 0 };

    let id = state.db.with_conn(|conn| {
        in_immediate_transaction(conn, |conn| {
            increment_cas_refs_for_storage_path(conn, &storage_path)?;
            conn.execute(
                "INSERT INTO versions (file_id, version_no, label, content_hash, storage_path, word_count, is_finalized, kind, created_at, vault_path, note_path)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
                    vault.to_string_lossy(),
                    path,
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })
    })?;

    if params.kind == VersionKind::AutoIdle {
        let _ = enforce_auto_idle_cap_scoped(state, &vault, &path, AUTO_IDLE_MAX_PER_FILE)?;
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
            is_legacy_unscoped: false,
        }),
        skip_reason: None,
    })
}

pub fn version_list(state: &AppState, path: &str) -> AppResult<Vec<VersionEntry>> {
    version_list_including_unassigned(state, path, false)
}

/// Unknown path history is exposed only when explicitly requested by the UI.
pub(crate) fn version_list_including_unassigned(
    state: &AppState,
    path: &str,
    include_legacy_unassigned: bool,
) -> AppResult<Vec<VersionEntry>> {
    let vault = state.vault_path()?;
    let path = normalized_relative(path);
    state.db.with_conn(|conn| {
        let sql = format!(
            "{VERSION_SELECT}
             FROM versions v
             WHERE ((v.note_path = ?1 AND (v.vault_path = ?2 OR v.vault_path IS NULL))
                    OR (?3 AND v.note_path IS NULL AND v.vault_path IS NULL)) AND v.recycle_id IS NULL
             ORDER BY v.created_at DESC"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params![path, vault.to_string_lossy(), include_legacy_unassigned], map_version_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    })
}

pub fn version_preview(state: &AppState, version_id: i64) -> AppResult<String> {
    let vault = state.vault_path()?;
    let (storage_path, _, hash, _) = load_accessible_version(state, &vault, version_id)?;
    verified_content(state, &vault, &storage_path, &hash).map_err(|error| match error {
        AppError::CasUnreadable(_) => AppError::CasUnreadable(
            "版本快照不可读（可能已损坏或加密密钥丢失），该版本无法预览。".into(),
        ),
        other => other,
    })
}

fn load_accessible_version(
    state: &AppState,
    vault: &Path,
    id: i64,
) -> AppResult<(String, Option<String>, String, bool)> {
    state.db.with_read_conn(|conn| {
        Ok(conn.query_row(
            "SELECT storage_path, note_path, content_hash, vault_path IS NULL FROM versions
             WHERE id = ?1 AND (vault_path = ?2 OR vault_path IS NULL) AND recycle_id IS NULL",
            rusqlite::params![id, vault.to_string_lossy()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?)
    })
}

fn verified_content(
    state: &AppState,
    vault: &Path,
    storage_path: &str,
    hash: &str,
) -> AppResult<String> {
    let content = read_version_content(state, vault, storage_path)?;
    if crate::cas::hash::content_hash_str(&content) != hash {
        return Err(AppError::msg("version_content_hash_mismatch"));
    }
    Ok(content)
}

/// Creates a pre-restore snapshot and returns the selected Markdown.
///
/// This deliberately does not write the note. The frontend document
/// persistence coordinator owns the resulting revision and awaits its durable
/// Markdown write receipt before it projects the restored content into the UI.
pub fn version_restore(
    state: &Arc<AppState>,
    version_id: i64,
    current_content: &str,
) -> AppResult<String> {
    version_restore_scoped(state, version_id, current_content, None, None, false)
}

/// Legacy restoration requires explicit target and vault confirmation; it never
/// assigns the unknown original row to the selected vault.
pub fn version_restore_scoped(
    state: &AppState,
    version_id: i64,
    current_content: &str,
    target_path: Option<&str>,
    expected_vault: Option<&str>,
    allow_legacy_unscoped: bool,
) -> AppResult<String> {
    crate::storage::atomic_write::with_vault_move_lock(|| {
        let vault = state.vault_path()?;
        if expected_vault.is_some_and(|expected| expected != vault.to_string_lossy()) {
            return Err(AppError::msg("version_vault_changed"));
        }
        let (storage_path, path, hash, legacy) =
            load_accessible_version(state, &vault, version_id)?;
        if legacy && (!allow_legacy_unscoped || target_path.is_none() || expected_vault.is_none()) {
            return Err(AppError::msg("version_legacy_confirmation_required"));
        }
        if !legacy && target_path.is_some_and(|target| Some(target) != path.as_deref()) {
            return Err(AppError::msg("version_target_mismatch"));
        }
        let path = target_path
            .or(path.as_deref())
            .ok_or_else(|| AppError::msg("version_target_required"))?;
        let content = verified_content(state, &vault, &storage_path, &hash)?;
        let pre_restore = create_snapshot_under_move_lock(
            state,
            path,
            current_content,
            SnapshotParams::pre_restore(),
        )?;
        if pre_restore.entry.is_none() {
            return Err(AppError::msg(
                "恢复前备份未能创建，已取消恢复以保护当前正文",
            ));
        }

        Ok(content)
    })
}

pub fn version_delete(state: &AppState, version_id: i64) -> AppResult<()> {
    crate::storage::atomic_write::with_vault_move_lock(|| {
        version_delete_under_move_lock(state, version_id)
    })
}

pub(crate) fn version_delete_under_move_lock(state: &AppState, version_id: i64) -> AppResult<()> {
    let vault = state.vault_path()?;
    let (storage_path, _, _, legacy) = load_accessible_version(state, &vault, version_id)?;
    if legacy {
        return Err(AppError::msg("version_legacy_delete_requires_ownership"));
    }
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

/// Snapshot taken immediately before closing a tab / leaving a note.
/// Bypasses hash dedup and idle cooldown so close always leaves a recoverable marker.
pub fn version_save_pre_close(
    state: &AppState,
    path: &str,
    content: &str,
) -> AppResult<Option<VersionEntry>> {
    create_snapshot(state, path, content, SnapshotParams::pre_close())
}

pub fn version_cleanup(state: &AppState) -> AppResult<usize> {
    crate::storage::atomic_write::with_vault_move_lock(|| {
        let vault = state.vault_path()?;
        let cutoff = Utc::now()
            .checked_sub_signed(chrono::Duration::days(7))
            .unwrap_or(Utc::now())
            .to_rfc3339();

        // Retention is determined by snapshot policy, never by whether today's
        // key or filesystem can read a blob. Preserve manual/finalized recovery
        // material even when unavailable; explicit deletion is a separate action.
        let candidates: Vec<(i64, String)> = state.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, storage_path FROM versions
             WHERE kind = 'auto_idle' AND is_finalized = 0 AND created_at < ?1
               AND vault_path = ?2 AND recycle_id IS NULL",
            )?;
            let rows = stmt.query_map(rusqlite::params![cutoff, vault.to_string_lossy()], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
            Ok(rows.collect::<Result<_, _>>()?)
        })?;

        let mut cleaned = 0;
        for (id, storage_path) in candidates {
            delete_version_row(state, &vault, id, &storage_path)?;
            cleaned += 1;
        }

        Ok(cleaned)
    })
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
    fn version_snapshot_stores_nested_note_path_with_forward_slashes() {
        let (_dir, state) = test_state();
        let vault = state.vault_path().unwrap();
        fs::create_dir_all(vault.join("nested").join("dir")).unwrap();
        fs::write(vault.join("nested").join("dir").join("note.md"), "body").unwrap();

        let entry = version_save_manual(&state, "nested/dir/note.md", "snap")
            .unwrap()
            .expect("snapshot");
        let stored: String = state
            .db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT note_path FROM versions WHERE id = ?1",
                    [entry.id],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(stored, "nested/dir/note.md");
        assert_eq!(version_list(&state, "nested/dir/note.md").unwrap().len(), 1);
    }

    #[test]
    fn durable_history_survives_index_prune_and_reindex() {
        let (_dir, state) = test_state();
        let vault = state.vault_path().unwrap();
        fs::write(vault.join("note.md"), "current disk body").unwrap();
        let snapshot = version_save_manual(&state, "note.md", "historical body")
            .unwrap()
            .unwrap();
        state
            .db
            .with_conn(|conn| {
                conn.execute("DELETE FROM files WHERE path = 'note.md'", [])?;
                Ok(())
            })
            .unwrap();
        assert_eq!(version_list(&state, "note.md").unwrap().len(), 1);
        assert_eq!(
            version_preview(&state, snapshot.id).unwrap(),
            "historical body"
        );
        assert!(version_save_manual(&state, "note.md", "historical body")
            .unwrap()
            .is_none());
        assert_eq!(
            fs::read_to_string(vault.join("note.md")).unwrap(),
            "current disk body"
        );
    }

    #[test]
    fn durable_history_isolates_same_path_between_vaults() {
        let (dir, state) = test_state();
        seed_file_in_db(&state, "note.md", "Note");
        let a = state.vault_path().unwrap();
        let snapshot_a = version_save_manual(&state, "note.md", "same body")
            .unwrap()
            .unwrap();
        let b = dir.path().join("vault-b");
        fs::create_dir_all(&b).unwrap();
        state.set_vault(b).unwrap();
        assert!(version_list(&state, "note.md").unwrap().is_empty());
        assert!(version_preview(&state, snapshot_a.id).is_err());
        assert!(version_restore(&state, snapshot_a.id, "current B").is_err());
        assert!(version_delete(&state, snapshot_a.id).is_err());
        let snapshot_b = version_save_manual(&state, "note.md", "same body")
            .unwrap()
            .unwrap();
        assert_ne!(snapshot_a.id, snapshot_b.id);
        state.set_vault(a).unwrap();
        assert_eq!(
            version_list(&state, "note.md").unwrap()[0].id,
            snapshot_a.id
        );
        assert_eq!(version_preview(&state, snapshot_a.id).unwrap(), "same body");
    }

    #[test]
    fn snapshot_bound_to_an_expected_vault_rejects_a_later_selection() {
        let (dir, state) = test_state();
        let vault_a = state.vault_path().unwrap();
        let vault_b = dir.path().join("vault-b");
        fs::create_dir_all(&vault_b).unwrap();
        state.set_vault(vault_b.clone()).unwrap();

        let error = create_snapshot_outcome_in_vault(
            &state,
            &vault_a,
            "note.md",
            "body captured in A",
            SnapshotParams::manual(),
        )
        .expect_err("queued snapshot must not cross the vault boundary");

        assert!(error.to_string().contains("version_vault_changed"));
        assert!(version_list(&state, "note.md").unwrap().is_empty());
        state
            .db
            .with_read_conn(|conn| {
                let count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM versions WHERE vault_path = ?1",
                    [vault_b.to_string_lossy()],
                    |row| row.get(0),
                )?;
                assert_eq!(count, 0);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn legacy_history_is_marked_and_requires_explicit_non_destructive_restore() {
        let (_dir, state) = test_state();
        let vault = state.vault_path().unwrap();
        let historical = "legacy historical body";
        let legacy_rel = "legacy/legacy-1.md";
        fs::create_dir_all(vault.join(".iris/versions/legacy")).unwrap();
        fs::write(vault.join(".iris/versions").join(legacy_rel), historical).unwrap();
        fs::write(vault.join("note.md"), "current disk body").unwrap();
        let hash = crate::cas::hash::content_hash_str(historical);
        let legacy_id = state
            .db
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO versions
                 (file_id, version_no, content_hash, storage_path, kind, created_at, note_path)
                 VALUES (0, 'legacy-1', ?1, ?2, 'manual', datetime('now'), 'note.md')",
                    rusqlite::params![hash, legacy_rel],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .unwrap();

        let listed = version_list(&state, "note.md").unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].is_legacy_unscoped);
        assert_eq!(version_preview(&state, legacy_id).unwrap(), historical);
        assert!(version_restore(&state, legacy_id, "current editor body").is_err());

        let known = version_save_manual(&state, "note.md", historical)
            .unwrap()
            .expect("unknown legacy history must not deduplicate a current-vault snapshot");
        assert!(!known.is_legacy_unscoped);
        let expected_vault = vault.to_string_lossy().into_owned();
        let restored = version_restore_scoped(
            &state,
            legacy_id,
            "current editor body",
            Some("note.md"),
            Some(&expected_vault),
            true,
        )
        .unwrap();
        assert_eq!(restored, historical);
        state
            .db
            .with_read_conn(|conn| {
                let ownership: Option<String> = conn.query_row(
                    "SELECT vault_path FROM versions WHERE id = ?1",
                    [legacy_id],
                    |row| row.get(0),
                )?;
                assert!(ownership.is_none(), "legacy row must remain unassigned");
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn version_cleanup_preserves_unreadable_manual_versions_and_only_expires_idle() {
        let (_dir, state) = test_state();
        let vault = state.vault_path().unwrap();
        seed_file_in_db(&state, "note.md", "Note");

        // 可读快照（新，不应被清理）。
        let entry = version_save_manual(&state, "note.md", "# Keep me")
            .unwrap()
            .expect("manual snapshot");
        let readable_storage: String = state
            .db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT storage_path FROM versions WHERE id = ?1",
                    [entry.id],
                    |r| r.get(0),
                )
                .map_err(Into::into)
            })
            .unwrap();

        // 旧 auto_idle 可读版本（既有规则：应被清理）。
        let readable_hash = readable_storage.strip_prefix("cas:").unwrap().to_string();
        let file_id: i64 = state
            .db
            .with_conn(|conn| {
                conn.query_row("SELECT id FROM files WHERE path = 'note.md'", [], |r| {
                    r.get(0)
                })
                .map_err(Into::into)
            })
            .unwrap();

        // A missing key is not proof that a finalized manual snapshot is disposable.
        let lost_body = b"# Lost forever";
        let lost_hash = crate::cas::hash::content_hash(lost_body);
        let lost_path = state.cas_store().unwrap().object_path(&lost_hash).unwrap();
        let mut buf = b"CASE".to_vec();
        buf.extend_from_slice(
            &crate::cas::encryption::encrypt_blob(lost_body, &[0xEE; 32]).unwrap(),
        );
        fs::create_dir_all(lost_path.parent().unwrap()).unwrap();
        fs::write(&lost_path, &buf).unwrap();
        state.ref_counter().increment(&lost_hash).unwrap();

        state
            .db
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO versions (file_id, version_no, label, content_hash, word_count, is_finalized, kind, created_at, storage_path, vault_path, note_path)
                     VALUES (?1, 'lost-1', NULL, ?2, 0, 1, 'manual', '2026-01-01T00:00:00+00:00', ?3, ?4, 'note.md')",
                    rusqlite::params![file_id, lost_hash, format!("cas:{lost_hash}"), vault.to_string_lossy()],
                )?;
                conn.execute(
                    "INSERT INTO versions (file_id, version_no, label, content_hash, word_count, is_finalized, kind, created_at, storage_path, vault_path, note_path)
                     VALUES (?1, 'stale-1', NULL, ?2, 0, 0, 'auto_idle', '2026-01-01T00:00:00+00:00', ?3, ?4, 'note.md')",
                    rusqlite::params![file_id, readable_hash, readable_storage, vault.to_string_lossy()],
                )?;
                Ok(())
            })
            .unwrap();

        assert!(version_preview(&state, entry.id).is_ok());
        let cleaned = version_cleanup(&state).unwrap();
        assert_eq!(cleaned, 1, "only an expired auto_idle may be removed");

        let remaining = version_list(&state, "note.md").unwrap();
        assert_eq!(remaining.len(), 2);
        assert!(remaining.iter().any(|version| version.id == entry.id));
        assert!(remaining
            .iter()
            .any(|version| version.version_no == "lost-1"));
        assert_eq!(
            state.ref_counter().get_count(&lost_hash).unwrap(),
            1,
            "unreadable manual snapshots must retain their blob ownership"
        );
    }

    #[test]
    fn classified_version_restore_prepares_content_without_writing_markdown() {
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
        state
            .db
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO files_metadata_fts (path, aliases, tags)
                     VALUES ('.classified/secret.md', 'secret', 'classified')",
                    [],
                )?;
                Ok(())
            })
            .unwrap();

        let restored = version_restore(&state, entry.id, "# Current").unwrap();
        assert_eq!(restored, "# Historical");

        let raw = fs::read(vault.join(".classified/secret.md")).unwrap();
        assert!(classified_io::has_csef_magic(&raw));
        let decrypted = classified_io::decrypt_cef(&raw, &key).unwrap();
        assert_eq!(String::from_utf8(decrypted).unwrap(), "# Current");

        let (chunks, fts, metadata_fts): (i64, i64, i64) = state
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
                let metadata_fts = conn.query_row(
                    "SELECT COUNT(*) FROM files_metadata_fts WHERE path = '.classified/secret.md'",
                    [],
                    |r| r.get(0),
                )?;
                Ok((chunks, fts, metadata_fts))
            })
            .unwrap();
        assert_eq!(chunks, 0);
        assert_eq!(fts, 0);
        assert_eq!(metadata_fts, 0);
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
    fn pre_close_creates_even_when_hash_matches_latest() {
        let (_dir, state) = test_state();
        seed_file_in_db(&state, "note.md", "Note");

        let manual = version_save_manual(&state, "note.md", "same body")
            .unwrap()
            .expect("manual");
        let pre_close = version_save_pre_close(&state, "note.md", "same body")
            .unwrap()
            .expect("pre_close must bypass hash dedup");

        assert_eq!(pre_close.kind, VersionKind::PreClose);
        assert!(!pre_close.is_finalized);
        assert_ne!(pre_close.id, manual.id);
    }

    #[test]
    fn consecutive_delta_snapshots_remain_readable_after_a_previous_delta() {
        let (_dir, state) = test_state();
        seed_file_in_db(&state, "note.md", "Note");
        let base = (0..200)
            .map(|i| format!("stable line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let second_body = base.replace("stable line 50\n", "first edit\n");
        let third_body = second_body.replace("stable line 150\n", "second edit\n");
        let first = version_save_manual(&state, "note.md", &base)
            .unwrap()
            .unwrap();
        let second = version_save_manual(&state, "note.md", &second_body)
            .unwrap()
            .unwrap();
        let third = version_save_manual(&state, "note.md", &third_body)
            .unwrap()
            .unwrap();
        let second_storage: String = state
            .db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT storage_path FROM versions WHERE id = ?1",
                    [second.id],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        assert!(
            second_storage.starts_with("dif:"),
            "fixture must exercise an actual delta parent"
        );
        assert_eq!(version_preview(&state, first.id).unwrap(), base);
        assert_eq!(version_preview(&state, second.id).unwrap(), second_body);
        assert_eq!(version_preview(&state, third.id).unwrap(), third_body);
        version_delete(&state, second.id).unwrap();
        assert_eq!(version_preview(&state, third.id).unwrap(), third_body);
    }

    #[test]
    fn diff_storage_increments_and_decrements_both_refs() {
        let (_dir, state) = test_state();
        seed_file_in_db(&state, "note.md", "Note");

        // Large shared prefix makes a delta worth storing.
        let base = format!("{}\nline-a\n", "shared-prefix-".repeat(80));
        let next = format!("{}\nline-b\n", "shared-prefix-".repeat(80));

        let first = version_save_manual(&state, "note.md", &base)
            .unwrap()
            .expect("base snapshot");
        let second = version_save_manual(&state, "note.md", &next)
            .unwrap()
            .expect("delta snapshot");

        let second_storage: String = state
            .db
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT storage_path FROM versions WHERE id = ?1",
                    [second.id],
                    |r| r.get(0),
                )?)
            })
            .unwrap();

        if let Some(rest) = second_storage.strip_prefix("dif:") {
            let (parent_hash, diff_hash) = rest.split_once(':').expect("dif path");
            let parent_refs: i64 = state
                .db
                .with_conn(|conn| {
                    Ok(conn.query_row(
                        "SELECT ref_count FROM cas_refs WHERE object_hash = ?1",
                        [parent_hash],
                        |r| r.get(0),
                    )?)
                })
                .unwrap();
            let diff_refs: i64 = state
                .db
                .with_conn(|conn| {
                    Ok(conn.query_row(
                        "SELECT ref_count FROM cas_refs WHERE object_hash = ?1",
                        [diff_hash],
                        |r| r.get(0),
                    )?)
                })
                .unwrap();
            assert!(parent_refs >= 1, "parent blob must be refcounted");
            assert_eq!(diff_refs, 1, "diff blob must be refcounted");

            version_delete(&state, second.id).unwrap();

            let parent_after: i64 = state
                .db
                .with_conn(|conn| {
                    Ok(conn
                        .query_row(
                            "SELECT COALESCE((SELECT ref_count FROM cas_refs WHERE object_hash = ?1), 0)",
                            [parent_hash],
                            |r| r.get(0),
                        )?)
                })
                .unwrap();
            let diff_after: i64 = state
                .db
                .with_conn(|conn| {
                    Ok(conn
                        .query_row(
                            "SELECT COALESCE((SELECT ref_count FROM cas_refs WHERE object_hash = ?1), 0)",
                            [diff_hash],
                            |r| r.get(0),
                        )?)
                })
                .unwrap();
            assert!(
                parent_after < parent_refs,
                "deleting dif: version must decrement parent"
            );
            assert_eq!(diff_after, 0, "deleting dif: version must decrement diff");
            let _ = first;
        } else {
            // Diff was not chosen (threshold); still a valid outcome — assert cas: path works.
            assert!(
                second_storage.starts_with("cas:"),
                "expected cas: or dif: storage, got {second_storage}"
            );
        }
    }

    #[test]
    fn version_restore_creates_pre_restore_snapshot() {
        let (_dir, state) = test_state();
        let vault = state.vault_path().unwrap();
        fs::write(vault.join("note.md"), "disk body before restore").unwrap();
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
        assert_eq!(
            fs::read_to_string(vault.join("note.md")).unwrap(),
            "disk body before restore"
        );

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
    fn failed_version_row_delete_keeps_its_cas_reference() {
        let (_dir, state) = test_state();
        let entry = version_save_manual(&state, "note.md", "protected body")
            .unwrap()
            .unwrap();
        let storage: String = state
            .db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT storage_path FROM versions WHERE id = ?1",
                    [entry.id],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        let hash = storage.strip_prefix("cas:").unwrap();
        state
            .db
            .with_conn(|conn| {
                conn.execute_batch(
                    "CREATE TRIGGER fail_version_delete BEFORE DELETE ON versions
                 BEGIN SELECT RAISE(ABORT, 'fault'); END;",
                )?;
                Ok(())
            })
            .unwrap();

        assert!(version_delete(&state, entry.id).is_err());
        assert_eq!(state.ref_counter().get_count(hash).unwrap(), 1);
        assert!(state
            .cas_store()
            .unwrap()
            .object_path(hash)
            .unwrap()
            .is_file());
        let remaining = state
            .db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM versions WHERE id = ?1",
                    [entry.id],
                    |row| row.get::<_, i64>(0),
                )?)
            })
            .unwrap();
        assert_eq!(remaining, 1);
    }

    #[test]
    fn enforce_auto_idle_cap_deletes_oldest_when_over_limit() {
        let (_dir, state) = test_state();
        let vault = state.vault_path().unwrap();
        let file_id = {
            let mut id = 0_i64;
            state
                .db
                .with_conn(|conn| {
                    id = seed_file(conn, "note.md", "Note");
                    for i in 0..31 {
                        let version_no = format!("202601010000000{i:02}");
                        conn.execute(
                            "INSERT INTO versions (file_id, version_no, content_hash, storage_path, is_finalized, kind, created_at, vault_path, note_path)
                             VALUES (?1, ?2, ?3, ?4, 0, 'auto_idle', ?5, ?6, 'note.md')",
                            rusqlite::params![
                                id,
                                version_no,
                                format!("hash{i}"),
                                format!("{id}/{version_no}.md"),
                                format!("2026-01-01T00:{i:02}:00Z"),
                                vault.to_string_lossy(),
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
        let vault = state.vault_path().unwrap();
        state
            .db
            .with_conn(|conn| {
                seed_file(conn, "note.md", "Note");
                conn.execute(
                    "INSERT INTO versions (file_id, version_no, content_hash, storage_path, is_finalized, kind, created_at, vault_path, note_path)
                     VALUES (1, '20200101000000000', 'old_auto', '1/old_auto.md', 0, 'auto_idle', '2020-01-01T00:00:00Z', ?1, 'note.md')",
                    [vault.to_string_lossy()],
                )?;
                conn.execute(
                    "INSERT INTO versions (file_id, version_no, content_hash, storage_path, is_finalized, kind, created_at, vault_path, note_path)
                     VALUES (1, '20200101000000001', 'old_manual', '1/old_manual.md', 0, 'manual', '2020-01-01T00:00:00Z', ?1, 'note.md')",
                    [vault.to_string_lossy()],
                )?;
                conn.execute(
                    "INSERT INTO versions (file_id, version_no, content_hash, storage_path, is_finalized, kind, created_at, vault_path, note_path)
                     VALUES (1, '20990101000000000', 'new_auto', '1/new_auto.md', 0, 'auto_idle', datetime('now'), ?1, 'note.md')",
                    [vault.to_string_lossy()],
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
