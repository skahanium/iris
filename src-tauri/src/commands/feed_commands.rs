//! 订阅资料库 IPC 命令（阶段 3：冻结契约）。
//!
//! 命令只做验证/授权边界与仓储/service 调用，不内嵌 SQL；所有 DTO 与
//! `src/types/ipc.ts` / `src/lib/ipc.ts` 保持 camelCase 一致；`source_payload`
//! 永不进入任何命令参数或返回值。

use std::sync::Arc;

use chrono::Utc;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::feed::discovery::{discover, FeedCandidate};
use crate::feed::fetch::ProdNetGate;
use crate::feed::model::{
    FeedFulltextEnqueueOutcome, FeedImageLease, FeedImageManifest, FeedImageSource,
    FeedImagesPrepareResult, FeedItemDetail, FeedItemQuery, FeedItemStatePatch, FeedItemSummary,
    FeedLibrarySummary, FeedSourcePatch, FeedSourceSummary, FeedSourceTrashPreview,
    FeedTrashSnapshot, FeedTrashSource, NewFeedSource,
};
use crate::feed::opml::canonicalize_https_url;
use crate::feed::opml::{export_opml, import_opml_with_interval, OpmlImportResult, OPML_MAX_BYTES};
use crate::feed::repository::FeedRepository;
use crate::feed::sync::{FeedSyncBatchOutcome, HistoryReadPolicy, SyncMode, SyncStatus};
use crate::network::safe_https::validate_https_url;

// ── 长度边界（所有 ID/URL/string 有界）─────────────────────

const MAX_ID_LEN: usize = 200;
const MAX_URL_LEN: usize = 2048;
const MAX_STRING_LEN: usize = 4096;
fn check_id(id: &str) -> AppResult<()> {
    if id.trim().is_empty() || id.len() > MAX_ID_LEN {
        return Err(AppError::msg("feed_validation_id"));
    }
    Ok(())
}

fn check_url(url: &str) -> AppResult<()> {
    if url.trim().is_empty() || url.len() > MAX_URL_LEN {
        return Err(AppError::msg("feed_validation_url"));
    }
    validate_https_url(url)
}

fn check_string(value: &str) -> AppResult<()> {
    if value.len() > MAX_STRING_LEN {
        return Err(AppError::msg("feed_validation_string"));
    }
    Ok(())
}

fn check_timestamp(value: &str) -> AppResult<()> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|_| ())
        .map_err(|_| AppError::msg("feed_validation_timestamp"))
}

fn check_feed_query(query: &FeedItemQuery) -> AppResult<()> {
    if !(crate::feed::repository::ITEM_LIMIT_MIN..=crate::feed::repository::ITEM_LIMIT_MAX)
        .contains(&query.limit)
    {
        return Err(AppError::msg("feed_validation_limit"));
    }
    if let Some(source_id) = &query.source_id {
        check_id(source_id)?;
    }
    if let Some(search) = &query.search {
        if search.trim().is_empty() || search.len() > MAX_STRING_LEN {
            return Err(AppError::msg("feed_validation_search"));
        }
    }
    if let Some(received_after) = &query.received_after {
        check_timestamp(received_after)?;
    }
    if let Some(cursor) = &query.cursor {
        check_timestamp(&cursor.sort_at)?;
        if cursor.row_id <= 0 {
            return Err(AppError::msg("feed_validation_cursor"));
        }
    }
    Ok(())
}

// ── 输入 DTO ───────────────────────────────────────────────

/// 添加订阅源：URL 与候选标题必填。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedSourceAddInput {
    pub url: String,
    pub title: String,
    pub title_override: Option<String>,
    pub folder_path: Option<String>,
    pub fetch_interval_minutes: Option<i64>,
    pub restore_deleted: Option<bool>,
}

/// 编辑订阅源：`titleOverride: null` 清除覆盖标题，缺省字段不改动。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FeedSourceUpdateInput {
    pub title_override: Option<Option<String>>,
    pub folder_path: Option<String>,
    pub fetch_interval_minutes: Option<i64>,
    pub is_enabled: Option<bool>,
    pub fulltext_enabled: Option<bool>,
}

/// 同步结果 DTO（status 为稳定字符串，事件另发 `feed:changed`）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedSyncOutcomeDto {
    pub status: String,
    pub new_items: u32,
    pub skipped_history: u32,
    pub error_code: Option<String>,
}

impl FeedSyncOutcomeDto {
    fn from_status(status: &SyncStatus) -> Self {
        match status {
            SyncStatus::Succeeded {
                new_items,
                skipped_history,
            } => Self {
                status: "succeeded".to_string(),
                new_items: *new_items as u32,
                skipped_history: *skipped_history as u32,
                error_code: None,
            },
            SyncStatus::NotModified => Self {
                status: "not_modified".to_string(),
                new_items: 0,
                skipped_history: 0,
                error_code: None,
            },
            SyncStatus::Skipped => Self {
                status: "skipped".to_string(),
                new_items: 0,
                skipped_history: 0,
                error_code: None,
            },
            SyncStatus::InFlight => Self {
                status: "in_flight".to_string(),
                new_items: 0,
                skipped_history: 0,
                error_code: None,
            },
            SyncStatus::Failed { code } => Self {
                status: "failed".to_string(),
                new_items: 0,
                skipped_history: 0,
                error_code: Some(code.clone()),
            },
        }
    }
}

