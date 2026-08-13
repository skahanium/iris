//! 单源同步事务：读取配置 → 获取/解析（SQLite 连接外）→ 短事务 upsert。
//!
//! 成功/304 清零失败计数并推进 `next_fetch_at`；失败只更新诊断列并保留
//! 旧 validators 与文章；退避固定 15m/1h/6h/24h，无随机抖动。同步失败是
//! 预期事件（以稳定错误码返回），不是崩溃路径。
//!

use chrono::{Duration as ChronoDuration, SecondsFormat, Utc};
use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::feed::fetch::{FeedHttpClient, FeedNetGate, FetchPurpose};
use crate::feed::model::{
    FeedItemInput, FeedSource, FeedSourceSyncState, FulltextStatus, UpsertSummary,
};
use crate::feed::normalize::{normalize_feed, FEED_CONVERSION_VERSION};
use crate::feed::repository::FeedRepository;
use crate::storage::db::Database;

/// 同步触发方式：自动（跳过暂停源）与手动（可刷新暂停源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyncMode {
    Manual,
    Automatic,
}

/// 首次同步历史项目策略：默认已读；可显式选择未读。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryReadPolicy {
    MarkRead,
    LeaveUnread,
}

/// 首次同步只保留最新历史，避免单个完整归档 Feed 无界占用本地空间。
pub(crate) const INITIAL_HISTORY_LIMIT: usize = 50;

/// 同步结果状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SyncStatus {
    Succeeded {
        new_items: usize,
        skipped_history: usize,
    },
    NotModified,
    Skipped,
    /// 同一 source 已在同步中（互斥标记拒绝重复）。
    InFlight,
    Failed {
        code: String,
    },
}

/// 同步结果（失败以稳定错误码表达，不抛错）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyncOutcome {
    pub status: SyncStatus,
}

/// 用户触发的批量同步汇总，不暴露 URL 或正文。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FeedSyncBatchOutcome {
    pub total: u32,
    pub succeeded: u32,
    pub failed: u32,
    pub skipped: u32,
    pub in_flight: u32,
    pub new_items: u32,
    pub skipped_history: u32,
}

/// 同步事件投影（IPC）：只含 sourceId、变更类型、计数与稳定错误码，
/// 不含 URL、正文或任何请求头；只提示前端重新查询，不建立 job 恢复协议。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FeedChangedEvent {
    pub source_id: String,
    pub kind: &'static str,
    pub new_items: u32,
    pub error_code: Option<String>,
}

impl FeedChangedEvent {
    fn from_outcome(source_id: &str, status: &SyncStatus) -> Option<Self> {
        match status {
            SyncStatus::Succeeded { new_items, .. } => Some(Self {
                source_id: source_id.to_string(),
                kind: if *new_items > 0 {
                    "items_changed"
                } else {
                    "sync_succeeded"
                },
                new_items: *new_items as u32,
                error_code: None,
            }),
            SyncStatus::NotModified => Some(Self {
                source_id: source_id.to_string(),
                kind: "sync_succeeded",
                new_items: 0,
                error_code: None,
            }),
            SyncStatus::Failed { code } => Some(Self {
                source_id: source_id.to_string(),
                kind: "sync_failed",
                new_items: 0,
                error_code: Some(code.clone()),
            }),
            SyncStatus::Skipped | SyncStatus::InFlight => None,
        }
    }
}

/// 固定退避：0→15m、1→1h、2→6h、3+→24h（以失败前计数为准）。
fn backoff_minutes(consecutive_failures: i64) -> i64 {
    match consecutive_failures {
        0 => 15,
        1 => 60,
        2 => 360,
        _ => 1440,
    }
}

