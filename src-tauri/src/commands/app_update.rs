use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::StreamExt;
use http::header::{ACCEPT, RANGE};
use minisign_verify::{PublicKey, Signature};
use reqwest::{ClientBuilder, StatusCode};
use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::io::AsyncWriteExt;

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
    pub notes: Option<String>,
    pub downloaded: bool,
    pub preflight_passed: bool,
    pub cached_bytes: Option<u64>,
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

pub struct PendingAppUpdate {
    cache_dir: PathBuf,
    inner: Mutex<PendingAppUpdateInner>,
    download_lock: tokio::sync::Mutex<()>,
}

impl PendingAppUpdate {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            inner: Mutex::new(PendingAppUpdateInner::default()),
            download_lock: tokio::sync::Mutex::new(()),
        }
    }
}

#[derive(Default)]
struct PendingAppUpdateInner {
    update: Option<Update>,
    info: Option<AppUpdateInfo>,
    verified_package: Option<PathBuf>,
    preflight_passed: bool,
}

#[derive(Debug, Clone)]
struct UpdateCacheEntry {
    part_path: PathBuf,
    verified_path: PathBuf,
}

impl UpdateCacheEntry {
    fn from_update(cache_dir: &Path, update: &Update) -> Self {
        let mut hasher = Sha256::new();
        for field in [
            update.version.as_str(),
            update.target.as_str(),
            update.download_url.as_str(),
            update.signature.as_str(),
        ] {
            hasher.update(field.as_bytes());
            hasher.update([0]);
        }
        let id = hex::encode(hasher.finalize());
        Self {
            part_path: cache_dir.join(format!("{id}.part")),
            verified_path: cache_dir.join(format!("{id}.ready")),
        }
    }
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
        Err(err) => return emit_check_error(&app, err),
    };
    let update = match updater.check().await.map_err(updater_error) {
        Ok(update) => update,
        Err(err) => return emit_check_error(&app, err),
    };

    let Some(update) = update else {
        cleanup_update_cache(&pending_update.cache_dir, None)?;
        let result = status_result(AppUpdateStatus::UpToDate, None, None);
        let mut guard = lock_pending(&pending_update)?;
        *guard = PendingAppUpdateInner::default();
        drop(guard);
        emit_status(&app, &result);
        return Ok(result);
    };

    let entry = UpdateCacheEntry::from_update(&pending_update.cache_dir, &update);
    cleanup_update_cache(&pending_update.cache_dir, Some(&entry))?;
    let verified_package = if entry.verified_path.is_file()
        && verify_update_file(&app, &update, &entry.verified_path).is_ok()
    {
        Some(entry.verified_path.clone())
    } else {
        let _ = fs::remove_file(&entry.verified_path);
        None
    };
    let cached_bytes = verified_package
        .as_ref()
        .or_else(|| entry.part_path.is_file().then_some(&entry.part_path))
        .and_then(|path| fs::metadata(path).ok().map(|metadata| metadata.len()));
    let downloaded = verified_package.is_some();
    let info = info_from_update(&update, downloaded, false, cached_bytes);
    {
        let mut guard = lock_pending(&pending_update)?;
        guard.update = Some(update);
        guard.info = Some(info.clone());
        guard.verified_package = verified_package;
        guard.preflight_passed = false;
    }

    let result = status_result(
        if downloaded {
            AppUpdateStatus::Downloaded
        } else {
            AppUpdateStatus::Available
        },
        Some(info),
        None,
    );
    emit_status(&app, &result);
    Ok(result)
}