// ── 命令（薄壳）────────────────────────────────────────────

#[tauri::command]
pub async fn feed_discover(url: String) -> AppResult<Vec<FeedCandidate>> {
    check_url(&url)?;
    discover(&ProdNetGate, &url).await
}

#[tauri::command]
pub fn feed_source_add(
    state: State<'_, Arc<AppState>>,
    input: FeedSourceAddInput,
) -> AppResult<FeedSourceSummary> {
    feed_source_add_impl(&state, &input)
}

#[tauri::command]
pub fn feed_source_list(state: State<'_, Arc<AppState>>) -> AppResult<Vec<FeedSourceSummary>> {
    state.db.with_read_conn(FeedRepository::list_sources)
}

#[tauri::command]
pub fn feed_source_update(
    state: State<'_, Arc<AppState>>,
    source_id: String,
    patch: FeedSourceUpdateInput,
) -> AppResult<()> {
    feed_source_update_impl(&state, &source_id, &patch)
}

#[tauri::command]
pub fn feed_source_remove(
    state: State<'_, Arc<AppState>>,
    source_id: String,
    keep_items: bool,
) -> AppResult<u32> {
    feed_source_remove_impl(&state, &source_id, keep_items)
}

#[tauri::command]
pub fn feed_source_trash(state: State<'_, Arc<AppState>>, source_id: String) -> AppResult<u32> {
    feed_source_trash_impl(&state, &source_id)
}

#[tauri::command]
pub fn feed_source_trash_restore(
    state: State<'_, Arc<AppState>>,
    source_id: String,
) -> AppResult<()> {
    check_id(&source_id)?;
    let restored = state
        .db
        .with_conn(|conn| FeedRepository::restore_source(conn, &source_id, Utc::now()))?;
    if !restored {
        return Err(AppError::msg("feed_trash_source_not_found"));
    }
    Ok(())
}

#[tauri::command]
pub fn feed_source_trash_purge(
    state: State<'_, Arc<AppState>>,
    source_id: String,
) -> AppResult<u32> {
    check_id(&source_id)?;
    cleanup_source_document_cache(&state, &source_id)?;
    let purged = state
        .db
        .with_conn(|conn| FeedRepository::purge_source(conn, &source_id))?
        .map(|count| count as u32)
        .ok_or_else(|| AppError::msg("feed_trash_source_not_found"))?;
    Ok(purged)
}

#[tauri::command]
pub fn feed_source_item_count(
    state: State<'_, Arc<AppState>>,
    source_id: String,
) -> AppResult<u32> {
    check_id(&source_id)?;
    let count = state
        .db
        .with_read_conn(|conn| FeedRepository::count_items(conn, &source_id))?;
    Ok(count as u32)
}

#[tauri::command]
pub fn feed_source_trash_preview(
    state: State<'_, Arc<AppState>>,
    source_id: String,
) -> AppResult<FeedSourceTrashPreview> {
    check_id(&source_id)?;
    state
        .db
        .with_read_conn(|conn| FeedRepository::source_trash_preview(conn, &source_id, Utc::now()))?
        .ok_or_else(|| AppError::msg("feed_source_not_found"))
}

#[tauri::command]
pub fn feed_source_trash_match(
    state: State<'_, Arc<AppState>>,
    url: String,
) -> AppResult<Option<FeedTrashSource>> {
    check_url(&url)?;
    let canonical = canonicalize_https_url(&url)?;
    state
        .db
        .with_read_conn(|conn| FeedRepository::find_deleted_source_by_feed_url(conn, &canonical))
}

/// 订阅全局设置与维护页的资料库汇总；不暴露单源 URL 或文章内容。
#[tauri::command]
pub fn feed_library_summary(state: State<'_, Arc<AppState>>) -> AppResult<FeedLibrarySummary> {
    state.db.with_read_conn(FeedRepository::library_summary)
}

/// RSS 回收站：仅返回已软删除的应用缓存条目，不涉及 Markdown 笔记回收站。
#[tauri::command]
pub fn feed_trash_list(state: State<'_, Arc<AppState>>) -> AppResult<FeedTrashSnapshot> {
    state
        .db
        .with_read_conn(|conn| FeedRepository::trash_snapshot(conn, 200))
}

#[tauri::command]
pub fn feed_trash_restore(state: State<'_, Arc<AppState>>, item_id: String) -> AppResult<()> {
    check_id(&item_id)?;
    let restored = state
        .db
        .with_conn(|conn| FeedRepository::restore_deleted_item(conn, &item_id, Utc::now()))?;
    if !restored {
        return Err(AppError::msg("feed_trash_item_not_found"));
    }
    Ok(())
}

#[tauri::command]
pub fn feed_trash_clear(state: State<'_, Arc<AppState>>) -> AppResult<u32> {
    state.db.with_conn(FeedRepository::clear_deleted_items)
}

/// 仅在用户明确触发时执行 VACUUM，避免后台锁住正在使用的 SQLite。
#[tauri::command]
pub fn feed_library_optimize(state: State<'_, Arc<AppState>>) -> AppResult<()> {
    state.db.with_conn(FeedRepository::vacuum_feed_library)
}

