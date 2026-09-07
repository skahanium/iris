//! Durable, vault-local cleanup checkpoints for destructive history operations.

use std::fs;
use std::path::{Component, Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::storage::atomic_write::{atomic_write, sync_parent_directory};
use crate::storage::db::Database;
use crate::version::repository::VersionOwnership;

const CHECKPOINT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub(super) enum CleanupKind {
    Purge {
        recycle_id: String,
        trash_rel_dir: String,
    },
    Discard {
        note_path: String,
        file_ids: Vec<i64>,
        version_ids: Vec<i64>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CandidateRoot {
    Vault,
    Versions,
}

#[derive(Debug, Clone)]
pub(super) struct Candidate {
    pub(super) root: CandidateRoot,
    pub(super) relative_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StagedFile {
    root: CandidateRoot,
    relative_path: String,
    staging_name: String,
    #[serde(default)]
    owner_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CleanupCheckpoint {
    schema_version: u32,
    operation_id: String,
    vault: String,
    operation: CleanupKind,
    #[serde(default)]
    version_owners: Vec<VersionOwnership>,
    files: Vec<StagedFile>,
    #[serde(skip)]
    directory: PathBuf,
}

pub(super) fn prepare(
    vault: &Path,
    operation: CleanupKind,
    version_owners: Vec<VersionOwnership>,
    candidates: Vec<Candidate>,
) -> AppResult<CleanupCheckpoint> {
    let vault = vault.canonicalize()?;
    if let CleanupKind::Discard { version_ids, .. } = &operation {
        let mut expected_ids = version_ids.clone();
        expected_ids.sort_unstable();
        let mut owner_ids = version_owners
            .iter()
            .map(|owner| owner.id)
            .collect::<Vec<_>>();
        owner_ids.sort_unstable();
        if expected_ids != owner_ids {
            return Err(AppError::msg("cleanup_version_witness_mismatch"));
        }
    }
    if let CleanupKind::Purge {
        recycle_id,
        trash_rel_dir,
    } = &operation
    {
        validate_bundle_manifest(&vault, recycle_id, trash_rel_dir)?;
    }
    let mut files = Vec::new();
    for (index, candidate) in candidates.into_iter().enumerate() {
        let source = resolve_candidate(&vault, candidate.root, &candidate.relative_path)?;
        if !source.exists() {
            continue;
        }
        let owner_ids = match candidate.root {
            CandidateRoot::Vault => Vec::new(),
            CandidateRoot::Versions => {
                let owner_ids = version_owners
                    .iter()
                    .filter(|owner| owner.storage_path == candidate.relative_path)
                    .map(|owner| owner.id)
                    .collect::<Vec<_>>();
                if owner_ids.is_empty() {
                    return Err(AppError::msg("cleanup_storage_witness_missing"));
                }
                owner_ids
            }
        };
        files.push(StagedFile {
            root: candidate.root,
            relative_path: candidate.relative_path,
            staging_name: format!("{index}.staged"),
            owner_ids,
        });
    }
    let staging_root = ensure_staging_root(&vault)?;
    let operation_id = Uuid::new_v4().to_string();
    let directory = staging_root.join(&operation_id);
    fs::create_dir(&directory)?;
    if let Err(error) = sync_parent_directory(&staging_root) {
        let _ = fs::remove_dir(&directory);
        return Err(error);
    }
    let mut checkpoint = CleanupCheckpoint {
        schema_version: CHECKPOINT_VERSION,
        operation_id,
        vault: vault.to_string_lossy().into_owned(),
        operation,
        version_owners,
        files,
        directory,
    };
    if let Err(error) = persist(&checkpoint) {
        let _ = fs::remove_dir_all(&checkpoint.directory);
        let _ = sync_parent_directory(&staging_root);
        return Err(error);
    }

    for file in &checkpoint.files {
        let source = resolve_staged_original(&vault, file)?;
        let target = resolve_staging_file(&checkpoint, file)?;
        if let Err(error) = move_no_replace(&source, &target) {
            if let Err(rollback_error) = rollback(&mut checkpoint) {
                return Err(AppError::msg(format!(
                    "cleanup staging failed and rollback is pending: {error}; {rollback_error}"
                )));
            }
            return Err(error);
        }
    }
    Ok(checkpoint)
}

pub(super) fn rollback(checkpoint: &mut CleanupCheckpoint) -> AppResult<()> {
    let vault = validated_checkpoint_vault(checkpoint)?;
    for file in checkpoint.files.iter().rev() {
        let source = resolve_staging_file(checkpoint, file)?;
        let target = resolve_staged_original(&vault, file)?;
        match (path_entry_exists(&source)?, path_entry_exists(&target)?) {
            (true, false) => move_no_replace(&source, &target)?,
            (true, true) => return Err(AppError::msg("cleanup_rollback_target_conflict")),
            (false, true) => {}
            (false, false) => return Err(AppError::msg("cleanup_rollback_material_missing")),
        }
    }
    remove_checkpoint_directory(checkpoint)
}

pub(super) fn finish(db: &Database, checkpoint: &CleanupCheckpoint) -> AppResult<u64> {
    let vault = validated_checkpoint_vault(checkpoint)?;
    let storage_paths = staged_storage_paths(checkpoint)?;
    db.with_conn(|conn| {
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            crate::version::repository::ensure_cleanup_storage_unowned(
                conn,
                &vault,
                &storage_paths,
            )?;
            finish_filesystem(&vault, checkpoint)
        })();
        match result {
            Ok(freed) => match conn.execute_batch("COMMIT") {
                Ok(()) => Ok(freed),
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
    })
}

pub(crate) fn recover_pending(db: &Database, vault: &Path) -> Vec<String> {
    let Ok(vault) = vault.canonicalize() else {
        return vec!["vault_not_canonical".to_string()];
    };
    let root = staging_root(&vault);
    if !root.exists() {
        return Vec::new();
    }
    let Ok(metadata) = fs::symlink_metadata(&root) else {
        return vec!["cleanup_staging_unreadable".to_string()];
    };
    let Ok(canonical_root) = root.canonicalize() else {
        return vec!["cleanup_staging_unreadable".to_string()];
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() || canonical_root != root {
        return vec!["cleanup_staging_boundary_violation".to_string()];
    }
    let Ok(entries) = fs::read_dir(&root) else {
        return vec!["cleanup_staging_unreadable".to_string()];
    };
    let mut unresolved = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            unresolved.push("cleanup_staging_entry_unreadable".to_string());
            continue;
        };
        let operation_id = entry.file_name().to_string_lossy().into_owned();
        let Ok(file_type) = entry.file_type() else {
            unresolved.push(operation_id);
            continue;
        };
        if !file_type.is_dir() || file_type.is_symlink() {
            unresolved.push(operation_id);
            continue;
        }
        let directory = entry.path();
        let result = (|| {
            let checkpoint_path = directory.join("checkpoint.json");
            let metadata = fs::symlink_metadata(&checkpoint_path)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(AppError::msg("cleanup_checkpoint_boundary_violation"));
            }
            let raw = fs::read(checkpoint_path)?;
            let mut checkpoint: CleanupCheckpoint = serde_json::from_slice(&raw)?;
            checkpoint.directory = directory;
            if checkpoint.schema_version != CHECKPOINT_VERSION
                || checkpoint.operation_id != operation_id
                || checkpoint.vault != vault.to_string_lossy()
            {
                return Err(AppError::msg("cleanup_checkpoint_identity_mismatch"));
            }
            if database_change_committed(db, &checkpoint)? {
                finish(db, &checkpoint).map(|_| ())
            } else {
                rollback(&mut checkpoint)
            }
        })();
        if result.is_err() {
            unresolved.push(operation_id);
        }
    }
    unresolved
}

fn database_change_committed(db: &Database, checkpoint: &CleanupCheckpoint) -> AppResult<bool> {
    db.with_read_conn(|conn| match &checkpoint.operation {
        CleanupKind::Purge { recycle_id, .. } => {
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM recycle_bin WHERE id = ?1)",
                [recycle_id],
                |row| row.get(0),
            )?;
            let version_state = exact_version_witness_state(conn, checkpoint)?;
            match (exists, version_state) {
                (true, Some(true)) | (false, Some(false)) => {
                    Err(AppError::msg("cleanup_database_witness_partial"))
                }
                (true, _) => Ok(false),
                (false, _) => Ok(true),
            }
        }
        CleanupKind::Discard {
            note_path,
            file_ids,
            version_ids,
        } => {
            let version_state = if checkpoint.version_owners.is_empty() {
                legacy_id_witness_state(conn, "versions", version_ids)?
            } else {
                exact_version_witness_state(conn, checkpoint)?
            };
            let file_state = exact_file_witness_state(conn, note_path, file_ids)?;
            committed_from_witness_states(version_state, file_state)
        }
    })
}

fn committed_from_witness_states(first: Option<bool>, second: Option<bool>) -> AppResult<bool> {
    let states = [first, second].into_iter().flatten().collect::<Vec<_>>();
    if states.is_empty() || states.iter().all(|committed| *committed) {
        return Ok(true);
    }
    if states.iter().all(|committed| !*committed) {
        return Ok(false);
    }
    Err(AppError::msg("cleanup_database_witness_partial"))
}

fn exact_version_witness_state(
    conn: &Connection,
    checkpoint: &CleanupCheckpoint,
) -> AppResult<Option<bool>> {
    if checkpoint.version_owners.is_empty() {
        return Ok(None);
    }
    let mut present = 0usize;
    for owner in &checkpoint.version_owners {
        match version_owner_by_id(conn, owner.id)? {
            Some(current) if current == *owner => present += 1,
            Some(_) => return Err(AppError::msg("cleanup_version_ownership_conflict")),
            None => {}
        }
    }
    if present == 0 {
        Ok(Some(true))
    } else if present == checkpoint.version_owners.len() {
        Ok(Some(false))
    } else {
        Err(AppError::msg("cleanup_database_witness_partial"))
    }
}

fn exact_file_witness_state(
    conn: &Connection,
    note_path: &str,
    ids: &[i64],
) -> AppResult<Option<bool>> {
    if ids.is_empty() {
        return Ok(None);
    }
    let mut present = 0usize;
    for id in ids {
        let current: Option<String> = conn
            .query_row("SELECT path FROM files WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .optional()?;
        match current {
            Some(path) if path == note_path => present += 1,
            Some(_) => return Err(AppError::msg("cleanup_file_ownership_conflict")),
            None => {}
        }
    }
    if present == 0 {
        Ok(Some(true))
    } else if present == ids.len() {
        Ok(Some(false))
    } else {
        Err(AppError::msg("cleanup_database_witness_partial"))
    }
}

fn legacy_id_witness_state(conn: &Connection, table: &str, ids: &[i64]) -> AppResult<Option<bool>> {
    if ids.is_empty() {
        return Ok(None);
    }
    let mut present = 0usize;
    for id in ids {
        let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?1)");
        if conn.query_row(&sql, [id], |row| row.get::<_, bool>(0))? {
            present += 1;
        }
    }
    if present == 0 {
        Ok(Some(true))
    } else if present == ids.len() {
        Ok(Some(false))
    } else {
        Err(AppError::msg("cleanup_database_witness_partial"))
    }
}

fn version_owner_by_id(conn: &Connection, id: i64) -> AppResult<Option<VersionOwnership>> {
    Ok(conn
        .query_row(
            "SELECT id, vault_path, note_path, recycle_id, storage_path
             FROM versions WHERE id = ?1",
            [id],
            |row| {
                Ok(VersionOwnership {
                    id: row.get(0)?,
                    vault_path: row.get(1)?,
                    note_path: row.get(2)?,
                    recycle_id: row.get(3)?,
                    storage_path: row.get(4)?,
                })
            },
        )
        .optional()?)
}

fn staged_storage_paths(checkpoint: &CleanupCheckpoint) -> AppResult<Vec<String>> {
    let mut paths = Vec::new();
    for file in &checkpoint.files {
        if matches!(file.root, CandidateRoot::Versions) {
            let expected_ids = checkpoint
                .version_owners
                .iter()
                .filter(|owner| owner.storage_path == file.relative_path)
                .map(|owner| owner.id)
                .collect::<Vec<_>>();
            let legacy_checkpoint =
                checkpoint.version_owners.is_empty() && file.owner_ids.is_empty();
            if !legacy_checkpoint && (file.owner_ids.is_empty() || file.owner_ids != expected_ids) {
                return Err(AppError::msg("cleanup_storage_witness_mismatch"));
            }
            paths.push(file.relative_path.clone());
        }
    }
    Ok(paths)
}

fn finish_filesystem(vault: &Path, checkpoint: &CleanupCheckpoint) -> AppResult<u64> {
    let mut freed = 0u64;
    if let CleanupKind::Purge {
        recycle_id,
        trash_rel_dir,
    } = &checkpoint.operation
    {
        let bundle = validated_bundle_path(vault, recycle_id, trash_rel_dir)?;
        if path_entry_exists(&bundle)? {
            freed = super::dir_size(&bundle);
            let metadata = fs::symlink_metadata(&bundle)?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(AppError::msg("recycle_bundle_boundary_violation"));
            }
            fs::remove_dir_all(&bundle)?;
            sync_parent_directory(
                bundle
                    .parent()
                    .ok_or_else(|| AppError::msg("recycle bundle has no parent"))?,
            )?;
        }
    }
    for file in &checkpoint.files {
        let staged = resolve_staging_file(checkpoint, file)?;
        if path_entry_exists(&staged)? {
            let metadata = fs::symlink_metadata(&staged)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(AppError::msg("cleanup_staging_boundary_violation"));
            }
            freed = freed.saturating_add(metadata.len());
            fs::remove_file(&staged)?;
            sync_parent_directory(&checkpoint.directory)?;
        }
    }
    remove_checkpoint_directory(checkpoint)?;
    Ok(freed)
}