fn rfc3339(value: chrono::DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// 从 AppError 提取稳定错误码（feed_* 错误码即消息本体）。
fn stable_error_code(error: &AppError) -> String {
    match error {
        AppError::Message(message) => message.clone(),
        _ => "feed_sync_failed".to_string(),
    }
}

/// 同步单个订阅源；获取/解析在 SQLite 连接外执行，最后用短事务 upsert。
pub(crate) async fn sync_source<G: FeedNetGate>(
    db: &Database,
    gate: &G,
    source_id: &str,
    mode: SyncMode,
    history: HistoryReadPolicy,
) -> AppResult<SyncOutcome> {
    // 1. 读取配置（连接外决策的基础）。
    let source = db
        .with_read_conn(|conn| FeedRepository::get_source(conn, source_id))?
        .ok_or_else(|| AppError::msg("feed_source_not_found"))?;
    if matches!(mode, SyncMode::Automatic) && !source.is_enabled {
        return Ok(SyncOutcome {
            status: SyncStatus::Skipped,
        });
    }

    let now = Utc::now();
    let now_str = rfc3339(now);

    // 2. 获取（连接外；逐跳校验/固定/有界；带条件请求 validators）。
    let fetch_result = FeedHttpClient
        .fetch(
            gate,
            &source.feed_url,
            FetchPurpose::Feed,
            source.etag.as_deref(),
            source.last_modified.as_deref(),
            Some(source_id),
        )
        .await;

    match fetch_result {
        // 304：同样视为成功——清零失败，吸收新 validators，推进下次同步。
        Ok(result) if result.status == 304 => {
            let state = success_state(&source, &result.etag, &result.last_modified, &now_str, now);
            db.with_conn(|conn| FeedRepository::update_source_sync_state(conn, source_id, &state))?;
            Ok(SyncOutcome {
                status: SyncStatus::NotModified,
            })
        }

        // 200：解析（连接外）→ 短事务 upsert。
        Ok(result) if result.status == 200 => {
            let normalized = match normalize_feed(&result.bytes, source_id) {
                Ok(feed) => feed,
                Err(error) => {
                    record_failure(db, &source, source_id, &error, now)?;
                    return Ok(SyncOutcome {
                        status: SyncStatus::Failed {
                            code: stable_error_code(&error),
                        },
                    });
                }
            };
            let metadata = (
                normalized.title.clone(),
                normalized.site_url.clone(),
                normalized.description.clone(),
                normalized.language.clone(),
            );
            let all_items = normalized.items;
            let is_first_sync =
                db.with_read_conn(|conn| FeedRepository::count_items(conn, source_id))? == 0;
            let skipped_history = if is_first_sync {
                all_items.len().saturating_sub(INITIAL_HISTORY_LIMIT)
            } else {
                0
            };
            let retained = select_sync_items(&source, all_items, is_first_sync);
            let items: Vec<FeedItemInput> = retained
                .iter()
                .map(|item| FeedItemInput {
                    id: uuid::Uuid::new_v4().to_string(),
                    source_id: source_id.to_string(),
                    external_key: item.external_key.clone(),
                    canonical_url: item.canonical_url.clone(),
                    title: item.title.clone(),
                    author_name: item.author_name.clone(),
                    published_at: item.published_at.clone(),
                    source_updated_at: item.source_updated_at.clone(),
                    received_at: now_str.clone(),
                    summary_markdown: item.summary_markdown.clone(),
                    content_markdown: item.content_markdown.clone(),
                    content_text: item.content_text.clone(),
                    source_payload: item.source_payload.clone(),
                    source_payload_kind: item.source_payload_kind,
                    content_hash: item.content_hash.clone(),
                    conversion_version: FEED_CONVERSION_VERSION,
                    conversion_status: item.conversion_status,
                    expires_at: rfc3339(now + ChronoDuration::days(7)),
                    fulltext_status: if source.fulltext_enabled
                        && item.is_summary_only
                        && item.canonical_url.is_some()
                    {
                        FulltextStatus::Pending
                    } else {
                        FulltextStatus::NotRequested
                    },
                })
                .collect();
            let state = success_state(&source, &result.etag, &result.last_modified, &now_str, now);

            let summary = db.with_conn(|conn| {
                persist_success(conn, &source, &items, history, &state, &metadata)
            })?;
            Ok(SyncOutcome {
                status: SyncStatus::Succeeded {
                    new_items: summary.inserted,
                    skipped_history,
                },
            })
        }

        // 其他 HTTP 状态（防御分支）：记录稳定错误码。
        Ok(result) => {
            let code = format!("feed_http_error_{}", result.status);
            let error = AppError::msg(code.clone());
            record_failure(db, &source, source_id, &error, now)?;
            Ok(SyncOutcome {
                status: SyncStatus::Failed { code },
            })
        }

        // 传输/超时/超限等：记录稳定错误码。
        Err(error) => {
            record_failure(db, &source, source_id, &error, now)?;
            Ok(SyncOutcome {
                status: SyncStatus::Failed {
                    code: stable_error_code(&error),
                },
            })
        }
    }
}

/// 首次同步按发布时间降序只保留最近历史；后续同步完整处理当前 Feed，
/// 因为唯一键会仅写入新增条目。无发布时间时保留 Feed 前序，符合 RSS 最新在前惯例。
fn select_sync_items(
    source: &FeedSource,
    mut items: Vec<crate::feed::normalize::NormalizedItem>,
    is_first_sync: bool,
) -> Vec<crate::feed::normalize::NormalizedItem> {
    if !is_first_sync {
        if let Some(boundary) = source.history_boundary_published_at.as_deref() {
            // 发布时间比 Feed 输出顺序更可靠：一些源会把新条目追加在末尾。
            // 对无发布时间的新条目，使用同一 Feed 中的历史边界位置作为安全
            // 回退：只保留边界之前（通常是最新端）的条目。这样不会丢掉新
            // 的无日期文章，也不会把边界之后的旧无日期归档重新灌入资料库。
            let boundary_position =
                source
                    .history_boundary_external_key
                    .as_deref()
                    .and_then(|boundary_key| {
                        items
                            .iter()
                            .position(|item| item.external_key == boundary_key)
                    });
            items = items
                .into_iter()
                .enumerate()
                .filter_map(|(position, item)| {
                    let retain = item
                        .published_at
                        .as_deref()
                        .is_some_and(|published| published >= boundary)
                        || (item.published_at.is_none()
                            && boundary_position.is_some_and(|edge| position <= edge));
                    retain.then_some(item)
                })
                .collect();
        } else if let Some(boundary_key) = source.history_boundary_external_key.as_deref() {
            // 无发布时间时只能遵循 Feed 前序约定，在首次历史边界处截断。
            if let Some(index) = items
                .iter()
                .position(|item| item.external_key == boundary_key)
            {
                items.truncate(index + 1);
            }
        }
        return items;
    }
    if items.len() <= INITIAL_HISTORY_LIMIT {
        return items;
    }
    items.sort_by(
        |left, right| match (&left.published_at, &right.published_at) {
            (Some(a), Some(b)) => b.cmp(a),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        },
    );
    items.truncate(INITIAL_HISTORY_LIMIT);
    items
}

/// 成功/304 的同步状态：清零失败、记录成功时间、推进下次同步。
fn success_state(
    source: &FeedSource,
    response_etag: &Option<String>,
    response_last_modified: &Option<String>,
    now_str: &str,
    now: chrono::DateTime<Utc>,
) -> FeedSourceSyncState {
    FeedSourceSyncState {
        etag: response_etag.clone().or_else(|| source.etag.clone()),
        last_modified: response_last_modified
            .clone()
            .or_else(|| source.last_modified.clone()),
        last_checked_at: now_str.to_string(),
        last_success_at: Some(now_str.to_string()),
        next_fetch_at: rfc3339(now + ChronoDuration::minutes(source.fetch_interval_minutes)),
        consecutive_failures: 0,
        last_error_code: None,
        last_error_at: None,
    }
}

/// 失败原子性：只更新诊断列与退避时间，保留旧 validators 与文章。
fn record_failure(
    db: &Database,
    source: &FeedSource,
    source_id: &str,
    error: &AppError,
    now: chrono::DateTime<Utc>,
) -> AppResult<()> {
    let failures = source.consecutive_failures;
    let code = stable_error_code(error);
    let state = FeedSourceSyncState {
        etag: source.etag.clone(),
        last_modified: source.last_modified.clone(),
        last_checked_at: rfc3339(now),
        last_success_at: source.last_success_at.clone(),
        next_fetch_at: rfc3339(now + ChronoDuration::minutes(backoff_minutes(failures))),
        consecutive_failures: failures + 1,
        last_error_code: Some(code.clone()),
        last_error_at: Some(rfc3339(now)),
    };
    db.with_conn(|conn| FeedRepository::update_source_sync_state(conn, source_id, &state))?;
    tracing::warn!(log_id = source_id, error_code = %code, "feed_sync_failed");
    Ok(())
}

/// 同步成功后的短事务：首次判定 → upsert → 历史已读 → 同步状态。
///
/// 任一环节失败时事务整体回滚（不留下部分文章或部分状态）。
fn persist_success(
    conn: &Connection,
    source: &FeedSource,
    items: &[FeedItemInput],
    history: HistoryReadPolicy,
    state: &FeedSourceSyncState,
    metadata: &(String, Option<String>, Option<String>, Option<String>),
) -> AppResult<UpsertSummary> {
    let tx = conn.unchecked_transaction()?;
    // 首次同步在同一事务内判断：source 尚无 item。
    let first_sync = FeedRepository::count_items(&tx, &source.id)? == 0;
    let summary = FeedRepository::upsert_items(&tx, items)?;
    if first_sync {
        if let Some(boundary) = items.last() {
            FeedRepository::set_history_boundary(
                &tx,
                &source.id,
                &boundary.external_key,
                boundary.published_at.as_deref(),
                &state.last_checked_at,
            )?;
        }
    }
    if first_sync && matches!(history, HistoryReadPolicy::MarkRead) {
        tx.execute(
            "UPDATE feed_items SET read_at = received_at
             WHERE source_id = ?1 AND read_at IS NULL",
            [&source.id],
        )?;
    }
    FeedRepository::update_source_sync_state(&tx, &source.id, state)?;
    FeedRepository::update_source_metadata(
        &tx,
        &source.id,
        &metadata.0,
        metadata.1.as_deref(),
        metadata.2.as_deref(),
        metadata.3.as_deref(),
        &state.last_checked_at,
    )?;
    tx.commit()?;
    Ok(summary)
}

// ── FeedSyncService（Task 2.6：调度器与手动刷新共用）─────────

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use tokio::sync::Mutex as AsyncMutex;

struct InFlightGuard {
    in_flight: Arc<StdMutex<HashSet<String>>>,
    source_id: String,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if let Ok(mut in_flight) = self.in_flight.lock() {
            in_flight.remove(&self.source_id);
        }
    }
}