#[tauri::command]
pub async fn feed_document_prepare(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    item_id: String,
) -> AppResult<crate::feed::document::FeedDocumentLease> {
    check_id(&item_id)?;
    let url = state
        .db
        .with_read_conn(|conn| FeedRepository::get_primary_document_url(conn, &item_id))?
        .ok_or_else(|| AppError::msg("feed_document_not_found"))?;
    let _ = app.emit(
        "feed:document-progress",
        serde_json::json!({ "itemId": item_id, "status": "downloading", "bytes": 0 }),
    );
    let cache_dir = state.cache_dir().join("feed-media").join("documents");
    let progress_app = app.clone();
    let progress_item_id = item_id.clone();
    let progress = Arc::new(move |bytes: u64| {
        let _ = progress_app.emit(
            "feed:document-progress",
            serde_json::json!({
                "itemId": progress_item_id,
                "status": "downloading",
                "bytes": bytes
            }),
        );
    });
    let result =
        crate::feed::document::prepare_document(&item_id, &url, &cache_dir, progress).await;
    let (status, bytes) = match &result {
        Ok(lease) => ("ready", lease.size_bytes),
        Err(error) if error.to_string() == "feed_document_cancelled" => ("cancelled", 0),
        Err(_) => ("failed", 0),
    };
    let _ = app.emit(
        "feed:document-progress",
        serde_json::json!({ "itemId": item_id, "status": status, "bytes": bytes }),
    );
    result
}

#[tauri::command]
pub fn feed_document_cancel(item_id: String) -> AppResult<()> {
    check_id(&item_id)?;
    crate::feed::document::cancel_document(&item_id)
}

#[tauri::command]
pub fn feed_document_release(handle: String) -> AppResult<()> {
    check_id(&handle)?;
    crate::feed::document::release_document(&handle)
}

#[tauri::command]
pub fn feed_document_cache_clear(state: State<'_, Arc<AppState>>) -> AppResult<u32> {
    crate::feed::document::clear_cache(&state.cache_dir().join("feed-media").join("documents"))
}

/// 对当前文章授予图片加载权限，并以 opaque 本地 lease 返回安全缓存结果。
#[tauri::command]
pub async fn feed_images_prepare(
    state: State<'_, Arc<AppState>>,
    item_id: String,
) -> AppResult<FeedImagesPrepareResult> {
    check_id(&item_id)?;
    let (markdown, article_url) = state
        .db
        .with_conn(|conn| FeedRepository::authorize_item_images(conn, &item_id, Utc::now()))?
        .ok_or_else(|| AppError::msg("feed_item_not_found"))?;
    Ok(crate::feed::image::prepare_images(
        &markdown,
        article_url.as_deref(),
        &state.cache_dir().join("feed-media").join("images"),
    )
    .await)
}

/// 授权当前文章的远程图片并返回可渐进请求的清单；不在此命令中下载任何文件。
#[tauri::command]
pub fn feed_images_authorize(
    state: State<'_, Arc<AppState>>,
    item_id: String,
) -> AppResult<FeedImageManifest> {
    check_id(&item_id)?;
    let (markdown, _) = state
        .db
        .with_conn(|conn| FeedRepository::authorize_item_images(conn, &item_id, Utc::now()))?
        .ok_or_else(|| AppError::msg("feed_item_not_found"))?;
    Ok(FeedImageManifest {
        images: crate::feed::image::extract_image_urls(&markdown)
            .into_iter()
            .enumerate()
            .map(|(index, source_url)| FeedImageSource {
                index: index as u32,
                source_url,
            })
            .collect(),
    })
}

/// 下载或恢复授权文章中的一张图片。`force_retry` 留给前端的显式重试操作。
#[tauri::command]
pub async fn feed_image_prepare(
    state: State<'_, Arc<AppState>>,
    item_id: String,
    index: u32,
    force_retry: Option<bool>,
) -> AppResult<FeedImageLease> {
    check_id(&item_id)?;
    let (markdown, article_url) = state
        .db
        .with_read_conn(|conn| FeedRepository::authorized_item_images(conn, &item_id))?
        .ok_or_else(|| AppError::msg("feed_image_not_authorized"))?;
    let urls = crate::feed::image::extract_image_urls(&markdown);
    let source_url = urls
        .get(index as usize)
        .cloned()
        .ok_or_else(|| AppError::msg("feed_image_not_found"))?;
    crate::feed::image::prepare_image(
        source_url,
        article_url.as_deref(),
        &state.cache_dir().join("feed-media").join("images"),
        force_retry.unwrap_or(false),
    )
    .await
}

/// 当前实现的下载任务按缓存键去重，取消阅读器排队不会中断其它文章可能共用的下载。
#[tauri::command]
pub fn feed_images_cancel(item_id: String) -> AppResult<()> {
    check_id(&item_id)
}

#[tauri::command]
pub fn feed_images_release(handles: Vec<String>) -> AppResult<()> {
    if handles.len() > 256 {
        return Err(AppError::msg("feed_validation_limit"));
    }
    for handle in &handles {
        check_id(handle)?;
    }
    crate::feed::image::release_images(&handles)
}

#[tauri::command]
pub fn feed_item_list(
    state: State<'_, Arc<AppState>>,
    query: FeedItemQuery,
) -> AppResult<Vec<FeedItemSummary>> {
    check_feed_query(&query)?;
    state
        .db
        .with_read_conn(|conn| FeedRepository::list_items(conn, &query, Utc::now()))
}

