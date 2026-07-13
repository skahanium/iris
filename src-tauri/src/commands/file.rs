use std::collections::BTreeSet;
use std::fs;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app::AppState;
use crate::cas::hash::content_hash as content_hash_bytes;
use crate::error::{AppError, AppResult};
use crate::indexer::frontmatter::resolve_display_title;
use crate::indexer::scan::{
    collect_vault_folders, content_hash, index_file_from_content, index_file_with_embed,
    index_vault_incremental, rename_file_index, FileEntry, IndexEmbeddingMode,
};
use crate::recycle::{discard_document, trash_document};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use crate::crypto::classified_io;
use crate::crypto::vault_key::VAULT_KEY;
use crate::storage::paths::{
    has_reserved_path_root, is_accessible_note_path, is_classified_note_path, is_user_note_path,
    read_file_lossy, resolve_vault_path,
};

const MAX_NOTE_FILE_BYTES: usize = 20 * 1024 * 1024;

#[cfg(test)]
fn vault_runtime_cleanup_sql() -> &'static str {
    ""
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VaultIndexProgress {
    status: &'static str,
    processed: usize,
    total: usize,
    path: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReadResult {
    pub content: String,
    pub is_locked: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSignatureResult {
    pub byte_length: u64,
    pub content_hash: String,
    pub is_locked: bool,
    pub modified_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentOpenScopeResult {
    pub token: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentOpenResult {
    pub token: String,
    pub content: String,
    pub is_locked: bool,
}

fn query_is_locked(db: &crate::storage::db::Database, path: &str) -> AppResult<bool> {
    db.with_read_conn(|conn| {
        let mut stmt = conn.prepare("SELECT is_locked FROM files WHERE path = ?1")?;
        match stmt.query_row([path], |row| row.get::<_, i64>(0)) {
            Ok(v) => Ok(v != 0),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(e) => Err(e.into()),
        }
    })
}

fn decode_file_content(raw_bytes: &[u8]) -> AppResult<String> {
    if classified_io::has_csef_magic(raw_bytes) {
        let vk_guard = VAULT_KEY
            .get()
            .ok_or_else(|| AppError::msg("保险库未初始化"))?;
        let vk = vk_guard
            .read()
            .map_err(|e| AppError::msg(format!("lock error: {e}")))?;
        let key = vk.key()?;
        let decrypted = classified_io::decrypt_cef(raw_bytes, key)?;
        String::from_utf8(decrypted).map_err(|_| AppError::msg("File is not valid UTF-8"))
    } else {
        std::str::from_utf8(raw_bytes)
            .map(str::to_owned)
            .map_err(|_| AppError::msg("File is not valid UTF-8"))
    }
}

fn encode_file_payload(path: &str, content: &str) -> AppResult<Vec<u8>> {
    if is_classified_note_path(path) {
        let vk_guard = VAULT_KEY
            .get()
            .ok_or_else(|| AppError::msg("保险库未初始化"))?;
        let vk = vk_guard
            .read()
            .map_err(|e| AppError::msg(format!("lock error: {e}")))?;
        let key = vk.key()?;
        classified_io::encrypt_cef(content.as_bytes(), key)
    } else if has_reserved_path_root(path) {
        Err(AppError::msg("Invalid classified path casing"))
    } else {
        Ok(content.as_bytes().to_vec())
    }
}

/// Vault-relative image/asset path (e.g. `assets/uuid.png`).
pub fn is_vault_asset_path(relative: &str) -> bool {
    let normalized = relative.replace('\\', "/");
    if !normalized.starts_with("assets/") {
        return false;
    }
    let name = normalized.strip_prefix("assets/").unwrap_or("");
    !name.is_empty() && !name.ends_with('/') && !name.contains("..")
}

pub(crate) fn allow_vault_assets_in_asset_protocol(app: &AppHandle, vault: &std::path::Path) {
    let Some(scopes) = app.try_state::<tauri::scope::Scopes>() else {
        return;
    };
    let assets_dir = vault.join("assets");
    if let Err(e) = scopes.allow_directory(&assets_dir, true) {
        tracing::warn!(
            "failed to allow vault assets for asset protocol ({}): {e}",
            assets_dir.display()
        );
    }
}

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
    let trimmed = path.trim_matches('/');
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

fn remap_folder_child_path(old_path: &str, new_path: &str, file_path: &str) -> Option<String> {
    if file_path == old_path {
        return Some(new_path.trim_matches('/').to_string());
    }
    let prefix = if old_path.ends_with('/') || old_path.is_empty() {
        old_path.to_string()
    } else {
        format!("{old_path}/")
    };
    let suffix = file_path.strip_prefix(&prefix)?;
    let new_prefix = new_path.trim_matches('/');
    if new_prefix.is_empty() {
        Some(suffix.trim_start_matches('/').to_string())
    } else {
        Some(format!("{new_prefix}/{}", suffix.trim_start_matches('/')))
    }
}

fn folder_rename_reindex_paths(
    old_path: &str,
    new_path: &str,
    affected_files: &[String],
    modified_sources: &[String],
) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for path in affected_files {
        if let Some(mapped) = remap_folder_child_path(old_path, new_path, path) {
            if is_user_note_path(&mapped) {
                paths.insert(mapped);
            }
        }
    }
    for path in modified_sources {
        let mapped = remap_folder_child_path(old_path, new_path, path)
            .unwrap_or_else(|| path.trim_matches('/').to_string());
        if is_user_note_path(&mapped) {
            paths.insert(mapped);
        }
    }
    paths.into_iter().collect()
}

fn start_vault_index_task(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn_blocking(move || {
        let vault = match state.vault_path() {
            Ok(vault) => vault,
            Err(e) => {
                let _ = app.emit(
                    "vault:index_progress",
                    &VaultIndexProgress {
                        status: "failed",
                        processed: 0,
                        total: 0,
                        path: None,
                        error: Some(e.to_string()),
                    },
                );
                return;
            }
        };
        let files = crate::indexer::scan::collect_vault_files(&vault);
        let total = files.len();
        let _ = app.emit(
            "vault:index_progress",
            &VaultIndexProgress {
                status: "started",
                processed: 0,
                total,
                path: None,
                error: None,
            },
        );

        let mut processed = 0usize;
        for abs in &files {
            if state.has_foreground_document_open() {
                std::thread::sleep(std::time::Duration::from_millis(8));
            }
            let rel = match crate::storage::paths::relative_path(&vault, abs) {
                Ok(rel) if crate::storage::paths::is_user_note_path(&rel) => rel,
                _ => continue,
            };
            if let Err(e) = state.db.with_conn(|conn| {
                crate::indexer::scan::index_file_with_embed(
                    conn,
                    &vault,
                    abs,
                    IndexEmbeddingMode::Queue(&state),
                )
            }) {
                tracing::warn!(path = %rel, "vault_set background index skipped: {e}");
            }
            processed += 1;
            let _ = app.emit(
                "vault:index_progress",
                &VaultIndexProgress {
                    status: "progress",
                    processed,
                    total,
                    path: Some(rel),
                    error: None,
                },
            );
        }

        if let Err(e) = state
            .db
            .with_conn(|conn| crate::indexer::scan::prune_stale_file_indexes(conn, &vault))
        {
            tracing::warn!("vault_set: background prune skipped: {e}");
            let _ = app.emit(
                "vault:index_progress",
                &VaultIndexProgress {
                    status: "failed",
                    processed,
                    total,
                    path: None,
                    error: Some(e.to_string()),
                },
            );
            return;
        }

        let _ = app.emit(
            "vault:index_progress",
            &VaultIndexProgress {
                status: "completed",
                processed,
                total,
                path: None,
                error: None,
            },
        );
    });
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileListItem {
    pub path: String,
    pub title: String,
    pub updated_at: String,
    pub is_locked: bool,
}

/// 列出受追踪的用户笔记（每篇文档一条，不含版本快照）。
/// `title` 为创建时确定的文档名；版本历史见 `version_list_cmd`。
#[tauri::command]
pub fn file_list(
    state: State<'_, Arc<AppState>>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Vec<FileListItem>> {
    file_list_inner(state.inner(), limit, offset)
}

fn file_list_inner(
    state: &AppState,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Vec<FileListItem>> {
    let _vault = state.vault_path()?;

    let lim = limit.unwrap_or(u32::MAX) as i64;
    let off = offset.unwrap_or(0) as i64;

    state.db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT path, title, updated_at, frontmatter, is_locked FROM files
             WHERE id IN (SELECT MAX(id) FROM files GROUP BY path)
               AND path NOT LIKE '.iris/%'
               AND path NOT LIKE '.classified/%'
             ORDER BY updated_at DESC
              LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![lim, off], |row| {
            let path: String = row.get(0)?;
            let stored_title: String = row.get(1)?;
            let updated_at: String = row.get(2)?;
            let frontmatter: Option<String> = row.get(3)?;
            let is_locked: bool = row.get::<_, i64>(4).unwrap_or(0) != 0;
            let stem = title_from_path(&path);
            let title = resolve_display_title(None, &stored_title, frontmatter.as_deref(), &stem);
            Ok(FileListItem {
                path,
                title,
                updated_at,
                is_locked,
            })
        })?;
        let mut files = Vec::new();
        for row in rows {
            let item = row?;
            if is_user_note_path(&item.path) {
                files.push(item);
            }
        }
        Ok(files)
    })
}

fn file_signature_inner(
    state: &AppState,
    path: &str,
    allow_classified: bool,
) -> AppResult<FileSignatureResult> {
    validate_file_read_path(path, allow_classified)?;
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, path)?;
    let raw_bytes = fs::read(&abs)?;
    let modified_ms = fs::metadata(&abs)?
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64);
    let is_locked = query_is_locked(&state.db, path)?;
    Ok(FileSignatureResult {
        byte_length: raw_bytes.len() as u64,
        content_hash: content_hash_bytes(&raw_bytes),
        is_locked,
        modified_ms,
    })
}

