use std::fs;
use std::sync::Arc;

use tauri::State;

use crate::app::AppState;
use crate::error::AppResult;

#[tauri::command]
pub fn export_file(
    _state: State<'_, Arc<AppState>>,
    dest_path: String,
    content: String,
) -> AppResult<()> {
    let dest = std::path::PathBuf::from(&dest_path);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&dest, &content)?;
    Ok(())
}