/// 同步服务：`tokio::sync::Mutex<HashSet<String>>` 防止同源重复同步，
/// 不创建 job 表或通用任务状态机。手动刷新与自动刷新调用同一入口。
/// 可选挂载 `AppHandle` 用于投影同步事件（未挂载时静默跳过）。
pub(crate) struct FeedSyncService<G: FeedNetGate> {
    db: Arc<Database>,
    gate: Arc<G>,
    in_flight: Arc<StdMutex<HashSet<String>>>,
    event_sink: Arc<AsyncMutex<Option<tauri::AppHandle>>>,
}

// 手写 Clone（Arc 克隆不需要 `G: Clone` 约束）。
impl<G: FeedNetGate> Clone for FeedSyncService<G> {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            gate: self.gate.clone(),
            in_flight: self.in_flight.clone(),
            event_sink: self.event_sink.clone(),
        }
    }
}

impl<G: FeedNetGate> FeedSyncService<G> {
    pub(crate) fn new(db: Arc<Database>, gate: Arc<G>) -> Self {
        Self {
            db,
            gate,
            in_flight: Arc::new(StdMutex::new(HashSet::new())),
            event_sink: Arc::new(AsyncMutex::new(None)),
        }
    }

    /// 挂载事件出口（同步完成/失败时投影 `feed:changed`）。
    pub(crate) fn attach_event_sink(&self, app: tauri::AppHandle) {
        if let Ok(mut sink) = self.event_sink.try_lock() {
            *sink = Some(app);
        }
    }

