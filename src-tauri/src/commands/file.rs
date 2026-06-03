use std::fs;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::indexer::frontmatter::resolve_display_title;
use crate::indexer::scan::{
    collect_vault_folders, index_file_with_embed, index_vault_incremental,
    peek_file_entry_after_write_fast, prune_stale_file_indexes, remove_file_index, FileEntry,
};
use crate::recycle::{discard_document, trash_document};
use crate::storage::paths::{is_user_note_path, read_file_lossy, resolve_vault_path};

fn title_from_path(path: &str) -> String {
    path.trim_end_matches(".md")
        .split('/')
        .next_back()
        .unwrap_or(path)
        .to_string()
}

fn default_create_content(_document_title: &str) -> String {
    String::new()
}

fn validate_folder_path(path: &str) -> AppResult<()> {
    if path.contains('\\') {
        return Err(AppError::msg("Backslashes are not allowed in folder paths"));
    }
    let normalized = path.replace('\\', "/");
    let trimmed = normalized.trim_matches('/');
    if trimmed.trim().is_empty() {
        return Err(AppError::msg("Folder path cannot be empty"));
    }
    if !is_user_note_path(trimmed) {
        return Err(AppError::msg("Folder path cannot target Iris metadata"));
    }
    const INVALID: &[char] = &[':', '*', '?', '"', '<', '>', '|'];
    for segment in trimmed.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(AppError::msg("Invalid folder path segment"));
        }
        if segment.chars().any(|c| INVALID.contains(&c)) {
            return Err(AppError::msg("Invalid folder path character"));
        }
    }
    Ok(())
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
        Ok(())
    })?;
    state.db.with_read_conn(|conn| {
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
    if !is_user_note_path(&path) {
        return Err(AppError::msg("只能读取用户笔记，不允许访问内部元数据路径"));
    }
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &path)?;
    read_file_lossy(&abs)
}

#[tauri::command]
pub fn file_write(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
) -> AppResult<FileEntry> {
    if !is_user_note_path(&path) {
        return Err(AppError::msg("只能写入用户笔记，不允许修改内部元数据路径"));
    }
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &path)?;

    let tmp = abs.with_extension("md.tmp");
    fs::write(&tmp, &content)?;
    if let Err(e) = fs::rename(&tmp, &abs) {
        let _ = crate::security::secure_delete::secure_delete(&tmp);
        return Err(e.into());
    }

    let hash = crate::indexer::scan::content_hash(&content);
    state.write_guard.mark(&path, &hash);

    let entry = state
        .db
        .with_read_conn(|conn| peek_file_entry_after_write_fast(conn, &vault, &abs))?;

    state.schedule_deferred_index(path, content, hash, abs, vault);

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
/// List subfolders under the vault (forward slashes, trailing `/`), including empty dirs.
#[tauri::command]
pub fn folder_list(state: State<'_, Arc<AppState>>) -> AppResult<Vec<String>> {
    let vault = state.vault_path()?;
    Ok(collect_vault_folders(&vault))
}

#[tauri::command]
pub fn folder_create(state: State<'_, Arc<AppState>>, path: String) -> AppResult<()> {
    validate_folder_path(&path)?;
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &path)?;
    if abs.exists() {
        return Err(AppError::msg("Folder already exists"));
    }
    fs::create_dir_all(&abs)?;
    Ok(())
}

