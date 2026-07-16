use std::sync::Arc;
use std::time::Duration;

use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use tauri::{AppHandle, Emitter};
use tracing::info;

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::indexer::scan::{
    content_hash, index_file_from_content, index_workspace_media_file, remove_file_index,
    remove_workspace_media_index, workspace_media_kind_for_path,
};
use crate::storage::paths::{is_user_note_path, relative_path};

/// Returns whether the file watcher should re-index or emit for a vault-relative path.
pub(crate) fn watcher_should_index_path(relative: &str) -> bool {
    is_user_note_path(relative)
}

fn event_relative_path(vault: &std::path::Path, path: &std::path::Path) -> AppResult<String> {
    if path.exists() {
        return relative_path(vault, path);
    }
    let rel = path
        .strip_prefix(vault)
        .map_err(|_| AppError::msg("Path is outside the vault"))?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

/// 持有 debouncer；Drop 时自动取消目录监听。切换 vault 时需 drop 后重建。
pub struct FileWatcher {
    _debouncer: Debouncer<notify::RecommendedWatcher, RecommendedCache>,
}

fn event_kind_label(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::Create(_) => "create",
        EventKind::Modify(_) => "modify",
        EventKind::Remove(_) => "remove",
        EventKind::Any => "any",
        EventKind::Other => "other",
        EventKind::Access(_) => "access",
    }
}

impl FileWatcher {
    pub fn start(app: AppHandle, state: Arc<AppState>) -> AppResult<Self> {
        let vault = state.vault_path()?;
        if !vault.exists() {
            return Err(crate::error::AppError::msg("Vault not configured"));
        }

        let app_clone = app.clone();
        let state_clone = state.clone();

        let mut debouncer = new_debouncer(
            Duration::from_millis(500),
            None,
            move |result: DebounceEventResult| {
                let Ok(events) = result else { return };
                for event in events {
                    if matches!(event.kind, EventKind::Access(_)) {
                        continue;
                    }
                    let kind = event_kind_label(&event.kind);
                    for path in &event.paths {
                        let is_note = path.extension().is_some_and(|e| e == "md");
                        let is_media = workspace_media_kind_for_path(path).is_some();
                        if is_note || is_media {
                            if is_note {
                                state_clone.clear_context_cache();
                            }
                            let Ok(vault) = state_clone.vault_path() else {
                                continue;
                            };
                            let Ok(rel) = event_relative_path(&vault, path) else {
                                continue;
                            };
                            if is_note && !watcher_should_index_path(&rel) {
                                tracing::debug!(
                                    path = %rel,
                                    "skipping classified/internal path in watcher"
                                );
                                continue;
                            }
                            let result = if is_note {
                                handle_file_event(&app_clone, &state_clone, path, kind)
                            } else {
                                handle_workspace_media_event(&app_clone, &state_clone, path, kind)
                            };
                            match result {
                                Ok(()) => {}
                                Err(e) => {
                                    tracing::warn!("File event error for {}: {e}", path.display())
                                }
                            };
                        }
                    }
                }
            },
        )
        .map_err(|e| crate::error::AppError::msg(format!("Watcher error: {e}")))?;

        debouncer
            .watch(&vault, RecursiveMode::Recursive)
            .map_err(|e| crate::error::AppError::msg(format!("Watch failed: {e}")))?;

        Ok(Self {
            _debouncer: debouncer,
        })
    }
}

fn handle_file_event(
    app: &AppHandle,
    state: &Arc<AppState>,
    path: &std::path::Path,
    event_type: &str,
) -> AppResult<()> {
    let vault = state.vault_path()?;
    if !path.exists() {
        if let Ok(rel) = event_relative_path(&vault, path) {
            state.db.with_conn(|conn| remove_file_index(conn, &rel))?;
            let _ = app.emit(
                "file:changed",
                serde_json::json!({
                    "path": rel,
                    "event_type": "removed",
                }),
            );
        }
        return Ok(());
    }

    let content = std::fs::read_to_string(path)?;
    let hash = content_hash(&content);
    let rel = event_relative_path(&vault, path)?;
    if state.storage.write_guard.should_skip_watcher(&rel, &hash) {
        tracing::debug!(path = %rel, "watcher skipped: recent app write");
        return Ok(());
    }
    state
        .db
        .with_conn(|conn| index_file_from_content(conn, &vault, path, &content, &hash))?;
    state.embedding_scheduler().notify_index_committed();

    info!(
        path = %path.display(),
        event_type = %event_type,
        "File change detected and processed"
    );

    let _ = app.emit(
        "file:changed",
        serde_json::json!({
            "path": rel,
            "hash": hash,
            "event_type": event_type,
        }),
    );
    Ok(())
}

fn handle_workspace_media_event(
    app: &AppHandle,
    state: &Arc<AppState>,
    path: &std::path::Path,
    event_type: &str,
) -> AppResult<()> {
    let vault = state.vault_path()?;
    let rel = event_relative_path(&vault, path)?;
    if !path.exists() {
        state
            .db
            .with_conn(|conn| remove_workspace_media_index(conn, &rel))?;
        let _ = app.emit(
            "file:changed",
            serde_json::json!({
                "path": rel,
                "event_type": "removed",
            }),
        );
        return Ok(());
    }

    state.db.with_conn(|conn| {
        let _ = index_workspace_media_file(conn, &vault, path)?;
        Ok(())
    })?;
    let _ = app.emit(
        "file:changed",
        serde_json::json!({
            "path": rel,
            "event_type": event_type,
        }),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watcher_skips_classified_and_iris_paths() {
        assert!(!watcher_should_index_path(".classified"));
        assert!(!watcher_should_index_path(".classified/secret.md"));
        assert!(!watcher_should_index_path(".iris/versions/1/snap.md"));
        assert!(watcher_should_index_path("notes/readme.md"));
        assert!(watcher_should_index_path("readme.md"));
    }
}
