//! Folder movement coordinates the same note protection and derived indexing
//! rules as single-note movement. No interaction or Agent state belongs here.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::indexer::scan::{collect_vault_files, index_file};
use crate::storage::atomic_write::with_vault_move_lock;
use crate::storage::move_journal::{MoveCheckpoint, MoveKind};
use crate::storage::note_move::{rewrite_wikilinks, BacklinkEdit};
use crate::storage::note_operations::protect_snapshot;
use crate::storage::note_write::{FileOperationReceipt, FileWriteIndexStatus, NoteWriteService};
use crate::storage::paths::{is_user_note_path, relative_path, resolve_vault_path};

/// A committed directory move and any incomplete backlink suffix are distinct
/// from derived-index degradation; callers must preserve both facts.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderMoveResult {
    pub index_status: FileWriteIndexStatus,
    pub operation: FileOperationReceipt,
}

pub(crate) fn validate_folder_path(path: &str) -> AppResult<()> {
    let path = path.trim_matches('/');
    if path.is_empty() || path.contains('\\') || !is_user_note_path(path) {
        return Err(AppError::msg("invalid_folder_path"));
    }
    for segment in path.split('/') {
        if segment.is_empty()
            || matches!(segment, "." | "..")
            || segment
                .chars()
                .any(|c| c.is_control() || ":*?\"<>|".contains(c))
        {
            return Err(AppError::msg("invalid_folder_path"));
        }
    }
    Ok(())
}

fn remap(path: &str, old: &str, new: &str) -> Option<String> {
    path.strip_prefix(old)
        .and_then(|suffix| suffix.strip_prefix('/'))
        .map(|suffix| format!("{new}/{suffix}"))
}

pub(crate) fn move_folder(
    state: &AppState,
    expected_vault: &Path,
    old: &str,
    new: &str,
) -> AppResult<FolderMoveResult> {
    let expected_vault = expected_vault
        .canonicalize()
        .map_err(|_| AppError::msg("note_vault_changed"))?;
    with_vault_move_lock(|| move_folder_locked(state, &expected_vault, old, new))
}