fn persist(checkpoint: &CleanupCheckpoint) -> AppResult<()> {
    atomic_write(
        &checkpoint.directory.join("checkpoint.json"),
        &serde_json::to_vec_pretty(checkpoint)?,
    )
}

fn staging_root(vault: &Path) -> PathBuf {
    vault.join(".iris").join("trash").join(".cleanup-staging")
}

fn ensure_staging_root(vault: &Path) -> AppResult<PathBuf> {
    let root = staging_root(vault);
    let mut current = vault.to_path_buf();
    for component in [".iris", "trash", ".cleanup-staging"] {
        current.push(component);
        if let Ok(metadata) = fs::symlink_metadata(&current) {
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(AppError::msg("cleanup_staging_boundary_violation"));
            }
        } else {
            let parent = current
                .parent()
                .ok_or_else(|| AppError::msg("cleanup staging path has no parent"))?;
            fs::create_dir(&current)?;
            sync_parent_directory(parent)?;
        }
    }
    let canonical = root.canonicalize()?;
    if canonical != root || !canonical.starts_with(vault) {
        return Err(AppError::msg("cleanup_staging_boundary_violation"));
    }
    Ok(root)
}

fn validated_checkpoint_vault(checkpoint: &CleanupCheckpoint) -> AppResult<PathBuf> {
    let vault = PathBuf::from(&checkpoint.vault).canonicalize()?;
    if vault.to_string_lossy() != checkpoint.vault {
        return Err(AppError::msg("cleanup_checkpoint_vault_mismatch"));
    }
    let root = ensure_staging_root(&vault)?;
    let directory = checkpoint.directory.canonicalize()?;
    if directory != checkpoint.directory || !directory.starts_with(&root) {
        return Err(AppError::msg("cleanup_checkpoint_boundary_violation"));
    }
    Ok(vault)
}

