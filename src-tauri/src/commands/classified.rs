use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use zeroize::Zeroizing;

use crate::app::AppState;
use crate::crypto::classified_io;
use crate::crypto::vault_key::{VaultKey, VAULT_KEY};
use crate::error::{AppError, AppResult};
use crate::indexer::fts::delete_fts;
use crate::indexer::scan::remove_file_index;
use crate::storage::note_write::NoteWriteService;
use crate::storage::paths::{
    is_user_note_path, read_file_lossy, relative_path, resolve_vault_path,
};

/// Vault-relative path under `.classified/` (or the directory itself).
pub(crate) fn is_classified_path(relative: &str) -> bool {
    let normalized = relative.replace('\\', "/");
    normalized.starts_with(".classified/") || normalized == ".classified"
}

fn is_noncanonical_classified_root(relative: &str) -> bool {
    let normalized = relative.replace('\\', "/");
    let first = normalized.split('/').next().unwrap_or("");
    first.eq_ignore_ascii_case(".classified") && first != ".classified"
}

fn vault_key_read() -> AppResult<std::sync::RwLockReadGuard<'static, VaultKey>> {
    VAULT_KEY
        .get()
        .ok_or_else(|| AppError::msg("保险库未初始化"))?
        .read()
        .map_err(|e| AppError::msg(format!("VAULT_KEY lock error: {e}")))
}

fn vault_key_write() -> AppResult<std::sync::RwLockWriteGuard<'static, VaultKey>> {
    VAULT_KEY
        .get()
        .ok_or_else(|| AppError::msg("保险库未初始化"))?
        .write()
        .map_err(|e| AppError::msg(format!("VAULT_KEY lock error: {e}")))
}

fn require_unlocked() -> AppResult<std::sync::RwLockReadGuard<'static, VaultKey>> {
    let vk = vault_key_read()?;
    if !vk.is_unlocked() {
        return Err(AppError::msg("保险库未解锁"));
    }
    Ok(vk)
}

/// Normalize import target to a vault-relative `.classified/...` directory path.
fn normalize_classified_target(target_folder: &str) -> AppResult<String> {
    let trimmed = target_folder
        .replace('\\', "/")
        .trim()
        .trim_matches('/')
        .to_string();
    if is_noncanonical_classified_root(&trimmed) {
        return Err(AppError::msg("Invalid classified path casing"));
    }
    let rel = if trimmed.is_empty() || trimmed == ".classified" {
        ".classified".to_string()
    } else if trimmed.starts_with(".classified/") {
        trimmed
    } else {
        format!(".classified/{trimmed}")
    };
    if !is_classified_path(&rel) {
        return Err(AppError::msg("导入目标必须在 .classified/ 目录内"));
    }
    if Path::new(&rel).components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return Err(AppError::msg("导入目标路径无效"));
    }
    Ok(rel)
}

/// Resolve a folder under `.classified/` for listing; rejects traversal.
fn classified_list_root(vault: &Path, folder: Option<&str>) -> AppResult<PathBuf> {
    let classified_dir = vault.join(".classified");
    let scan_root = match folder {
        None | Some("") => classified_dir.clone(),
        Some(f) => {
            let normalized = f.replace('\\', "/").trim_matches('/').to_string();
            if normalized.contains("..") {
                return Err(AppError::msg("Invalid folder path"));
            }
            if is_noncanonical_classified_root(&normalized) {
                return Err(AppError::msg("Invalid classified path casing"));
            }
            let rel = if normalized.starts_with(".classified/") {
                normalized
            } else {
                format!(".classified/{normalized}")
            };
            if !is_classified_path(&rel) {
                return Err(AppError::msg("只能浏览 .classified/ 目录"));
            }
            resolve_vault_path(vault, &rel)?
        }
    };
    let classified_canon = classified_dir
        .canonicalize()
        .unwrap_or_else(|_| classified_dir.clone());
    let scan_canon = scan_root
        .canonicalize()
        .unwrap_or_else(|_| scan_root.clone());
    if !scan_canon.starts_with(&classified_canon) {
        return Err(AppError::msg("Path outside classified vault"));
    }
    Ok(scan_root)
}

