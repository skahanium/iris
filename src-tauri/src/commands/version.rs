use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::app::AppState;
use crate::error::AppResult;
use crate::version::{self, VersionEntry};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionSaveResult {
    pub created: bool,
    pub version_id: Option<i64>,
    pub skip_reason: Option<String>,
}

#[derive(Clone, Copy)]
enum VersionSaveKind {
    Manual,
    Idle,
}

impl VersionSaveKind {
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

async fn run_version_save(
    state: Arc<AppState>,
    path: String,
    content: String,
    kind: VersionSaveKind,
) -> AppResult<VersionSaveResult> {
    let outcome = tokio::task::spawn_blocking(move || kind.run(&state, &path, &content))
        .await
        .map_err(|e| crate::error::AppError::msg(format!("version save task failed: {e}")))??;
    Ok(VersionSaveResult {
        created: outcome.entry.is_some(),
        version_id: outcome.entry.map(|entry| entry.id),
        skip_reason: outcome
            .skip_reason
            .map(|reason| reason.as_str().to_string()),
    })
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
    let content = version::version_restore(state.inner(), version_id, &current_content)?;
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

/// Creates a manual snapshot and returns its durable result.
#[tauri::command]
pub async fn version_save_manual_cmd(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
) -> AppResult<VersionSaveResult> {
    run_version_save(
        state.inner().clone(),
        path,
        content,
        VersionSaveKind::Manual,
    )
    .await
}

/// Creates an idle snapshot and returns its durable result.
#[tauri::command]
pub async fn version_save_idle_cmd(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
) -> AppResult<VersionSaveResult> {
    run_version_save(state.inner().clone(), path, content, VersionSaveKind::Idle).await
}