#[tauri::command]
pub async fn file_signature(
    state: State<'_, Arc<AppState>>,
    path: String,
    allow_classified: Option<bool>,
) -> AppResult<FileSignatureResult> {
    validate_file_read_path(&path, allow_classified.unwrap_or(false))?;
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        file_signature_inner(&state, &path, allow_classified.unwrap_or(false))
    })
    .await
    .map_err(|e| AppError::msg(format!("task join: {e}")))?
}

#[tauri::command]
pub fn document_open_begin(state: State<'_, Arc<AppState>>) -> AppResult<DocumentOpenScopeResult> {
    Ok(DocumentOpenScopeResult {
        token: state.begin_document_open(),
    })
}

#[tauri::command]
pub fn document_open_end(state: State<'_, Arc<AppState>>, token: String) -> AppResult<()> {
    state.end_document_open(&token);
    Ok(())
}

/// Open a document with a single IPC roundtrip.
///
/// Acquires a foreground scope token (so the indexer yields), reads the file
/// from disk (with classified vault decryption if needed), checks the lock
/// status, and returns everything in one response. The caller must release the
/// returned token with `document_open_end` after the first editor frame commits
/// or the open is cancelled.
#[tauri::command]
pub async fn document_open(
    state: State<'_, Arc<AppState>>,
    path: String,
    allow_classified: Option<bool>,
) -> AppResult<DocumentOpenResult> {
    validate_file_read_path(&path, allow_classified.unwrap_or(false))?;
    let token = state.begin_document_open();
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &path)?;
    let db = state.inner().db.clone();
    let path_for_db = path.clone();

    let result = tokio::task::spawn_blocking(move || {
        let raw_bytes = std::fs::read(&abs)?;
        let content = decode_file_content(&raw_bytes)?;
        let is_locked = query_is_locked(&db, &path_for_db)?;
        Ok(DocumentOpenResult {
            token,
            content,
            is_locked,
        })
    })
    .await
    .map_err(|e| AppError::msg(format!("task join: {e}")))?;
    result
}

