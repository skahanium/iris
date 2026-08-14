//! RSS 主文档的安全缓存与短期媒体 lease。
//!
//! 远程 PDF 只在用户点击后下载到应用缓存；网络层固定目标并限制响应，
//! 本层负责临时文件、文件头、LRU 与 opaque lease，绝不进入 Vault 或索引。

use std::collections::{HashMap, HashSet};
use std::fs::{File, FileTimes};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::http::header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE};
use tauri::http::{Request, Response, StatusCode};
use tokio::sync::watch;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::network::safe_https::{
    fixed_https_download_to_path, resolve_public_addrs, validate_https_url,
};

pub(crate) const DOCUMENT_MAX_BYTES: u64 = 100 * 1024 * 1024;
const DOCUMENT_CACHE_MAX_BYTES: u64 = 512 * 1024 * 1024;
const DOCUMENT_CACHE_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const DOCUMENT_LEASE_TTL: Duration = Duration::from_secs(10 * 60);
const DOCUMENT_LEASE_MAX: usize = 256;
const MAX_RANGE_LEN: u64 = 1024 * 1024;
const USER_AGENT: &str = concat!("Iris/", env!("CARGO_PKG_VERSION"), " RSS Document");

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedDocumentLease {
    pub handle: String,
    pub url: String,
    pub mime_type: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
struct DocumentLease {
    path: PathBuf,
    size_bytes: u64,
    modified: Option<SystemTime>,
    created_at: SystemTime,
}

static DOCUMENT_LEASES: OnceLock<Mutex<HashMap<String, DocumentLease>>> = OnceLock::new();

#[derive(Debug, Clone)]
enum SharedDownloadOutcome {
    Ready(PathBuf),
    Failed(String),
}

struct SharedDownloadTask {
    cancelled: AtomicBool,
    cancel_notify: tokio::sync::Notify,
    outcome: watch::Sender<Option<SharedDownloadOutcome>>,
}

#[derive(Default)]
struct DownloadRegistry {
    by_cache_key: HashMap<String, Arc<SharedDownloadTask>>,
    item_cache_keys: HashMap<String, String>,
}

static DOCUMENT_DOWNLOADS: OnceLock<Mutex<DownloadRegistry>> = OnceLock::new();
static DOCUMENT_DOWNLOAD_GATE: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();

fn leases() -> &'static Mutex<HashMap<String, DocumentLease>> {
    DOCUMENT_LEASES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn downloads() -> &'static Mutex<DownloadRegistry> {
    DOCUMENT_DOWNLOADS.get_or_init(|| Mutex::new(DownloadRegistry::default()))
}

fn download_gate() -> Arc<tokio::sync::Semaphore> {
    DOCUMENT_DOWNLOAD_GATE
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(1)))
        .clone()
}

pub(crate) fn cache_key(url: &str) -> String {
    hex::encode(Sha256::digest(url.as_bytes()))
}

fn canonical_document_url(url: &str) -> AppResult<String> {
    validate_https_url(url)?;
    let mut parsed =
        reqwest::Url::parse(url).map_err(|_| AppError::msg("feed_document_url_invalid"))?;
    parsed.set_fragment(None);
    Ok(parsed.to_string())
}

pub(crate) fn validate_cached_pdf(path: &Path) -> AppResult<u64> {
    let metadata =
        std::fs::metadata(path).map_err(|_| AppError::msg("feed_document_cache_missing"))?;
    if metadata.len() == 0 || metadata.len() > DOCUMENT_MAX_BYTES {
        return Err(AppError::msg("feed_document_too_large"));
    }
    let mut file = File::open(path).map_err(|_| AppError::msg("feed_document_cache_missing"))?;
    let mut header = [0_u8; 5];
    file.read_exact(&mut header)
        .map_err(|_| AppError::msg("feed_document_invalid_pdf"))?;
    if &header != b"%PDF-" {
        return Err(AppError::msg("feed_document_invalid_pdf"));
    }
    Ok(metadata.len())
}

