//! RSS 图片的受控本地缓存。
//!
//! 图片只会在用户对单篇文章明确授权后下载。下载走与 RSS/PDF 相同的
//! DNS 固定目标 HTTPS 通道；WebView 永远只读取 opaque lease，不能热链远程 URL。

use std::collections::{HashMap, HashSet};
use std::fs::{File, FileTimes};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};
use tauri::http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use tauri::http::{Request, Response, StatusCode};
use tokio::sync::{watch, Semaphore};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::feed::model::{FeedImageLease, FeedImagesPrepareResult};
use crate::network::safe_https::{
    fixed_https_download_to_path, resolve_public_addrs, validate_https_url,
};

pub(crate) const IMAGE_MAX_BYTES: u64 = 10 * 1024 * 1024;
const IMAGE_CACHE_MAX_BYTES: u64 = 512 * 1024 * 1024;
const IMAGE_CACHE_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// A partial is either actively written by this process or a recent remnant from an
/// interrupted download. Never delete it just because another image finishes.
const IMAGE_PARTIAL_MAX_AGE: Duration = Duration::from_secs(30 * 60);
const IMAGE_LEASE_TTL: Duration = Duration::from_secs(10 * 60);
const IMAGE_LEASE_MAX: usize = 256;
const IMAGE_FAILURE_BACKOFF: Duration = Duration::from_secs(30);
const MAX_IMAGES_PER_ITEM: usize = 24;
const USER_AGENT: &str = concat!("Iris/", env!("CARGO_PKG_VERSION"), " RSS Image");

#[derive(Debug, Clone)]
struct ImageLease {
    path: PathBuf,
    mime_type: &'static str,
    size_bytes: u64,
    modified: Option<SystemTime>,
    created_at: SystemTime,
}

#[derive(Debug, Clone)]
struct CachedImage {
    path: PathBuf,
    mime_type: &'static str,
    size_bytes: u64,
}

#[derive(Debug, Clone)]
enum SharedOutcome {
    Ready(CachedImage),
    Failed(String),
}

struct SharedTask {
    outcome: watch::Sender<Option<SharedOutcome>>,
}

static LEASES: OnceLock<Mutex<HashMap<String, ImageLease>>> = OnceLock::new();
static TASKS: OnceLock<Mutex<HashMap<String, Arc<SharedTask>>>> = OnceLock::new();
static DOWNLOAD_GATE: OnceLock<Arc<Semaphore>> = OnceLock::new();
static ACTIVE_PARTIALS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
static FAILED_UNTIL: OnceLock<Mutex<HashMap<String, SystemTime>>> = OnceLock::new();

fn leases() -> &'static Mutex<HashMap<String, ImageLease>> {
    LEASES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn tasks() -> &'static Mutex<HashMap<String, Arc<SharedTask>>> {
    TASKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn active_partials() -> &'static Mutex<HashSet<PathBuf>> {
    ACTIVE_PARTIALS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn failed_until() -> &'static Mutex<HashMap<String, SystemTime>> {
    FAILED_UNTIL.get_or_init(|| Mutex::new(HashMap::new()))
}

struct PartialDownloadGuard {
    path: PathBuf,
}

impl PartialDownloadGuard {
    fn register(path: PathBuf) -> AppResult<Self> {
        active_partials()
            .lock()
            .map_err(|_| AppError::msg("feed_image_state_failed"))?
            .insert(path.clone());
        Ok(Self { path })
    }
}

impl Drop for PartialDownloadGuard {
    fn drop(&mut self) {
        if let Ok(mut paths) = active_partials().lock() {
            paths.remove(&self.path);
        }
    }
}

fn download_gate() -> Arc<Semaphore> {
    DOWNLOAD_GATE
        .get_or_init(|| Arc::new(Semaphore::new(2)))
        .clone()
}

fn cache_key(url: &str) -> String {
    hex::encode(Sha256::digest(url.as_bytes()))
}

fn canonical_image_url(url: &str) -> AppResult<String> {
    validate_https_url(url)?;
    let mut parsed =
        reqwest::Url::parse(url).map_err(|_| AppError::msg("feed_image_url_invalid"))?;
    parsed.set_fragment(None);
    Ok(parsed.to_string())
}