/// Rename/move a folder. Cascades wikilink and session updates for all affected files.
#[tauri::command]
pub fn folder_rename(
    state: State<'_, Arc<AppState>>,
    old_path: String,
    new_path: String,
) -> AppResult<()> {
    validate_folder_path(&old_path)?;
    validate_folder_path(&new_path)?;
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &old_path)?;
    let new_abs = resolve_vault_path(&vault, &new_path)?;
    if !abs.is_dir() {
        return Err(AppError::msg("Source path is not a folder"));
    }
    if new_abs.exists() {
        return Err(AppError::msg("Target path already exists"));
    }

    // Collect all .md files under the old folder
    let affected_files: Vec<String> = state.db.with_read_conn(|conn| {
        let mut stmt = conn.prepare("SELECT path FROM files WHERE path LIKE ?1")?;
        let prefix = if old_path.ends_with('/') || old_path.is_empty() {
            old_path.to_string()
        } else {
            format!("{}/", old_path)
        };
        let rows = stmt.query_map([format!("{}%", prefix)], |row| row.get::<_, String>(0))?;
        Ok(rows.flatten().collect())
    })?;

    // Step 1: rewrite wikilinks and sessions for each affected file (disk-only for wikilinks)
    let mut all_modified_sources: Vec<String> = Vec::new();
    for file_path in &affected_files {
        let rel_old = file_path.as_str();
        let rel_new = if let Some(suffix) = rel_old.strip_prefix(&old_path) {
            let trimmed = suffix.trim_start_matches('/');
            if new_path.is_empty() || new_path == "/" {
                trimmed.to_string()
            } else {
                format!("{}/{}", new_path.trim_end_matches('/'), trimmed)
            }
        } else {
            continue;
        };

        let old_stem = title_from_path(rel_old);
        let new_stem = title_from_path(&rel_new);
        let mut mods = cascade_rewrite_wikilinks_on_disk(
            &state, &vault, rel_old, &rel_new, &old_stem, &new_stem,
        )?;
        all_modified_sources.append(&mut mods);
        cascade_rename_sessions(&state, rel_old, &rel_new)?;
    }

    // Step 2: rename the folder on disk
    if let Some(parent) = new_abs.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&abs, &new_abs)?;

    // Step 3: full reindex (target files get new paths)
    let vault_clone = vault.clone();
    state
        .db
        .with_conn(|conn| index_vault_incremental(conn, &vault_clone, Some(state.inner())))?;

    // Step 4: reindex source files that were modified (wikilinks now resolve correctly)
    for src_path in &all_modified_sources {
        if let Ok(abs_src) = resolve_vault_path(&vault, src_path) {
            if let Ok(h) = crate::indexer::scan::file_hash(&abs_src) {
                state.write_guard.mark(src_path, &h);
            }
            let _ = state.db.with_conn(|conn| {
                index_file_with_embed(conn, &vault, &abs_src, Some(state.inner()))
            });
        }
    }

    Ok(())
}

/// Delete an empty folder. Fails if the folder is not empty.
#[tauri::command]
pub fn folder_delete(state: State<'_, Arc<AppState>>, path: String) -> AppResult<()> {
    validate_folder_path(&path)?;
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

    #[test]
    fn default_create_content_is_frontmatter_free_blank() {
        assert_eq!(default_create_content("新建文档"), "");
    }

    #[test]
    fn folder_paths_accept_nested_relative_paths_only() {
        assert!(validate_folder_path("inbox").is_ok());
        assert!(validate_folder_path("notes/inbox").is_ok());
        assert!(validate_folder_path("notes\\inbox").is_err());
        assert!(validate_folder_path("../secret").is_err());
        assert!(validate_folder_path(".iris/trash").is_err());
    }
}

#[tauri::command]
pub fn file_rename(
    state: State<'_, Arc<AppState>>,
    path: String,
    new_path: String,
) -> AppResult<FileEntry> {
    if !is_user_note_path(&path) || !is_user_note_path(&new_path) {
        return Err(AppError::msg("只能重命名用户笔记路径"));
    }
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &path)?;
    let new_abs = resolve_vault_path(&vault, &new_path)?;
    if new_abs.exists() {
        return Err(AppError::msg("Target path already exists"));
    }
    if let Some(parent) = new_abs.parent() {
        fs::create_dir_all(parent)?;
    }

    let old_stem = title_from_path(&path);
    let new_stem = title_from_path(&new_path);

    // ── Step 1: rewrite wikilink text in source files (disk-only, no reindex yet) ──
    let modified_sources =
        cascade_rewrite_wikilinks_on_disk(&state, &vault, &path, &new_path, &old_stem, &new_stem)?;

    // ── Step 2: update session note_path → session_key ──
    cascade_rename_sessions(&state, &path, &new_path)?;

    // ── Step 3: rename the target file on disk ──
    fs::rename(&abs, &new_abs)?;
    let hash = crate::indexer::scan::file_hash(&new_abs)?;
    state.write_guard.mark(&new_path, &hash);

    // ── Step 4: remove old index, reindex target under new path ──
    state.db.with_conn(|conn| {
        remove_file_index(conn, &path)?;
        index_file_with_embed(conn, &vault, &new_abs, Some(state.inner()))
    })?;

    // ── Step 5: reindex source files so their wikilinks resolve to the new target row ──
    for src_path in &modified_sources {
        if let Ok(abs_src) = resolve_vault_path(&vault, src_path) {
            if let Ok(h) = crate::indexer::scan::file_hash(&abs_src) {
                state.write_guard.mark(src_path, &h);
            }
            let _ = state.db.with_conn(|conn| {
                index_file_with_embed(conn, &vault, &abs_src, Some(state.inner()))
            });
        }
    }

    // Return the new file entry
    state.db.with_read_conn(|conn| {
        let entry = conn.query_row(
            "SELECT id, path, title, updated_at, word_count FROM files WHERE path = ?1",
            [&new_path],
            |row| {
                Ok(FileEntry {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    title: row.get(2)?,
                    updated_at: row.get(3)?,
                    word_count: row.get(4)?,
                })
            },
        )?;
        Ok(entry)
    })
}