    /// 单源同步（带互斥标记）：同源并发请求直接返回 `InFlight`。
    /// 手动刷新与自动刷新共用本入口；首次同步历史默认已读。
    #[cfg(test)]
    pub(crate) async fn sync_source(
        &self,
        source_id: &str,
        mode: SyncMode,
    ) -> AppResult<SyncOutcome> {
        self.sync_source_with_history(source_id, mode, HistoryReadPolicy::MarkRead)
            .await
    }

    /// 单源同步（显式历史策略；添加流程「历史也设为未读」使用）。
    pub(crate) async fn sync_source_with_history(
        &self,
        source_id: &str,
        mode: SyncMode,
        history: HistoryReadPolicy,
    ) -> AppResult<SyncOutcome> {
        let inserted = {
            let mut in_flight = self
                .in_flight
                .lock()
                .map_err(|_| AppError::msg("feed_sync_lock_failed"))?;
            in_flight.insert(source_id.to_string())
        };
        if !inserted {
            return Ok(SyncOutcome {
                status: SyncStatus::InFlight,
            });
        }
        let _guard = InFlightGuard {
            in_flight: self.in_flight.clone(),
            source_id: source_id.to_string(),
        };

        let outcome =
            crate::feed::sync::sync_source(&self.db, self.gate.as_ref(), source_id, mode, history)
                .await;

        if let Ok(outcome_ref) = &outcome {
            if let Some(event) = FeedChangedEvent::from_outcome(source_id, &outcome_ref.status) {
                if let Some(app) = self.event_sink.lock().await.as_ref().cloned() {
                    use tauri::Emitter;
                    let _ = app.emit("feed:changed", &event);
                }
            }
        }
        outcome
    }

