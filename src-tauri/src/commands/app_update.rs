use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::Connection;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::app::AppState;
use crate::error::{AppError, AppResult};

const STATUS_EVENT: &str = "app-update:status";
const PROGRESS_EVENT: &str = "app-update:progress";
const APP_UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(12);
const APP_UPDATE_NETWORK_ERROR_MESSAGE: &str = "无法连接更新服务器，请检查网络后重试";
const APP_UPDATE_MANIFEST_ERROR_MESSAGE: &str =
    "当前发布暂不支持应用内更新，可前往 GitHub Release 手动下载";
const APP_UPDATE_SIGNATURE_ERROR_MESSAGE: &str = "更新包验证失败";

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppUpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Downloaded,
    ReadyToInstall,
    Unsupported,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateInfo {
    pub current_version: String,
    pub version: String,
    pub pub_date: Option<String>,
    pub notes: Option<String>,
    pub downloaded: bool,
    pub preflight_passed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateCheckResult {
    pub status: AppUpdateStatus,
    pub info: Option<AppUpdateInfo>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateProgressEvent {
    pub phase: String,
    pub chunk_length: usize,
    pub content_length: Option<u64>,
    pub downloaded: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppUpdatePreflightCheckStatus {
    Passed,
    Failed,
    Warning,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdatePreflightCheck {
    pub id: String,
    pub label: String,
    pub status: AppUpdatePreflightCheckStatus,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdatePreflightResult {
    pub ok: bool,
    pub checks: Vec<AppUpdatePreflightCheck>,
}

#[derive(Default)]
pub struct PendingAppUpdate {
    inner: Mutex<PendingAppUpdateInner>,
}

#[derive(Default)]
struct PendingAppUpdateInner {
    update: Option<Update>,
    info: Option<AppUpdateInfo>,
    downloaded_bytes: Option<Vec<u8>>,
    preflight_passed: bool,
}

#[tauri::command]
pub async fn app_update_check_cmd(
    app: AppHandle,
    pending_update: State<'_, PendingAppUpdate>,
) -> AppResult<AppUpdateCheckResult> {
    if !is_supported_updater_target() {
        let result = status_result(
            AppUpdateStatus::Unsupported,
            None,
            Some("当前平台暂不支持应用内更新".to_string()),
        );
        emit_status(&app, &result);
        return Ok(result);
    }

    let checking = status_result(AppUpdateStatus::Checking, None, None);
    emit_status(&app, &checking);

    let updater = match app
        .updater_builder()
        .timeout(APP_UPDATE_CHECK_TIMEOUT)
        .build()
        .map_err(updater_error)
    {
        Ok(updater) => updater,
        Err(err) => {
            let result = status_result(
                AppUpdateStatus::Error,
                None,
                Some(sanitize_check_error_message(&err.to_string())),
            );
            emit_status(&app, &result);
            return Ok(result);
        }
    };

    let update = match updater.check().await.map_err(updater_error) {
        Ok(update) => update,
        Err(err) => {
            let result = status_result(
                AppUpdateStatus::Error,
                None,
                Some(sanitize_check_error_message(&err.to_string())),
            );
            emit_status(&app, &result);
            return Ok(result);
        }
    };

    let Some(update) = update else {
        let result = status_result(AppUpdateStatus::UpToDate, None, None);
        let mut guard = lock_pending(&pending_update)?;
        guard.update = None;
        guard.info = None;
        guard.downloaded_bytes = None;
        guard.preflight_passed = false;
        drop(guard);
        emit_status(&app, &result);
        return Ok(result);
    };

    let info = info_from_update(&update, false, false);
    {
        let mut guard = lock_pending(&pending_update)?;
        guard.update = Some(update);
        guard.info = Some(info.clone());
        guard.downloaded_bytes = None;
        guard.preflight_passed = false;
    }

    let result = status_result(AppUpdateStatus::Available, Some(info), None);
    emit_status(&app, &result);
    Ok(result)
}

#[tauri::command]
pub async fn app_update_download_cmd(
    app: AppHandle,
    pending_update: State<'_, PendingAppUpdate>,
) -> AppResult<AppUpdateCheckResult> {
    let update = {
        let guard = lock_pending(&pending_update)?;
        guard
            .update
            .clone()
            .ok_or_else(|| AppError::msg("没有可下载的待更新版本"))?
    };

    let downloading = status_result(
        AppUpdateStatus::Downloading,
        Some(info_from_update(&update, false, false)),
        None,
    );
    emit_status(&app, &downloading);

    let mut started = false;
    let downloaded = AtomicU64::new(0);
    let bytes = update
        .download(
            |chunk_length, content_length| {
                if !started {
                    started = true;
                    emit_progress(
                        &app,
                        AppUpdateProgressEvent {
                            phase: "started".to_string(),
                            chunk_length: 0,
                            content_length,
                            downloaded: 0,
                        },
                    );
                }

                let downloaded = downloaded.fetch_add(chunk_length as u64, Ordering::Relaxed)
                    + chunk_length as u64;
                emit_progress(
                    &app,
                    AppUpdateProgressEvent {
                        phase: "progress".to_string(),
                        chunk_length,
                        content_length,
                        downloaded,
                    },
                );
            },
            || {
                emit_progress(
                    &app,
                    AppUpdateProgressEvent {
                        phase: "finished".to_string(),
                        chunk_length: 0,
                        content_length: None,
                        downloaded: downloaded.load(Ordering::Relaxed),
                    },
                );
            },
        )
        .await
        .map_err(|err| {
            let message = if err.to_string().to_lowercase().contains("signature") {
                "更新包验证失败".to_string()
            } else {
                "更新下载失败".to_string()
            };
            AppError::msg(format!("{message}：{}", err.sanitized_message_for_update()))
        })?;

    let info = info_from_update(&update, true, false);
    {
        let mut guard = lock_pending(&pending_update)?;
        guard.downloaded_bytes = Some(bytes);
        guard.preflight_passed = false;
        guard.info = Some(info.clone());
    }

    let result = status_result(AppUpdateStatus::Downloaded, Some(info), None);
    emit_status(&app, &result);
    Ok(result)
}

#[tauri::command]
pub fn app_update_preflight_cmd(
    app: AppHandle,
    state: State<'_, std::sync::Arc<AppState>>,
    pending_update: State<'_, PendingAppUpdate>,
) -> AppResult<AppUpdatePreflightResult> {
    let has_downloaded = lock_pending(&pending_update)?.downloaded_bytes.is_some();
    let result = if has_downloaded {
        run_preflight(&state)
    } else {
        AppUpdatePreflightResult {
            ok: false,
            checks: vec![AppUpdatePreflightCheck {
                id: "pending_update_missing".to_string(),
                label: "更新包".to_string(),
                status: AppUpdatePreflightCheckStatus::Failed,
                message: "请先下载更新包".to_string(),
            }],
        }
    };

    {
        let mut guard = lock_pending(&pending_update)?;
        guard.preflight_passed = result.ok;
        if let Some(info) = guard.info.as_mut() {
            info.preflight_passed = result.ok;
        }
        if result.ok {
            let status = status_result(AppUpdateStatus::ReadyToInstall, guard.info.clone(), None);
            emit_status(&app, &status);
        }
    }

    Ok(result)
}

#[tauri::command]
pub fn app_update_install_cmd(
    app: AppHandle,
    pending_update: State<'_, PendingAppUpdate>,
) -> AppResult<()> {
    let (update, bytes) = {
        let mut guard = lock_pending(&pending_update)?;
        if !guard.preflight_passed {
            return Err(AppError::msg("兼容性预检未通过，已阻止安装"));
        }
        let update = guard
            .update
            .clone()
            .ok_or_else(|| AppError::msg("缺少待安装更新"))?;
        let bytes = guard
            .downloaded_bytes
            .take()
            .ok_or_else(|| AppError::msg("缺少已下载更新包"))?;
        (update, bytes)
    };

    update.install(bytes).map_err(updater_error)?;
    app.restart();
}

fn status_result(
    status: AppUpdateStatus,
    info: Option<AppUpdateInfo>,
    message: Option<String>,
) -> AppUpdateCheckResult {
    AppUpdateCheckResult {
        status,
        info,
        message,
    }
}

fn emit_status(app: &AppHandle, payload: &AppUpdateCheckResult) {
    let _ = app.emit(STATUS_EVENT, payload);
}

fn emit_progress(app: &AppHandle, payload: AppUpdateProgressEvent) {
    let _ = app.emit(PROGRESS_EVENT, payload);
}

fn lock_pending(
    pending_update: &PendingAppUpdate,
) -> AppResult<std::sync::MutexGuard<'_, PendingAppUpdateInner>> {
    pending_update
        .inner
        .lock()
        .map_err(|_| AppError::msg("Update state lock poisoned"))
}

fn info_from_update(update: &Update, downloaded: bool, preflight_passed: bool) -> AppUpdateInfo {
    AppUpdateInfo {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        pub_date: update.date.map(|date| date.to_string()),
        notes: update.body.clone(),
        downloaded,
        preflight_passed,
    }
}

fn is_supported_updater_target() -> bool {
    (cfg!(target_os = "macos") && cfg!(target_arch = "aarch64"))
        || (cfg!(target_os = "windows") && cfg!(target_arch = "x86_64"))
}

fn run_preflight(state: &AppState) -> AppUpdatePreflightResult {
    let checks = vec![
        check_vault_access(state),
        check_database_state(state),
        check_settings(state),
        check_credential_markers(state),
        check_state_tables(state),
        check_classified_state(state),
        check_cas_access(state),
    ];

    let ok = checks
        .iter()
        .all(|check| !matches!(check.status, AppUpdatePreflightCheckStatus::Failed));
    AppUpdatePreflightResult { ok, checks }
}

fn passed(id: &str, label: &str, message: impl Into<String>) -> AppUpdatePreflightCheck {
    AppUpdatePreflightCheck {
        id: id.to_string(),
        label: label.to_string(),
        status: AppUpdatePreflightCheckStatus::Passed,
        message: message.into(),
    }
}

fn failed(id: &str, label: &str, message: impl Into<String>) -> AppUpdatePreflightCheck {
    AppUpdatePreflightCheck {
        id: id.to_string(),
        label: label.to_string(),
        status: AppUpdatePreflightCheckStatus::Failed,
        message: message.into(),
    }
}

fn warning(id: &str, label: &str, message: impl Into<String>) -> AppUpdatePreflightCheck {
    AppUpdatePreflightCheck {
        id: id.to_string(),
        label: label.to_string(),
        status: AppUpdatePreflightCheckStatus::Warning,
        message: message.into(),
    }
}

fn check_vault_access(state: &AppState) -> AppUpdatePreflightCheck {
    let Ok(vault) = state.vault_path() else {
        return failed("vault_path", "Vault 路径", "未配置 vault_path");
    };
    if !vault.is_dir() {
        return failed("vault_path", "Vault 路径", "当前 vault 路径不可访问");
    }
    if fs::read_dir(&vault).is_err() {
        return failed("vault_path", "Vault 路径", "当前 vault 不可读");
    }

    let iris_dir = vault.join(".iris");
    if fs::create_dir_all(&iris_dir).is_err() {
        return failed("vault_path", "Vault 路径", "无法访问 vault 内 .iris 目录");
    }
    let probe = iris_dir.join("update-preflight.tmp");
    if fs::write(&probe, b"ok").is_err() {
        return failed("vault_path", "Vault 路径", "当前 vault 不可写");
    }
    let _ = fs::remove_file(probe);

    passed("vault_path", "Vault 路径", "当前 vault 可读写")
}

fn check_database_state(state: &AppState) -> AppUpdatePreflightCheck {
    let db_path = state.data_dir().join("iris.db");
    if !db_path.is_file() {
        return failed("iris_db", "iris.db", "iris.db 不存在");
    }
    if Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).is_err() {
        return failed("iris_db", "iris.db", "iris.db 无法打开");
    }

    match state.db.with_read_conn(|conn| {
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM _migrations", [], |row| row.get(0))?;
        Ok(count)
    }) {
        Ok(count) => passed("iris_db", "iris.db", format!("已读取 _migrations：{count}")),
        Err(_) => failed("iris_db", "iris.db", "_migrations 状态不可读"),
    }
}

fn check_settings(state: &AppState) -> AppUpdatePreflightCheck {
    const IMPORTANT_KEYS: &[&str] = &[
        "vault_path",
        "auto_version_enabled",
        "auto_version_idle_minutes",
        "web_search_enabled",
        "llm_routing",
        "prompt_profile",
    ];

    let result = state.db.with_read_conn(|conn| {
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        for key in IMPORTANT_KEYS {
            let _ = stmt.query_map([key], |_| Ok(()))?.count();
        }
        Ok(())
    });

    match result {
        Ok(()) => passed("settings", "关键设置", "settings 关键键可查询"),
        Err(_) => failed("settings", "关键设置", "settings 关键键不可读取"),
    }
}

fn check_credential_markers(state: &AppState) -> AppUpdatePreflightCheck {
    let markers = match state.db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT key FROM settings WHERE key LIKE 'credential.configured.%' ORDER BY key",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.filter_map(Result::ok).collect::<Vec<_>>())
    }) {
        Ok(markers) => markers,
        Err(_) => {
            return failed(
                "credentials",
                "加密凭据",
                "credential.configured. 标记不可读取",
            )
        }
    };

    if markers.is_empty() {
        return passed("credentials", "加密凭据", "未配置本地凭据 marker");
    }

    if crate::credentials::local_credential_store_accessible().is_err() {
        return failed("credentials", "加密凭据", "master.key 或凭据目录不可访问");
    }

    for marker in &markers {
        let service = marker.trim_start_matches("credential.configured.");
        match crate::credentials::credential_available(service) {
            Ok(true) => {}
            _ => {
                return failed(
                    "credentials",
                    "加密凭据",
                    "凭据 marker 与本地加密凭据状态不一致",
                )
            }
        }
    }

    passed(
        "credentials",
        "加密凭据",
        "凭据 marker 与 master.key 可访问",
    )
}

fn check_state_tables(state: &AppState) -> AppUpdatePreflightCheck {
    const TABLES: &[&str] = &[
        "versions",
        "recycle_bin",
        "sessions",
        "session_messages",
        "session_evidence",
    ];

    let result = state.db.with_read_conn(|conn| {
        for table in TABLES {
            if !table_exists(conn, table)? {
                return Err(AppError::msg(format!("missing table {table}")));
            }
            let sql = format!("SELECT COUNT(*) FROM {table}");
            let _: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
        }
        Ok(())
    });

    match result {
        Ok(()) => passed("state_tables", "应用状态表", "版本、回收站和 AI 会话可查询"),
        Err(_) => failed(
            "state_tables",
            "应用状态表",
            "版本记录、回收站或 AI 会话表不可查询",
        ),
    }
}

fn check_classified_state(state: &AppState) -> AppUpdatePreflightCheck {
    let Ok(vault) = state.vault_path() else {
        return warning("classified", "涉密状态", "未配置 vault，跳过涉密状态检查");
    };
    let classified_dir = vault.join(".classified");
    if !classified_dir.exists() {
        return passed("classified", "涉密状态", "未配置涉密目录");
    }
    if !classified_dir.is_dir() || fs::read_dir(&classified_dir).is_err() {
        return failed("classified", "涉密状态", "涉密目录不可访问");
    }
    if crate::crypto::vault_key::VaultKey::is_initialized(&vault) {
        let key_file = vault.join(".iris").join("vault.key");
        if !key_file.is_file() {
            return failed("classified", "涉密状态", "涉密加密材料不可访问");
        }
    }

    let classified_ai_dir = classified_dir.join(".iris-ai");
    if classified_ai_dir.exists() && !classified_ai_dir.is_dir() {
        return failed("classified", "涉密状态", "涉密 AI 会话目录不可访问");
    }

    passed("classified", "涉密状态", "涉密目录与必要加密材料可访问")
}

fn check_cas_access(state: &AppState) -> AppUpdatePreflightCheck {
    let Ok(vault) = state.vault_path() else {
        return warning("cas", "CAS 存储", "未配置 vault，跳过 CAS 检查");
    };
    let cas = vault.join(".iris").join("cas");
    if !cas.exists() {
        return passed("cas", "CAS 存储", "CAS 尚未初始化");
    }
    if !cas.is_dir() || fs::read_dir(cas).is_err() {
        return failed("cas", "CAS 存储", ".iris/cas 不可访问");
    }
    passed("cas", "CAS 存储", ".iris/cas 可访问")
}

fn table_exists(conn: &Connection, name: &str) -> AppResult<bool> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get(0),
    )?;
    Ok(exists > 0)
}

