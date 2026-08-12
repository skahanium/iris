//! 订阅资料库 IPC 命令（阶段 3：冻结契约）。
//!
//! 命令只做验证/授权边界与仓储/service 调用，不内嵌 SQL；所有 DTO 与
//! `src/types/ipc.ts` / `src/lib/ipc.ts` 保持 camelCase 一致；`source_payload`
//! 永不进入任何命令参数或返回值。

use std::sync::Arc;

use chrono::Utc;
use serde::Deserialize;
use tauri::State;

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::feed::discovery::{discover, FeedCandidate};
use crate::feed::fetch::ProdNetGate;
use crate::feed::model::{
    FeedItemDetail, FeedItemQuery, FeedItemStatePatch, FeedItemSummary, FeedSourcePatch,
    FeedSourceSummary, NewFeedSource,
};
use crate::feed::repository::FeedRepository;
use crate::feed::sync::{HistoryReadPolicy, SyncMode, SyncStatus};
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
}

/// 编辑订阅源：`titleOverride: null` 清除覆盖标题，缺省字段不改动。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FeedSourceUpdateInput {
    pub title_override: Option<Option<String>>,
    pub folder_path: Option<String>,
    pub fetch_interval_minutes: Option<i64>,
    pub is_enabled: Option<bool>,
}

/// 同步结果 DTO（status 为稳定字符串，事件另发 `feed:changed`）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedSyncOutcomeDto {
    pub status: String,
    pub new_items: u32,
    pub error_code: Option<String>,
}

impl FeedSyncOutcomeDto {
    fn from_status(status: &SyncStatus) -> Self {
        match status {
            SyncStatus::Succeeded { new_items } => Self {
                status: "succeeded".to_string(),
                new_items: *new_items as u32,
                error_code: None,
            },
            SyncStatus::NotModified => Self {
                status: "not_modified".to_string(),
                new_items: 0,
                error_code: None,
            },
            SyncStatus::Skipped => Self {
                status: "skipped".to_string(),
                new_items: 0,
                error_code: None,
            },
            SyncStatus::InFlight => Self {
                status: "in_flight".to_string(),
                new_items: 0,
                error_code: None,
            },
            SyncStatus::Failed { code } => Self {
                status: "failed".to_string(),
                new_items: 0,
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
pub fn feed_item_list(
    state: State<'_, Arc<AppState>>,
    query: FeedItemQuery,
) -> AppResult<Vec<FeedItemSummary>> {
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

#[tauri::command]
pub fn feed_items_mark_read(
    state: State<'_, Arc<AppState>>,
    query: FeedItemQuery,
) -> AppResult<u32> {
    let affected = state
        .db
        .with_conn(|conn| FeedRepository::mark_items_read(conn, &query, Utc::now()))?;
    Ok(affected as u32)
}

#[tauri::command]
pub fn feed_search(
    state: State<'_, Arc<AppState>>,
    query: String,
    source_id: Option<String>,
    limit: Option<u32>,
) -> AppResult<Vec<FeedItemSummary>> {
    if let Some(id) = &source_id {
        check_id(id)?;
    }
    state.db.with_read_conn(|conn| {
        FeedRepository::search(conn, &query, source_id.as_deref(), limit.unwrap_or(50))
    })
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
pub async fn feed_sync_all(state: State<'_, Arc<AppState>>) -> AppResult<()> {
    state.feed_sync.sync_all().await
}

// ── 可测试的实现（命令薄壳只做 State 解包）─────────────────

fn feed_source_add_impl(
    state: &AppState,
    input: &FeedSourceAddInput,
) -> AppResult<FeedSourceSummary> {
    check_url(&input.url)?;
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
            FeedRepository::create_source(
                conn,
                &NewFeedSource {
                    id: uuid::Uuid::new_v4().to_string(),
                    feed_url: input.url.trim().to_string(),
                    site_url: None,
                    title: input.title.trim().to_string(),
                    title_override: input.title_override.clone(),
                    description: None,
                    icon_url: None,
                    language: None,
                    folder_path: input.folder_path.clone().unwrap_or_default(),
                    fetch_interval_minutes: input.fetch_interval_minutes.unwrap_or(60),
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
        // 删除订阅及其文章（cascade + FTS 清理）。
        let removed = state
            .db
            .with_conn(|conn| FeedRepository::delete_source(conn, source_id))?;
        removed
            .map(|count| count as u32)
            .ok_or_else(|| AppError::msg("feed_source_not_found"))
    }
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
    Ok(FeedSyncOutcomeDto::from_status(&outcome.status))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::model::FeedItemInput;
    use crate::feed::normalize::normalize_feed;

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

        // 删除路径返回文章数并级联清理。
        let removed = feed_source_remove_impl(&state, &id, false).expect("delete");
        assert_eq!(removed, 3);
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
            },
        )
        .expect_err("超长标题必须拒绝");
        assert!(error.to_string().contains("feed_validation_string"));
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
            (SyncStatus::Succeeded { new_items: 3 }, "succeeded", 3, None),
            (SyncStatus::Succeeded { new_items: 0 }, "succeeded", 0, None),
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
        assert!(error.to_string().contains("仅允许 HTTPS"));
    }
}