    /// 自动同步一轮：取最多 2 个到期源并发同步；失败不阻断其他源。
    ///
    /// 使用 `join_all` 就地并发（AFIT future 非 Send，不能跨线程 spawn）。
    pub(crate) async fn sync_due_batch(&self) -> AppResult<FeedSyncBatchOutcome> {
        let now = Utc::now();
        let due = self
            .db
            .with_read_conn(|conn| FeedRepository::list_due_sources(conn, &rfc3339(now), 2))?;
        Ok(self
            .sync_sources(due, SyncMode::Automatic, HistoryReadPolicy::MarkRead)
            .await)
    }

    /// 用户手动刷新全部启用源；以固定宽度 2 分批执行。
    pub(crate) async fn sync_all(&self) -> AppResult<FeedSyncBatchOutcome> {
        let sources = self
            .db
            .with_read_conn(FeedRepository::list_enabled_sources)?;
        Ok(self
            .sync_sources(sources, SyncMode::Manual, HistoryReadPolicy::MarkRead)
            .await)
    }

    /// 同步给定来源 ID；保持调用顺序、去重且并发最多 2。
    pub(crate) async fn sync_batch(
        &self,
        source_ids: &[String],
        history: HistoryReadPolicy,
    ) -> FeedSyncBatchOutcome {
        let mut seen = HashSet::new();
        let ids: Vec<String> = source_ids
            .iter()
            .filter(|id| seen.insert((*id).clone()))
            .cloned()
            .collect();
        self.sync_ids(ids, SyncMode::Manual, history).await
    }

    async fn sync_sources(
        &self,
        sources: Vec<FeedSource>,
        mode: SyncMode,
        history: HistoryReadPolicy,
    ) -> FeedSyncBatchOutcome {
        self.sync_ids(
            sources.into_iter().map(|source| source.id).collect(),
            mode,
            history,
        )
        .await
    }

    async fn sync_ids(
        &self,
        ids: Vec<String>,
        mode: SyncMode,
        history: HistoryReadPolicy,
    ) -> FeedSyncBatchOutcome {
        let mut batch = FeedSyncBatchOutcome {
            total: ids.len() as u32,
            ..Default::default()
        };
        for chunk in ids.chunks(2) {
            let outcomes = futures_util::future::join_all(chunk.iter().map(|source_id| {
                let service = self.clone();
                let source_id = source_id.clone();
                async move {
                    service
                        .sync_source_with_history(&source_id, mode, history)
                        .await
                }
            }))
            .await;
            for outcome in outcomes {
                match outcome {
                    Ok(SyncOutcome {
                        status:
                            SyncStatus::Succeeded {
                                new_items,
                                skipped_history,
                            },
                    }) => {
                        batch.succeeded += 1;
                        batch.new_items += new_items as u32;
                        batch.skipped_history += skipped_history as u32;
                    }
                    Ok(SyncOutcome {
                        status: SyncStatus::NotModified,
                    }) => batch.succeeded += 1,
                    Ok(SyncOutcome {
                        status: SyncStatus::Skipped,
                    }) => batch.skipped += 1,
                    Ok(SyncOutcome {
                        status: SyncStatus::InFlight,
                    }) => batch.in_flight += 1,
                    Ok(SyncOutcome {
                        status: SyncStatus::Failed { .. },
                    })
                    | Err(_) => batch.failed += 1,
                }
            }
        }
        batch
    }
}