/// 返回用户明确授权当前文章图片后，后端可发送的最小 Referer。
///
/// 这是为兼容要求落地页 Referer 的图片 CDN 而保留的唯一来源上下文：它只能是
/// 已保存文章的安全 HTTPS URL，移除 query 和 fragment 后发送；不会携带 Cookie、用户浏览
/// 历史、代理地址或任意前端传入的请求头。
fn image_referer(article_url: Option<&str>) -> Option<String> {
    let article_url = article_url?;
    validate_https_url(article_url).ok()?;
    let mut parsed = reqwest::Url::parse(article_url).ok()?;
    parsed.set_query(None);
    parsed.set_fragment(None);
    Some(parsed.to_string())
}

/// 从已净化保存的 Markdown 中提取有界的 HTTPS 图片地址。
pub(crate) fn extract_image_urls(markdown: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for segment in markdown.match_indices("](") {
        if result.len() >= MAX_IMAGES_PER_ITEM {
            break;
        }
        let prefix = &markdown[..segment.0];
        let Some(image_start) = prefix.rfind("![") else {
            continue;
        };
        if prefix[image_start + 2..].contains(']') {
            continue;
        }
        let tail = &markdown[segment.0 + segment.1.len()..];
        let Some(end) = tail.find(')') else { continue };
        let value = tail[..end]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches('<')
            .trim_matches('>');
        if value.len() > 2048 || !value.starts_with("https://") {
            continue;
        }
        if canonical_image_url(value).is_ok() {
            let source_url = value.to_string();
            if seen.insert(source_url.clone()) {
                result.push(source_url);
            }
        }
    }
    result
}

fn extension_for_mime(mime: &str) -> Option<&'static str> {
    match mime {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/avif" => Some("avif"),
        _ => None,
    }
}