#[tauri::command]
pub async fn app_update_download_cmd(
    app: AppHandle,
    pending_update: State<'_, PendingAppUpdate>,
) -> AppResult<AppUpdateCheckResult> {
    let _download_guard = pending_update.download_lock.lock().await;
    let update = {
        let guard = lock_pending(&pending_update)?;
        guard
            .update
            .clone()
            .ok_or_else(|| AppError::msg("没有可下载的待更新版本"))?
    };
    let entry = UpdateCacheEntry::from_update(&pending_update.cache_dir, &update);
    let downloading = status_result(
        AppUpdateStatus::Downloading,
        Some(info_from_update(
            &update,
            false,
            false,
            file_size(&entry.part_path),
        )),
        None,
    );
    emit_status(&app, &downloading);

    let verified_path = download_update_to_cache(&app, &update, &entry).await?;
    let cached_bytes = file_size(&verified_path);
    let info = info_from_update(&update, true, false, cached_bytes);
    {
        let mut guard = lock_pending(&pending_update)?;
        guard.verified_package = Some(verified_path);
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
    let has_downloaded = lock_pending(&pending_update)?
        .verified_package
        .as_ref()
        .is_some_and(|path| path.is_file());
    let result = if has_downloaded {
        run_preflight(&state)
    } else {
        AppUpdatePreflightResult {
            ok: false,
            checks: vec![failed("pending_update_missing", "更新包", "请先下载更新包")],
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
    let (update, package) = {
        let guard = lock_pending(&pending_update)?;
        if !guard.preflight_passed {
            return Err(AppError::msg("兼容性预检未通过，已阻止安装"));
        }
        (
            guard
                .update
                .clone()
                .ok_or_else(|| AppError::msg("缺少待安装更新"))?,
            guard
                .verified_package
                .clone()
                .ok_or_else(|| AppError::msg("缺少已下载更新包"))?,
        )
    };

    verify_update_file(&app, &update, &package)?;
    let bytes = fs::read(&package).map_err(|_| AppError::msg("无法读取已下载更新包"))?;
    update.install(bytes).map_err(updater_error)?;
    let _ = fs::remove_file(package);
    app.restart();
}

async fn download_update_to_cache(
    app: &AppHandle,
    update: &Update,
    entry: &UpdateCacheEntry,
) -> AppResult<PathBuf> {
    fs::create_dir_all(
        entry
            .part_path
            .parent()
            .ok_or_else(|| AppError::msg("更新缓存目录无效"))?,
    )
    .map_err(|_| AppError::msg("无法访问更新缓存目录"))?;

    let mut offset = file_size(&entry.part_path).unwrap_or(0);
    let response = loop {
        let headers = download_headers(&update.headers, offset)?;
        let mut client = ClientBuilder::new();
        if let Some(timeout) = update.timeout {
            client = client.timeout(timeout);
        }
        if update.no_proxy {
            client = client.no_proxy();
        } else if let Some(proxy) = &update.proxy {
            client = client.proxy(
                reqwest::Proxy::all(proxy.as_str())
                    .map_err(|_| AppError::msg("无法连接更新服务器，请检查网络后重试"))?,
            );
        }
        let response = client
            .build()
            .map_err(|_| AppError::msg(APP_UPDATE_NETWORK_ERROR_MESSAGE))?
            .get(update.download_url.clone())
            .headers(headers)
            .send()
            .await
            .map_err(|_| AppError::msg(APP_UPDATE_NETWORK_ERROR_MESSAGE))?;

        match response.status() {
            StatusCode::PARTIAL_CONTENT if content_range_matches_response(&response, offset) => {
                break response
            }
            StatusCode::OK => {
                if offset > 0 {
                    remove_partial_cache(&entry.part_path)?;
                    offset = 0;
                }
                break response;
            }
            StatusCode::RANGE_NOT_SATISFIABLE if offset > 0 => {
                remove_partial_cache(&entry.part_path)?;
                offset = 0;
            }
            StatusCode::PARTIAL_CONTENT if offset > 0 => {
                remove_partial_cache(&entry.part_path)?;
                offset = 0;
            }
            StatusCode::PARTIAL_CONTENT => {
                return Err(AppError::msg(APP_UPDATE_NETWORK_ERROR_MESSAGE));
            }
            _ => return Err(AppError::msg(APP_UPDATE_NETWORK_ERROR_MESSAGE)),
        }
    };

    let content_range = response_content_range(&response);
    let expected_range_length = content_range.and_then(ContentRange::length);
    let content_length = content_length(&response, offset);
    emit_progress(
        app,
        AppUpdateProgressEvent {
            phase: "started".to_string(),
            chunk_length: 0,
            content_length,
            downloaded: offset,
        },
    );
    let mut options = tokio::fs::OpenOptions::new();
    options.create(true).write(true);
    if offset == 0 {
        options.truncate(true);
    } else {
        options.append(true);
    }
    let mut file = options
        .open(&entry.part_path)
        .await
        .map_err(|_| AppError::msg("无法写入更新缓存"))?;
    let mut downloaded = offset;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| AppError::msg(APP_UPDATE_NETWORK_ERROR_MESSAGE))?;
        file.write_all(&chunk)
            .await
            .map_err(|_| AppError::msg("无法写入更新缓存"))?;
        downloaded += chunk.len() as u64;
        emit_progress(
            app,
            AppUpdateProgressEvent {
                phase: "progress".to_string(),
                chunk_length: chunk.len(),
                content_length,
                downloaded,
            },
        );
    }
    file.flush()
        .await
        .map_err(|_| AppError::msg("无法写入更新缓存"))?;
    file.sync_data()
        .await
        .map_err(|_| AppError::msg("无法写入更新缓存"))?;
    drop(file);

    if let Some(expected_range_length) = expected_range_length {
        if downloaded.checked_sub(offset) != Some(expected_range_length) {
            remove_partial_cache(&entry.part_path)?;
            return Err(AppError::msg(APP_UPDATE_NETWORK_ERROR_MESSAGE));
        }
    }
    if let Some(total) = content_length {
        if downloaded != total {
            return Err(AppError::msg(APP_UPDATE_NETWORK_ERROR_MESSAGE));
        }
    }
    if let Err(error) = verify_update_file(app, update, &entry.part_path) {
        let _ = fs::remove_file(&entry.part_path);
        return Err(error);
    }
    let _ = fs::remove_file(&entry.verified_path);
    fs::rename(&entry.part_path, &entry.verified_path)
        .map_err(|_| AppError::msg("无法完成更新缓存"))?;
    emit_progress(
        app,
        AppUpdateProgressEvent {
            phase: "finished".to_string(),
            chunk_length: 0,
            content_length,
            downloaded,
        },
    );
    Ok(entry.verified_path.clone())
}

fn verify_update_file(app: &AppHandle, update: &Update, path: &Path) -> AppResult<()> {
    let public_key = updater_public_key(app)?;
    let public_key_text = decode_base64_text(&public_key)?;
    let signature_text = decode_base64_text(&update.signature)?;
    let public_key = PublicKey::decode(&public_key_text)
        .map_err(|_| AppError::msg(APP_UPDATE_SIGNATURE_ERROR_MESSAGE))?;
    let signature = Signature::decode(&signature_text)
        .map_err(|_| AppError::msg(APP_UPDATE_SIGNATURE_ERROR_MESSAGE))?;
    verify_minisign_file(&public_key, &signature, path)
}

fn verify_minisign_file(
    public_key: &PublicKey,
    signature: &Signature,
    path: &Path,
) -> AppResult<()> {
    let mut verifier = match public_key.verify_stream(signature) {
        Ok(verifier) => verifier,
        Err(minisign_verify::Error::UnsupportedLegacyMode) => {
            let bytes = fs::read(path).map_err(|_| AppError::msg("无法读取已下载更新包"))?;
            return public_key
                .verify(&bytes, signature, true)
                .map_err(|_| AppError::msg(APP_UPDATE_SIGNATURE_ERROR_MESSAGE));
        }
        Err(_) => return Err(AppError::msg(APP_UPDATE_SIGNATURE_ERROR_MESSAGE)),
    };
    let file = fs::File::open(path).map_err(|_| AppError::msg("无法读取已下载更新包"))?;
    let mut reader = BufReader::new(file);
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| AppError::msg("无法读取已下载更新包"))?;
        if read == 0 {
            break;
        }
        verifier.update(&buffer[..read]);
    }
    verifier
        .finalize()
        .map_err(|_| AppError::msg(APP_UPDATE_SIGNATURE_ERROR_MESSAGE))
}

