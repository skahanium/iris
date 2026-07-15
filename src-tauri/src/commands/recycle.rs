use std::sync::Arc;

use tauri::State;

use crate::app::AppState;
use crate::error::AppResult;
use crate::recycle::{list_recycle, purge_recycle_item, restore_document, RecycleBinItem};
use crate::storage::note_write::FileWriteResult;

#[tauri::command]
pub fn recycle_list_cmd(state: State<'_, Arc<AppState>>) -> AppResult<Vec<RecycleBinItem>> {
    list_recycle(state.inner())
}

#[tauri::command]
pub fn recycle_restore_cmd(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> AppResult<FileWriteResult> {
    restore_document(state.inner(), &id)
}

#[tauri::command]
pub fn recycle_purge_cmd(state: State<'_, Arc<AppState>>, id: String) -> AppResult<()> {
    purge_recycle_item(state.inner(), &id)
}