fn resolve_staged_original(vault: &Path, file: &StagedFile) -> AppResult<PathBuf> {
    resolve_candidate(vault, file.root, &file.relative_path)
}

fn resolve_staging_file(checkpoint: &CleanupCheckpoint, file: &StagedFile) -> AppResult<PathBuf> {
    let mut components = Path::new(&file.staging_name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(AppError::msg("cleanup_staging_boundary_violation"));
    }
    Ok(checkpoint.directory.join(&file.staging_name))
}

fn resolve_candidate(vault: &Path, root: CandidateRoot, relative: &str) -> AppResult<PathBuf> {
    let relative = Path::new(relative);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(AppError::msg("cleanup_path_boundary_violation"));
    }
    let base = match root {
        CandidateRoot::Vault => vault.to_path_buf(),
        CandidateRoot::Versions => {
            let versions = vault.join(".iris").join("versions");
            if !path_entry_exists(&versions)? {
                return Ok(versions.join(relative));
            }
            let canonical = versions.canonicalize()?;
            if canonical != versions || !canonical.starts_with(vault) {
                return Err(AppError::msg("version_storage_boundary_violation"));
            }
            versions
        }
    };
    let candidate = base.join(relative);
    let mut current = base.clone();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(AppError::msg("cleanup_path_boundary_violation"));
        };
        current.push(component);
        if let Ok(metadata) = fs::symlink_metadata(&current) {
            if metadata.file_type().is_symlink() {
                return Err(AppError::msg("cleanup_path_boundary_violation"));
            }
        }
    }
    if path_entry_exists(&candidate)? {
        let metadata = fs::symlink_metadata(&candidate)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(AppError::msg("cleanup_target_is_not_regular_file"));
        }
        let canonical = candidate.canonicalize()?;
        if canonical != candidate || !canonical.starts_with(&base) {
            return Err(AppError::msg("cleanup_path_boundary_violation"));
        }
    }
    Ok(candidate)
}

