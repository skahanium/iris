use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri::http::header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE};
use tauri::http::{Request, Response, StatusCode};
use tauri::State;
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::paths::{has_reserved_path_root, resolve_vault_path};
use std::sync::Arc;

const MAX_RANGE_LEN: u64 = 1024 * 1024;
const MAX_FULL_RESPONSE_LEN: u64 = 2 * 1024 * 1024;
const MEDIA_LEASE_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_MEDIA_LEASES: usize = 256;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceItem {
    pub kind: String,
    pub media_kind: Option<String>,
    pub mime_type: Option<String>,
    pub attachment_role: String,
    pub is_locked: bool,
    pub size_bytes: Option<u64>,
    pub updated_at: Option<String>,
    pub title: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaMetadata {
    pub handle: String,
    pub url: String,
    pub media_kind: String,
    pub mime_type: String,
    pub path: String,
    pub size_bytes: u64,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone)]
struct MediaLease {
    vault: PathBuf,
    relative_path: String,
    mime_type: String,
    size_bytes: u64,
    modified: Option<SystemTime>,
    created_at: Instant,
}

#[derive(Debug, Clone)]
struct ValidatedMediaPath {
    path: PathBuf,
    relative_path: String,
    media_kind: String,
    mime_type: String,
    size_bytes: u64,
    modified: Option<SystemTime>,
    updated_at: Option<String>,
}

static MEDIA_LEASES: OnceLock<Mutex<HashMap<String, MediaLease>>> = OnceLock::new();
static MEDIA_VALIDATION_CACHE: OnceLock<Mutex<HashMap<String, ValidatedMediaPath>>> =
    OnceLock::new();

#[cfg(test)]
static MEDIA_HEADER_VALIDATION_COUNTS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();

fn media_leases() -> &'static Mutex<HashMap<String, MediaLease>> {
    MEDIA_LEASES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn media_validation_cache() -> &'static Mutex<HashMap<String, ValidatedMediaPath>> {
    MEDIA_VALIDATION_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn media_validation_cache_key(vault: &Path, relative: &str) -> String {
    let vault_key = vault
        .canonicalize()
        .unwrap_or_else(|_| vault.to_path_buf())
        .to_string_lossy()
        .to_string();
    vault_key + "\0" + relative
}

fn sweep_media_leases(leases: &mut HashMap<String, MediaLease>) {
    let now = Instant::now();
    leases.retain(|_, lease| now.duration_since(lease.created_at) <= MEDIA_LEASE_TTL);
    while leases.len() > MAX_MEDIA_LEASES {
        let Some(oldest) = leases
            .iter()
            .min_by_key(|(_, lease)| lease.created_at)
            .map(|(handle, _)| handle.clone())
        else {
            break;
        };
        leases.remove(&oldest);
    }
}

pub(crate) fn clear_media_leases() {
    if let Ok(mut leases) = media_leases().lock() {
        leases.clear();
    }
    clear_media_validation_cache();
}

fn clear_media_validation_cache() {
    if let Ok(mut cache) = media_validation_cache().lock() {
        cache.clear();
    }
}

#[cfg(test)]
fn media_header_validation_counts() -> &'static Mutex<HashMap<String, usize>> {
    MEDIA_HEADER_VALIDATION_COUNTS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(test)]
fn media_header_validation_count_for(path: &Path) -> usize {
    let key = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string();
    media_header_validation_counts()
        .lock()
        .ok()
        .and_then(|counts| counts.get(&key).copied())
        .unwrap_or(0)
}

fn media_kind_for_extension(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "avif" | "png" | "jpg" | "jpeg" | "gif" | "webp" => Some("image"),
        "m4v" | "mp4" | "webm" | "mov" => Some("video"),
        "pdf" => Some("pdf"),
        _ => None,
    }
}