pub(crate) fn maintain_cache(cache_dir: &Path) -> AppResult<()> {
    std::fs::create_dir_all(cache_dir)?;
    let now = SystemTime::now();
    let protected = protected_lease_paths(now)?;
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(cache_dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let expired = now.duration_since(modified).unwrap_or_default() > DOCUMENT_CACHE_TTL;
        if !protected.contains(&path)
            && (path
                .extension()
                .is_some_and(|extension| extension == "part")
                || expired)
        {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        entries.push((modified, metadata.len(), path));
    }
    entries.sort_by_key(|(modified, _, _)| *modified);
    let mut total = entries.iter().map(|(_, size, _)| *size).sum::<u64>();
    for (_, size, path) in entries {
        if total <= DOCUMENT_CACHE_MAX_BYTES {
            break;
        }
        if !protected.contains(&path) && std::fs::remove_file(path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
    Ok(())
}

fn protected_lease_paths(now: SystemTime) -> AppResult<HashSet<PathBuf>> {
    let mut active = leases()
        .lock()
        .map_err(|_| AppError::msg("feed_document_state_failed"))?;
    active.retain(|_, lease| {
        now.duration_since(lease.created_at).unwrap_or_default() <= DOCUMENT_LEASE_TTL
    });
    Ok(active.values().map(|lease| lease.path.clone()).collect())
}

async fn download_document(
    url: &str,
    cache_dir: &Path,
    cancelled: &AtomicBool,
    progress: &Arc<dyn Fn(u64) + Send + Sync>,
) -> AppResult<PathBuf> {
    let canonical = canonical_document_url(url)?;
    let key = cache_key(&canonical);
    let ready = cache_dir.join(format!("{key}.pdf"));
    if validate_cached_pdf(&ready).is_ok() {
        if let Ok(file) = File::options().write(true).open(&ready) {
            let _ = file.set_times(FileTimes::new().set_modified(SystemTime::now()));
        }
        return Ok(ready);
    }
    maintain_cache(cache_dir)?;
    let partial = cache_dir.join(format!("{key}-{}.part", Uuid::new_v4()));
    let result = async {
        let mut current = canonical.clone();
        for _ in 0..=5 {
            if cancelled.load(Ordering::Acquire) {
                return Err(AppError::msg("feed_document_cancelled"));
            }
            let parsed = reqwest::Url::parse(&current)
                .map_err(|_| AppError::msg("feed_document_url_invalid"))?;
            let hop_host = parsed
                .host_str()
                .ok_or_else(|| AppError::msg("feed_document_url_invalid"))?;
            let addrs = resolve_public_addrs(hop_host).await?;
            let response = fixed_https_download_to_path(
                &current,
                &addrs,
                USER_AGENT,
                None,
                &partial,
                DOCUMENT_MAX_BYTES,
                cancelled,
                progress.as_ref(),
            )
            .await?;
            if (300..400).contains(&response.status) {
                let location = response
                    .headers
                    .get(reqwest::header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| AppError::msg("feed_document_redirect_invalid"))?;
                current = parsed
                    .join(location)
                    .map_err(|_| AppError::msg("feed_document_redirect_invalid"))?
                    .to_string();
                validate_https_url(&current)?;
                continue;
            }
            if !(200..300).contains(&response.status) {
                return Err(AppError::msg("feed_document_http_failed"));
            }
            let content_type = response
                .headers
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !content_type.contains("application/pdf") {
                return Err(AppError::msg("feed_document_content_type"));
            }
            if response.bytes_written == 0 {
                return Err(AppError::msg("feed_document_invalid_pdf"));
            }
            validate_cached_pdf(&partial)?;
            tokio::fs::rename(&partial, &ready)
                .await
                .map_err(|_| AppError::msg("feed_document_cache_write_failed"))?;
            maintain_cache(cache_dir)?;
            return Ok(ready);
        }
        Err(AppError::msg("feed_document_redirect_limit"))
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&partial).await;
    }
    result
}

pub(crate) async fn prepare_document(
    item_id: &str,
    url: &str,
    cache_dir: &Path,
    progress: Arc<dyn Fn(u64) + Send + Sync>,
) -> AppResult<FeedDocumentLease> {
    let canonical = canonical_document_url(url)?;
    let key = cache_key(&canonical);
    let (task, owner) = {
        let mut registry = downloads()
            .lock()
            .map_err(|_| AppError::msg("feed_document_state_failed"))?;
        registry
            .item_cache_keys
            .insert(item_id.to_string(), key.clone());
        if let Some(existing) = registry.by_cache_key.get(&key) {
            (existing.clone(), false)
        } else {
            let (outcome, _) = watch::channel(None);
            let task = Arc::new(SharedDownloadTask {
                cancelled: AtomicBool::new(false),
                cancel_notify: tokio::sync::Notify::new(),
                outcome,
            });
            registry.by_cache_key.insert(key.clone(), task.clone());
            (task, true)
        }
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    if owner {
        let task_for_run = task.clone();
        let run = async {
            let _permit = tokio::select! {
                biased;
                _ = task_for_run.cancel_notify.notified() => {
                    return Err(AppError::msg("feed_document_cancelled"));
                }
                permit = download_gate().acquire_owned() => {
                    permit.map_err(|_| AppError::msg("feed_document_state_failed"))?
                }
            };
            if task_for_run.cancelled.load(Ordering::Acquire) {
                return Err(AppError::msg("feed_document_cancelled"));
            }
            tokio::select! {
                biased;
                _ = task_for_run.cancel_notify.notified() => {
                    Err(AppError::msg("feed_document_cancelled"))
                }
                result = download_document(
                    &canonical,
                    cache_dir,
                    &task_for_run.cancelled,
                    &progress,
                ) => result,
            }
        };
        let result = tokio::time::timeout_at(deadline, run)
            .await
            .map_err(|_| AppError::msg("feed_document_timeout"))?;
        let shared = match result {
            Ok(path) => SharedDownloadOutcome::Ready(path),
            Err(error) => {
                // 连接或读取中取消会丢弃 I/O future；单任务 gate 保证这里
                // 清理 `.part` 时不会误删另一项正在写入的临时文件。
                let _ = maintain_cache(cache_dir);
                SharedDownloadOutcome::Failed(error.to_string())
            }
        };
        task.outcome.send_replace(Some(shared));
        let mut registry = downloads()
            .lock()
            .map_err(|_| AppError::msg("feed_document_state_failed"))?;
        if registry
            .by_cache_key
            .get(&key)
            .is_some_and(|current| Arc::ptr_eq(current, &task))
        {
            registry.by_cache_key.remove(&key);
        }
        registry.item_cache_keys.retain(|_, value| value != &key);
    }
    let mut receiver = task.outcome.subscribe();
    let outcome = loop {
        if let Some(outcome) = receiver.borrow().clone() {
            break outcome;
        }
        tokio::time::timeout_at(deadline, receiver.changed())
            .await
            .map_err(|_| AppError::msg("feed_document_timeout"))?
            .map_err(|_| AppError::msg("feed_document_state_failed"))?;
    };
    let path = match outcome {
        SharedDownloadOutcome::Ready(path) => path,
        SharedDownloadOutcome::Failed(code) => return Err(AppError::msg(code)),
    };
    create_lease(path)
}

fn create_lease(path: PathBuf) -> AppResult<FeedDocumentLease> {
    let metadata = std::fs::metadata(&path)?;
    let handle = Uuid::new_v4().to_string();
    let now = SystemTime::now();
    let mut active_leases = leases()
        .lock()
        .map_err(|_| AppError::msg("feed_document_state_failed"))?;
    active_leases.retain(|_, lease| {
        now.duration_since(lease.created_at).unwrap_or_default() <= DOCUMENT_LEASE_TTL
    });
    if active_leases.len() >= DOCUMENT_LEASE_MAX {
        return Err(AppError::msg("feed_document_lease_capacity"));
    }
    active_leases.insert(
        handle.clone(),
        DocumentLease {
            path,
            size_bytes: metadata.len(),
            modified: metadata.modified().ok(),
            created_at: now,
        },
    );
    Ok(FeedDocumentLease {
        url: format!("iris-feed-document://localhost/{handle}"),
        handle,
        mime_type: "application/pdf".to_string(),
        size_bytes: metadata.len(),
    })
}

pub(crate) fn cancel_document(item_id: &str) -> AppResult<()> {
    let task = {
        let mut registry = downloads()
            .lock()
            .map_err(|_| AppError::msg("feed_document_state_failed"))?;
        let Some(key) = registry.item_cache_keys.remove(item_id) else {
            return Ok(());
        };
        let has_waiter = registry.item_cache_keys.values().any(|value| value == &key);
        (!has_waiter)
            .then(|| registry.by_cache_key.get(&key).cloned())
            .flatten()
    };
    if let Some(task) = task {
        task.cancelled.store(true, Ordering::Release);
        task.cancel_notify.notify_one();
    }
    Ok(())
}

pub(crate) fn release_document(handle: &str) -> AppResult<()> {
    leases()
        .lock()
        .map_err(|_| AppError::msg("feed_document_state_failed"))?
        .remove(handle);
    Ok(())
}

pub(crate) fn clear_cache(cache_dir: &Path) -> AppResult<u32> {
    std::fs::create_dir_all(cache_dir)?;
    let protected = protected_lease_paths(SystemTime::now())?;
    let mut removed = 0_u32;
    for entry in std::fs::read_dir(cache_dir)? {
        let path = entry?.path();
        if path.is_file() && !protected.contains(&path) && std::fs::remove_file(path).is_ok() {
            removed = removed.saturating_add(1);
        }
    }
    Ok(removed)
}

pub(crate) fn remove_cached_url(cache_dir: &Path, url: &str) -> AppResult<u32> {
    let canonical = canonical_document_url(url)?;
    let key = cache_key(&canonical);
    cancel_document_url(url)?;
    let protected = protected_lease_paths(SystemTime::now())?;
    let mut removed = 0_u32;
    let ready = cache_dir.join(format!("{key}.pdf"));
    if !protected.contains(&ready) && std::fs::remove_file(ready).is_ok() {
        removed = removed.saturating_add(1);
    }
    if let Ok(entries) = std::fs::read_dir(cache_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let matches_partial =
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with(&format!("{key}-")) && name.ends_with(".part")
                    });
            if matches_partial && std::fs::remove_file(path).is_ok() {
                removed = removed.saturating_add(1);
            }
        }
    }
    Ok(removed)
}

