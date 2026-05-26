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

#[tauri::command]
pub fn version_save_manual_cmd(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
) -> AppResult<Option<VersionEntry>> {
    version::version_save_manual(&state, &path, &content)
}

#[tauri::command]
pub fn version_save_idle_cmd(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
) -> AppResult<Option<VersionEntry>> {
    version::version_save_idle(&state, &path, &content)
}