fn validated_bundle_path(vault: &Path, id: &str, trash_rel_dir: &str) -> AppResult<PathBuf> {
    let mut components = Path::new(id).components();
    if !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
        || trash_rel_dir != format!(".iris/trash/{id}")
    {
        return Err(AppError::msg("recycle_bundle_identity_mismatch"));
    }
    let root = vault.join(".iris").join("trash");
    let bundle = root.join(id);
    if path_entry_exists(&bundle)? {
        let canonical_root = root.canonicalize()?;
        let canonical_bundle = bundle.canonicalize()?;
        if canonical_root != root
            || canonical_bundle != bundle
            || !canonical_bundle.starts_with(&canonical_root)
        {
            return Err(AppError::msg("recycle_bundle_boundary_violation"));
        }
    }
    Ok(bundle)
}

pub(super) fn validate_bundle_manifest(
    vault: &Path,
    id: &str,
    trash_rel_dir: &str,
) -> AppResult<()> {
    let bundle = validated_bundle_path(vault, id, trash_rel_dir)?;
    let manifest = bundle.join("manifest.json");
    let Ok(metadata) = fs::symlink_metadata(&manifest) else {
        return Err(AppError::msg("recycle_bundle_manifest_missing"));
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(AppError::msg("recycle_bundle_manifest_boundary_violation"));
    }
    Ok(())
}

