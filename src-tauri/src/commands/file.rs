use std::fs;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::indexer::scan::{index_file, remove_file_index, scan_vault, FileEntry};
use crate::storage::paths::resolve_vault_path;
use crate::version;

#[derive(Debug, Clone, Serialize)]
pub struct FileListItem {
    pub path: String,
    pub title: String,
    pub updated_at: String,
}

#[tauri::command]
pub fn file_list(state: State<'_, Arc<AppState>>) -> AppResult<Vec<FileListItem>> {
    state.db.with_conn(|conn| {
        let mut stmt =
            conn.prepare("SELECT path, title, updated_at FROM files ORDER BY updated_at DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok(FileListItem {
                path: row.get(0)?,
                title: row.get(1)?,
                updated_at: row.get(2)?,
            })
        })?;
        Ok(rows.flatten().collect())
    })
}

#[tauri::command]
pub fn file_read(state: State<'_, Arc<AppState>>, path: String) -> AppResult<String> {
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &path)?;
    Ok(fs::read_to_string(abs)?)
}

#[tauri::command]
pub fn file_write(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
) -> AppResult<FileEntry> {
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &path)?;

    let tmp = abs.with_extension("md.tmp");
    fs::write(&tmp, &content)?;
    fs::rename(&tmp, &abs)?;

    let entry = state.db.with_conn(|conn| index_file(conn, &vault, &abs))?;

    // Auto-snapshot after save
    let _ = version::create_snapshot(&state, &path, &content);

    Ok(entry)
}

#[tauri::command]
pub fn file_delete(state: State<'_, Arc<AppState>>, path: String) -> AppResult<()> {
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &path)?;
    fs::remove_file(&abs)?;
    state.db.with_conn(|conn| remove_file_index(conn, &path))
}

#[tauri::command]
pub fn file_rename(
    state: State<'_, Arc<AppState>>,
    path: String,
    new_path: String,
) -> AppResult<FileEntry> {
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &path)?;
    let new_abs = resolve_vault_path(&vault, &new_path)?;
    if new_abs.exists() {
        return Err(AppError::msg("Target path already exists"));
    }
    if let Some(parent) = new_abs.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&abs, &new_abs)?;
    state.db.with_conn(|conn| {
        remove_file_index(conn, &path)?;
        index_file(conn, &vault, &new_abs)
    })
}

#[tauri::command]
pub fn file_create(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: Option<String>,
) -> AppResult<FileEntry> {
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &path)?;
    if abs.exists() {
        return Err(AppError::msg("File already exists"));
    }
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = content.unwrap_or_else(|| format!("# {}\n\n", path.trim_end_matches(".md")));
    fs::write(&abs, &body)?;
    state.db.with_conn(|conn| index_file(conn, &vault, &abs))
}

#[tauri::command]
pub fn vault_set(app: AppHandle, state: State<'_, Arc<AppState>>, path: String) -> AppResult<()> {
    use std::path::PathBuf;

    let p = PathBuf::from(&path);
    if !p.is_dir() {
        return Err(AppError::msg("Vault path must be an existing directory"));
    }
    state.set_vault(p)?;
    let vault = state.vault_path()?;
    state.db.with_conn(|conn| scan_vault(conn, &vault))?;
    state.restart_file_watcher(app)?;
    Ok(())
}

#[tauri::command]
pub fn vault_get(state: State<'_, Arc<AppState>>) -> AppResult<Option<String>> {
    Ok(state
        .vault_path()
        .ok()
        .map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
pub fn index_rescan(state: State<'_, Arc<AppState>>) -> AppResult<Vec<FileEntry>> {
    let vault = state.vault_path()?;
    state.db.with_conn(|conn| scan_vault(conn, &vault))
}

#[derive(Debug, Clone, Serialize)]
pub struct BacklinkEntry {
    pub source_path: String,
    pub source_title: String,
    pub context: Option<String>,
}

#[tauri::command]
pub fn file_backlinks(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> AppResult<Vec<BacklinkEntry>> {
    state.db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT f.path, f.title, l.context
             FROM links l
             JOIN files f ON f.id = l.source_id
             JOIN files t ON t.id = l.target_id
             WHERE t.path = ?1
             ORDER BY f.title",
        )?;
        let rows = stmt.query_map([&path], |row| {
            Ok(BacklinkEntry {
                source_path: row.get(0)?,
                source_title: row.get(1)?,
                context: row.get(2)?,
            })
        })?;
        Ok(rows.flatten().collect())
    })
}