fn inspect_image(path: &Path) -> AppResult<(&'static str, u64)> {
    let metadata =
        std::fs::metadata(path).map_err(|_| AppError::msg("feed_image_cache_missing"))?;
    if metadata.len() == 0 || metadata.len() > IMAGE_MAX_BYTES {
        return Err(AppError::msg("feed_image_too_large"));
    }
    let mut header = [0_u8; 32];
    let mut file = File::open(path).map_err(|_| AppError::msg("feed_image_cache_missing"))?;
    let count = file
        .read(&mut header)
        .map_err(|_| AppError::msg("feed_image_invalid"))?;
    let bytes = &header[..count];
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.len() >= 3 && bytes[..3] == [0xff, 0xd8, 0xff] {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && (bytes[8..12] == *b"avif" || bytes[8..12] == *b"avis")
    {
        "image/avif"
    } else {
        return Err(AppError::msg("feed_image_invalid"));
    };
    Ok((mime, metadata.len()))
}

fn cached_image(cache_dir: &Path, key: &str) -> Option<CachedImage> {
    for extension in ["png", "jpg", "gif", "webp", "avif"] {
        let path = cache_dir.join(format!("{key}.{extension}"));
        let Ok((mime_type, size_bytes)) = inspect_image(&path) else {
            continue;
        };
        if let Ok(file) = File::options().write(true).open(&path) {
            let _ = file.set_times(FileTimes::new().set_modified(SystemTime::now()));
        }
        return Some(CachedImage {
            path,
            mime_type,
            size_bytes,
        });
    }
    None
}

fn protected_paths(now: SystemTime) -> AppResult<HashSet<PathBuf>> {
    let mut current = leases()
        .lock()
        .map_err(|_| AppError::msg("feed_image_state_failed"))?;
    current.retain(|_, lease| {
        now.duration_since(lease.created_at).unwrap_or_default() <= IMAGE_LEASE_TTL
    });
    let mut protected: HashSet<PathBuf> =
        current.values().map(|lease| lease.path.clone()).collect();
    protected.extend(
        active_partials()
            .lock()
            .map_err(|_| AppError::msg("feed_image_state_failed"))?
            .iter()
            .cloned(),
    );
    Ok(protected)
}

pub(crate) fn maintain_cache(cache_dir: &Path) -> AppResult<()> {
    std::fs::create_dir_all(cache_dir)?;
    let now = SystemTime::now();
    let protected = protected_paths(now)?;
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(cache_dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let is_partial = path.extension().is_some_and(|ext| ext == "part");
        let expired = now.duration_since(modified).unwrap_or_default() > IMAGE_CACHE_TTL;
        let stale_partial =
            is_partial && now.duration_since(modified).unwrap_or_default() > IMAGE_PARTIAL_MAX_AGE;
        if !protected.contains(&path) && (stale_partial || (!is_partial && expired)) {
            let _ = std::fs::remove_file(path);
            continue;
        }
        entries.push((modified, metadata.len(), path));
    }
    entries.sort_by_key(|(modified, _, _)| *modified);
    let mut total: u64 = entries.iter().map(|(_, size, _)| *size).sum();
    for (_, size, path) in entries {
        if total <= IMAGE_CACHE_MAX_BYTES {
            break;
        }
        if !protected.contains(&path) && std::fs::remove_file(path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
    Ok(())
}

async fn download_image(
    url: &str,
    referer: Option<&str>,
    cache_dir: &Path,
) -> AppResult<CachedImage> {
    let canonical = canonical_image_url(url)?;
    let key = cache_key(&canonical);
    if let Some(image) = cached_image(cache_dir, &key) {
        return Ok(image);
    }
    maintain_cache(cache_dir)?;
    let partial = cache_dir.join(format!("{key}-{}.part", Uuid::new_v4()));
    let _partial_guard = PartialDownloadGuard::register(partial.clone())?;
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let progress = |_bytes: u64| {};
    let result = async {
        let mut current = canonical;
        for _ in 0..=5 {
            let parsed = reqwest::Url::parse(&current)
                .map_err(|_| AppError::msg("feed_image_url_invalid"))?;
            let host = parsed
                .host_str()
                .ok_or_else(|| AppError::msg("feed_image_url_invalid"))?;
            let addresses = resolve_public_addrs(host).await?;
            let response = fixed_https_download_to_path(
                &current,
                &addresses,
                USER_AGENT,
                referer,
                &partial,
                IMAGE_MAX_BYTES,
                &cancelled,
                &progress,
            )
            .await?;
            if (300..400).contains(&response.status) {
                let location = response
                    .headers
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| AppError::msg("feed_image_redirect_invalid"))?;
                current = parsed
                    .join(location)
                    .map_err(|_| AppError::msg("feed_image_redirect_invalid"))?
                    .to_string();
                validate_https_url(&current)?;
                continue;
            }
            if !(200..300).contains(&response.status) {
                return Err(AppError::msg("feed_image_http_failed"));
            }
            let claimed = response
                .headers
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if extension_for_mime(&claimed).is_none() {
                return Err(AppError::msg("feed_image_content_type"));
            }
            let (detected, size_bytes) = inspect_image(&partial)?;
            if detected != claimed {
                return Err(AppError::msg("feed_image_content_type"));
            }
            let ready = cache_dir.join(format!(
                "{key}.{}",
                extension_for_mime(detected).expect("validated image mime")
            ));
            tokio::fs::rename(&partial, &ready)
                .await
                .map_err(|_| AppError::msg("feed_image_cache_write_failed"))?;
            maintain_cache(cache_dir)?;
            return Ok(CachedImage {
                path: ready,
                mime_type: detected,
                size_bytes,
            });
        }
        Err(AppError::msg("feed_image_redirect_limit"))
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&partial).await;
    }
    result
}

async fn prepare_one(
    source_url: String,
    referer: Option<String>,
    cache_dir: PathBuf,
    force_retry: bool,
) -> AppResult<FeedImageLease> {
    let canonical = canonical_image_url(&source_url)?;
    let key = cache_key(&canonical);
    {
        let mut failures = failed_until()
            .lock()
            .map_err(|_| AppError::msg("feed_image_state_failed"))?;
        failures.retain(|_, retry_after| *retry_after > SystemTime::now());
        if !force_retry && failures.contains_key(&key) {
            return Err(AppError::msg("feed_image_retry_later"));
        }
        if force_retry {
            failures.remove(&key);
        }
    }
    let (task, owner) = {
        let mut registry = tasks()
            .lock()
            .map_err(|_| AppError::msg("feed_image_state_failed"))?;
        if let Some(existing) = registry.get(&key) {
            (existing.clone(), false)
        } else {
            let (outcome, _) = watch::channel(None);
            let task = Arc::new(SharedTask { outcome });
            registry.insert(key.clone(), task.clone());
            (task, true)
        }
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    if owner {
        let task_for_run = task.clone();
        let result = match tokio::time::timeout_at(deadline, async {
            let _permit = download_gate()
                .acquire_owned()
                .await
                .map_err(|_| AppError::msg("feed_image_state_failed"))?;
            download_image(&canonical, referer.as_deref(), &cache_dir).await
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(AppError::msg("feed_image_timeout")),
        };
        task_for_run.outcome.send_replace(Some(match result {
            Ok(image) => {
                if let Ok(mut failures) = failed_until().lock() {
                    failures.remove(&key);
                }
                SharedOutcome::Ready(image)
            }
            Err(error) => {
                if let Ok(mut failures) = failed_until().lock() {
                    failures.insert(key.clone(), SystemTime::now() + IMAGE_FAILURE_BACKOFF);
                }
                SharedOutcome::Failed(error.to_string())
            }
        }));
        tasks()
            .lock()
            .map_err(|_| AppError::msg("feed_image_state_failed"))?
            .remove(&key);
    }
    let mut receiver = task.outcome.subscribe();
    let outcome = loop {
        if let Some(value) = receiver.borrow().clone() {
            break value;
        }
        tokio::time::timeout_at(deadline, receiver.changed())
            .await
            .map_err(|_| AppError::msg("feed_image_timeout"))?
            .map_err(|_| AppError::msg("feed_image_state_failed"))?;
    };
    let image = match outcome {
        SharedOutcome::Ready(image) => image,
        SharedOutcome::Failed(code) => return Err(AppError::msg(code)),
    };
    let handle = Uuid::new_v4().to_string();
    let now = SystemTime::now();
    let mut current = leases()
        .lock()
        .map_err(|_| AppError::msg("feed_image_state_failed"))?;
    current.retain(|_, lease| {
        now.duration_since(lease.created_at).unwrap_or_default() <= IMAGE_LEASE_TTL
    });
    if current.len() >= IMAGE_LEASE_MAX {
        return Err(AppError::msg("feed_image_lease_capacity"));
    }
    let modified = std::fs::metadata(&image.path)
        .ok()
        .and_then(|value| value.modified().ok());
    current.insert(
        handle.clone(),
        ImageLease {
            path: image.path,
            mime_type: image.mime_type,
            size_bytes: image.size_bytes,
            modified,
            created_at: now,
        },
    );
    Ok(FeedImageLease {
        source_url,
        handle: handle.clone(),
        url: format!("iris-feed-image://localhost/{handle}"),
        mime_type: image.mime_type.to_string(),
        size_bytes: image.size_bytes,
    })
}

/// 为当前文章的全部图片建立本地 lease；单图失败不影响同篇其它图片。
pub(crate) async fn prepare_images(
    markdown: &str,
    article_url: Option<&str>,
    cache_dir: &Path,
) -> FeedImagesPrepareResult {
    let referer = image_referer(article_url);
    let urls = extract_image_urls(markdown);
    let mut tasks = tokio::task::JoinSet::new();
    for url in urls {
        tasks.spawn(prepare_one(
            url,
            referer.clone(),
            cache_dir.to_path_buf(),
            false,
        ));
    }
    let mut images = Vec::new();
    let mut failed_count = 0_u32;
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(lease)) => images.push(lease),
            _ => failed_count = failed_count.saturating_add(1),
        }
    }
    FeedImagesPrepareResult {
        images,
        failed_count,
    }
}

