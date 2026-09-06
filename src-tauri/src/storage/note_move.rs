//! Shared move/backlink operation. Preview and execution consume the same bytes.
use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::indexer::scan::{collect_vault_files, content_hash, index_file, rename_file_index};
use crate::storage::atomic_write::move_file_no_replace_locked;
use crate::storage::note_operations::protect_snapshot;
use crate::storage::note_title::title_from_path;
use crate::storage::note_write::{
    noop_write_receipt, FileWriteIndexStatus, FileWriteResult, NoteWriteService,
};
use crate::storage::paths::{relative_path, validate_user_note_relative_path};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BacklinkEdit {
    pub path: String,
    pub before: String,
    pub after: String,
}

/// Frozen impact, containing exact original and replacement Markdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NoteMovePlan {
    pub path: String,
    pub new_path: String,
    pub source_hash: String,
    pub backlinks: Vec<BacklinkEdit>,
}

/// A partial move remains a move; unapplied backlinks are explicitly reported.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NoteMoveReceipt {
    pub write: FileWriteResult,
    pub previous_path: String,
    pub applied_paths: Vec<String>,
    pub pending_paths: Vec<String>,
    pub recovery_versions: Vec<(String, i64)>,
}

/// Compute the actual on-disk impact, not only potentially stale index links.
pub(crate) fn prepare_move(
    state: &AppState,
    path: &str,
    new_path: &str,
) -> AppResult<NoteMovePlan> {
    if !path.ends_with(".md") || !new_path.ends_with(".md") {
        return Err(AppError::msg("note_move_requires_markdown_paths"));
    }
    let vault = state.vault_path()?;
    let source = validate_user_note_relative_path(&vault, path)?;
    let target = validate_user_note_relative_path(&vault, new_path)?;
    if path == new_path || target.exists() {
        return Err(AppError::msg("file_already_exists"));
    }
    NoteWriteService::ensure_unlocked(state, path)?;
    let content = std::fs::read_to_string(source)?;
    let mut backlinks = Vec::new();
    for absolute in collect_vault_files(&vault) {
        let source_path = relative_path(&vault, &absolute)?;
        let before = std::fs::read_to_string(absolute)?;
        let after = rewrite_wikilinks(&before, path, new_path);
        if before != after {
            NoteWriteService::ensure_unlocked(state, &source_path)?;
            backlinks.push(BacklinkEdit {
                path: source_path,
                before,
                after,
            });
        }
    }
    backlinks.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(NoteMovePlan {
        path: path.into(),
        new_path: new_path.into(),
        source_hash: content_hash(&content),
        backlinks,
    })
}