fn updater_public_key(app: &AppHandle) -> AppResult<String> {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|value| value.get("pubkey"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| AppError::msg(APP_UPDATE_SIGNATURE_ERROR_MESSAGE))
}

fn decode_base64_text(value: &str) -> AppResult<String> {
    let bytes = STANDARD
        .decode(value)
        .map_err(|_| AppError::msg(APP_UPDATE_SIGNATURE_ERROR_MESSAGE))?;
    String::from_utf8(bytes).map_err(|_| AppError::msg(APP_UPDATE_SIGNATURE_ERROR_MESSAGE))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ContentRange {
    start: u64,
    end: u64,
    total: Option<u64>,
}

impl ContentRange {
    fn length(self) -> Option<u64> {
        self.end.checked_sub(self.start)?.checked_add(1)
    }
}

fn response_content_range(response: &reqwest::Response) -> Option<ContentRange> {
    response
        .headers()
        .get("content-range")
        .and_then(|value| value.to_str().ok())
        .and_then(parse_content_range)
}

fn content_range_matches_response(response: &reqwest::Response, expected: u64) -> bool {
    let Some(range) = response_content_range(response) else {
        return false;
    };
    if range.start != expected {
        return false;
    }
    match response.content_length() {
        Some(length) => range.length() == Some(length),
        None => true,
    }
}

fn content_length(response: &reqwest::Response, offset: u64) -> Option<u64> {
    if response.status() == StatusCode::PARTIAL_CONTENT {
        return response_content_range(response).and_then(|range| range.total);
    }
    response
        .content_length()
        .and_then(|length| length.checked_add(offset))
}

fn download_headers(base_headers: &http::HeaderMap, offset: u64) -> AppResult<http::HeaderMap> {
    let mut headers = base_headers.clone();
    if !headers.contains_key(ACCEPT) {
        headers.insert(
            ACCEPT,
            http::HeaderValue::from_static("application/octet-stream"),
        );
    }
    if offset > 0 {
        let range = http::HeaderValue::from_str(&format!("bytes={offset}-"))
            .map_err(|_| AppError::msg("更新续传范围无效"))?;
        headers.insert(RANGE, range);
    }
    Ok(headers)
}

fn parse_content_range(value: &str) -> Option<ContentRange> {
    let value = value.strip_prefix("bytes ")?;
    let (range, total) = value.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let start = start.parse().ok()?;
    let end = end.parse().ok()?;
    if end < start {
        return None;
    }
    let total = if total == "*" {
        None
    } else {
        let total = total.parse().ok()?;
        if total <= end {
            return None;
        }
        Some(total)
    };
    Some(ContentRange { start, end, total })
}

fn cleanup_update_cache(cache_dir: &Path, current: Option<&UpdateCacheEntry>) -> AppResult<()> {
    fs::create_dir_all(cache_dir).map_err(|_| AppError::msg("无法访问更新缓存目录"))?;
    for entry in fs::read_dir(cache_dir).map_err(|_| AppError::msg("无法访问更新缓存目录"))?
    {
        let path = entry
            .map_err(|_| AppError::msg("无法访问更新缓存目录"))?
            .path();
        let is_current = current
            .is_some_and(|expected| path == expected.part_path || path == expected.verified_path);
        let is_update_artifact = matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("part") | Some("ready")
        );
        if is_update_artifact && !is_current {
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}

fn remove_partial_cache(path: &Path) -> AppResult<()> {
    if path.exists() {
        fs::remove_file(path).map_err(|_| AppError::msg("无法重置更新缓存"))?;
    }
    Ok(())
}

fn file_size(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
}

fn emit_check_error(app: &AppHandle, err: AppError) -> AppResult<AppUpdateCheckResult> {
    let result = status_result(
        AppUpdateStatus::Error,
        None,
        Some(sanitize_check_error_message(&err.to_string())),
    );
    emit_status(app, &result);
    Ok(result)
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

fn info_from_update(
    update: &Update,
    downloaded: bool,
    preflight_passed: bool,
    cached_bytes: Option<u64>,
) -> AppUpdateInfo {
    AppUpdateInfo {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        notes: update.body.clone(),
        downloaded,
        preflight_passed,
        cached_bytes,
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
    if !vault.is_dir() || fs::read_dir(&vault).is_err() {
        return failed("vault_path", "Vault 路径", "当前 vault 路径不可访问");
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
    if !db_path.is_file()
        || Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .is_err()
    {
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
        let mut statement = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        for key in IMPORTANT_KEYS {
            let _ = statement.query_map([key], |_| Ok(()))?.count();
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
        let mut statement = conn.prepare(
            "SELECT key FROM settings WHERE key LIKE 'credential.configured.%' ORDER BY key",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
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
        if !matches!(crate::credentials::credential_available(service), Ok(true)) {
            return failed(
                "credentials",
                "加密凭据",
                "凭据 marker 与本地加密凭据状态不一致",
            );
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
        Ok(()) => passed(
            "state_tables",
            "应用状态表",
            "版本记录、回收站和 AI 会话可查询",
        ),
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
    if crate::crypto::vault_key::VaultKey::config_accessible(&vault).is_err() {
        return failed("classified", "涉密状态", "涉密保险库配置不可访问或已损坏");
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
    APP_UPDATE_NETWORK_ERROR_MESSAGE.to_string()
}

trait UpdateErrorMessage {
    fn sanitized_message_for_update(&self) -> String;
}

impl UpdateErrorMessage for tauri_plugin_updater::Error {
    fn sanitized_message_for_update(&self) -> String {
        if self.to_string().to_lowercase().contains("signature") {
            APP_UPDATE_SIGNATURE_ERROR_MESSAGE.to_string()
        } else {
            self.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn cleanup_removes_stale_update_artifacts() {
        let dir = tempdir().unwrap();
        let first = dir.path().join("first.part");
        let second = dir.path().join("second.ready");
        fs::write(&first, b"partial").unwrap();
        fs::write(&second, b"complete").unwrap();
        let expected = UpdateCacheEntry {
            part_path: dir.path().join("current.part"),
            verified_path: dir.path().join("current.ready"),
        };
        cleanup_update_cache(dir.path(), Some(&expected)).unwrap();
        assert!(!first.exists());
        assert!(!second.exists());
    }

    #[test]
    fn content_range_parser_rejects_invalid_bounds() {
        assert_eq!(
            parse_content_range("bytes 12-99/100"),
            Some(ContentRange {
                start: 12,
                end: 99,
                total: Some(100),
            })
        );
        assert_eq!(
            parse_content_range("bytes 12-99/*"),
            Some(ContentRange {
                start: 12,
                end: 99,
                total: None,
            })
        );
        assert_eq!(parse_content_range("bytes 99-12/100"), None);
        assert_eq!(parse_content_range("bytes 12-100/100"), None);
        assert_eq!(parse_content_range("invalid"), None);
    }

    #[test]
    fn partial_cache_records_received_bytes() {
        let dir = tempdir().unwrap();
        let part = dir.path().join("update.part");
        fs::write(&part, b"partial").unwrap();
        assert_eq!(file_size(&part), Some(7));
    }

    #[test]
    fn verification_supports_legacy_and_prehashed_minisign_releases() {
        const PUBLIC_KEY: &str = "untrusted comment: minisign public key E7620F1842B4E81F\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
        const LEGACY_SIGNATURE: &str = "untrusted comment: signature from minisign secret key\nRWQf6LRCGA9i59SLOFxz6NxvASXDJeRtuZykwQepbDEGt87ig1BNpWaVWuNrm73YiIiJbq71Wi+dP9eKL8OC351vwIasSSbXxwA=\ntrusted comment: timestamp:1555779966\tfile:test\nQtKMXWyYcwdpZAlPF7tE2ENJkRd1ujvKjlj1m9RtHTBnZPa5WKU5uWRs5GoP5M/VqE81QFuMKI5k/SfNQUaOAA==";
        const PREHASHED_SIGNATURE: &str = "untrusted comment: signature from minisign secret key\nRUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\ntrusted comment: timestamp:1556193335\tfile:test\ny/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg==";

        let dir = tempdir().unwrap();
        let package = dir.path().join("update.bin");
        fs::write(&package, b"test").unwrap();
        let public_key = PublicKey::decode(PUBLIC_KEY).unwrap();

        let legacy = Signature::decode(LEGACY_SIGNATURE).unwrap();
        verify_minisign_file(&public_key, &legacy, &package).unwrap();

        let prehashed = Signature::decode(PREHASHED_SIGNATURE).unwrap();
        verify_minisign_file(&public_key, &prehashed, &package).unwrap();

        fs::write(&package, b"tampered").unwrap();
        assert!(verify_minisign_file(&public_key, &legacy, &package).is_err());
        assert!(verify_minisign_file(&public_key, &prehashed, &package).is_err());
    }

    #[test]
    fn download_headers_preserve_custom_accept_and_add_resume_range() {
        let mut headers = http::HeaderMap::new();
        headers.insert(ACCEPT, http::HeaderValue::from_static("application/custom"));

        let headers = download_headers(&headers, 128).unwrap();
        assert_eq!(headers.get(ACCEPT).unwrap(), "application/custom");
        assert_eq!(headers.get(RANGE).unwrap(), "bytes=128-");
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
            sanitize_check_error_message("404 not found latest.json"),
            APP_UPDATE_MANIFEST_ERROR_MESSAGE
        );
        assert_eq!(
            sanitize_check_error_message("signature verification failed"),
            APP_UPDATE_SIGNATURE_ERROR_MESSAGE
        );
    }

    #[test]
    fn target_support_is_explicitly_limited_for_first_release() {
        let expected = (cfg!(target_os = "macos") && cfg!(target_arch = "aarch64"))
            || (cfg!(target_os = "windows") && cfg!(target_arch = "x86_64"));
        assert_eq!(is_supported_updater_target(), expected);
    }
}