fn classified_setup_inner(state: &AppState, password: &str) -> AppResult<()> {
    let vault = state.vault_path()?;
    if VaultKey::is_initialized(&vault) {
        return Err(AppError::msg("保险库已设置密码"));
    }
    VaultKey::setup(password, &vault)?;
    let mut vk = vault_key_write()?;
    vk.unlock(password, &vault)?;
    Ok(())
}

fn classified_unlock_inner(state: &AppState, password: &str) -> AppResult<()> {
    let vault = state.vault_path()?;
    if !VaultKey::is_initialized(&vault) {
        return Err(AppError::msg("保险库尚未设置密码"));
    }
    let mut vk = vault_key_write()?;
    // VaultKey::unlock does Argon2 derivation + decryption; if this fails,
    // the caller records the failure via brute_force.record_failure.
    vk.unlock(password, &vault)
}

async fn classified_unlock_async_inner(
    state: &Arc<AppState>,
    password: &str,
    vault_path: &std::path::Path,
) -> AppResult<()> {
    state.brute_force.check(vault_path)?;

    match classified_unlock_inner(state, password) {
        Ok(()) => {
            state.brute_force.record_success(vault_path);
            Ok(())
        }
        Err(e) => {
            state.brute_force.record_failure(vault_path)?;
            // Re-check for backoff delay after record_failure
            let err = state.brute_force.check(vault_path);
            match err {
                Ok(()) => Err(e),
                Err(backoff) => Err(backoff),
            }
        }
    }
}

fn classified_lock_inner() -> AppResult<()> {
    let mut vk = vault_key_write()?;
    vk.lock();
    crate::ai_runtime::classified_session::classified_ai_cache_clear()?;
    crate::ai_runtime::classified_retrieval::clear_classified_index();
    Ok(())
}