fn move_folder_locked(
    state: &AppState,
    vault: &Path,
    old: &str,
    new: &str,
) -> AppResult<FolderMoveResult> {
    if state.vault_path()? != vault {
        return Err(AppError::msg("note_vault_changed"));
    }
    validate_folder_path(old)?;
    validate_folder_path(new)?;
    let old = old.trim_matches('/');
    let new = new.trim_matches('/');
    let source = resolve_vault_path(vault, old)?;
    let destination = resolve_vault_path(vault, new)?;
    if !source.is_dir() || destination.exists() || destination.starts_with(&source) {
        return Err(AppError::msg("folder_move_target_conflict"));
    }

    let mut contents = BTreeMap::new();
    let mut mappings = BTreeMap::new();
    for absolute in collect_vault_files(vault) {
        let path = relative_path(vault, &absolute)?;
        if let Some(target) = remap(&path, old, new) {
            NoteWriteService::ensure_unlocked(state, &path)?;
            mappings.insert(path.clone(), target);
        }
        contents.insert(path, std::fs::read_to_string(absolute)?);
    }
    // Missing indexed files still own history. Move that identity without
    // materializing a phantom note or relying on the disposable files table.
    for path in state
        .db
        .with_read_conn(|conn| crate::version::repository::active_paths(conn, vault))?
    {
        if let Some(target) = remap(&path, old, new) {
            mappings.insert(path, target);
        }
    }
    state.db.with_read_conn(|conn| {
        for target in mappings.values() {
            crate::version::repository::ensure_destination_available(conn, vault, target)?;
        }
        Ok(())
    })?;
    let mut edits = Vec::new();
    for (path, before) in &contents {
        let mut after = before.clone();
        for (from, to) in &mappings {
            after = rewrite_wikilinks(&after, from, to);
        }
        if after != *before {
            NoteWriteService::ensure_unlocked(state, path)?;
            edits.push(BacklinkEdit {
                path: path.clone(),
                before: before.clone(),
                after,
            });
        }
    }
    let protected: BTreeSet<_> = mappings
        .keys()
        .filter(|path| contents.contains_key(*path))
        .chain(edits.iter().map(|edit| &edit.path))
        .cloned()
        .collect();
    let mut recovery_versions = Vec::new();
    for path in protected {
        recovery_versions.push((
            path.clone(),
            protect_snapshot(state, &path, &contents[&path])?,
        ));
    }
    let mut checkpoint = MoveCheckpoint::create(
        state,
        vault,
        MoveKind::Folder,
        (old, new),
        &mappings,
        &edits,
        &recovery_versions,
    )?;
    checkpoint.move_filesystem()?;
    if checkpoint.commit_identity(state).is_err() {
        return match checkpoint.rollback_filesystem() {
            Ok(()) => Err(AppError::msg("folder_move_identity_failed_source_restored")),
            Err(_) => Err(AppError::msg(
                "folder_move_identity_failed_recovery_required",
            )),
        };
    }

    let mut index_status = FileWriteIndexStatus::Synced;
    let mut applied_paths: Vec<_> = mappings
        .iter()
        .filter(|(from, _)| contents.contains_key(*from))
        .map(|(_, to)| to.clone())
        .collect();
    for (from, to) in &mappings {
        let Some(content) = contents.get(from) else {
            continue;
        };
        state.storage.write_guard.mark_removed(from);
        state
            .storage
            .write_guard
            .mark(to, &crate::cas::hash::content_hash_str(content));
        match resolve_vault_path(vault, to)
            .and_then(|target| state.db.with_conn(|conn| index_file(conn, vault, &target)))
        {
            Ok(_) => {}
            Err(_) => {
                index_status = FileWriteIndexStatus::Degraded;
                NoteWriteService::schedule_index_repair(state, to);
            }
        }
    }
    let backlink_outcome = checkpoint.apply_backlinks(state);
    let pending_paths = backlink_outcome.pending_paths;
    let recovery_warnings = backlink_outcome.recovery_warnings;
    for target in backlink_outcome.applied_paths {
        if !applied_paths.contains(&target) {
            applied_paths.push(target.clone());
        }
        match resolve_vault_path(vault, &target).and_then(|absolute| {
            state
                .db
                .with_conn(|conn| index_file(conn, vault, &absolute))
        }) {
            Ok(_) => {}
            Err(_) => {
                index_status = FileWriteIndexStatus::Degraded;
                NoteWriteService::schedule_index_repair(state, &target);
            }
        }
    }
    state.embedding_scheduler().notify_index_committed();
    Ok(FolderMoveResult {
        index_status,
        operation: FileOperationReceipt {
            previous_path: old.into(),
            applied_paths,
            pending_paths,
            recovery_versions,
            recovery_warnings,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (
        tempfile::TempDir,
        std::sync::Arc<AppState>,
        std::path::PathBuf,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(vault.join("old")).unwrap();
        std::fs::write(vault.join("old/note.md"), "original").unwrap();
        std::fs::write(vault.join("ref.md"), "See [[old/note.md]]").unwrap();
        let state = AppState::new(dir.path().join("data")).unwrap();
        state.set_vault(vault.clone()).unwrap();
        let vault = state.vault_path().unwrap();
        state
            .db
            .with_conn(|conn| {
                index_file(conn, &vault, &vault.join("old/note.md"))?;
                index_file(conn, &vault, &vault.join("ref.md"))?;
                Ok(())
            })
            .unwrap();
        (dir, state, vault)
    }

    #[test]
    fn locked_backlink_rejects_before_moving_any_file() {
        let (_dir, state, vault) = setup();
        state
            .db
            .with_conn(|conn| {
                conn.execute("UPDATE files SET is_locked = 1 WHERE path = 'ref.md'", [])?;
                Ok(())
            })
            .unwrap();
        assert!(move_folder(&state, &vault, "old", "new").is_err());
        assert!(vault.join("old/note.md").exists());
        assert!(!vault.join("new").exists());
        assert_eq!(
            std::fs::read_to_string(vault.join("ref.md")).unwrap(),
            "See [[old/note.md]]"
        );
    }

    #[test]
    fn snapshot_failure_is_not_a_successful_move_with_degraded_index() {
        let (_dir, state, vault) = setup();
        state.db.with_conn(|conn| {
            conn.execute_batch("CREATE TRIGGER fail_protection BEFORE INSERT ON versions BEGIN SELECT RAISE(ABORT, 'snapshot unavailable'); END;")?;
            Ok(())
        }).unwrap();
        assert!(move_folder(&state, &vault, "old", "new").is_err());
        assert!(vault.join("old/note.md").exists());
        assert!(!vault.join("new").exists());
    }

    #[test]
    fn identity_failure_restores_disk_and_keeps_original_history() {
        let (_dir, state, vault) = setup();
        let version = crate::version::version_save_manual(&state, "old/note.md", "historical")
            .unwrap()
            .unwrap();
        state.db.with_conn(|conn| {
            conn.execute_batch("CREATE TRIGGER fail_identity BEFORE UPDATE OF note_path ON versions BEGIN SELECT RAISE(ABORT, 'identity unavailable'); END;")?;
            Ok(())
        }).unwrap();
        assert!(move_folder(&state, &vault, "old", "new").is_err());
        assert!(vault.join("old/note.md").exists());
        assert!(!vault.join("new").exists());
        assert!(crate::version::version_list(&state, "old/note.md")
            .unwrap()
            .iter()
            .any(|entry| entry.id == version.id));
        assert_eq!(
            std::fs::read_to_string(vault.join("ref.md")).unwrap(),
            "See [[old/note.md]]"
        );
    }

    #[test]
    fn moved_folder_does_not_leave_old_search_paths() {
        let (_dir, state, vault) = setup();
        move_folder(&state, &vault, "old", "new").unwrap();
        state
            .db
            .with_read_conn(|conn| {
                let old_paths: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM files_fts WHERE path = 'old/note.md'",
                    [],
                    |row| row.get(0),
                )?;
                assert_eq!(old_paths, 0);
                let old_metadata: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM files_metadata_fts WHERE path = 'old/note.md'",
                    [],
                    |row| row.get(0),
                )?;
                assert_eq!(old_metadata, 0);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn queued_folder_move_cannot_apply_in_another_vault() {
        let (dir, state, vault) = setup();
        let other = dir.path().join("other");
        std::fs::create_dir_all(other.join("old")).unwrap();
        std::fs::write(other.join("old/note.md"), "unrelated").unwrap();
        state.set_vault(other.clone()).unwrap();
        assert!(move_folder(&state, &vault, "old", "new").is_err());
        assert!(vault.join("old/note.md").exists());
        assert_eq!(
            std::fs::read_to_string(other.join("old/note.md")).unwrap(),
            "unrelated"
        );
    }

    #[cfg(unix)]
    #[test]
    fn missing_target_below_alias_into_source_is_rejected_without_side_effects() {
        use std::os::unix::fs::symlink;

        let (_dir, state, vault) = setup();
        std::fs::create_dir_all(vault.join("old/sub")).unwrap();
        symlink(vault.join("old/sub"), vault.join("alias")).unwrap();

        let error = move_folder(&state, &vault, "old", "alias/new")
            .expect_err("canonical destination is inside source");

        assert!(error.to_string().contains("target_conflict"));
        assert_eq!(
            std::fs::read_to_string(vault.join("old/note.md")).unwrap(),
            "original"
        );
        assert!(!vault.join("old/sub/new").exists());
        assert_eq!(
            std::fs::read_dir(vault.join(".iris/operations/moves"))
                .map(|entries| entries.count())
                .unwrap_or(0),
            0
        );
    }
}
