use std::fs;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::indexer::frontmatter::{parse_note, resolve_display_title};
use crate::indexer::scan::{
    index_file, prune_stale_file_indexes, remove_file_index, scan_vault, FileEntry,
};
use crate::recycle::{discard_document, trash_document};
use crate::storage::paths::resolve_vault_path;

fn title_from_path(path: &str) -> String {
    path.trim_end_matches(".md")
        .split('/')
        .next_back()
        .unwrap_or(path)
        .to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct FileListItem {
    pub path: String,
    pub title: String,
    pub updated_at: String,
}

/// 列出受追踪的用户笔记（每篇文档一条，不含版本快照）。
/// `title` 为创建时确定的文档名；版本历史见 `version_list_cmd`。
#[tauri::command]
pub fn file_list(state: State<'_, Arc<AppState>>) -> AppResult<Vec<FileListItem>> {
    let vault = state.vault_path()?;
    state.db.with_conn(|conn| {
        prune_stale_file_indexes(conn, &vault)?;
        let mut stmt = conn.prepare(
            "SELECT path, title, updated_at, frontmatter FROM files
             WHERE id IN (SELECT MAX(id) FROM files GROUP BY path)
               AND path NOT LIKE '.iris/%'
             ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let path: String = row.get(0)?;
            let stored_title: String = row.get(1)?;
            let updated_at: String = row.get(2)?;
            let frontmatter: Option<String> = row.get(3)?;
            let stem = title_from_path(&path);
            let title = resolve_display_title(None, &stored_title, frontmatter.as_deref(), &stem);
            Ok(FileListItem {
                path,
                title,
                updated_at,
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

    let wc = content.split_whitespace().count() as i64;
    let parsed = parse_note(&content)?;
    let stem = title_from_path(&path);
    let title = resolve_display_title(
        parsed.title.as_deref(),
        "",
        parsed.frontmatter_json.as_deref(),
        &stem,
    );
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();

    let entry = FileEntry {
        id: 0,
        path: path.clone(),
        title,
        updated_at: now,
        word_count: wc,
    };

    let state_clone = state.inner().clone();
    let abs_clone = abs.clone();
    let vault_clone = vault.clone();
    std::thread::spawn(move || {
        let _ = state_clone
            .db
            .with_conn(|conn| index_file(conn, &vault_clone, &abs_clone));
    });

    Ok(entry)
}

/// Move note + all version snapshots into recycle bin (15-day retention).
#[tauri::command]
pub fn file_delete(state: State<'_, Arc<AppState>>, path: String) -> AppResult<()> {
    trash_document(state.inner(), &path)
}

/// Permanently remove a blank note (no recycle bin).
#[tauri::command]
pub fn file_discard(state: State<'_, Arc<AppState>>, path: String) -> AppResult<()> {
    discard_document(state.inner(), &path)
}

/// Create a folder under the vault. Fails if the folder already exists.
#[tauri::command]
pub fn folder_create(state: State<'_, Arc<AppState>>, path: String) -> AppResult<()> {
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &path)?;
    if abs.exists() {
        return Err(AppError::msg("Folder already exists"));
    }
    fs::create_dir_all(&abs)?;
    Ok(())
}

/// Rename/move a folder. Fails if the target already exists.
#[tauri::command]
pub fn folder_rename(
    state: State<'_, Arc<AppState>>,
    old_path: String,
    new_path: String,
) -> AppResult<()> {
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &old_path)?;
    let new_abs = resolve_vault_path(&vault, &new_path)?;
    if !abs.is_dir() {
        return Err(AppError::msg("Source path is not a folder"));
    }
    if new_abs.exists() {
        return Err(AppError::msg("Target path already exists"));
    }
    if let Some(parent) = new_abs.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&abs, &new_abs)?;
    // Re-index all files under the renamed folder
    let vault_clone = vault.clone();
    state.db.with_conn(|conn| scan_vault(conn, &vault_clone))?;
    Ok(())
}

/// Delete an empty folder. Fails if the folder is not empty.
#[tauri::command]
pub fn folder_delete(state: State<'_, Arc<AppState>>, path: String) -> AppResult<()> {
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &path)?;
    if !abs.is_dir() {
        return Err(AppError::msg("Path is not a folder"));
    }
    // Check if folder is empty
    let entries: Vec<_> = fs::read_dir(&abs)?.filter_map(|e| e.ok()).collect();
    if !entries.is_empty() {
        return Err(AppError::msg("Folder is not empty"));
    }
    fs::remove_dir(&abs)?;
    Ok(())
}

/// 标题→路径同步建议（spec §7.1）。
#[derive(Debug, Clone, Serialize)]
pub struct PathSyncSuggest {
    pub current_path: String,
    pub suggested_path: String,
    pub needs_sync: bool,
    pub conflict_resolved: bool,
}

const PLACEHOLDER_TITLE_MARKERS: &[&str] = &["新建文档", "无标题", "untitled"];

fn is_placeholder_title(title: &str) -> bool {
    let t = title.trim();
    if t.is_empty() {
        return true;
    }
    let lower = t.to_lowercase();
    PLACEHOLDER_TITLE_MARKERS.iter().any(|p| lower.contains(p))
}

fn sanitize_title_for_path(title: &str) -> String {
    const INVALID: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];
    let mut s = title.trim().to_string();
    for c in INVALID {
        s = s.replace(*c, "_");
    }
    if s.is_empty() {
        "新建文档".to_string()
    } else {
        s
    }
}

fn allocate_path_for_title(
    parent: &str,
    title: &str,
    exclude_path: &str,
    conn: &rusqlite::Connection,
) -> AppResult<(String, bool)> {
    let base = sanitize_title_for_path(title);
    let mut candidate = if parent.is_empty() {
        format!("{base}.md")
    } else {
        format!("{parent}/{base}.md")
    };
    let mut conflict_resolved = false;

    let exists = |p: &str| -> AppResult<bool> {
        if p == exclude_path {
            return Ok(false);
        }
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM files WHERE path = ?1", [p], |r| {
            r.get(0)
        })?;
        Ok(n > 0)
    };

    if !exists(&candidate)? {
        return Ok((candidate, conflict_resolved));
    }

    conflict_resolved = true;
    let mut n = 1u32;
    loop {
        let titled = format!("{base}（{n}）");
        candidate = if parent.is_empty() {
            format!("{titled}.md")
        } else {
            format!("{parent}/{titled}.md")
        };
        if !exists(&candidate)? {
            return Ok((candidate, conflict_resolved));
        }
        n += 1;
        if n > 500 {
            return Err(AppError::msg("无法分配不冲突的路径"));
        }
    }
}

