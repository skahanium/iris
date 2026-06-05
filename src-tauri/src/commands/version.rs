use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::app::AppState;
use crate::error::AppResult;
use crate::version::{self, VersionEntry};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionSaveCompletePayload {
    pub path: String,
    pub kind: String,
    pub created: bool,
    pub version_id: Option<i64>,
    pub skip_reason: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Copy)]
enum VersionSaveKind {
    Manual,
    Idle,
}

impl VersionSaveKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Idle => "auto_idle",
        }
    }

    fn run(
        self,
        state: &Arc<AppState>,
        path: &str,
        content: &str,
    ) -> AppResult<version::VersionSaveOutcome> {
        match self {
            Self::Manual => version::version_save_manual_outcome(state, path, content),
            Self::Idle => version::version_save_idle_outcome(state, path, content),
        }
    }
}

/// Queue version snapshot work off the IPC thread; result is emitted on `version:save_complete`.
fn spawn_version_save(
    app: AppHandle,
    state: Arc<AppState>,
    path: String,
    content: String,
    kind: VersionSaveKind,
) {
    let kind_str = kind.as_str().to_string();
    tauri::async_runtime::spawn(async move {
        let path_for_payload = path.clone();
        let result = tokio::task::spawn_blocking(move || kind.run(&state, &path, &content)).await;

        let payload = match result {
            Ok(Ok(outcome)) => VersionSaveCompletePayload {
                path: path_for_payload,
                kind: kind_str,
                created: outcome.entry.is_some(),
                version_id: outcome.entry.map(|entry| entry.id),
                skip_reason: outcome
                    .skip_reason
                    .map(|reason| reason.as_str().to_string()),
                error: None,
            },
            Ok(Err(e)) => VersionSaveCompletePayload {
                path: path_for_payload,
                kind: kind_str,
                created: false,
                version_id: None,
                skip_reason: None,
                error: Some(e.to_string()),
            },
            Err(e) => VersionSaveCompletePayload {
                path: path_for_payload,
                kind: kind_str,
                created: false,
                version_id: None,
                skip_reason: None,
                error: Some(format!("version save task failed: {e}")),
            },
        };
        let _ = app.emit("version:save_complete", &payload);
    });
}

#[derive(Debug, Clone, Serialize)]
pub struct VersionRestoreResult {
    pub content: String,
}

#[tauri::command]
pub fn version_list_cmd(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> AppResult<Vec<VersionEntry>> {
    version::version_list(&state, &path)
}

#[tauri::command]
pub fn version_preview_cmd(state: State<'_, Arc<AppState>>, version_id: i64) -> AppResult<String> {
    version::version_preview(&state, version_id)
}

#[tauri::command]
pub fn version_restore_cmd(
    state: State<'_, Arc<AppState>>,
    version_id: i64,
    current_content: String,
) -> AppResult<VersionRestoreResult> {
    let content = version::version_restore(&state, version_id, &current_content)?;
    Ok(VersionRestoreResult { content })
}

#[tauri::command]
pub fn version_delete_cmd(state: State<'_, Arc<AppState>>, version_id: i64) -> AppResult<()> {
    version::version_delete(&state, version_id)
}

#[tauri::command]
pub fn version_finalize_current_cmd(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
    label: Option<String>,
) -> AppResult<Option<VersionEntry>> {
    version::version_finalize_current(&state, &path, &content, label)
}

#[tauri::command]
pub fn version_cleanup_cmd(state: State<'_, Arc<AppState>>) -> AppResult<usize> {
    version::version_cleanup(&state)
}

/// Enqueue a manual snapshot; returns immediately. Listen for `version:save_complete`.
#[tauri::command]
pub fn version_save_manual_cmd(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
) -> AppResult<()> {
    spawn_version_save(
        app,
        state.inner().clone(),
        path,
        content,
        VersionSaveKind::Manual,
    );
    Ok(())
}

/// Enqueue an idle snapshot; returns immediately. Listen for `version:save_complete`.
#[tauri::command]
pub fn version_save_idle_cmd(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
) -> AppResult<()> {
    spawn_version_save(
        app,
        state.inner().clone(),
        path,
        content,
        VersionSaveKind::Idle,
    );
    Ok(())
}