fn updater_error(err: tauri_plugin_updater::Error) -> AppError {
    AppError::msg(err.sanitized_message_for_update())
}

fn sanitize_check_error_message(message: &str) -> String {
    let text = message.to_lowercase();
    if text.contains("signature") || message.contains(APP_UPDATE_SIGNATURE_ERROR_MESSAGE) {
        return APP_UPDATE_SIGNATURE_ERROR_MESSAGE.to_string();
    }
    if text.contains("latest.json")
        || text.contains("404")
        || text.contains("not found")
        || text.contains("manifest")
    {
        return APP_UPDATE_MANIFEST_ERROR_MESSAGE.to_string();
    }
    if text.contains("error sending request")
        || text.contains("timed out")
        || text.contains("timeout")
        || text.contains("dns")
        || text.contains("tls")
        || text.contains("connection")
        || text.contains("connect")
        || text.contains("network")
    {
        return APP_UPDATE_NETWORK_ERROR_MESSAGE.to_string();
    }
    APP_UPDATE_NETWORK_ERROR_MESSAGE.to_string()
}

trait UpdateErrorMessage {
    fn sanitized_message_for_update(&self) -> String;
}

impl UpdateErrorMessage for tauri_plugin_updater::Error {
    fn sanitized_message_for_update(&self) -> String {
        let text = self.to_string();
        if text.to_lowercase().contains("signature") {
            APP_UPDATE_SIGNATURE_ERROR_MESSAGE.to_string()
        } else {
            text
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_status_serializes_as_snake_case() {
        let json = serde_json::to_string(&AppUpdateStatus::UpToDate).unwrap();
        assert_eq!(json, "\"up_to_date\"");
    }

    #[test]
    fn preflight_without_download_blocks_install_path() {
        let result = AppUpdatePreflightResult {
            ok: false,
            checks: vec![failed("pending_update_missing", "更新包", "请先下载更新包")],
        };

        assert!(!result.ok);
        assert_eq!(result.checks[0].id, "pending_update_missing");
    }

    #[test]
    fn table_exists_detects_missing_state_table() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE settings (key TEXT PRIMARY KEY)", [])
            .unwrap();

        assert!(table_exists(&conn, "settings").unwrap());
        assert!(!table_exists(&conn, "session_messages").unwrap());
    }

    #[test]
    fn update_check_timeout_is_bounded() {
        assert_eq!(APP_UPDATE_CHECK_TIMEOUT, Duration::from_secs(12));
    }

    #[test]
    fn check_error_messages_are_sanitized_for_ui() {
        assert_eq!(
            sanitize_check_error_message(
                "error sending request for url (https://github.com/skahanium/iris/releases/latest/download/latest.json)"
            ),
            APP_UPDATE_MANIFEST_ERROR_MESSAGE
        );
        assert_eq!(
            sanitize_check_error_message("404 not found latest.json"),
            APP_UPDATE_MANIFEST_ERROR_MESSAGE
        );
        assert_eq!(
            sanitize_check_error_message("request timed out while connecting"),
            APP_UPDATE_NETWORK_ERROR_MESSAGE
        );
        assert_eq!(
            sanitize_check_error_message("signature verification failed"),
            APP_UPDATE_SIGNATURE_ERROR_MESSAGE
        );
    }

    #[test]
    fn target_support_is_explicitly_limited_for_first_release() {
        let supported = is_supported_updater_target();
        let expected = (cfg!(target_os = "macos") && cfg!(target_arch = "aarch64"))
            || (cfg!(target_os = "windows") && cfg!(target_arch = "x86_64"));
        assert_eq!(supported, expected);
    }
}