fn mime_type_for_extension(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "avif" => Some("image/avif"),
        "gif" => Some("image/gif"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "m4v" => Some("video/x-m4v"),
        "mov" => Some("video/quicktime"),
        "mp4" => Some("video/mp4"),
        "pdf" => Some("application/pdf"),
        "png" => Some("image/png"),
        "webm" => Some("video/webm"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn media_kind_and_mime(path: &Path) -> Option<(&'static str, &'static str)> {
    let ext = path.extension()?.to_str()?;
    Some((
        media_kind_for_extension(ext)?,
        mime_type_for_extension(ext)?,
    ))
}

fn validate_media_file_header(path: &Path, mime_type: &str) -> AppResult<()> {
    #[cfg(test)]
    if let Ok(mut counts) = media_header_validation_counts().lock() {
        *counts
            .entry(path.to_string_lossy().to_string())
            .or_insert(0) += 1;
    }
    let mut file = File::open(path)?;
    let mut header = [0_u8; 16];
    let read = file.read(&mut header)?;
    let bytes = &header[..read];
    let ok = match mime_type {
        "application/pdf" => bytes.starts_with(b"%PDF-"),
        "image/avif" => bytes.len() >= 12 && &bytes[4..12] == b"ftypavif",
        "image/gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "image/jpeg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/webp" => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP",
        "video/mp4" | "video/x-m4v" | "video/quicktime" => {
            bytes.len() >= 12 && &bytes[4..8] == b"ftyp"
        }
        "video/webm" => bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]),
        _ => true,
    };
    if ok {
        Ok(())
    } else {
        Err(AppError::msg("媒体文件头与类型不匹配"))
    }
}

fn supported_path_kind(path: &Path) -> Option<(&'static str, Option<&'static str>)> {
    let ext = path.extension()?.to_str()?;
    if ext.eq_ignore_ascii_case("md") {
        return Some(("note", None));
    }
    media_kind_for_extension(ext).map(|kind| ("media", Some(kind)))
}

fn title_from_relative_path(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(name)
        .to_string()
}

fn updated_at_from_system_time(time: SystemTime) -> String {
    let updated: DateTime<Utc> = time.into();
    updated.to_rfc3339()
}

fn normalized_relative_path(vault: &Path, path: &Path) -> AppResult<String> {
    let vault = vault
        .canonicalize()
        .map_err(|_| AppError::msg("Path is outside the vault"))?;
    let path = path
        .canonicalize()
        .map_err(|_| AppError::msg("Path is outside the vault"))?;
    let rel = path
        .strip_prefix(&vault)
        .map_err(|_| AppError::msg("Path is outside the vault"))?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

fn should_walk_entry(vault: &Path, entry: &DirEntry) -> bool {
    match normalized_relative_path(vault, entry.path()) {
        Ok(path) if path.is_empty() => true,
        Ok(path) => !has_reserved_path_root(&path),
        Err(_) => false,
    }
}

fn workspace_item_from_path(vault: &Path, path: &Path) -> AppResult<Option<WorkspaceItem>> {
    let Some((kind, media_kind)) = supported_path_kind(path) else {
        return Ok(None);
    };
    let rel = normalized_relative_path(vault, path)?;
    if has_reserved_path_root(&rel) {
        return Ok(None);
    }
    let metadata = std::fs::metadata(path)?;
    let mime_type = media_kind.and_then(|_| {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(mime_type_for_extension)
            .map(str::to_string)
    });
    Ok(Some(WorkspaceItem {
        kind: kind.to_string(),
        media_kind: media_kind.map(str::to_string),
        mime_type,
        attachment_role: if rel.starts_with("assets/") {
            "attachment"
        } else {
            "formal"
        }
        .to_string(),
        is_locked: false,
        size_bytes: Some(metadata.len()),
        updated_at: Some(updated_at_from_system_time(metadata.modified()?)),
        title: title_from_relative_path(&rel),
        path: rel,
    }))
}

#[cfg(test)]
fn validate_media_relative_path(vault: &Path, relative: &str) -> AppResult<PathBuf> {
    Ok(validate_media_relative_path_cached(vault, relative)?.path)
}

fn validate_media_relative_path_cached(
    vault: &Path,
    relative: &str,
) -> AppResult<ValidatedMediaPath> {
    if has_reserved_path_root(relative) {
        return Err(AppError::msg("不允许访问内部元数据路径"));
    }
    let resolved = resolve_vault_path(vault, relative)?;
    if !resolved.is_file() {
        return Err(AppError::msg("媒体路径不是文件"));
    }
    let resolved_relative = normalized_relative_path(vault, &resolved)?;
    if has_reserved_path_root(&resolved_relative) {
        return Err(AppError::msg("不允许访问内部元数据路径"));
    }
    let (media_kind, mime_type) =
        media_kind_and_mime(&resolved).ok_or_else(|| AppError::msg("不支持的媒体类型"))?;
    let metadata = std::fs::metadata(&resolved)?;
    let modified = metadata.modified().ok();
    let cache_key = media_validation_cache_key(vault, &resolved_relative);
    if let Ok(cache) = media_validation_cache().lock() {
        if let Some(cached) = cache.get(&cache_key) {
            if cached.size_bytes == metadata.len()
                && cached.modified == modified
                && cached.mime_type == mime_type
                && cached.path == resolved
            {
                return Ok(cached.clone());
            }
        }
    }

    validate_media_file_header(&resolved, mime_type)?;
    let validated = ValidatedMediaPath {
        path: resolved,
        relative_path: resolved_relative,
        media_kind: media_kind.to_string(),
        mime_type: mime_type.to_string(),
        size_bytes: metadata.len(),
        modified,
        updated_at: metadata.modified().ok().map(updated_at_from_system_time),
    };
    if let Ok(mut cache) = media_validation_cache().lock() {
        cache.insert(cache_key, validated.clone());
    }
    Ok(validated)
}

#[cfg(test)]
fn media_metadata_from_path(
    vault: &Path,
    path: &Path,
    handle: String,
    url: String,
) -> AppResult<MediaMetadata> {
    let (media_kind, mime_type) =
        media_kind_and_mime(path).ok_or_else(|| AppError::msg("不支持的媒体类型"))?;
    let metadata = std::fs::metadata(path)?;
    Ok(MediaMetadata {
        handle,
        url,
        media_kind: media_kind.to_string(),
        mime_type: mime_type.to_string(),
        path: normalized_relative_path(vault, path)?,
        size_bytes: metadata.len(),
        updated_at: Some(updated_at_from_system_time(metadata.modified()?)),
    })
}

fn workspace_list_from_vault(vault: &Path) -> AppResult<Vec<WorkspaceItem>> {
    let mut items = Vec::new();
    for entry in WalkDir::new(vault)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| should_walk_entry(vault, entry))
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if let Some(item) = workspace_item_from_path(vault, entry.path())? {
            items.push(item);
        }
    }
    items.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(items)
}