/// Execute with the caller holding the common vault operation guard.
/// All preconditions and snapshots precede the first Markdown side effect.
pub(crate) fn execute_move_locked(
    state: &AppState,
    plan: &NoteMovePlan,
) -> AppResult<NoteMoveReceipt> {
    let vault = state.vault_path()?;
    let fresh = prepare_move(state, &plan.path, &plan.new_path)?;
    if serde_json::to_value(&fresh)? != serde_json::to_value(plan)? {
        return Err(AppError::msg("note_move_preview_conflict"));
    }
    let source = validate_user_note_relative_path(&vault, &plan.path)?;
    let target = validate_user_note_relative_path(&vault, &plan.new_path)?;
    let content = std::fs::read_to_string(&source)?;
    // Identity migration is a hard boundary, not a disposable content-index
    // refresh. Ensure the source has one stable row and reject a destination
    // that owns history before the first filesystem side effect.
    state.db.with_conn(|conn| {
        if lookup_indexed_file_id(conn, &plan.path)?.is_none() {
            index_file(conn, &vault, &source)?;
        }
        ensure_destination_identity_available(conn, &plan.new_path)
    })?;
    let mut versions = vec![(
        plan.path.clone(),
        protect_snapshot(state, &plan.path, &content)?,
    )];
    for edit in &plan.backlinks {
        versions.push((
            edit.path.clone(),
            protect_snapshot(state, &edit.path, &edit.before)?,
        ));
    }
    move_file_no_replace_locked(&source, &target)?;
    let identity_result = state.db.with_conn(|conn| {
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            ensure_destination_identity_available(conn, &plan.new_path)?;
            conn.execute(
                "DELETE FROM files
                 WHERE path = ?1
                   AND NOT EXISTS (SELECT 1 FROM versions WHERE file_id = files.id)",
                [&plan.new_path],
            )?;
            rename_file_index(conn, &plan.path, &plan.new_path)
        })();
        match result {
            Ok(file_id) => {
                if let Err(error) = conn.execute_batch("COMMIT") {
                    let _ = conn.execute_batch("ROLLBACK");
                    return Err(error.into());
                }
                Ok(file_id)
            }
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    });
    if let Err(identity_error) = identity_result {
        return match move_file_no_replace_locked(&target, &source) {
            Ok(()) => Err(AppError::msg(format!(
                "note_move_identity_migration_failed: {identity_error}"
            ))),
            Err(compensation_error) => Err(AppError::msg(format!(
                "note_move_identity_migration_failed_and_disk_compensation_failed: {identity_error}; compensation: {compensation_error}"
            ))),
        };
    }
    state.storage.write_guard.mark_removed(&plan.path);
    state
        .storage
        .write_guard
        .mark(&plan.new_path, &plan.source_hash);
    let mut write = noop_write_receipt(&plan.new_path, &content);
    // Only the content-derived refresh is degradable after identity committed.
    let indexed = state.db.with_conn(|conn| index_file(conn, &vault, &target));
    match indexed {
        Ok(entry) => write.entry = entry,
        Err(_) => write.index_status = FileWriteIndexStatus::Degraded,
    }
    let mut applied_paths = vec![plan.new_path.clone()];
    let mut pending_paths = Vec::new();
    for (index, edit) in plan.backlinks.iter().enumerate() {
        let path = if edit.path == plan.path {
            &plan.new_path
        } else {
            &edit.path
        };
        match NoteWriteService::write_under_move_lock(state, path, &edit.after) {
            Ok(receipt) => {
                if path == &plan.new_path {
                    write.content_hash = receipt.content_hash;
                }
                if receipt.index_status == FileWriteIndexStatus::Degraded {
                    write.index_status = FileWriteIndexStatus::Degraded;
                }
                if !applied_paths.contains(path) {
                    applied_paths.push(path.clone());
                }
            }
            Err(_) => {
                pending_paths.extend(plan.backlinks[index..].iter().map(|edit| edit.path.clone()));
                break;
            }
        }
    }
    state.embedding_scheduler().notify_index_committed();
    write.operation = Some(crate::storage::note_write::FileOperationReceipt {
        previous_path: plan.path.clone(),
        applied_paths: applied_paths.clone(),
        pending_paths: pending_paths.clone(),
        recovery_versions: versions.clone(),
    });
    Ok(NoteMoveReceipt {
        write,
        previous_path: plan.path.clone(),
        applied_paths,
        pending_paths,
        recovery_versions: versions,
    })
}

fn lookup_indexed_file_id(conn: &rusqlite::Connection, path: &str) -> AppResult<Option<i64>> {
    use rusqlite::OptionalExtension;

    conn.query_row("SELECT id FROM files WHERE path = ?1", [path], |row| {
        row.get(0)
    })
    .optional()
    .map_err(Into::into)
}

fn ensure_destination_identity_available(
    conn: &rusqlite::Connection,
    new_path: &str,
) -> AppResult<()> {
    let target_has_history = conn.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM versions v
             JOIN files f ON f.id = v.file_id
             WHERE f.path = ?1
         )",
        [new_path],
        |row| row.get::<_, bool>(0),
    )?;
    if target_has_history {
        return Err(AppError::msg("note_destination_history_conflict"));
    }
    Ok(())
}

