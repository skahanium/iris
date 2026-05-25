use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::app::AppState;
use crate::error::AppResult;
use crate::version::{self, VersionEntry};

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
pub fn version_finalize_cmd(
    state: State<'_, Arc<AppState>>,
    version_id: i64,
    label: Option<String>,
) -> AppResult<()> {
    version::version_finalize(&state, version_id, label)
}

#[tauri::command]
pub fn version_cleanup_cmd(state: State<'_, Arc<AppState>>) -> AppResult<usize> {
    version::version_cleanup(&state)
}
