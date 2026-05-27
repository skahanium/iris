use std::sync::Arc;
use std::time::Duration;

use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, FileIdMap};
use tauri::{AppHandle, Emitter};

use crate::app::AppState;
use crate::error::AppResult;
use crate::indexer::scan::{file_hash, index_file, remove_file_index};
use crate::storage::paths::{is_user_note_path, relative_path};

/// 持有 debouncer；Drop 时自动取消目录监听。切换 vault 时需 drop 后重建。
pub struct FileWatcher {
    _debouncer: Debouncer<notify::RecommendedWatcher, FileIdMap>,
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
                    let kind = event_kind_label(&event.kind);
                    for path in &event.paths {
                        if path.extension().is_some_and(|e| e == "md") {
                            let Ok(vault) = state_clone.vault_path() else {
                                continue;
                            };
                            let Ok(rel) = relative_path(&vault, path) else {
                                continue;
                            };
                            if !is_user_note_path(&rel) {
                                continue;
                            }
                            match handle_file_event(&app_clone, &state_clone, path, kind) {
                                Ok(()) => {},
                                Err(e) => tracing::warn!("File event error for {}: {e}", path.display()),
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
    state: &AppState,
    path: &std::path::Path,
    event_type: &str,
) -> AppResult<()> {
    let vault = state.vault_path()?;
    if !path.exists() {
        if let Ok(rel) = relative_path(&vault, path) {
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

    let hash = file_hash(path)?;
    let rel = relative_path(&vault, path)?;
    state.db.with_conn(|conn| index_file(conn, &vault, path))?;

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