/// Replace link targets only, preserving whitespace, fences, frontmatter and code.
pub(crate) fn rewrite_wikilinks(content: &str, old_path: &str, new_path: &str) -> String {
    let old_stem = title_from_path(old_path);
    let new_stem = title_from_path(new_path);
    let mut fence = crate::indexer::code_fence::FenceState::new();
    let mut frontmatter = content.starts_with("---\n") || content.starts_with("---\r\n");
    let mut output = String::with_capacity(content.len());
    for (index, line) in content.split_inclusive('\n').enumerate() {
        if frontmatter {
            output.push_str(line);
            if index > 0 && matches!(line.trim(), "---" | "...") {
                frontmatter = false;
            }
            continue;
        }
        if fence.feed(line.trim_end_matches(['\r', '\n'])) {
            output.push_str(line);
            continue;
        }
        let mut cursor = 0;
        for (start, _) in line.match_indices("[[") {
            if start < cursor
                || crate::indexer::code_fence::FenceState::is_inside_inline_code_or_comment(
                    line, start,
                )
            {
                continue;
            }
            let Some(end) = line[start + 2..].find("]]").map(|end| start + 2 + end) else {
                continue;
            };
            let target = &line[start + 2..end];
            let replacement = if target == old_path {
                Some(new_path)
            } else if target == old_stem {
                Some(new_stem.as_str())
            } else if old_path.strip_suffix(".md") == Some(target) {
                Some(new_path.strip_suffix(".md").unwrap_or(new_path))
            } else {
                None
            };
            if let Some(replacement) = replacement {
                output.push_str(&line[cursor..start + 2]);
                output.push_str(replacement);
                cursor = end;
            }
        }
        output.push_str(&line[cursor..]);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::version_save_manual;

    #[test]
    fn note_move_rejects_a_non_markdown_destination_before_snapshots() {
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(vault.join("note.md"), "original").unwrap();
        let state = AppState::new(directory.path().join("data")).unwrap();
        state.set_vault(vault.clone()).unwrap();
        assert!(prepare_move(&state, "note.md", "note.txt").is_err());
        assert!(vault.join("note.md").exists());
        assert!(crate::version::version_list(&state, "note.md")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn identity_migration_failure_restores_disk_and_preserves_version_binding() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let vault = directory.path().join("vault");
        std::fs::create_dir_all(&vault).expect("vault directory");
        std::fs::write(vault.join("old.md"), "# Old\nbody\n").expect("source note");
        let state = AppState::new(directory.path().join("data")).expect("state");
        state.set_vault(vault.clone()).expect("activate vault");
        let original_file_id = state
            .db
            .with_conn(|connection| index_file(connection, &vault, &vault.join("old.md")))
            .expect("index source")
            .id;
        let version = version_save_manual(&state, "old.md", "# Old\nsnapshot\n")
            .expect("save version")
            .expect("version entry");
        state
            .db
            .with_conn(|connection| {
                connection.execute_batch(
                    "CREATE TRIGGER fail_note_identity_move
                     BEFORE UPDATE OF path ON files
                     WHEN OLD.path = 'old.md'
                     BEGIN
                       SELECT RAISE(ABORT, 'simulated identity migration failure');
                     END;",
                )?;
                Ok(())
            })
            .expect("install fault");

        let plan = prepare_move(&state, "old.md", "new.md").expect("prepare move");
        let error =
            execute_move_locked(&state, &plan).expect_err("identity failure must abort move");

        assert!(error.to_string().contains("identity"));
        assert!(vault.join("old.md").is_file());
        assert!(!vault.join("new.md").exists());
        state
            .db
            .with_read_conn(|connection| {
                let current_file_id: i64 = connection.query_row(
                    "SELECT id FROM files WHERE path = 'old.md'",
                    [],
                    |row| row.get(0),
                )?;
                let new_rows: i64 = connection.query_row(
                    "SELECT COUNT(*) FROM files WHERE path = 'new.md'",
                    [],
                    |row| row.get(0),
                )?;
                let version_file_id: i64 = connection.query_row(
                    "SELECT file_id FROM versions WHERE id = ?1",
                    [version.id],
                    |row| row.get(0),
                )?;
                assert_eq!(current_file_id, original_file_id);
                assert_eq!(version_file_id, original_file_id);
                assert_eq!(new_rows, 0);
                Ok(())
            })
            .expect("verify identity");
    }
}