fn move_no_replace(source: &Path, target: &Path) -> AppResult<()> {
    let source_metadata = fs::symlink_metadata(source)?;
    if !source_metadata.is_file() || source_metadata.file_type().is_symlink() {
        return Err(AppError::msg("cleanup source is not a regular file"));
    }
    if path_entry_exists(target)? {
        return Err(AppError::msg("cleanup_staging_target_conflict"));
    }
    let source_parent = source
        .parent()
        .ok_or_else(|| AppError::msg("cleanup source has no parent"))?;
    let target_parent = target
        .parent()
        .ok_or_else(|| AppError::msg("cleanup target has no parent"))?;
    fs::rename(source, target)?;
    sync_parent_directory(source_parent)?;
    if source_parent != target_parent {
        sync_parent_directory(target_parent)?;
    }
    Ok(())
}

fn remove_checkpoint_directory(checkpoint: &CleanupCheckpoint) -> AppResult<()> {
    let parent = checkpoint
        .directory
        .parent()
        .ok_or_else(|| AppError::msg("cleanup checkpoint has no parent"))?;
    if path_entry_exists(&checkpoint.directory)? {
        fs::remove_dir_all(&checkpoint.directory)?;
        sync_parent_directory(parent)?;
    }
    Ok(())
}

fn path_entry_exists(path: &Path) -> AppResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn setup() -> (tempfile::TempDir, PathBuf, Database) {
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("vault");
        fs::create_dir_all(vault.join(".iris/versions/recycled")).unwrap();
        let vault = vault.canonicalize().unwrap();
        let database = Database::open_in_memory().unwrap();
        (directory, vault, database)
    }

    fn seed_version(database: &Database, vault: &Path, relative: &str) -> i64 {
        database
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO versions
                     (file_id, version_no, content_hash, storage_path, kind, created_at,
                      vault_path, note_path)
                     VALUES (0, 'restart', 'hash', ?1, 'manual', datetime('now'), ?2, 'note.md')",
                    params![relative, vault.to_string_lossy()],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .unwrap()
    }

    fn prepare_version_cleanup(
        database: &Database,
        vault: &Path,
    ) -> (i64, PathBuf, CleanupCheckpoint) {
        let relative = "recycled/restart.md";
        let original = vault.join(".iris/versions").join(relative);
        fs::write(&original, "recoverable snapshot").unwrap();
        let id = seed_version(database, vault, relative);
        let checkpoint = prepare(
            vault,
            CleanupKind::Discard {
                note_path: "note.md".to_string(),
                file_ids: Vec::new(),
                version_ids: vec![id],
            },
            vec![VersionOwnership {
                id,
                vault_path: Some(vault.to_string_lossy().into_owned()),
                note_path: Some("note.md".to_string()),
                recycle_id: None,
                storage_path: relative.to_string(),
            }],
            vec![Candidate {
                root: CandidateRoot::Versions,
                relative_path: relative.to_string(),
            }],
        )
        .unwrap();
        (id, original, checkpoint)
    }

    #[test]
    fn restart_rolls_back_staging_when_database_ownership_remains() {
        let (_directory, vault, database) = setup();
        let (_id, original, checkpoint) = prepare_version_cleanup(&database, &vault);
        assert!(!original.exists());
        assert!(checkpoint.directory.join("0.staged").is_file());

        assert!(recover_pending(&database, &vault).is_empty());

        assert_eq!(
            fs::read_to_string(original).unwrap(),
            "recoverable snapshot"
        );
        assert_eq!(fs::read_dir(staging_root(&vault)).unwrap().count(), 0);
    }

    #[test]
    fn restart_finishes_staging_when_database_change_committed() {
        let (_directory, vault, database) = setup();
        let (id, original, checkpoint) = prepare_version_cleanup(&database, &vault);
        database
            .with_conn(|conn| {
                conn.execute("DELETE FROM versions WHERE id = ?1", [id])?;
                Ok(())
            })
            .unwrap();
        assert!(checkpoint.directory.join("0.staged").is_file());

        assert!(recover_pending(&database, &vault).is_empty());

        assert!(!original.exists());
        assert_eq!(fs::read_dir(staging_root(&vault)).unwrap().count(), 0);
    }

    #[test]
    fn restart_finishes_committed_purge_bundle_and_snapshot() {
        let (_directory, vault, database) = setup();
        let id = "legacy-recycle-id";
        let trash_rel = format!(".iris/trash/{id}");
        let bundle = vault.join(&trash_rel);
        fs::create_dir_all(&bundle).unwrap();
        fs::write(bundle.join("manifest.json"), "{}").unwrap();
        fs::write(bundle.join("document.md"), "trashed body").unwrap();
        let relative = "recycled/purge-restart.md";
        let version = vault.join(".iris/versions").join(relative);
        fs::write(&version, "snapshot").unwrap();
        let version_id = seed_version(&database, &vault, relative);
        database
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO recycle_bin
                     (id, original_path, title, deleted_at, expires_at, trash_rel_dir)
                     VALUES (?1, 'note.md', 'Note', datetime('now'), datetime('now'), ?2)",
                    params![id, trash_rel],
                )?;
                Ok(())
            })
            .unwrap();
        let checkpoint = prepare(
            &vault,
            CleanupKind::Purge {
                recycle_id: id.to_string(),
                trash_rel_dir: trash_rel,
            },
            vec![VersionOwnership {
                id: version_id,
                vault_path: Some(vault.to_string_lossy().into_owned()),
                note_path: Some("note.md".to_string()),
                recycle_id: Some(id.to_string()),
                storage_path: relative.to_string(),
            }],
            vec![Candidate {
                root: CandidateRoot::Versions,
                relative_path: relative.to_string(),
            }],
        )
        .unwrap();
        database
            .with_conn(|conn| {
                conn.execute("DELETE FROM versions WHERE id = ?1", [version_id])?;
                conn.execute("DELETE FROM recycle_bin WHERE id = ?1", [id])?;
                Ok(())
            })
            .unwrap();
        assert!(checkpoint.directory.join("0.staged").is_file());

        assert!(recover_pending(&database, &vault).is_empty());

        assert!(!bundle.exists());
        assert!(!version.exists());
        assert_eq!(fs::read_dir(staging_root(&vault)).unwrap().count(), 0);
    }

    #[test]
    fn restart_never_overwrites_a_conflicting_rollback_target() {
        let (_directory, vault, database) = setup();
        let (_id, original, checkpoint) = prepare_version_cleanup(&database, &vault);
        fs::write(&original, "new owner material").unwrap();

        let unresolved = recover_pending(&database, &vault);

        assert_eq!(unresolved, vec![checkpoint.operation_id]);
        assert_eq!(fs::read_to_string(original).unwrap(), "new owner material");
        assert_eq!(
            fs::read_to_string(checkpoint.directory.join("0.staged")).unwrap(),
            "recoverable snapshot"
        );
    }

    #[test]
    fn finish_preserves_staging_when_a_new_storage_owner_exists() {
        let (_directory, vault, database) = setup();
        let (id, original, checkpoint) = prepare_version_cleanup(&database, &vault);
        database
            .with_conn(|conn| {
                conn.execute("DELETE FROM versions WHERE id = ?1", [id])?;
                conn.execute(
                    "INSERT INTO versions
                     (file_id, version_no, content_hash, storage_path, kind, created_at,
                      vault_path, note_path)
                     VALUES (0, 'new-owner', 'hash', 'recycled/restart.md', 'manual',
                             datetime('now'), ?1, 'new-owner.md')",
                    [vault.to_string_lossy()],
                )?;
                Ok(())
            })
            .unwrap();
        let owner_count = database
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM versions
                     WHERE storage_path = 'recycled/restart.md' AND vault_path = ?1",
                    [vault.to_string_lossy()],
                    |row| row.get::<_, i64>(0),
                )?)
            })
            .unwrap();
        assert_eq!(owner_count, 1, "precondition: the new owner is durable");
        assert_eq!(
            staged_storage_paths(&checkpoint).unwrap(),
            vec!["recycled/restart.md"]
        );

        assert!(finish(&database, &checkpoint).is_err());
        assert!(!original.exists());
        assert_eq!(
            fs::read_to_string(checkpoint.directory.join("0.staged")).unwrap(),
            "recoverable snapshot"
        );
        assert!(checkpoint.directory.join("checkpoint.json").is_file());
    }

    #[test]
    fn restart_preserves_staging_when_a_new_storage_owner_exists_after_commit() {
        let (_directory, vault, database) = setup();
        let (id, original, checkpoint) = prepare_version_cleanup(&database, &vault);
        database
            .with_conn(|conn| {
                conn.execute("DELETE FROM versions WHERE id = ?1", [id])?;
                conn.execute(
                    "INSERT INTO versions
                     (file_id, version_no, content_hash, storage_path, kind, created_at,
                      vault_path, note_path)
                     VALUES (0, 'restart-new-owner', 'hash', 'recycled/restart.md', 'manual',
                             datetime('now'), ?1, 'new-owner.md')",
                    [vault.to_string_lossy()],
                )?;
                Ok(())
            })
            .unwrap();

        let unresolved = recover_pending(&database, &vault);

        assert_eq!(unresolved, vec![checkpoint.operation_id]);
        assert!(!original.exists());
        assert_eq!(
            fs::read_to_string(checkpoint.directory.join("0.staged")).unwrap(),
            "recoverable snapshot"
        );
        assert!(checkpoint.directory.join("checkpoint.json").is_file());
    }
}