pub(crate) fn cancel_document_url(url: &str) -> AppResult<()> {
    let key = cache_key(&canonical_document_url(url)?);
    let task = downloads()
        .lock()
        .map_err(|_| AppError::msg("feed_document_state_failed"))?
        .by_cache_key
        .get(&key)
        .cloned();
    if let Some(task) = task {
        task.cancelled.store(true, Ordering::Release);
        task.cancel_notify.notify_one();
    }
    Ok(())
}

fn parse_range(header: &str, len: u64) -> Option<(u64, u64)> {
    if len == 0 {
        return None;
    }
    let first = header.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (start, end) = first.split_once('-')?;
    if start.is_empty() {
        let suffix = end.parse::<u64>().ok()?;
        if suffix == 0 {
            return None;
        }
        return Some((len.saturating_sub(suffix), len - 1));
    }
    let start = start.parse::<u64>().ok()?;
    let end = if end.is_empty() {
        len.saturating_sub(1)
    } else {
        end.parse().ok()?
    };
    (start < len && end >= start).then_some((start, end.min(len - 1)))
}

fn protocol_response(request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let handle = request.uri().path().trim_start_matches('/');
    let lease = leases().lock().ok().and_then(|mut values| {
        values.retain(|_, lease| {
            SystemTime::now()
                .duration_since(lease.created_at)
                .unwrap_or_default()
                <= DOCUMENT_LEASE_TTL
        });
        values.get(handle).cloned()
    });
    let Some(lease) = lease else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap();
    };
    let Ok(metadata) = std::fs::metadata(&lease.path) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap();
    };
    if metadata.len() != lease.size_bytes || metadata.modified().ok() != lease.modified {
        let _ = release_document(handle);
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap();
    }
    let mut file = match File::open(&lease.path) {
        Ok(file) => file,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Vec::new())
                .unwrap()
        }
    };
    let len = lease.size_bytes;
    let requested_range = request
        .headers()
        .get(RANGE)
        .and_then(|value| value.to_str().ok());
    let range = requested_range.and_then(|value| parse_range(value, len));
    if requested_range.is_some() && range.is_none() {
        return Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(CONTENT_RANGE, format!("bytes */{len}"))
            .body(Vec::new())
            .unwrap();
    }
    let (start, end, status) = match range {
        Some((start, end)) => (
            start,
            start + (end - start).min(MAX_RANGE_LEN - 1),
            StatusCode::PARTIAL_CONTENT,
        ),
        None => (
            0,
            len.min(MAX_RANGE_LEN).saturating_sub(1),
            StatusCode::PARTIAL_CONTENT,
        ),
    };
    use std::io::{Seek, SeekFrom};
    let mut body = vec![0_u8; (end + 1 - start) as usize];
    if file.seek(SeekFrom::Start(start)).is_err() || file.read_exact(&mut body).is_err() {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Vec::new())
            .unwrap();
    }
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "application/pdf")
        .header(ACCEPT_RANGES, "bytes")
        .header(CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
        .header(CONTENT_LENGTH, body.len())
        .body(body)
        .unwrap()
}