#[tauri::command]
pub fn feed_item_get(
    state: State<'_, Arc<AppState>>,
    item_id: String,
) -> AppResult<FeedItemDetail> {
    check_id(&item_id)?;
    let detail = state
        .db
        .with_read_conn(|conn| FeedRepository::get_item_detail(conn, &item_id))?;
    detail.ok_or_else(|| AppError::msg("feed_item_not_found"))
}

#[tauri::command]
pub fn feed_item_set_state(
    state: State<'_, Arc<AppState>>,
    item_id: String,
    patch: FeedItemStatePatch,
) -> AppResult<()> {
    feed_item_set_state_impl(&state, &item_id, &patch)
}

/// 将用户刚打开的一篇本地摘要加入正文补全队列；不会扫描来源历史。
#[tauri::command]
pub fn feed_fulltext_enqueue_item(
    state: State<'_, Arc<AppState>>,
    item_id: String,
) -> AppResult<FeedFulltextEnqueueOutcome> {
    check_id(&item_id)?;
    let queued = state
        .db
        .with_conn(|conn| FeedRepository::enqueue_item_fulltext(conn, &item_id, Utc::now()))?;
    if queued == FeedFulltextEnqueueOutcome::Queued {
        state.feed_fulltext.schedule();
    }
    Ok(queued)
}

#[tauri::command]
pub fn feed_items_mark_read(
    state: State<'_, Arc<AppState>>,
    query: FeedItemQuery,
) -> AppResult<u32> {
    check_feed_query(&query)?;
    let affected = state
        .db
        .with_conn(|conn| FeedRepository::mark_items_read(conn, &query, Utc::now()))?;
    Ok(affected as u32)
}

#[tauri::command]
pub async fn feed_sync_source(
    state: State<'_, Arc<AppState>>,
    source_id: String,
    mark_history_read: Option<bool>,
) -> AppResult<FeedSyncOutcomeDto> {
    feed_sync_source_impl(&state, &source_id, mark_history_read.unwrap_or(true)).await
}

#[tauri::command]
pub async fn feed_sync_all(state: State<'_, Arc<AppState>>) -> AppResult<FeedSyncBatchOutcome> {
    let outcome = state.feed_sync.sync_all().await?;
    state.feed_fulltext.schedule();
    Ok(outcome)
}

#[tauri::command]
pub async fn feed_sync_batch(
    state: State<'_, Arc<AppState>>,
    source_ids: Vec<String>,
    mark_history_read: Option<bool>,
) -> AppResult<FeedSyncBatchOutcome> {
    const MAX_BATCH_SOURCES: usize = 10_000;
    if source_ids.len() > MAX_BATCH_SOURCES {
        return Err(AppError::msg("feed_sync_batch_too_large"));
    }
    for source_id in &source_ids {
        check_id(source_id)?;
    }
    let history = if mark_history_read.unwrap_or(true) {
        HistoryReadPolicy::MarkRead
    } else {
        HistoryReadPolicy::LeaveUnread
    };
    let outcome = state.feed_sync.sync_batch(&source_ids, history).await;
    state.feed_fulltext.schedule();
    Ok(outcome)
}

#[tauri::command]
pub fn feed_opml_import(
    state: State<'_, Arc<AppState>>,
    xml: String,
    dry_run: Option<bool>,
) -> AppResult<OpmlImportResult> {
    // 输入经 IPC 传有界 UTF-8 字符串；命令不接收任意文件路径。
    if xml.len() > OPML_MAX_BYTES {
        return Err(AppError::msg("feed_opml_too_large"));
    }
    state.db.with_conn(|conn| {
        let interval = feed_default_interval(conn)?;
        import_opml_with_interval(conn, &xml, dry_run.unwrap_or(false), interval)
    })
}

#[tauri::command]
pub fn feed_opml_export(state: State<'_, Arc<AppState>>) -> AppResult<String> {
    state.db.with_read_conn(export_opml)
}

// ── 可测试的实现（命令薄壳只做 State 解包）─────────────────

fn feed_source_add_impl(
    state: &AppState,
    input: &FeedSourceAddInput,
) -> AppResult<FeedSourceSummary> {
    check_url(&input.url)?;
    let feed_url = canonicalize_https_url(&input.url)?;
    check_string(&input.title)?;
    if let Some(override_title) = &input.title_override {
        check_string(override_title)?;
    }
    if let Some(folder) = &input.folder_path {
        check_string(folder)?;
    }
    let source = state
        .db
        .with_conn(|conn| {
            if let Some(deleted_id) =
                FeedRepository::get_deleted_source_by_feed_url(conn, &feed_url)?
            {
                if input.restore_deleted != Some(true) {
                    return Err(AppError::msg("feed_source_restore_required"));
                }
                FeedRepository::restore_source(conn, &deleted_id, Utc::now())?;
                FeedRepository::update_source(
                    conn,
                    &deleted_id,
                    &FeedSourcePatch {
                        is_enabled: Some(true),
                        ..Default::default()
                    },
                    Utc::now(),
                )?;
                return FeedRepository::get_source(conn, &deleted_id)?
                    .ok_or_else(|| AppError::msg("feed_source_restore_failed"));
            }
            FeedRepository::create_source(
                conn,
                &NewFeedSource {
                    id: uuid::Uuid::new_v4().to_string(),
                    feed_url,
                    site_url: None,
                    title: input.title.trim().to_string(),
                    title_override: input.title_override.clone(),
                    description: None,
                    icon_url: None,
                    language: None,
                    folder_path: input.folder_path.clone().unwrap_or_default(),
                    fetch_interval_minutes: input
                        .fetch_interval_minutes
                        .unwrap_or(feed_default_interval(conn)?),
                },
                Utc::now(),
            )
        })?
        .id;
    let summary = state
        .db
        .with_read_conn(FeedRepository::list_sources)?
        .into_iter()
        .find(|item| item.id == source)
        .ok_or_else(|| AppError::msg("feed_source_readback"))?;
    Ok(summary)
}