fn workspace_list_from_index(
    conn: &rusqlite::Connection,
    limit: Option<usize>,
    offset: Option<usize>,
) -> AppResult<Vec<WorkspaceItem>> {
    let limit = limit.map(|value| value as i64).unwrap_or(-1);
    let offset = offset.unwrap_or(0) as i64;
    let mut stmt = conn.prepare(
        "SELECT kind, media_kind, mime_type, attachment_role, is_locked,
                size_bytes, updated_at, title, path
         FROM (
            SELECT
                'note' AS kind,
                NULL AS media_kind,
                NULL AS mime_type,
                'formal' AS attachment_role,
                COALESCE(is_locked, 0) AS is_locked,
                NULL AS size_bytes,
                updated_at,
                COALESCE(NULLIF(title, ''), path) AS title,
                path
            FROM files
            WHERE path <> '.iris'
              AND path NOT LIKE '.iris/%'
              AND path <> '.classified'
              AND path NOT LIKE '.classified/%'
            UNION ALL
            SELECT
                'media' AS kind,
                media_kind,
                mime_type,
                CASE WHEN path LIKE 'assets/%' THEN 'attachment' ELSE 'formal' END
                    AS attachment_role,
                0 AS is_locked,
                size_bytes,
                updated_at,
                title,
                path
            FROM workspace_media
            WHERE path <> '.iris'
              AND path NOT LIKE '.iris/%'
              AND path <> '.classified'
              AND path NOT LIKE '.classified/%'
         )
         ORDER BY path
         LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![limit, offset], |row| {
        let size_bytes = row
            .get::<_, Option<i64>>(5)?
            .and_then(|value| u64::try_from(value).ok());
        Ok(WorkspaceItem {
            kind: row.get(0)?,
            media_kind: row.get(1)?,
            mime_type: row.get(2)?,
            attachment_role: row.get(3)?,
            is_locked: row.get(4)?,
            size_bytes,
            updated_at: row.get(6)?,
            title: row.get(7)?,
            path: row.get(8)?,
        })
    })?;
    let items = rows.collect::<Result<Vec<_>, _>>()?;
    Ok(items)
}