pub(crate) fn register_document_protocol(
    builder: tauri::Builder<tauri::Wry>,
) -> tauri::Builder<tauri::Wry> {
    builder.register_uri_scheme_protocol("iris-feed-document", |_ctx, request| {
        protocol_response(request)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_pdf_header_and_size_without_loading_whole_file() {
        let dir = tempfile::tempdir().unwrap();
        let valid = dir.path().join("valid.pdf");
        std::fs::write(&valid, b"%PDF-1.7\nfixture").unwrap();
        assert_eq!(validate_cached_pdf(&valid).unwrap(), 16);
        let invalid = dir.path().join("invalid.pdf");
        std::fs::write(&invalid, b"<html>not pdf</html>").unwrap();
        assert!(validate_cached_pdf(&invalid).is_err());
    }

    #[test]
    fn cache_key_never_contains_remote_url() {
        let key = cache_key("https://papers.example/private-title.pdf");
        assert_eq!(key.len(), 64);
        assert!(key.chars().all(|character| character.is_ascii_hexdigit()));
        assert!(!key.contains("papers"));
    }

    #[test]
    fn targeted_cache_removal_does_not_touch_another_document() {
        let dir = tempfile::tempdir().unwrap();
        let first_url = "https://papers.example/first.pdf";
        let second_url = "https://papers.example/second.pdf";
        let first = dir.path().join(format!("{}.pdf", cache_key(first_url)));
        let second = dir.path().join(format!("{}.pdf", cache_key(second_url)));
        std::fs::write(&first, b"%PDF-first").unwrap();
        std::fs::write(&second, b"%PDF-second").unwrap();

        assert_eq!(remove_cached_url(dir.path(), first_url).unwrap(), 1);
        assert!(!first.exists());
        assert!(second.exists());
    }

    #[test]
    fn cache_cleanup_preserves_files_held_by_a_live_lease() {
        let dir = tempfile::tempdir().unwrap();
        let url = "https://papers.example/leased.pdf";
        let path = dir.path().join(format!("{}.pdf", cache_key(url)));
        std::fs::write(&path, b"%PDF-leased").unwrap();
        let lease = create_lease(path.clone()).unwrap();

        assert_eq!(clear_cache(dir.path()).unwrap(), 0);
        assert!(path.exists());
        assert_eq!(remove_cached_url(dir.path(), url).unwrap(), 0);
        assert!(path.exists());

        release_document(&lease.handle).unwrap();
        assert_eq!(remove_cached_url(dir.path(), url).unwrap(), 1);
        assert!(!path.exists());
    }

    #[test]
    fn lease_capacity_rejects_new_preview_without_revoking_existing_leases() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shared.pdf");
        std::fs::write(&path, b"%PDF-shared").unwrap();
        let mut handles = Vec::with_capacity(DOCUMENT_LEASE_MAX);
        for _ in 0..DOCUMENT_LEASE_MAX {
            handles.push(create_lease(path.clone()).expect("lease below cap"));
        }

        assert!(create_lease(path.clone()).is_err());
        assert!(leases().lock().unwrap().contains_key(&handles[0].handle));

        for lease in handles {
            release_document(&lease.handle).unwrap();
        }
    }

    #[test]
    fn cancelling_one_shared_request_keeps_download_alive_for_another_waiter() {
        let url = "https://papers.example/shared.pdf";
        let key = cache_key(url);
        let (outcome, _) = watch::channel(None);
        let task = Arc::new(SharedDownloadTask {
            cancelled: AtomicBool::new(false),
            cancel_notify: tokio::sync::Notify::new(),
            outcome,
        });
        {
            let mut registry = downloads().lock().unwrap();
            registry.by_cache_key.clear();
            registry.item_cache_keys.clear();
            registry.by_cache_key.insert(key.clone(), task.clone());
            registry
                .item_cache_keys
                .insert("one".to_string(), key.clone());
            registry.item_cache_keys.insert("two".to_string(), key);
        }

        cancel_document("one").unwrap();
        assert!(!task.cancelled.load(Ordering::Acquire));
        cancel_document("two").unwrap();
        assert!(task.cancelled.load(Ordering::Acquire));

        let mut registry = downloads().lock().unwrap();
        registry.by_cache_key.clear();
        registry.item_cache_keys.clear();
    }

    #[test]
    fn range_parser_supports_suffixes_and_rejects_invalid_offsets() {
        assert_eq!(parse_range("bytes=0-9", 100), Some((0, 9)));
        assert_eq!(parse_range("bytes=10-", 100), Some((10, 99)));
        assert_eq!(parse_range("bytes=-5", 100), Some((95, 99)));
        assert_eq!(parse_range("bytes=100-101", 100), None);
        assert_eq!(parse_range("bytes=0-1", 0), None);
    }
}