/// Rewrite wikilink text in all files referencing `old_path` on disk.
/// Returns the list of modified source file paths.
/// Does NOT reindex — caller must reindex after the target has been reindexed.
fn cascade_rewrite_wikilinks_on_disk(
    state: &Arc<AppState>,
    vault: &std::path::Path,
    old_path: &str,
    new_path: &str,
    old_stem: &str,
    new_stem: &str,
) -> AppResult<Vec<String>> {
    let source_paths: Vec<String> = state.db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT f.path
             FROM links l
             JOIN files f ON f.id = l.source_id
             JOIN files t ON t.id = l.target_id
             WHERE t.path = ?1",
        )?;
        let rows = stmt.query_map([old_path], |row| row.get::<_, String>(0))?;
        Ok(rows.flatten().collect())
    })?;

    let mut modified = Vec::new();

    for src_path in &source_paths {
        let abs = resolve_vault_path(vault, src_path)?;
        let content = read_file_lossy(&abs)?;
        let mut updated = content.clone();

        let pattern_stem = format!("[[{}]]", old_stem);
        let replacement_stem = format!("[[{}]]", new_stem);
        updated = updated.replace(&pattern_stem, &replacement_stem);

        let pattern_path = format!("[[{}]]", old_path);
        let replacement_path = format!("[[{}]]", new_path);
        updated = updated.replace(&pattern_path, &replacement_path);

        if let Some(old_no_ext) = old_path.strip_suffix(".md") {
            let pattern_noext = format!("[[{}]]", old_no_ext);
            let new_no_ext = new_path.strip_suffix(".md").unwrap_or(new_path);
            let replacement_noext = format!("[[{}]]", new_no_ext);
            updated = updated.replace(&pattern_noext, &replacement_noext);
        }

        if updated != content {
            let tmp = abs.with_extension("md.tmp");
            fs::write(&tmp, &updated)?;
            if let Err(e) = fs::rename(&tmp, &abs) {
                let _ = crate::security::secure_delete::secure_delete(&tmp);
                return Err(e.into());
            }
            modified.push(src_path.clone());
        }
    }

    Ok(modified)
}

/// Update all AI session `note_path` and `session_key` references from old to new path.
fn cascade_rename_sessions(state: &Arc<AppState>, old_path: &str, new_path: &str) -> AppResult<()> {
    state.db.with_conn(|conn| {
        // Update note_path (direct reference)
        let updated_note = conn.execute(
            "UPDATE sessions SET note_path = ?1 WHERE note_path = ?2",
            rusqlite::params![new_path, old_path],
        )?;

        // Update session_key — rebuild it from scene + new note_path
        // session_key format = "scene:note_path" or "scene:__global__"
        let updated_key = conn.execute(
            "UPDATE sessions SET session_key = scene || ':' || ?1
             WHERE session_key = scene || ':' || ?2",
            rusqlite::params![new_path, old_path],
        )?;

        if updated_note > 0 || updated_key > 0 {
            tracing::info!(
                old = %old_path,
                new = %new_path,
                note_updated = updated_note,
                key_updated = updated_key,
                "cascade_rename: updated session references"
            );
        }
        Ok(())
    })
}

#[tauri::command]
pub fn file_create(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: Option<String>,
) -> AppResult<FileEntry> {
    if !is_user_note_path(&path) {
        return Err(AppError::msg("只能创建用户笔记，不允许写入内部元数据路径"));
    }
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
    let body = content.unwrap_or_else(|| default_create_content(&document_title));
    fs::write(&abs, &body)?;
    let hash = crate::indexer::scan::file_hash(&abs)?;
    state.write_guard.mark(&path, &hash);
    state
        .db
        .with_conn(|conn| index_file_with_embed(conn, &vault, &abs, Some(state.inner())))
}

#[tauri::command]
pub fn vault_set(app: AppHandle, state: State<'_, Arc<AppState>>, path: String) -> AppResult<()> {
    use std::path::PathBuf;

    let p = PathBuf::from(path.trim());
    if p.as_os_str().is_empty() {
        return Err(AppError::msg("笔记目录路径不能为空"));
    }
    if !p.is_dir() {
        return Err(AppError::msg("请选择已存在的文件夹作为笔记目录"));
    }
    state.set_vault(p)?;

    // Clear in-memory AI state to prevent data leakage between vaults
    state.clear_ai_state();

    if let Err(e) = state.restart_file_watcher(app) {
        tracing::warn!("vault_set: file watcher did not start: {e}");
    }

    if let Ok(vault) = state.vault_path() {
        if let Err(e) = state
            .db
            .with_conn(|conn| index_vault_incremental(conn, &vault, Some(state.inner())))
        {
            tracing::warn!("vault_set: initial index skipped: {e}");
        }
    }

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
    state
        .db
        .with_conn(|conn| index_vault_incremental(conn, &vault, Some(state.inner())))
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
    state.db.with_read_conn(|conn| {
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