#[tauri::command]
pub fn media_metadata(state: State<'_, Arc<AppState>>, path: String) -> AppResult<MediaMetadata> {
    let vault = state.vault_path()?;
    let validated = validate_media_relative_path_cached(&vault, &path)?;
    Ok(MediaMetadata {
        handle: String::new(),
        url: String::new(),
        media_kind: validated.media_kind,
        mime_type: validated.mime_type,
        path: validated.relative_path,
        size_bytes: validated.size_bytes,
        updated_at: validated.updated_at,
    })
}

#[tauri::command]
pub fn media_resolve(state: State<'_, Arc<AppState>>, path: String) -> AppResult<MediaMetadata> {
    let vault = state.vault_path()?;
    let validated = validate_media_relative_path_cached(&vault, &path)?;
    let handle = Uuid::new_v4().to_string();
    let mut leases = media_leases()
        .lock()
        .map_err(|_| AppError::msg("Media lease lock poisoned"))?;
    sweep_media_leases(&mut leases);
    leases.insert(
        handle.clone(),
        MediaLease {
            vault: vault.canonicalize().unwrap_or(vault.clone()),
            relative_path: validated.relative_path.clone(),
            mime_type: validated.mime_type.clone(),
            size_bytes: validated.size_bytes,
            modified: validated.modified,
            created_at: Instant::now(),
        },
    );

    let url = format!("iris-media://localhost/{handle}");
    Ok(MediaMetadata {
        handle,
        url,
        media_kind: validated.media_kind,
        mime_type: validated.mime_type,
        path: validated.relative_path,
        size_bytes: validated.size_bytes,
        updated_at: validated.updated_at,
    })
}

#[tauri::command]
pub fn media_release(handle: String) -> AppResult<bool> {
    let released = media_leases()
        .lock()
        .map_err(|_| AppError::msg("Media lease lock poisoned"))?
        .remove(&handle)
        .is_some();
    Ok(released)
}

fn apply_window<T>(items: Vec<T>, limit: Option<usize>, offset: Option<usize>) -> Vec<T> {
    let start = offset.unwrap_or(0).min(items.len());
    let end = limit
        .map(|limit| start.saturating_add(limit).min(items.len()))
        .unwrap_or(items.len());
    items.into_iter().skip(start).take(end - start).collect()
}

#[tauri::command]
pub fn workspace_list(
    state: State<'_, Arc<AppState>>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> AppResult<Vec<WorkspaceItem>> {
    match state
        .db
        .with_read_conn(|conn| workspace_list_from_index(conn, limit, offset))
    {
        Ok(items) => Ok(items),
        Err(err) => {
            tracing::warn!(error = %err, "workspace_list index query failed; falling back to vault scan");
            let vault = state.vault_path()?;
            Ok(apply_window(
                workspace_list_from_vault(&vault)?,
                limit,
                offset,
            ))
        }
    }
}

fn response_with_status(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .body(Vec::new())
        .expect("empty protocol response should be valid")
}