fn feed_default_interval(conn: &rusqlite::Connection) -> AppResult<i64> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'feed_default_fetch_interval_minutes'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(value
        .and_then(|raw| serde_json::from_str::<i64>(&raw).ok())
        .filter(|minutes| (15..=10_080).contains(minutes))
        .unwrap_or(60))
}

fn feed_source_update_impl(
    state: &AppState,
    source_id: &str,
    patch: &FeedSourceUpdateInput,
) -> AppResult<()> {
    check_id(source_id)?;
    if let Some(Some(title)) = &patch.title_override {
        check_string(title)?;
    }
    if let Some(folder) = &patch.folder_path {
        check_string(folder)?;
    }
    let model_patch = FeedSourcePatch {
        title_override: patch.title_override.clone().flatten(),
        clear_title_override: matches!(patch.title_override, Some(None)),
        folder_path: patch.folder_path.clone(),
        fetch_interval_minutes: patch.fetch_interval_minutes,
        is_enabled: patch.is_enabled,
        fulltext_enabled: patch.fulltext_enabled,
    };
    let updated = state.db.with_conn(|conn| {
        FeedRepository::update_source(conn, source_id, &model_patch, Utc::now())
    })?;
    if !updated {
        return Err(AppError::msg("feed_source_not_found"));
    }
    Ok(())
}

fn feed_source_remove_impl(state: &AppState, source_id: &str, keep_items: bool) -> AppResult<u32> {
    check_id(source_id)?;
    if keep_items {
        // 保留已下载文章并暂停：仅置 disabled，不删除任何数据。
        let updated = state.db.with_conn(|conn| {
            FeedRepository::update_source(
                conn,
                source_id,
                &FeedSourcePatch {
                    is_enabled: Some(false),
                    ..Default::default()
                },
                Utc::now(),
            )
        })?;
        if !updated {
            return Err(AppError::msg("feed_source_not_found"));
        }
        Ok(0)
    } else {
        // 兼容旧 IPC，但删除语义已经收敛为可恢复的 RSS 回收站。
        feed_source_trash_impl(state, source_id)
    }
}

fn feed_source_trash_impl(state: &AppState, source_id: &str) -> AppResult<u32> {
    check_id(source_id)?;
    let document_urls = state
        .db
        .with_read_conn(|conn| FeedRepository::source_primary_document_urls(conn, source_id))?;
    let moved = state
        .db
        .with_conn(|conn| FeedRepository::trash_source(conn, source_id, Utc::now()))?
        .map(|count| count as u32)
        .ok_or_else(|| AppError::msg("feed_source_not_found"))?;
    cleanup_document_urls(state, document_urls)?;
    Ok(moved)
}

fn cleanup_source_document_cache(state: &AppState, source_id: &str) -> AppResult<()> {
    let urls = state
        .db
        .with_read_conn(|conn| FeedRepository::source_primary_document_urls(conn, source_id))?;
    cleanup_document_urls(state, urls)
}

fn cleanup_document_urls(state: &AppState, urls: Vec<String>) -> AppResult<()> {
    let cache_dir = state.cache_dir().join("feed-media").join("documents");
    for url in urls {
        let still_referenced = state.db.with_read_conn(|conn| {
            FeedRepository::active_primary_document_reference_count(conn, &url)
        })?;
        if still_referenced == 0 {
            let _ = crate::feed::document::remove_cached_url(&cache_dir, &url);
        }
    }
    Ok(())
}

fn feed_item_set_state_impl(
    state: &AppState,
    item_id: &str,
    patch: &FeedItemStatePatch,
) -> AppResult<()> {
    check_id(item_id)?;
    if patch.is_empty() {
        return Err(AppError::msg("feed_item_state_patch_empty"));
    }
    let changed = state
        .db
        .with_conn(|conn| FeedRepository::set_item_state(conn, item_id, patch, Utc::now()))?;
    if !changed {
        return Err(AppError::msg("feed_item_not_found"));
    }
    Ok(())
}