/// 为一张已获文章级授权的图片创建本地 lease；单张失败不影响同篇其它图片。
pub(crate) async fn prepare_image(
    source_url: String,
    article_url: Option<&str>,
    cache_dir: &Path,
    force_retry: bool,
) -> AppResult<FeedImageLease> {
    prepare_one(
        source_url,
        image_referer(article_url),
        cache_dir.to_path_buf(),
        force_retry,
    )
    .await
}

pub(crate) fn release_images(handles: &[String]) -> AppResult<()> {
    let mut current = leases()
        .lock()
        .map_err(|_| AppError::msg("feed_image_state_failed"))?;
    for handle in handles {
        current.remove(handle);
    }
    Ok(())
}

/// Clear only inactive cache files; active WebView leases and in-flight partials
/// remain valid until their short lease expires.
pub(crate) fn clear_cache(cache_dir: &Path) -> AppResult<u32> {
    std::fs::create_dir_all(cache_dir)?;
    let protected = protected_paths(SystemTime::now())?;
    let mut removed = 0;
    for entry in std::fs::read_dir(cache_dir)? {
        let path = entry?.path();
        if path.is_file() && !protected.contains(&path) && std::fs::remove_file(path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

fn protocol_response(request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let handle = request.uri().path().trim_start_matches('/');
    let lease = leases().lock().ok().and_then(|mut current| {
        current.retain(|_, lease| {
            SystemTime::now()
                .duration_since(lease.created_at)
                .unwrap_or_default()
                <= IMAGE_LEASE_TTL
        });
        current.get(handle).cloned()
    });
    let Some(lease) = lease else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap();
    };
    let valid = std::fs::metadata(&lease.path).ok().is_some_and(|metadata| {
        metadata.len() == lease.size_bytes && metadata.modified().ok() == lease.modified
    });
    if !valid {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap();
    }
    match std::fs::read(&lease.path) {
        Ok(body) if body.len() as u64 <= IMAGE_MAX_BYTES => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, lease.mime_type)
            .header(CONTENT_LENGTH, body.len())
            .body(body)
            .unwrap(),
        _ => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap(),
    }
}

pub(crate) fn register_image_protocol(
    builder: tauri::Builder<tauri::Wry>,
) -> tauri::Builder<tauri::Wry> {
    builder.register_uri_scheme_protocol("iris-feed-image", |_ctx, request| {
        protocol_response(request)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_only_deduplicated_https_markdown_images() {
        let images = extract_image_urls("[link](https://cdn.example/link) ![a](https://cdn.example/a.png) ![again](https://cdn.example/a.png) ![bad](http://cdn.example/b.png)");
        assert_eq!(images, vec!["https://cdn.example/a.png"]);
    }

    #[test]
    fn keeps_markdown_source_url_for_renderer_mapping() {
        let images = extract_image_urls("![a](https://cdn.example/a.png#display)");
        assert_eq!(images, vec!["https://cdn.example/a.png#display"]);
    }

    #[test]
    fn permits_only_safe_article_url_as_image_referer() {
        assert_eq!(
            image_referer(Some("https://reader.example/posts/42?tracking=1#section")).as_deref(),
            Some("https://reader.example/posts/42")
        );
        assert_eq!(image_referer(Some("http://reader.example/posts/42")), None);
        assert_eq!(image_referer(Some("https://localhost/posts/42")), None);
        assert_eq!(image_referer(None), None);
    }

    #[test]
    fn validates_image_magic_without_loading_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\nfixture").unwrap();
        assert_eq!(inspect_image(&path).unwrap().0, "image/png");
        std::fs::write(&path, b"<html>not an image</html>").unwrap();
        assert!(inspect_image(&path).is_err());
    }

    #[test]
    fn maintenance_keeps_a_fresh_partial_download() {
        let dir = tempfile::tempdir().unwrap();
        let partial = dir.path().join("image-download.part");
        std::fs::write(&partial, b"in-progress").unwrap();

        maintain_cache(dir.path()).unwrap();

        assert!(partial.exists(), "an active download must not be deleted");
    }
}