fn parse_single_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?;
    let first = spec.split(',').next()?.trim();
    let (start, end) = first.split_once('-')?;
    if start.is_empty() {
        let suffix = end.parse::<u64>().ok()?;
        if suffix == 0 {
            return None;
        }
        let start = len.saturating_sub(suffix);
        return Some((start, len.saturating_sub(1)));
    }
    let start = start.parse::<u64>().ok()?;
    let end = if end.is_empty() {
        len.saturating_sub(1)
    } else {
        end.parse::<u64>().ok()?
    };
    if len == 0 || start >= len || end < start {
        return None;
    }
    Some((start, end.min(len - 1)))
}

fn media_protocol_response(request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let handle = request.uri().path().trim_start_matches('/');
    let Ok(mut leases) = media_leases().lock() else {
        return response_with_status(StatusCode::INTERNAL_SERVER_ERROR);
    };
    sweep_media_leases(&mut leases);
    let Some(lease) = leases.get(handle).cloned() else {
        return response_with_status(StatusCode::NOT_FOUND);
    };
    drop(leases);

    let Ok(path) = resolve_vault_path(&lease.vault, &lease.relative_path) else {
        let _ = media_release(handle.to_string());
        return response_with_status(StatusCode::NOT_FOUND);
    };
    let Ok(resolved_relative) = normalized_relative_path(&lease.vault, &path) else {
        let _ = media_release(handle.to_string());
        return response_with_status(StatusCode::NOT_FOUND);
    };
    if resolved_relative != lease.relative_path || has_reserved_path_root(&resolved_relative) {
        let _ = media_release(handle.to_string());
        return response_with_status(StatusCode::NOT_FOUND);
    }
    let Ok(mut file) = File::open(&path) else {
        return response_with_status(StatusCode::NOT_FOUND);
    };
    let Ok(metadata) = file.metadata() else {
        return response_with_status(StatusCode::NOT_FOUND);
    };
    let len = metadata.len();
    let modified = metadata.modified().ok();
    if len != lease.size_bytes || modified != lease.modified {
        let _ = media_release(handle.to_string());
        return response_with_status(StatusCode::NOT_FOUND);
    }
    let mut builder = Response::builder()
        .header(CONTENT_TYPE, lease.mime_type.as_str())
        .header(ACCEPT_RANGES, "bytes");

    if let Some(range_header) = request
        .headers()
        .get(RANGE)
        .and_then(|value| value.to_str().ok())
    {
        let Some((start, mut end)) = parse_single_range(range_header, len) else {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(CONTENT_RANGE, format!("bytes */{len}"))
                .body(Vec::new())
                .expect("range-not-satisfiable response should be valid");
        };
        end = start + (end - start).min(MAX_RANGE_LEN - 1);
        let nbytes = end + 1 - start;
        let mut body = Vec::with_capacity(nbytes as usize);
        if file.seek(SeekFrom::Start(start)).is_err()
            || file.take(nbytes).read_to_end(&mut body).is_err()
        {
            return response_with_status(StatusCode::INTERNAL_SERVER_ERROR);
        }
        builder = builder
            .status(StatusCode::PARTIAL_CONTENT)
            .header(CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
            .header(CONTENT_LENGTH, nbytes);
        return builder
            .body(body)
            .expect("partial media protocol response should be valid");
    }

    if len > MAX_FULL_RESPONSE_LEN {
        let end = len.min(MAX_RANGE_LEN).saturating_sub(1);
        let nbytes = end + 1;
        let mut body = Vec::with_capacity(nbytes as usize);
        if file.take(nbytes).read_to_end(&mut body).is_err() {
            return response_with_status(StatusCode::INTERNAL_SERVER_ERROR);
        }
        return builder
            .status(StatusCode::PARTIAL_CONTENT)
            .header(CONTENT_RANGE, format!("bytes 0-{end}/{len}"))
            .header(CONTENT_LENGTH, nbytes)
            .body(body)
            .expect("initial partial media protocol response should be valid");
    }

    let mut body = Vec::with_capacity(len as usize);
    if file.read_to_end(&mut body).is_err() {
        return response_with_status(StatusCode::INTERNAL_SERVER_ERROR);
    }
    builder
        .header(CONTENT_LENGTH, len)
        .body(body)
        .expect("media protocol response should be valid")
}

pub(crate) fn register_media_protocol(
    builder: tauri::Builder<tauri::Wry>,
) -> tauri::Builder<tauri::Wry> {
    builder.register_uri_scheme_protocol("iris-media", |_ctx, request| {
        media_protocol_response(request)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn media_paths_reject_reserved_roots_case_insensitively() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(vault.join(".iris")).unwrap();
        fs::create_dir_all(vault.join(".classified")).unwrap();
        fs::write(vault.join(".iris/hidden.png"), b"png").unwrap();
        fs::write(vault.join(".classified/secret.png"), b"png").unwrap();

        for path in [
            ".iris/hidden.png",
            ".IRIS/hidden.png",
            ".classified/secret.png",
            ".CLASSIFIED/secret.png",
        ] {
            let err = validate_media_relative_path(&vault, path).unwrap_err();
            assert!(
                err.to_string().contains("内部元数据"),
                "{path} should be rejected as a reserved root"
            );
        }
    }

    #[test]
    fn workspace_list_from_index_merges_notes_and_media_without_disk_scan() {
        let db = crate::storage::db::Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO files
                 (path, title, content_hash, created_at, updated_at, is_locked)
                 VALUES (?1, ?2, 'hash-a', '2026-01-01', '2026-01-02', 1)",
                rusqlite::params!["notes/a.md", "Alpha"],
            )?;
            conn.execute(
                "INSERT INTO workspace_media
                 (path, title, media_kind, mime_type, size_bytes, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    "assets/photo.png",
                    "photo",
                    "image",
                    "image/png",
                    42_i64,
                    "2026-01-03"
                ],
            )?;
            Ok(())
        })
        .unwrap();

        let items = db
            .with_read_conn(|conn| workspace_list_from_index(conn, None, None))
            .unwrap();

        assert_eq!(
            items
                .iter()
                .map(|item| item.path.as_str())
                .collect::<Vec<_>>(),
            vec!["assets/photo.png", "notes/a.md"]
        );
        let media = items
            .iter()
            .find(|item| item.path == "assets/photo.png")
            .unwrap();
        assert_eq!(media.kind, "media");
        assert_eq!(media.media_kind.as_deref(), Some("image"));
        assert_eq!(media.attachment_role, "attachment");
        assert_eq!(media.size_bytes, Some(42));
        let note = items.iter().find(|item| item.path == "notes/a.md").unwrap();
        assert_eq!(note.kind, "note");
        assert!(note.is_locked);
    }

    #[test]
    fn workspace_list_from_index_applies_limit_offset_in_sql() {
        let db = crate::storage::db::Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            for path in ["a.md", "b.md", "c.md", "d.md"] {
                conn.execute(
                    "INSERT INTO files
                     (path, title, content_hash, created_at, updated_at)
                     VALUES (?1, ?1, ?1, '2026-01-01', '2026-01-01')",
                    [path],
                )?;
            }
            Ok(())
        })
        .unwrap();

        let items = db
            .with_read_conn(|conn| workspace_list_from_index(conn, Some(2), Some(1)))
            .unwrap();

        assert_eq!(
            items
                .iter()
                .map(|item| item.path.as_str())
                .collect::<Vec<_>>(),
            vec!["b.md", "c.md"]
        );
    }

    #[test]
    fn workspace_list_classifies_supported_files_and_skips_reserved_roots() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(vault.join("notes")).unwrap();
        fs::create_dir_all(vault.join("media")).unwrap();
        fs::create_dir_all(vault.join(".IRIS")).unwrap();
        fs::create_dir_all(vault.join(".classified")).unwrap();
        fs::write(vault.join("notes/Plan.md"), "# Plan").unwrap();
        fs::write(vault.join("media/photo.JPG"), b"jpg").unwrap();
        fs::write(vault.join("media/clip.mp4"), b"mp4").unwrap();
        fs::write(vault.join("media/doc.pdf"), b"pdf").unwrap();
        fs::write(vault.join("media/ignore.txt"), b"text").unwrap();
        fs::write(vault.join(".IRIS/internal.png"), b"png").unwrap();
        fs::write(vault.join(".classified/secret.md"), b"# Secret").unwrap();

        let items = workspace_list_from_vault(&vault).unwrap();
        let paths = items
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            paths,
            vec![
                "media/clip.mp4",
                "media/doc.pdf",
                "media/photo.JPG",
                "notes/Plan.md"
            ]
        );

        let note = items
            .iter()
            .find(|item| item.path == "notes/Plan.md")
            .unwrap();
        assert_eq!(note.kind, "note");
        assert_eq!(note.media_kind, None);
        assert_eq!(note.title, "Plan");

        let image = items
            .iter()
            .find(|item| item.path == "media/photo.JPG")
            .unwrap();
        assert_eq!(image.kind, "media");
        assert_eq!(image.media_kind.as_deref(), Some("image"));
        assert_eq!(image.mime_type.as_deref(), Some("image/jpeg"));
        assert_eq!(image.attachment_role, "formal");
        assert_eq!(image.size_bytes, Some(3));

        let video = items
            .iter()
            .find(|item| item.path == "media/clip.mp4")
            .unwrap();
        assert_eq!(video.kind, "media");
        assert_eq!(video.media_kind.as_deref(), Some("video"));

        let document = items
            .iter()
            .find(|item| item.path == "media/doc.pdf")
            .unwrap();
        assert_eq!(document.kind, "media");
        assert_eq!(document.media_kind.as_deref(), Some("pdf"));
        assert_eq!(document.mime_type.as_deref(), Some("application/pdf"));
    }

    #[test]
    fn media_metadata_rejects_markdown_notes() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("Plan.md"), "# Plan").unwrap();

        let err = validate_media_relative_path(&vault, "Plan.md").unwrap_err();
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn media_resolve_metadata_uses_opaque_lease_urls() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(vault.join("assets")).unwrap();
        let path = vault.join("assets/paper.pdf");
        fs::write(&path, b"pdf").unwrap();

        let handle = "lease-1".to_string();
        let metadata = media_metadata_from_path(
            &vault,
            &path,
            handle.clone(),
            format!("iris-media://localhost/{handle}"),
        )
        .unwrap();

        assert_eq!(metadata.media_kind, "pdf");
        assert_eq!(metadata.mime_type, "application/pdf");
        assert_eq!(metadata.path, "assets/paper.pdf");
        assert_eq!(metadata.url, "iris-media://localhost/lease-1");
        assert!(!metadata.url.contains(vault.to_string_lossy().as_ref()));
    }

    #[test]
    fn media_validation_rejects_extension_spoofing() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(vault.join("assets")).unwrap();
        fs::write(vault.join("assets/fake.png"), b"not actually png").unwrap();

        let err = validate_media_relative_path(&vault, "assets/fake.png").unwrap_err();
        assert!(err.to_string().contains("文件头"));
    }

    #[cfg(unix)]
    #[test]
    fn media_validation_rechecks_canonical_reserved_roots() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(vault.join("assets")).unwrap();
        fs::create_dir_all(vault.join(".classified")).unwrap();
        fs::write(
            vault.join(".classified/secret.png"),
            b"\x89PNG\r\n\x1a\nsecret",
        )
        .unwrap();
        std::os::unix::fs::symlink(
            vault.join(".classified/secret.png"),
            vault.join("assets/innocent.png"),
        )
        .unwrap();

        let err = validate_media_relative_path(&vault, "assets/innocent.png").unwrap_err();
        assert!(err.to_string().contains("内部元数据"));
    }

    #[test]
    fn media_resolve_reuses_cached_validation_for_unchanged_file() {
        clear_media_leases();
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(vault.join("assets")).unwrap();
        let photo = vault.join("assets/photo.png");
        fs::write(&photo, b"\x89PNG\r\n\x1a\nbody").unwrap();

        let before = media_header_validation_count_for(&photo);
        validate_media_relative_path_cached(&vault, "assets/photo.png").unwrap();
        validate_media_relative_path_cached(&vault, "assets/photo.png").unwrap();

        assert_eq!(media_header_validation_count_for(&photo) - before, 1);
    }

    #[test]
    fn media_cache_invalidates_when_size_or_modified_changes() {
        clear_media_leases();
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(vault.join("assets")).unwrap();
        let photo = vault.join("assets/photo.png");
        fs::write(&photo, b"\x89PNG\r\n\x1a\nbody").unwrap();

        let before = media_header_validation_count_for(&photo);
        validate_media_relative_path_cached(&vault, "assets/photo.png").unwrap();
        fs::write(&photo, b"not actually png and longer").unwrap();

        let err = validate_media_relative_path_cached(&vault, "assets/photo.png").unwrap_err();
        assert!(!err.to_string().is_empty());
        assert_eq!(media_header_validation_count_for(&photo) - before, 2);
    }

    #[test]
    fn media_protocol_does_not_revalidate_header_for_valid_lease() {
        clear_media_leases();
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(vault.join("assets")).unwrap();
        let photo = vault.join("assets/photo.png");
        fs::write(&photo, b"\x89PNG\r\n\x1a\nbody").unwrap();
        let before = media_header_validation_count_for(&photo);
        let validated = validate_media_relative_path_cached(&vault, "assets/photo.png").unwrap();
        media_leases().lock().unwrap().insert(
            "cached".to_string(),
            MediaLease {
                vault: vault.canonicalize().unwrap(),
                relative_path: validated.relative_path,
                mime_type: validated.mime_type,
                size_bytes: validated.size_bytes,
                modified: validated.modified,
                created_at: Instant::now(),
            },
        );

        for _ in 0..2 {
            let request = Request::builder()
                .uri("iris-media://localhost/cached")
                .body(Vec::new())
                .unwrap();
            let response = media_protocol_response(request);
            assert_eq!(response.status(), StatusCode::OK);
        }
        assert_eq!(media_header_validation_count_for(&photo) - before, 1);
    }

    #[test]
    fn media_protocol_serves_large_no_range_requests_as_initial_chunk() {
        clear_media_leases();
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(vault.join("assets")).unwrap();
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.resize((MAX_FULL_RESPONSE_LEN + 128) as usize, 0);
        fs::write(vault.join("assets/large.png"), bytes).unwrap();
        media_leases().lock().unwrap().insert(
            "large".to_string(),
            MediaLease {
                vault: vault.canonicalize().unwrap(),
                relative_path: "assets/large.png".to_string(),
                mime_type: "image/png".to_string(),
                size_bytes: (MAX_FULL_RESPONSE_LEN + 128),
                modified: std::fs::metadata(vault.join("assets/large.png"))
                    .unwrap()
                    .modified()
                    .ok(),
                created_at: Instant::now(),
            },
        );

        let request = Request::builder()
            .uri("iris-media://localhost/large")
            .body(Vec::new())
            .unwrap();
        let response = media_protocol_response(request);

        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            response.headers().get(CONTENT_RANGE).unwrap(),
            "bytes 0-1048575/2097280"
        );
        assert_eq!(response.body().len(), MAX_RANGE_LEN as usize);
    }

    #[test]
    fn range_parser_clamps_large_requests() {
        assert_eq!(parse_single_range("bytes=0-9", 100), Some((0, 9)));
        assert_eq!(parse_single_range("bytes=10-", 100), Some((10, 99)));
        assert_eq!(parse_single_range("bytes=-5", 100), Some((95, 99)));
        assert_eq!(parse_single_range("bytes=200-300", 100), None);
    }
}