async fn feed_sync_source_impl(
    state: &AppState,
    source_id: &str,
    mark_history_read: bool,
) -> AppResult<FeedSyncOutcomeDto> {
    check_id(source_id)?;
    let history = if mark_history_read {
        HistoryReadPolicy::MarkRead
    } else {
        HistoryReadPolicy::LeaveUnread
    };
    let outcome = state
        .feed_sync
        .sync_source_with_history(source_id, SyncMode::Manual, history)
        .await?;
    state.feed_fulltext.schedule();
    Ok(FeedSyncOutcomeDto::from_status(&outcome.status))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::model::{FeedItemInput, FulltextStatus};
    use crate::feed::normalize::normalize_feed;
    use crate::feed::opml::import_opml;

    fn test_state() -> Arc<AppState> {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        // AppState 生命周期长于本测试；遗忘守卫以保持数据库目录存活，
        // 测试进程结束时由系统临时目录清理。
        std::mem::forget(dir);
        AppState::new_with_test_cas_key(path, [7u8; 32]).expect("app state")
    }

    fn seed_items(state: &AppState, source_id: &str, count: usize) {
        let xml = include_str!("../../tests/fixtures/feeds/rss2-basic.xml");
        let feed = normalize_feed(xml.as_bytes(), source_id).expect("normalize");
        let items: Vec<FeedItemInput> = feed
            .items
            .iter()
            .take(count)
            .map(|item| FeedItemInput {
                id: uuid::Uuid::new_v4().to_string(),
                source_id: source_id.to_string(),
                external_key: item.external_key.clone(),
                canonical_url: item.canonical_url.clone(),
                title: item.title.clone(),
                author_name: item.author_name.clone(),
                published_at: item.published_at.clone(),
                source_updated_at: item.source_updated_at.clone(),
                received_at: "2026-08-01T08:00:00Z".to_string(),
                summary_markdown: item.summary_markdown.clone(),
                content_markdown: item.content_markdown.clone(),
                content_text: item.content_text.clone(),
                source_payload: item.source_payload.clone(),
                source_payload_kind: item.source_payload_kind,
                content_hash: item.content_hash.clone(),
                conversion_version: 1,
                conversion_status: item.conversion_status,
                expires_at: "2026-08-08T08:00:00Z".to_string(),
                fulltext_status: FulltextStatus::NotRequested,
            })
            .collect();
        state
            .db
            .with_conn(|conn| FeedRepository::upsert_items(conn, &items))
            .expect("seed items");
    }

    fn add_source(state: &AppState, url: &str) -> String {
        let summary = feed_source_add_impl(
            state,
            &FeedSourceAddInput {
                url: url.to_string(),
                title: "Example Feed".to_string(),
                title_override: None,
                folder_path: Some("tech".to_string()),
                fetch_interval_minutes: Some(60),
                restore_deleted: None,
            },
        )
        .expect("add source");
        summary.id
    }

    #[test]
    fn source_add_list_update_remove_roundtrip() {
        let state = test_state();
        let id = add_source(&state, "https://example.com/feed.xml");

        let list = state
            .db
            .with_read_conn(FeedRepository::list_sources)
            .expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Example Feed");
        assert_eq!(list[0].folder_path, "tech");

        feed_source_update_impl(
            &state,
            &id,
            &FeedSourceUpdateInput {
                title_override: Some(Some("Renamed".to_string())),
                folder_path: None,
                fetch_interval_minutes: Some(120),
                is_enabled: Some(false),
                fulltext_enabled: None,
            },
        )
        .expect("update");
        let updated = state
            .db
            .with_read_conn(|conn| FeedRepository::get_source(conn, &id))
            .expect("get")
            .expect("exists");
        assert_eq!(updated.title_override.as_deref(), Some("Renamed"));
        assert!(!updated.is_enabled);

        // 清除覆盖标题：显式 null。
        feed_source_update_impl(
            &state,
            &id,
            &FeedSourceUpdateInput {
                title_override: Some(None),
                ..Default::default()
            },
        )
        .expect("clear override");
        let cleared = state
            .db
            .with_read_conn(|conn| FeedRepository::get_source(conn, &id))
            .expect("get")
            .expect("exists");
        assert_eq!(cleared.title_override, None);

        let removed = feed_source_remove_impl(&state, &id, false).expect("remove");
        assert_eq!(removed, 0);
        assert!(state
            .db
            .with_read_conn(|conn| FeedRepository::get_source(conn, &id))
            .expect("get")
            .is_none());
        let trashed: bool = state
            .db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT deleted_at IS NOT NULL FROM feed_sources WHERE id = ?1",
                    [&id],
                    |row| row.get(0),
                )?)
            })
            .expect("trash state");
        assert!(trashed, "旧删除命令也只能移入 RSS 回收站");
    }

    #[test]
    fn remove_keep_items_only_disables_source() {
        let state = test_state();
        let id = add_source(&state, "https://example.com/feed.xml");
        seed_items(&state, &id, 3);

        let removed = feed_source_remove_impl(&state, &id, true).expect("keep items");
        assert_eq!(removed, 0, "保留文章不删除");
        let source = state
            .db
            .with_read_conn(|conn| FeedRepository::get_source(conn, &id))
            .expect("get")
            .expect("exists");
        assert!(!source.is_enabled, "保留文章 = 置 disabled");
        let count = state
            .db
            .with_read_conn(|conn| FeedRepository::count_items(conn, &id))
            .expect("count");
        assert_eq!(count, 3, "文章保留");

        // 旧删除路径返回移入回收站的文章数，不允许级联硬删除。
        let removed = feed_source_remove_impl(&state, &id, false).expect("delete");
        assert_eq!(removed, 3);
        let retained: i64 = state
            .db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM feed_items WHERE source_id = ?1",
                    [&id],
                    |row| row.get(0),
                )?)
            })
            .expect("retained trash items");
        assert_eq!(retained, 3);
    }

    #[test]
    fn item_list_get_set_state_mark_read_via_commands() {
        let state = test_state();
        let id = add_source(&state, "https://example.com/feed.xml");
        seed_items(&state, &id, 3);

        let items = state
            .db
            .with_read_conn(|conn| {
                FeedRepository::list_items(
                    conn,
                    &FeedItemQuery {
                        view: crate::feed::model::FeedView::All,
                        source_id: None,
                        search: None,
                        received_after: None,
                        cursor: None,
                        limit: 50,
                    },
                    Utc::now(),
                )
            })
            .expect("list");
        assert_eq!(items.len(), 3);

        let detail = state
            .db
            .with_read_conn(|conn| FeedRepository::get_item_detail(conn, &items[0].id))
            .expect("detail")
            .expect("exists");
        assert!(!detail.content_markdown.is_empty());

        // 空 patch 被验证拒绝。
        let empty = feed_item_set_state_impl(&state, &items[0].id, &FeedItemStatePatch::default());
        assert!(empty.is_err(), "空 patch 必须拒绝");

        feed_item_set_state_impl(
            &state,
            &items[0].id,
            &FeedItemStatePatch {
                is_read: Some(true),
                ..Default::default()
            },
        )
        .expect("mark read");

        let affected = state
            .db
            .with_conn(|conn| {
                FeedRepository::mark_items_read(
                    conn,
                    &FeedItemQuery {
                        view: crate::feed::model::FeedView::Inbox,
                        source_id: None,
                        search: None,
                        received_after: None,
                        cursor: None,
                        limit: 50,
                    },
                    Utc::now(),
                )
            })
            .expect("mark read all");
        assert_eq!(affected, 2, "收件箱剩余 2 条未读");

        // 搜索命中正文。
        let hits = state
            .db
            .with_read_conn(|conn| FeedRepository::search(conn, "fixture", None, 50))
            .expect("search");
        assert_eq!(hits.len(), 3);
    }

    #[test]
    fn source_item_count_reports_article_count() {
        let state = test_state();
        let id = add_source(&state, "https://example.com/feed.xml");
        seed_items(&state, &id, 3);
        let count = state
            .db
            .with_read_conn(|conn| FeedRepository::count_items(conn, &id))
            .expect("count");
        assert_eq!(count, 3);
        // 不存在的源：0（或由命令层校验拒绝）。
        assert!(check_id("missing-id").is_ok());
    }

    #[test]
    fn validation_bounds_reject_bad_inputs() {
        let state = test_state();
        assert!(check_id("").is_err());
        assert!(check_id(&"x".repeat(201)).is_err());
        assert!(check_url("http://example.com/").is_err(), "非 HTTPS 拒绝");
        assert!(check_url("https://192.168.0.1/").is_err(), "私网拒绝");
        assert!(check_url(&format!("https://example.com/{}", "a".repeat(3000))).is_err());

        // 超长标题。
        let long_title = "x".repeat(MAX_STRING_LEN + 1);
        let error = feed_source_add_impl(
            &state,
            &FeedSourceAddInput {
                url: "https://example.com/feed.xml".to_string(),
                title: long_title,
                title_override: None,
                folder_path: None,
                fetch_interval_minutes: None,
                restore_deleted: None,
            },
        )
        .expect_err("超长标题必须拒绝");
        assert!(error.to_string().contains("feed_validation_string"));

        assert!(check_feed_query(&FeedItemQuery {
            view: crate::feed::model::FeedView::All,
            source_id: None,
            search: Some("x".repeat(MAX_STRING_LEN + 1)),
            received_after: None,
            cursor: None,
            limit: 50,
        })
        .is_err());
        assert!(check_feed_query(&FeedItemQuery {
            view: crate::feed::model::FeedView::All,
            source_id: None,
            search: None,
            received_after: Some("not-a-time".to_string()),
            cursor: None,
            limit: 50,
        })
        .is_err());
        assert!(check_feed_query(&FeedItemQuery {
            view: crate::feed::model::FeedView::All,
            source_id: None,
            search: None,
            received_after: None,
            cursor: Some(crate::feed::model::FeedPageCursor {
                sort_at: "2026-08-01T08:00:00Z".to_string(),
                row_id: 0,
            }),
            limit: 50,
        })
        .is_err());
        assert!(check_feed_query(&FeedItemQuery {
            view: crate::feed::model::FeedView::All,
            source_id: None,
            search: None,
            received_after: None,
            cursor: None,
            limit: 0,
        })
        .is_err());
        assert!(check_feed_query(&FeedItemQuery {
            view: crate::feed::model::FeedView::All,
            source_id: None,
            search: None,
            received_after: None,
            cursor: None,
            limit: 201,
        })
        .is_err());
    }

    #[test]
    fn source_add_canonicalizes_semantically_equivalent_url() {
        let state = test_state();
        let source = feed_source_add_impl(
            &state,
            &FeedSourceAddInput {
                url: "https://EXAMPLE.com:443/feed.xml#fragment".to_string(),
                title: "Example".to_string(),
                title_override: None,
                folder_path: None,
                fetch_interval_minutes: None,
                restore_deleted: None,
            },
        )
        .expect("add");
        assert_eq!(source.feed_url, "https://example.com/feed.xml");
    }

    #[test]
    fn readding_a_trashed_url_requires_explicit_restore_confirmation() {
        let state = test_state();
        let id = add_source(&state, "https://example.com/restore.xml");
        feed_source_trash_impl(&state, &id).expect("trash");

        let input = FeedSourceAddInput {
            url: "https://example.com/restore.xml".to_string(),
            title: "Restored".to_string(),
            title_override: None,
            folder_path: None,
            fetch_interval_minutes: None,
            restore_deleted: None,
        };
        let error = feed_source_add_impl(&state, &input).expect_err("confirmation required");
        assert!(error.to_string().contains("feed_source_restore_required"));

        let restored = feed_source_add_impl(
            &state,
            &FeedSourceAddInput {
                restore_deleted: Some(true),
                ..input
            },
        )
        .expect("explicit restore");
        assert_eq!(restored.id, id);
        assert!(restored.is_enabled, "explicit re-subscribe enables sync");
    }

    #[test]
    fn sync_commands_surface_stable_outcomes() {
        let state = test_state();
        // 源不存在：硬错误。
        let missing =
            crate::commands::feed_commands::feed_sync_source_impl(&state, "missing", true);
        let error = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(missing)
            .expect_err("missing source");
        assert!(error.to_string().contains("feed_source_not_found"));

        // feed_sync_all 空库直接成功。
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(state.feed_sync.sync_all());
        assert!(result.is_ok());
    }

    #[test]
    fn sync_outcome_dto_maps_all_statuses() {
        let cases = [
            (
                SyncStatus::Succeeded {
                    new_items: 3,
                    skipped_history: 0,
                },
                "succeeded",
                3,
                None,
            ),
            (
                SyncStatus::Succeeded {
                    new_items: 0,
                    skipped_history: 0,
                },
                "succeeded",
                0,
                None,
            ),
            (SyncStatus::NotModified, "not_modified", 0, None),
            (SyncStatus::Skipped, "skipped", 0, None),
            (SyncStatus::InFlight, "in_flight", 0, None),
            (
                SyncStatus::Failed {
                    code: "feed_http_error_500".to_string(),
                },
                "failed",
                0,
                Some("feed_http_error_500".to_string()),
            ),
        ];
        for (status, expected_status, expected_items, expected_code) in cases {
            let dto = FeedSyncOutcomeDto::from_status(&status);
            assert_eq!(dto.status, expected_status);
            assert_eq!(dto.new_items, expected_items);
            assert_eq!(dto.error_code, expected_code);
        }
    }

    // 提示：feed_discover 的完整行为由 discovery_tests 覆盖；这里只验证
    // 命令层 URL 校验（非 HTTPS 在发起请求前拒绝）。
    #[tokio::test]
    async fn feed_discover_rejects_non_https_before_network() {
        let error = feed_discover("http://example.com/feed.xml".to_string())
            .await
            .expect_err("http must be rejected");
        assert!(error.to_string().contains("https_url_invalid"));
    }

    #[test]
    fn opml_commands_import_export_roundtrip_via_state() {
        let state = test_state();
        let xml = include_str!("../../tests/fixtures/opml/nested.opml");

        let preview = feed_opml_import_impl(&state, xml, true).expect("dry run preview");
        assert_eq!(preview.added, 3);
        assert!(
            state
                .db
                .with_read_conn(FeedRepository::list_sources)
                .expect("list")
                .is_empty(),
            "dry run 不写库"
        );

        let executed = feed_opml_import_impl(&state, xml, false).expect("import");
        assert_eq!(executed.added, 3);
        assert_eq!(executed.added_ids.len(), 3);
        assert!(executed.added_ids.iter().all(|id| check_id(id).is_ok()));

        let exported = feed_opml_export_impl(&state).expect("export");
        assert!(exported.contains("xmlUrl=\"https://example.com/feeds/rust.xml\""));
        assert!(exported.contains("text=\"技术\""));

        // 重复导入幂等。
        let again = feed_opml_import_impl(&state, xml, false).expect("again");
        assert_eq!(again.added, 0);
        assert_eq!(again.updated, 0);
    }

    #[test]
    fn opml_import_rejects_oversized_and_malformed_input() {
        let state = test_state();
        let oversized = "x".repeat(OPML_MAX_BYTES + 1);
        let error = feed_opml_import_impl(&state, &oversized, false).expect_err("超限必须拒绝");
        assert!(error.to_string().contains("feed_opml_too_large"));

        let xxe = r#"<!DOCTYPE opml [<!ENTITY xxe SYSTEM "file:///etc/hosts">]>
<opml version="2.0"><body><outline text="x" xmlUrl="https://example.com/f.xml"/></body></opml>"#;
        let error = feed_opml_import_impl(&state, xxe, false).expect_err("XXE 拒绝");
        assert!(error.to_string().contains("feed_xml_unsafe_declaration"));

        // 非法 XML 结构：稳定错误码，无正文泄漏。
        let error =
            feed_opml_import_impl(&state, "<opml><body>", false).expect_err("畸形 XML 拒绝");
        assert!(error.to_string().contains("feed_opml_parse_failed"));
    }

    /// 命令层可测试入口（薄壳等价物）。
    fn feed_opml_import_impl(
        state: &AppState,
        xml: &str,
        dry_run: bool,
    ) -> AppResult<OpmlImportResult> {
        if xml.len() > OPML_MAX_BYTES {
            return Err(AppError::msg("feed_opml_too_large"));
        }
        state.db.with_conn(|conn| import_opml(conn, xml, dry_run))
    }

    fn feed_opml_export_impl(state: &AppState) -> AppResult<String> {
        state.db.with_read_conn(export_opml)
    }
}