/// 根据显示标题建议人类可读路径（处理冲突）。
pub fn path_sync_suggest_inner(
    state: &AppState,
    current_path: String,
    title: String,
) -> AppResult<PathSyncSuggest> {
    if is_placeholder_title(&title) {
        return Ok(PathSyncSuggest {
            current_path: current_path.clone(),
            suggested_path: current_path,
            needs_sync: false,
            conflict_resolved: false,
        });
    }

    let parent = current_path
        .rsplit_once('/')
        .map(|(p, _)| p)
        .unwrap_or("")
        .to_string();

    let (suggested_path, conflict_resolved) = state
        .db
        .with_conn(|conn| allocate_path_for_title(&parent, &title, &current_path, conn))?;

    let current_stem = title_from_path(&current_path);
    let target_stem = sanitize_title_for_path(&title);
    let needs_sync = current_stem != target_stem && suggested_path != current_path;

    Ok(PathSyncSuggest {
        current_path,
        suggested_path,
        needs_sync,
        conflict_resolved,
    })
}

#[tauri::command]
pub fn path_sync_suggest(
    state: State<'_, Arc<AppState>>,
    current_path: String,
    title: String,
) -> AppResult<PathSyncSuggest> {
    path_sync_suggest_inner(&state, current_path, title)
}

#[cfg(test)]
mod path_sync_tests {
    use super::*;

    #[test]
    fn placeholder_skips_sync() {
        assert!(is_placeholder_title("新建文档"));
        assert!(!is_placeholder_title("民法总则笔记"));
    }

    #[test]
    fn sanitize_strips_invalid_chars() {
        assert_eq!(sanitize_title_for_path("a/b"), "a_b");
    }
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
    let document_title = abs
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| title_from_path(&path));
    let body = content.unwrap_or_else(|| format!("# {document_title}\n\n"));
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