#[tauri::command]
pub async fn file_read(
    state: State<'_, Arc<AppState>>,
    path: String,
    allow_classified: Option<bool>,
) -> AppResult<FileReadResult> {
    validate_file_read_path(&path, allow_classified.unwrap_or(false))?;
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &path)?;
    let db = state.inner().db.clone();
    let path_for_db = path.clone();
    tokio::task::spawn_blocking(move || {
        let raw_bytes = std::fs::read(&abs)?;
        let content = decode_file_content(&raw_bytes)?;
        let is_locked = query_is_locked(&db, &path_for_db)?;
        Ok(FileReadResult { content, is_locked })
    })
    .await
    .map_err(|e| AppError::msg(format!("task join: {e}")))?
}

fn validate_file_read_path(path: &str, allow_classified: bool) -> AppResult<()> {
    if is_user_note_path(path) {
        return Ok(());
    }
    if is_classified_note_path(path) {
        if allow_classified {
            return Ok(());
        }
        return Err(AppError::msg("涉密笔记只能从涉密保险库打开"));
    }
    Err(AppError::msg("只能读取用户笔记，不允许访问内部元数据路径"))
}

#[tauri::command]
pub async fn file_write(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
) -> AppResult<FileEntry> {
    if !is_accessible_note_path(&path) {
        return Err(AppError::msg("只能写入用户笔记，不允许修改内部元数据路径"));
    }
    if content.len() > MAX_NOTE_FILE_BYTES {
        return Err(AppError::msg(format!(
            "笔记内容超过 20MB 限制（{} 字节）",
            content.len()
        )));
    }
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let vault = state.vault_path()?;
        let abs = resolve_vault_path(&vault, &path)?;

        let tmp = abs.with_extension("md.tmp");
        let data = encode_file_payload(&path, &content)?;
        fs::write(&tmp, &data)?;
        if let Err(e) = fs::rename(&tmp, &abs) {
            let _ = crate::security::secure_delete::secure_delete(&tmp);
            return Err(e.into());
        }

        let hash = content_hash(&content);
        state.storage.write_guard.mark(&path, &hash);

        if is_classified_note_path(&path) {
            Ok(FileEntry {
                id: 0,
                path: path.clone(),
                title: title_from_path(&path),
                updated_at: chrono::Utc::now().to_rfc3339(),
                word_count: 0,
            })
        } else {
            let entry = state.db.with_conn(|conn| {
                index_file_from_content(
                    conn,
                    &vault,
                    &abs,
                    &content,
                    &hash,
                    IndexEmbeddingMode::Queue(&state),
                )
            })?;
            Ok(entry)
        }
    })
    .await
    .map_err(|e| AppError::msg(format!("task join: {e}")))?
}

/// Write a binary asset under `assets/` (editor image drop / paste).
#[tauri::command]
pub async fn vault_asset_write(
    state: State<'_, Arc<AppState>>,
    path: String,
    data_base64: String,
) -> AppResult<String> {
    if !is_vault_asset_path(&path) {
        return Err(AppError::msg("资源路径必须位于 assets/ 下"));
    }
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &path)?;
    tokio::task::spawn_blocking(move || {
        const MAX_BYTES: usize = 20 * 1024 * 1024;
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = STANDARD
            .decode(data_base64.trim())
            .map_err(|e| AppError::msg(format!("无效的图片数据: {e}")))?;
        if bytes.is_empty() {
            return Err(AppError::msg("图片数据为空"));
        }
        if bytes.len() > MAX_BYTES {
            return Err(AppError::msg("图片超过 20MB 限制"));
        }
        let tmp = abs.with_extension("tmp");
        fs::write(&tmp, &bytes)?;
        if let Err(e) = fs::rename(&tmp, &abs) {
            let _ = fs::remove_file(&tmp);
            return Err(e.into());
        }
        Ok(path)
    })
    .await
    .map_err(|e| AppError::msg(format!("task join: {e}")))?
}

/// Move note + all version snapshots into recycle bin (15-day retention).
#[tauri::command]
pub fn file_delete(state: State<'_, Arc<AppState>>, path: String) -> AppResult<()> {
    if is_classified_note_path(&path) {
        return Err(AppError::msg("涉密文件不能通过此接口删除"));
    }
    trash_document(state.inner(), &path)
}