fn classified_status_inner(state: &AppState) -> AppResult<String> {
    let vault = state.vault_path()?;
    if !VaultKey::is_initialized(&vault) {
        return Ok("needs_setup".to_string());
    }
    let vk = vault_key_read()?;
    if vk.is_unlocked() {
        Ok("unlocked".to_string())
    } else {
        Ok("locked".to_string())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifiedFileEntry {
    pub path: String,
    pub is_dir: bool,
}

fn classified_files_inner(
    state: &AppState,
    folder: Option<String>,
) -> AppResult<Vec<ClassifiedFileEntry>> {
    let _vk = require_unlocked()?;
    let vault = state.vault_path()?;
    let scan_root = classified_list_root(&vault, folder.as_deref())?;

    let mut entries = Vec::new();
    if scan_root.is_dir() {
        for entry in fs::read_dir(&scan_root)? {
            let entry = entry?;
            let path = entry.path();
            let rel = relative_path(&vault, &path)?;

            // Hide .iris-ai directory from file listings
            if path.is_dir() && path.file_name().is_some_and(|n| n == ".iris-ai") {
                continue;
            }

            entries.push(ClassifiedFileEntry {
                is_dir: path.is_dir(),
                path: rel,
            });
        }
        entries.sort_by(|a, b| a.path.cmp(&b.path));
    }
    Ok(entries)
}

fn classified_import_inner(
    state: &Arc<AppState>,
    app: Option<&AppHandle>,
    path: &str,
    target_folder: &str,
) -> AppResult<()> {
    let _vk = require_unlocked()?;
    if !is_user_note_path(path) || is_classified_path(path) {
        return Err(AppError::msg("只能导入用户笔记"));
    }

    let vault = state.vault_path()?;
    let src = resolve_vault_path(&vault, path)?;
    let target_rel = normalize_classified_target(target_folder)?;
    let file_name = src
        .file_name()
        .ok_or_else(|| AppError::msg("invalid source path"))?;

    let dest_rel = if target_rel == ".classified" {
        format!(".classified/{}", file_name.to_string_lossy())
    } else {
        format!("{}/{}", target_rel, file_name.to_string_lossy())
    };
    fs::create_dir_all(vault.join(&target_rel))?;
    let dest = resolve_vault_path(&vault, &dest_rel)?;

    if dest.exists() {
        return Err(AppError::msg(format!(
            "目标位置已存在同名文件: {}",
            file_name.to_string_lossy()
        )));
    }

    let content = read_file_lossy(&src)?;
    NoteWriteService::create(
        state,
        &dest_rel,
        &content,
        crate::indexer::scan::IndexEmbeddingMode::Queue(state),
    )?;
    fs::remove_file(&src)?;

    if state
        .db
        .with_conn(|conn| remove_file_index(conn, path))
        .is_err()
    {
        tracing::warn!(
            result_code = "classified_import_index_degraded",
            "classified import completed with derived index degradation"
        );
    }

    if let Some(app) = app {
        let _ = app.emit("classified:file_taken", serde_json::json!({ "path": path }));
    }

    Ok(())
}

fn classified_export_inner(
    state: &Arc<AppState>,
    path: &str,
    target_folder: &str,
    overwrite: bool,
) -> AppResult<()> {
    let vk = require_unlocked()?;
    if !is_classified_path(path) {
        return Err(AppError::msg("只能导出涉密文件"));
    }

    let vault = state.vault_path()?;
    let src = resolve_vault_path(&vault, path)?;

    let target_rel = target_folder
        .replace('\\', "/")
        .trim()
        .trim_matches('/')
        .to_string();
    if target_rel.is_empty() {
        return Err(AppError::msg("目标文件夹不能为空"));
    }
    if !is_user_note_path(&target_rel) || is_classified_path(&target_rel) {
        return Err(AppError::msg("只能导出到普通笔记目录"));
    }

    let file_name = src
        .file_name()
        .ok_or_else(|| AppError::msg("invalid source path"))?;
    let dest_rel = format!("{}/{}", target_rel, file_name.to_string_lossy());
    if !is_user_note_path(&dest_rel) {
        return Err(AppError::msg("导出目标路径无效"));
    }

    fs::create_dir_all(vault.join(&target_rel))?;
    let dest = resolve_vault_path(&vault, &dest_rel)?;

    if dest.exists() && !overwrite {
        return Err(AppError::msg(format!(
            "目标位置已存在同名文件: {}",
            file_name.to_string_lossy()
        )));
    }
    if dest.is_dir() {
        return Err(AppError::msg("导出目标不能是文件夹"));
    }
    if src.is_dir() {
        return Err(AppError::msg("请选择文件而非文件夹进行导出"));
    }

    let raw = fs::read(&src)?;
    let content = if classified_io::has_csef_magic(&raw) {
        let key = vk.key()?;
        String::from_utf8(classified_io::decrypt_cef(&raw, key)?)
            .map_err(|_| AppError::msg("File is not valid UTF-8"))?
    } else {
        std::str::from_utf8(&raw)
            .map(str::to_owned)
            .map_err(|_| AppError::msg("File is not valid UTF-8"))?
    };

    NoteWriteService::write(
        state,
        &dest_rel,
        &content,
        crate::indexer::scan::IndexEmbeddingMode::Queue(state),
    )?;
    fs::remove_file(&src)?;

    if state
        .db
        .with_conn(|conn| remove_file_index(conn, path))
        .is_err()
    {
        tracing::warn!(
            result_code = "classified_export_index_degraded",
            "classified export completed with derived index degradation"
        );
    }

    Ok(())
}

fn remove_classified_metadata(state: &AppState, path: &str) -> AppResult<()> {
    let normalized = path.replace('\\', "/").trim_end_matches('/').to_string();
    let like = format!("{normalized}/%");
    state.db.with_conn(|conn| {
        let paths: Vec<String> = {
            let mut stmt =
                conn.prepare("SELECT path FROM files WHERE path = ?1 OR path LIKE ?2")?;
            let rows = stmt
                .query_map(rusqlite::params![normalized, like], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for path in paths {
            remove_file_index(conn, &path)?;
        }
        Ok(())
    })
}

fn rename_classified_metadata(state: &AppState, old_path: &str, new_path: &str) -> AppResult<()> {
    let old = old_path
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    let new = new_path
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    let old_like = format!("{old}/%");
    state.db.with_conn(|conn| {
        let paths: Vec<String> = {
            let mut stmt =
                conn.prepare("SELECT path FROM files WHERE path = ?1 OR path LIKE ?2")?;
            let rows = stmt
                .query_map(rusqlite::params![old, old_like], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for path in paths {
            let next = if path == old {
                new.clone()
            } else {
                format!("{new}{}", &path[old.len()..])
            };
            let title = display_title(&next);
            delete_fts(conn, &path)?;
            delete_fts(conn, &next)?;
            conn.execute(
                "UPDATE files
                 SET path = ?1, title = ?2, updated_at = datetime('now')
                 WHERE path = ?3",
                rusqlite::params![next, title, path],
            )?;
        }
        Ok(())
    })
}

fn display_title(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn classified_delete_inner(state: &AppState, path: &str) -> AppResult<()> {
    let _vk = require_unlocked()?;
    if !is_classified_path(path) {
        return Err(AppError::msg("只能删除涉密文件"));
    }
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, path)?;
    if abs.is_dir() {
        fs::remove_dir_all(&abs)?;
    } else if abs.exists() {
        fs::remove_file(&abs)?;
    } else {
        return Err(AppError::msg("文件不存在"));
    }
    remove_classified_metadata(state, path)?;
    Ok(())
}

fn classified_mkdir_inner(state: &AppState, folder: &str) -> AppResult<()> {
    let _vk = require_unlocked()?;
    let target_rel = normalize_classified_target(folder)?;
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &target_rel)?;
    fs::create_dir_all(&abs)?;
    Ok(())
}

fn classified_rename_inner(state: &AppState, path: &str, new_path: &str) -> AppResult<()> {
    let _vk = require_unlocked()?;
    if !is_classified_path(path) || !is_classified_path(new_path) {
        return Err(AppError::msg("只能重命名涉密路径"));
    }
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, path)?;
    let new_abs = resolve_vault_path(&vault, new_path)?;
    if new_abs.exists() {
        return Err(AppError::msg("目标路径已存在"));
    }
    if let Some(parent) = new_abs.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&abs, &new_abs)?;
    rename_classified_metadata(state, path, new_path)?;
    Ok(())
}

#[tauri::command]
pub fn classified_setup(state: State<'_, Arc<AppState>>, password: String) -> AppResult<()> {
    let password = Zeroizing::new(password);
    classified_setup_inner(state.inner(), &password)
}

#[tauri::command]
pub async fn classified_unlock(state: State<'_, Arc<AppState>>, password: String) -> AppResult<()> {
    let password = Zeroizing::new(password);
    let vault = state.vault_path()?;
    classified_unlock_async_inner(state.inner(), &password, &vault).await
}

#[tauri::command]
pub fn classified_lock() -> AppResult<()> {
    classified_lock_inner()
}

#[tauri::command]
pub fn classified_status(state: State<'_, Arc<AppState>>) -> AppResult<String> {
    classified_status_inner(state.inner())
}

#[tauri::command]
pub fn classified_files(
    state: State<'_, Arc<AppState>>,
    folder: Option<String>,
) -> AppResult<Vec<ClassifiedFileEntry>> {
    classified_files_inner(state.inner(), folder)
}

#[tauri::command]
pub fn classified_import(
    app_handle: AppHandle,
    state: State<'_, Arc<AppState>>,
    path: String,
    target_folder: String,
) -> AppResult<()> {
    classified_import_inner(state.inner(), Some(&app_handle), &path, &target_folder)
}

#[tauri::command]
pub fn classified_export(
    state: State<'_, Arc<AppState>>,
    path: String,
    target_folder: String,
    overwrite: Option<bool>,
) -> AppResult<()> {
    classified_export_inner(
        state.inner(),
        &path,
        &target_folder,
        overwrite.unwrap_or(false),
    )
}

#[tauri::command]
pub fn classified_delete(state: State<'_, Arc<AppState>>, path: String) -> AppResult<()> {
    classified_delete_inner(state.inner(), &path)
}

#[tauri::command]
pub fn classified_mkdir(state: State<'_, Arc<AppState>>, folder: String) -> AppResult<()> {
    classified_mkdir_inner(state.inner(), folder.as_str())
}

#[tauri::command]
pub fn classified_rename(
    state: State<'_, Arc<AppState>>,
    path: String,
    new_path: String,
) -> AppResult<()> {
    classified_rename_inner(state.inner(), &path, &new_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::vault_key::{init_vault_key, VAULT_KEY_TEST_LOCK};
    use crate::indexer::scan::index_file;
    use std::sync::OnceLock;
    use tempfile::tempdir;

    static INIT_KEY: OnceLock<()> = OnceLock::new();

    fn key_test_lock() -> std::sync::MutexGuard<'static, ()> {
        VAULT_KEY_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn ensure_vault_key() {
        INIT_KEY.get_or_init(|| {
            init_vault_key();
        });
    }

    fn test_state() -> (tempfile::TempDir, Arc<AppState>) {
        ensure_vault_key();
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let state = AppState::new(dir.path().join("data")).unwrap();
        state.set_vault(vault).unwrap();
        (dir, state)
    }

    fn write_note(vault: &Path, rel: &str, content: &str) {
        let abs = vault.join(rel);
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&abs, content).unwrap();
    }

    #[test]
    fn is_classified_path_detects_classified_roots() {
        assert!(is_classified_path(".classified"));
        assert!(is_classified_path(".classified/secret.md"));
        assert!(!is_classified_path("notes/open.md"));
    }

    #[test]
    fn classified_status_needs_setup_before_password() {
        let (_dir, state) = test_state();
        assert_eq!(classified_status_inner(&state).unwrap(), "needs_setup");
    }

    #[test]
    fn classified_setup_unlock_and_lock_lifecycle() {
        let _guard = key_test_lock();
        let (_dir, state) = test_state();

        classified_setup_inner(&state, "test-pass").unwrap();
        assert_eq!(classified_status_inner(&state).unwrap(), "unlocked");

        let err = classified_setup_inner(&state, "other").unwrap_err();
        assert!(err.to_string().contains("已设置"));

        classified_lock_inner().unwrap();
        assert_eq!(classified_status_inner(&state).unwrap(), "locked");

        classified_unlock_inner(&state, "test-pass").unwrap();
        assert_eq!(classified_status_inner(&state).unwrap(), "unlocked");

        let err = classified_unlock_inner(&state, "wrong").unwrap_err();
        assert!(err.to_string().contains("密码不正确"));
    }

    #[test]
    fn classified_files_requires_unlock() {
        let _guard = key_test_lock();
        let (_dir, state) = test_state();
        classified_setup_inner(&state, "test-pass").unwrap();
        classified_lock_inner().unwrap();

        let err = classified_files_inner(&state, None).unwrap_err();
        assert!(err.to_string().contains("未解锁"));
    }

    #[test]
    fn classified_files_lists_classified_entries() {
        let _guard = key_test_lock();
        let (_dir, state) = test_state();
        let vault = state.vault_path().unwrap();
        classified_setup_inner(&state, "test-pass").unwrap();

        fs::create_dir_all(vault.join(".classified/inbox")).unwrap();
        write_note(&vault, ".classified/secret.md", "# Secret");
        write_note(&vault, ".classified/inbox/note.md", "# Inbox");

        let entries = classified_files_inner(&state, None).unwrap();
        let paths: Vec<_> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&".classified/inbox"));
        assert!(paths.contains(&".classified/secret.md"));

        let inbox = classified_files_inner(&state, Some("inbox".to_string())).unwrap();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].path, ".classified/inbox/note.md");
    }

    #[test]
    fn classified_import_encrypts_and_removes_index() {
        let _guard = key_test_lock();
        let (_dir, state) = test_state();
        let vault = state.vault_path().unwrap();
        classified_setup_inner(&state, "test-pass").unwrap();

        write_note(&vault, "notes/open.md", "# Open\n\nBody.");
        state
            .db
            .with_conn(|conn| index_file(conn, &vault, &vault.join("notes/open.md")))
            .unwrap();

        classified_import_inner(&state, None, "notes/open.md", ".classified").unwrap();

        assert!(!vault.join("notes/open.md").exists());
        let dest = vault.join(".classified/open.md");
        assert!(dest.exists());
        let raw = fs::read(&dest).unwrap();
        assert!(classified_io::has_csef_magic(&raw));

        let count: i64 = state
            .db
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM files WHERE path = 'notes/open.md'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn classified_export_roundtrip_to_plaintext() {
        let _guard = key_test_lock();
        let (_dir, state) = test_state();
        let vault = state.vault_path().unwrap();
        classified_setup_inner(&state, "test-pass").unwrap();

        write_note(&vault, "notes/source.md", "# Source\n\nContent.");
        classified_import_inner(&state, None, "notes/source.md", ".classified").unwrap();

        classified_export_inner(&state, ".classified/source.md", "exported", false).unwrap();

        let plain = fs::read_to_string(vault.join("exported/source.md")).unwrap();
        assert_eq!(plain, "# Source\n\nContent.");
        assert!(!vault.join(".classified/source.md").exists());

        let indexed: i64 = state
            .db
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM files WHERE path = 'exported/source.md'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(indexed, 1);
    }

    #[test]
    fn classified_export_can_overwrite_after_explicit_confirmation() {
        let _guard = key_test_lock();
        let (_dir, state) = test_state();
        let vault = state.vault_path().unwrap();
        classified_setup_inner(&state, "test-pass").unwrap();

        write_note(&vault, "notes/source.md", "# Source\n\nContent.");
        classified_import_inner(&state, None, "notes/source.md", ".classified").unwrap();
        write_note(&vault, "exported/source.md", "# Old");
        state
            .db
            .with_conn(|conn| index_file(conn, &vault, &vault.join("exported/source.md")))
            .unwrap();

        let blocked = classified_export_inner(&state, ".classified/source.md", "exported", false)
            .unwrap_err();
        assert!(blocked.to_string().contains("目标位置已存在同名文件"));

        classified_export_inner(&state, ".classified/source.md", "exported", true).unwrap();

        let plain = fs::read_to_string(vault.join("exported/source.md")).unwrap();
        assert_eq!(plain, "# Source\n\nContent.");
        assert!(!vault.join(".classified/source.md").exists());

        let indexed: i64 = state
            .db
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM files WHERE path = 'exported/source.md'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(indexed, 1);
    }

    #[test]
    fn classified_import_keeps_markdown_move_successful_when_index_cleanup_fails() {
        let _guard = key_test_lock();
        let (_dir, state) = test_state();
        let vault = state.vault_path().unwrap();
        classified_setup_inner(&state, "test-pass").unwrap();
        write_note(&vault, "notes/source.md", "# Source\n\nContent.");
        state
            .db
            .with_conn(|conn| {
                index_file(conn, &vault, &vault.join("notes/source.md"))?;
                conn.execute_batch(
                    "CREATE TRIGGER fail_source_index_cleanup
                     BEFORE DELETE ON files
                     WHEN OLD.path = 'notes/source.md'
                     BEGIN
                       SELECT RAISE(ABORT, 'simulated index cleanup failure');
                     END;",
                )?;
                Ok(())
            })
            .unwrap();

        classified_import_inner(&state, None, "notes/source.md", ".classified").unwrap();

        assert!(!vault.join("notes/source.md").exists());
        assert!(classified_io::has_csef_magic(
            &fs::read(vault.join(".classified/source.md")).unwrap()
        ));
    }

    #[test]
    fn classified_export_keeps_markdown_move_successful_when_index_refresh_fails() {
        let _guard = key_test_lock();
        let (_dir, state) = test_state();
        let vault = state.vault_path().unwrap();
        classified_setup_inner(&state, "test-pass").unwrap();
        write_note(&vault, "notes/source.md", "# Source\n\nContent.");
        classified_import_inner(&state, None, "notes/source.md", ".classified").unwrap();
        write_note(&vault, "exported/source.md", "# Old");
        state
            .db
            .with_conn(|conn| {
                index_file(conn, &vault, &vault.join("exported/source.md"))?;
                conn.execute_batch(
                    "CREATE TRIGGER fail_export_index_refresh
                     BEFORE INSERT ON files
                     WHEN NEW.path = 'exported/source.md'
                     BEGIN
                       SELECT RAISE(ABORT, 'simulated index refresh failure');
                     END;",
                )?;
                Ok(())
            })
            .unwrap();

        classified_export_inner(&state, ".classified/source.md", "exported", true).unwrap();

        assert_eq!(
            fs::read_to_string(vault.join("exported/source.md")).unwrap(),
            "# Source\n\nContent."
        );
        assert!(!vault.join(".classified/source.md").exists());
    }

    #[test]
    fn classified_import_rejects_non_user_paths() {
        let _guard = key_test_lock();
        let (_dir, state) = test_state();
        classified_setup_inner(&state, "test-pass").unwrap();

        let err = classified_import_inner(&state, None, ".iris/x.md", ".classified").unwrap_err();
        assert!(err.to_string().contains("只能导入用户笔记"));
    }
}