/// Permanently remove a blank note (no recycle bin).
#[tauri::command]
pub fn file_discard(state: State<'_, Arc<AppState>>, path: String) -> AppResult<()> {
    if is_classified_note_path(&path) {
        return Err(AppError::msg("涉密文件不能通过此接口删除"));
    }
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

/// Rename/move a folder and cascade wikilink updates for all affected files.
#[tauri::command]
pub async fn folder_rename(
    state: State<'_, Arc<AppState>>,
    old_path: String,
    new_path: String,
) -> AppResult<()> {
    validate_folder_path(&old_path)?;
    validate_folder_path(&new_path)?;
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let vault = state.vault_path()?;
        let abs = resolve_vault_path(&vault, &old_path)?;
        let new_abs = resolve_vault_path(&vault, &new_path)?;
        if !abs.is_dir() {
            return Err(AppError::msg("Source path is not a folder"));
        }
        if new_abs.exists() {
            return Err(AppError::msg("Target path already exists"));
        }

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

        // Step 1: rewrite wikilinks while old paths still exist on disk.
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
            match cascade_rewrite_wikilinks_on_disk(
                &state, &vault, rel_old, &rel_new, &old_stem, &new_stem,
            ) {
                Ok(mut mods) => all_modified_sources.append(&mut mods),
                Err(err) => tracing::warn!(
                    old = %rel_old,
                    new = %rel_new,
                    error = %err,
                    "folder_rename: skipped wikilink cascade for child"
                ),
            }
        }

        // Step 2: rename the folder on disk.
        if let Some(parent) = new_abs.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&abs, &new_abs)?;

        // Step 3: reindex only moved files and sources whose wikilinks changed.
        let reindex_paths = folder_rename_reindex_paths(
            &old_path,
            &new_path,
            &affected_files,
            &all_modified_sources,
        );
        for rel in &reindex_paths {
            let Ok(abs_path) = resolve_vault_path(&vault, rel) else {
                continue;
            };
            if let Ok(h) = crate::indexer::scan::file_hash(&abs_path) {
                state.storage.write_guard.mark(rel, &h);
            }
            let _ = state.db.with_conn(|conn| {
                index_file_with_embed(conn, &vault, &abs_path, IndexEmbeddingMode::Queue(&state))
            });
        }

        let _ = state
            .db
            .with_conn(|conn| crate::indexer::scan::prune_stale_file_indexes(conn, &vault));

        Ok(())
    })
    .await
    .map_err(|e| AppError::msg(format!("task join: {e}")))?
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

const UNNAMED_DOCUMENT_TITLE: &str = "未命名文档";
const PLACEHOLDER_TITLE_MARKERS: &[&str] = &["未命名文档", "新建文档", "无标题", "untitled"];

fn is_placeholder_title(title: &str) -> bool {
    let t = title.trim();
    if t.is_empty() {
        return false;
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
        UNNAMED_DOCUMENT_TITLE.to_string()
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
    let parent = current_path
        .rsplit_once('/')
        .map(|(p, _)| p)
        .unwrap_or("")
        .to_string();

    let sync_title = if title.trim().is_empty() {
        UNNAMED_DOCUMENT_TITLE.to_string()
    } else if is_placeholder_title(&title) {
        return Ok(PathSyncSuggest {
            current_path: current_path.clone(),
            suggested_path: current_path,
            needs_sync: false,
            conflict_resolved: false,
        });
    } else {
        title
    };

    let (suggested_path, conflict_resolved) = state
        .db
        .with_conn(|conn| allocate_path_for_title(&parent, &sync_title, &current_path, conn))?;

    let needs_sync = suggested_path != current_path;

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

/// 更新笔记锁定状态（仅用户笔记路径）。
pub fn set_file_lock(state: &AppState, path: &str, locked: bool) -> AppResult<()> {
    if !is_user_note_path(path) {
        return Err(AppError::msg("只能操作用户笔记"));
    }
    state.db.with_conn(|conn| {
        conn.execute(
            "UPDATE files SET is_locked = ?1 WHERE path = ?2",
            rusqlite::params![locked as i64, path],
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn file_set_lock(state: State<'_, Arc<AppState>>, path: String, locked: bool) -> AppResult<()> {
    set_file_lock(state.inner(), &path, locked)
}

#[tauri::command]
pub async fn file_rename(
    state: State<'_, Arc<AppState>>,
    path: String,
    new_path: String,
) -> AppResult<FileEntry> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || file_rename_inner(state, path, new_path))
        .await
        .map_err(|e| AppError::msg(format!("task join: {e}")))?
}

fn fallback_file_entry(path: &str, absolute: &std::path::Path) -> AppResult<FileEntry> {
    let content = read_file_lossy(absolute)?;
    Ok(FileEntry {
        id: 0,
        path: path.to_string(),
        title: title_from_path(path),
        updated_at: chrono::Utc::now().to_rfc3339(),
        word_count: content.split_whitespace().count() as i64,
    })
}

fn file_rename_inner(state: Arc<AppState>, path: String, new_path: String) -> AppResult<FileEntry> {
    if !is_user_note_path(&path) || !is_user_note_path(&new_path) {
        return Err(AppError::msg("Only user note paths can be renamed"));
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

    if let Err(err) = state
        .db
        .with_conn(|conn| crate::indexer::scan::prune_stale_file_indexes(conn, &vault))
    {
        tracing::warn!(
            old = %path,
            new = %new_path,
            error = %err,
            "file_rename: skipped stale index prune before move"
        );
    }

    let modified_sources = match cascade_rewrite_wikilinks_on_disk(
        &state, &vault, &path, &new_path, &old_stem, &new_stem,
    ) {
        Ok(paths) => paths,
        Err(err) => {
            tracing::warn!(
                old = %path,
                new = %new_path,
                error = %err,
                "file_rename: skipped wikilink cascade before move"
            );
            Vec::new()
        }
    };

    fs::rename(&abs, &new_abs)?;
    let hash = crate::indexer::scan::file_hash(&new_abs)?;
    state.storage.write_guard.mark(&new_path, &hash);

    let entry = match state.db.with_conn(|conn| {
        if let Err(err) = rename_file_index(conn, &path, &new_path) {
            tracing::warn!(
                old = %path,
                new = %new_path,
                error = %err,
                "file_rename: skipped stale or missing file index rename"
            );
        }
        let entry =
            index_file_with_embed(conn, &vault, &new_abs, IndexEmbeddingMode::Queue(&state))?;
        if let Err(err) = crate::indexer::scan::prune_stale_file_indexes(conn, &vault) {
            tracing::warn!(
                old = %path,
                new = %new_path,
                error = %err,
                "file_rename: skipped stale index prune after move"
            );
        }
        Ok(entry)
    }) {
        Ok(entry) => entry,
        Err(err) => {
            tracing::warn!(
                old = %path,
                new = %new_path,
                error = %err,
                "file_rename: moved file but index refresh failed"
            );
            fallback_file_entry(&new_path, &new_abs)?
        }
    };

    for src_path in &modified_sources {
        if let Ok(abs_src) = resolve_vault_path(&vault, src_path) {
            if let Ok(h) = crate::indexer::scan::file_hash(&abs_src) {
                state.storage.write_guard.mark(src_path, &h);
            }
            let _ = state.db.with_conn(|conn| {
                index_file_with_embed(conn, &vault, &abs_src, IndexEmbeddingMode::Queue(&state))
            });
        }
    }

    Ok(entry)
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
        if !abs.exists() {
            tracing::warn!(
                source = %src_path,
                target = %old_path,
                "skipping stale wikilink source during file rename"
            );
            continue;
        }
        let content = read_file_lossy(&abs)?;

        // Rewrite line-by-line, skipping code blocks
        let mut fence = crate::indexer::code_fence::FenceState::new();
        let mut lines: Vec<String> = Vec::new();
        let mut changed = false;

        for line in content.lines() {
            let in_fence = fence.feed(line);
            if in_fence {
                lines.push(line.to_string());
                continue;
            }

            let mut updated_line = line.to_string();
            let pattern_stem = format!("[[{}]]", old_stem);
            let replacement_stem = format!("[[{}]]", new_stem);
            if updated_line.contains(&pattern_stem) {
                updated_line = updated_line.replace(&pattern_stem, &replacement_stem);
            }

            let pattern_path = format!("[[{}]]", old_path);
            let replacement_path = format!("[[{}]]", new_path);
            if updated_line.contains(&pattern_path) {
                updated_line = updated_line.replace(&pattern_path, &replacement_path);
            }

            if let Some(old_no_ext) = old_path.strip_suffix(".md") {
                let pattern_noext = format!("[[{}]]", old_no_ext);
                let new_no_ext = new_path.strip_suffix(".md").unwrap_or(new_path);
                let replacement_noext = format!("[[{}]]", new_no_ext);
                if updated_line.contains(&pattern_noext) {
                    updated_line = updated_line.replace(&pattern_noext, &replacement_noext);
                }
            }

            if updated_line != line {
                changed = true;
            }
            lines.push(updated_line);
        }

        if changed {
            let updated = lines.join("\n");
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

#[tauri::command]
pub async fn file_create(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: Option<String>,
) -> AppResult<FileEntry> {
    if !is_accessible_note_path(&path) {
        return Err(AppError::msg("只能创建用户笔记，不允许写入内部元数据路径"));
    }
    if let Some(ref body) = content {
        if body.len() > MAX_NOTE_FILE_BYTES {
            return Err(AppError::msg(format!(
                "笔记内容超过 20MB 限制（{} 字节）",
                body.len()
            )));
        }
    }
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
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
        let tmp = abs.with_extension("md.tmp");
        let data = encode_file_payload(&path, &body)?;
        fs::write(&tmp, &data)?;
        if let Err(e) = fs::rename(&tmp, &abs) {
            let _ = fs::remove_file(&tmp);
            return Err(e.into());
        }
        let hash = content_hash(&body);
        state.storage.write_guard.mark(&path, &hash);
        if is_classified_note_path(&path) {
            Ok(FileEntry {
                id: 0,
                path: path.clone(),
                title: document_title,
                updated_at: chrono::Utc::now().to_rfc3339(),
                word_count: 0,
            })
        } else {
            state.db.with_conn(|conn| {
                index_file_with_embed(conn, &vault, &abs, IndexEmbeddingMode::Queue(&state))
            })
        }
    })
    .await
    .map_err(|e| AppError::msg(format!("task join: {e}")))?
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
    let state = state.inner().clone();
    state.set_vault(p)?;

    // Clear in-memory AI state to prevent data leakage between vaults
    state.clear_ai_state();
    crate::commands::media::clear_media_leases();

    if let Err(e) = state.restart_file_watcher(app.clone()) {
        tracing::warn!("vault_set: file watcher did not start: {e}");
    }

    if let Ok(vault) = state.vault_path() {
        allow_vault_assets_in_asset_protocol(&app, &vault);
    }
    start_vault_index_task(app, state);

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
pub async fn index_rescan(state: State<'_, Arc<AppState>>) -> AppResult<Vec<FileEntry>> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let vault = state.vault_path()?;
        state.db.with_conn(|conn| {
            index_vault_incremental(conn, &vault, IndexEmbeddingMode::Queue(&state))
        })
    })
    .await
    .map_err(|e| AppError::msg(format!("task join: {e}")))?
}

#[derive(Debug, Clone, Serialize)]
pub struct BacklinkEntry {
    pub source_path: String,
    pub source_title: String,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileLinkPreview {
    pub path: String,
    pub title: String,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileLinkSummary {
    pub inbound_count: usize,
    pub outbound_count: usize,
    pub inbound: Vec<FileLinkPreview>,
    pub outbound: Vec<FileLinkPreview>,
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
               AND f.path NOT LIKE '.classified/%'
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

pub fn file_link_summary_inner(state: &AppState, path: &str) -> AppResult<FileLinkSummary> {
    state.db.with_read_conn(|conn| {
        let inbound_count: usize = conn.query_row(
            "SELECT COUNT(*)
             FROM links l
             JOIN files f ON f.id = l.source_id
             JOIN files t ON t.id = l.target_id
             WHERE t.path = ?1
               AND f.path NOT LIKE '.classified/%'",
            [path],
            |row| row.get(0),
        )?;
        let outbound_count: usize = conn.query_row(
            "SELECT COUNT(*)
             FROM links l
             JOIN files f ON f.id = l.source_id
             JOIN files t ON t.id = l.target_id
             WHERE f.path = ?1
               AND t.path NOT LIKE '.classified/%'",
            [path],
            |row| row.get(0),
        )?;

        let mut inbound_stmt = conn.prepare(
            "SELECT f.path, f.title, l.context
             FROM links l
             JOIN files f ON f.id = l.source_id
             JOIN files t ON t.id = l.target_id
             WHERE t.path = ?1
               AND f.path NOT LIKE '.classified/%'
             ORDER BY f.updated_at DESC, f.title
             LIMIT 3",
        )?;
        let inbound = inbound_stmt
            .query_map([path], |row| {
                Ok(FileLinkPreview {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    context: row.get(2)?,
                })
            })?
            .flatten()
            .collect();

        let mut outbound_stmt = conn.prepare(
            "SELECT t.path, t.title, l.context
             FROM links l
             JOIN files f ON f.id = l.source_id
             JOIN files t ON t.id = l.target_id
             WHERE f.path = ?1
               AND t.path NOT LIKE '.classified/%'
             ORDER BY t.updated_at DESC, t.title
             LIMIT 3",
        )?;
        let outbound = outbound_stmt
            .query_map([path], |row| {
                Ok(FileLinkPreview {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    context: row.get(2)?,
                })
            })?
            .flatten()
            .collect();

        Ok(FileLinkSummary {
            inbound_count,
            outbound_count,
            inbound,
            outbound,
        })
    })
}

#[tauri::command]
pub fn file_link_summary(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> AppResult<FileLinkSummary> {
    file_link_summary_inner(state.inner(), &path)
}

#[cfg(test)]
mod file_io_pipeline_tests {
    use super::*;
    use crate::crypto::classified_io;
    use crate::crypto::vault_key::{VaultKey, VAULT_KEY, VAULT_KEY_TEST_LOCK};
    use crate::storage::db::Database;
    use crate::storage::migrate::migrate_up;
    use std::fs;
    use std::sync::OnceLock;
    use tempfile::tempdir;

    static INIT_KEY: OnceLock<()> = OnceLock::new();
    fn ensure_vault_key() {
        INIT_KEY.get_or_init(|| {
            let _ = VAULT_KEY.get_or_init(|| std::sync::RwLock::new(VaultKey::new()));
        });
    }

    fn unlock_test_vault(vault: &std::path::Path) {
        ensure_vault_key();
        VaultKey::setup("test-pass", vault).unwrap();
        VAULT_KEY
            .get()
            .unwrap()
            .write()
            .unwrap()
            .unlock("test-pass", vault)
            .unwrap();
    }

    #[test]
    fn file_list_sql_excludes_classified_paths() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            migrate_up(conn)?;
            conn.execute(
                "INSERT INTO files (path, title, content_hash, created_at, updated_at)
                 VALUES ('notes/a.md', 'A', 'h1', '2020-01-01', '2020-01-01'),
                        ('.classified/secret.md', 'S', 'h2', '2020-01-01', '2020-01-01')",
                [],
            )?;
            let mut stmt = conn.prepare(
                "SELECT path FROM files
                 WHERE id IN (SELECT MAX(id) FROM files GROUP BY path)
                   AND path NOT LIKE '.iris/%'
                   AND path NOT LIKE '.classified/%'
                 ORDER BY path",
            )?;
            let paths: Vec<String> = stmt.query_map([], |row| row.get(0))?.flatten().collect();
            assert_eq!(paths, vec!["notes/a.md".to_string()]);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn file_list_inner_hides_missing_plain_note_after_classified_move() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(vault.join(".classified")).unwrap();
        fs::write(vault.join(".classified/moved.md"), "# Secret").unwrap();
        let state = AppState::new(dir.path().join("data")).unwrap();
        state.set_vault(vault).unwrap();
        state
            .db
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO files (path, title, content_hash, created_at, updated_at)
                     VALUES ('notes/moved.md', 'Moved', 'h1', '2020-01-01', '2020-01-01'),
                            ('.classified/moved.md', 'Moved', 'h2', '2020-01-02', '2020-01-02')",
                    [],
                )?;
                Ok(())
            })
            .unwrap();

        // prune has moved off the file_list hot path — explicitly prune
        state
            .db
            .with_conn(|conn| {
                crate::indexer::scan::prune_stale_file_indexes(conn, &state.vault_path().unwrap())
            })
            .unwrap();

        let files = file_list_inner(&state, None, None).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn file_signature_inner_reports_hash_size_modified_and_lock_state() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(vault.join("notes")).unwrap();
        fs::write(
            vault.join("notes/sig.md"),
            b"# Signature

Body",
        )
        .unwrap();
        let state = AppState::new(dir.path().join("data")).unwrap();
        state.set_vault(vault).unwrap();
        state
            .db
            .with_conn(|conn| {
                migrate_up(conn)?;
                conn.execute(
                    "INSERT INTO files (path, title, content_hash, created_at, updated_at, is_locked)
                     VALUES ('notes/sig.md', 'Sig', 'old', '2020-01-01', '2020-01-01', 0)",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        set_file_lock(&state, "notes/sig.md", true).unwrap();

        let signature = file_signature_inner(&state, "notes/sig.md", false).unwrap();

        assert_eq!(signature.byte_length, 17);
        assert_eq!(signature.content_hash.len(), 64);
        assert!(signature.modified_ms.is_some());
        assert!(signature.is_locked);
    }
    #[test]
    fn file_read_validation_requires_classified_opt_in() {
        assert!(validate_file_read_path("notes/a.md", false).is_ok());

        let err = validate_file_read_path(".classified/secret.md", false).unwrap_err();
        assert!(err.to_string().contains("涉密笔记只能从涉密保险库打开"));

        assert!(validate_file_read_path(".classified/secret.md", true).is_ok());
        assert!(validate_file_read_path(".CLASSIFIED/secret.md", true).is_err());
        assert!(validate_file_read_path(".IRIS/versions/1.md", false).is_err());
    }

    #[test]
    fn set_file_lock_persists_flag() {
        let dir = tempdir().unwrap();
        let state = AppState::new(dir.path().join("data")).unwrap();
        state
            .db
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO files (path, title, content_hash, created_at, updated_at, is_locked)
                     VALUES ('notes/a.md', 'A', 'h1', '2020-01-01', '2020-01-01', 0)",
                    [],
                )?;
                Ok(())
            })
            .unwrap();

        set_file_lock(&state, "notes/a.md", true).unwrap();
        assert!(query_is_locked(&state.db, "notes/a.md").unwrap());

        let err = set_file_lock(&state, ".classified/secret.md", true).unwrap_err();
        assert!(err.to_string().contains("只能操作用户笔记"));
    }

    #[test]
    fn query_is_locked_defaults_false_when_missing() {
        let db = Database::open_in_memory().unwrap();
        assert!(!query_is_locked(&db, "missing.md").unwrap());
    }

    #[test]
    fn file_rename_inner_prunes_stale_target_index_before_move() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        fs::write(
            vault.join("old.md"),
            "# Old

Body",
        )
        .unwrap();
        let state = AppState::new(dir.path().join("data")).unwrap();
        state.set_vault(vault.clone()).unwrap();
        state
            .db
            .with_conn(|conn| {
                index_file_with_embed(conn, &vault, &vault.join("old.md"), IndexEmbeddingMode::Skip)?;
                conn.execute(
                    "INSERT INTO files (path, title, content_hash, created_at, updated_at)
                     VALUES ('new.md', 'Stale', 'stale', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    [],
                )?;
                Ok(())
            })
            .unwrap();

        let old_id: i64 = state
            .db
            .with_read_conn(|conn| {
                conn.query_row("SELECT id FROM files WHERE path = 'old.md'", [], |row| {
                    row.get(0)
                })
                .map_err(Into::into)
            })
            .unwrap();

        let entry =
            file_rename_inner(state.clone(), "old.md".to_string(), "new.md".to_string()).unwrap();

        assert_eq!(entry.id, old_id);
        assert_eq!(entry.path, "new.md");
        assert!(vault.join("new.md").is_file());
    }

    #[test]
    fn decode_file_content_decrypts_csef_when_unlocked() {
        let _guard = VAULT_KEY_TEST_LOCK.lock().unwrap();
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        unlock_test_vault(&vault);

        let key = *VAULT_KEY.get().unwrap().read().unwrap().key().unwrap();
        let enc = classified_io::encrypt_cef(b"# Secret", &key).unwrap();
        let content = decode_file_content(&enc).unwrap();
        assert_eq!(content, "# Secret");
    }

    #[test]
    fn decode_file_content_rejects_invalid_utf8() {
        let err = decode_file_content(&[0xff, b'#', b' ', b'B']).unwrap_err();
        assert!(err.to_string().contains("UTF-8"));
    }

    #[test]
    fn encode_file_payload_encrypts_classified_path() {
        let _guard = VAULT_KEY_TEST_LOCK.lock().unwrap();
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        unlock_test_vault(&vault);

        let data = encode_file_payload(".classified/secret.md", "# Hi").unwrap();
        assert!(classified_io::has_csef_magic(&data));

        let plain = encode_file_payload("notes/open.md", "# Hi").unwrap();
        assert!(!classified_io::has_csef_magic(&plain));
        assert_eq!(plain, b"# Hi");

        let err = encode_file_payload(".CLASSIFIED/secret.md", "# Hi").unwrap_err();
        assert!(err.to_string().contains("classified"));
    }

    #[test]
    fn file_rename_inner_returns_fallback_entry_when_index_refresh_fails_after_move() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("old.md"), "# Old\n\nBody").unwrap();
        let state = AppState::new(dir.path().join("data")).unwrap();
        state.set_vault(vault.clone()).unwrap();
        state
            .db
            .with_conn(|conn| {
                index_file_with_embed(
                    conn,
                    &vault,
                    &vault.join("old.md"),
                    IndexEmbeddingMode::Skip,
                )?;
                conn.execute_batch(
                    "CREATE TRIGGER fail_index_refresh
                     BEFORE UPDATE OF title ON files
                     WHEN NEW.path = 'new.md'
                     BEGIN
                       SELECT RAISE(ABORT, 'simulated index failure');
                     END;",
                )?;
                Ok(())
            })
            .unwrap();
        fs::write(vault.join("old.md"), "# Changed\n\nBody").unwrap();

        let entry = file_rename_inner(state, "old.md".to_string(), "new.md".to_string()).unwrap();

        assert_eq!(entry.id, 0);
        assert_eq!(entry.path, "new.md");
        assert!(!vault.join("old.md").exists());
        assert_eq!(
            fs::read_to_string(vault.join("new.md")).unwrap(),
            "# Changed\n\nBody"
        );
    }
    #[test]
    fn wikilink_rewrite_skips_stale_inbound_sources() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        fs::write(
            vault.join("target.md"),
            "# Target
",
        )
        .unwrap();
        let state = AppState::new(dir.path().join("data")).unwrap();
        state.set_vault(vault.clone()).unwrap();
        state
            .db
            .with_conn(|conn| {
                migrate_up(conn)?;
                conn.execute_batch(
                    "INSERT INTO files (id, path, title, content_hash, created_at, updated_at)
                     VALUES (1, 'target.md', 'Target', 'h1', '2020-01-01', '2020-01-01'),
                            (2, 'missing-source.md', 'Missing', 'h2', '2020-01-02', '2020-01-02');
                     INSERT INTO links (source_id, target_id, context)
                     VALUES (2, 1, 'Missing links [[Target]]');",
                )?;
                Ok(())
            })
            .unwrap();

        let modified = cascade_rewrite_wikilinks_on_disk(
            &state,
            &vault,
            "target.md",
            "folder/target.md",
            "target",
            "target",
        )
        .unwrap();

        assert!(modified.is_empty());
    }

    #[test]
    fn file_link_summary_counts_inbound_and_outbound_links() {
        let dir = tempdir().unwrap();
        let state = AppState::new(dir.path().join("data")).unwrap();
        state
            .db
            .with_conn(|conn| {
                migrate_up(conn)?;
                conn.execute_batch(
                    "INSERT INTO files (id, path, title, content_hash, created_at, updated_at)
                     VALUES (1, 'target.md', 'Target', 'h1', '2020-01-01', '2020-01-01'),
                            (2, 'source-a.md', 'Source A', 'h2', '2020-01-02', '2020-01-02'),
                            (3, 'source-b.md', 'Source B', 'h3', '2020-01-03', '2020-01-03'),
                            (4, 'out.md', 'Outbound', 'h4', '2020-01-04', '2020-01-04'),
                            (5, '.classified/secret.md', 'Secret', 'h5', '2020-01-05', '2020-01-05');
                     INSERT INTO links (source_id, target_id, context)
                     VALUES (2, 1, 'A links [[Target]]'),
                            (3, 1, 'B links [[Target]]'),
                            (5, 1, 'Secret links [[Target]]'),
                            (1, 4, 'Target links [[Outbound]]'),
                            (1, 5, 'Target links [[Secret]]');",
                )?;
                Ok(())
            })
            .unwrap();

        let summary = file_link_summary_inner(&state, "target.md").unwrap();

        assert_eq!(summary.inbound_count, 2);
        assert_eq!(summary.outbound_count, 1);
        assert_eq!(
            summary
                .inbound
                .iter()
                .map(|entry| entry.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Source B", "Source A"]
        );
        assert_eq!(summary.outbound[0].path, "out.md");
        assert_eq!(
            summary.outbound[0].context.as_deref(),
            Some("Target links [[Outbound]]")
        );
    }

    #[test]
    fn file_link_summary_returns_empty_for_unlinked_note() {
        let dir = tempdir().unwrap();
        let state = AppState::new(dir.path().join("data")).unwrap();
        state
            .db
            .with_conn(|conn| {
                migrate_up(conn)?;
                conn.execute(
                    "INSERT INTO files (id, path, title, content_hash, created_at, updated_at)
                     VALUES (1, 'lonely.md', 'Lonely', 'h1', '2020-01-01', '2020-01-01')",
                    [],
                )?;
                Ok(())
            })
            .unwrap();

        let summary = file_link_summary_inner(&state, "lonely.md").unwrap();

        assert_eq!(summary.inbound_count, 0);
        assert_eq!(summary.outbound_count, 0);
        assert!(summary.inbound.is_empty());
        assert!(summary.outbound.is_empty());
    }
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

    #[test]
    fn vault_asset_path_must_live_under_assets() {
        assert!(is_vault_asset_path("assets/photo.png"));
        assert!(!is_vault_asset_path("notes/x.md"));
        assert!(!is_vault_asset_path("assets/../secret.png"));
        assert!(!is_vault_asset_path("assets/"));
    }

    #[test]
    fn vault_runtime_cleanup_is_disabled_for_vault_switch_retention() {
        let sql = vault_runtime_cleanup_sql();
        assert!(
            sql.trim().is_empty(),
            "vault switching must not delete persisted runtime rows automatically"
        );
    }

    #[test]
    fn folder_rename_reindex_paths_are_limited_to_moved_and_modified_files() {
        let paths = folder_rename_reindex_paths(
            "old",
            "new",
            &[
                "old/a.md".to_string(),
                "old/deep/b.md".to_string(),
                "other/ignore.md".to_string(),
            ],
            &[
                "refs/source.md".to_string(),
                "new/a.md".to_string(),
                "refs/source.md".to_string(),
                "old/ref.md".to_string(),
            ],
        );
        assert_eq!(
            paths,
            vec![
                "new/a.md".to_string(),
                "new/deep/b.md".to_string(),
                "new/ref.md".to_string(),
                "refs/source.md".to_string()
            ]
        );
    }
}
