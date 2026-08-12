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
use crate::feed::model::{FeedItemInput, FeedSource, FeedSourceSyncState, UpsertSummary};
use crate::feed::normalize::{normalize_feed, FEED_CONVERSION_VERSION};
use crate::feed::repository::FeedRepository;
use crate::storage::db::Database;

/// 同步触发方式：自动（跳过暂停源）与手动（可刷新暂停源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyncMode {
    // 阶段 3 `feed_sync_source` 手动刷新消费；届时移除标注。
    #[allow(dead_code)]
    Manual,
    Automatic,
}

/// 首次同步历史项目策略：默认已读；可显式选择未读。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryReadPolicy {
    MarkRead,
    // 阶段 3 添加流程「历史也设为未读」消费；届时移除标注。
    #[allow(dead_code)]
    LeaveUnread,
}

/// 同步结果状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SyncStatus {
    Succeeded {
        new_items: usize,
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
            let items: Vec<FeedItemInput> = normalized
                .items
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
                })
                .collect();
            let state = success_state(&source, &result.etag, &result.last_modified, &now_str, now);

            let summary =
                db.with_conn(|conn| persist_success(conn, &source, &items, history, &state))?;
            Ok(SyncOutcome {
                status: SyncStatus::Succeeded {
                    new_items: summary.inserted,
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
) -> AppResult<UpsertSummary> {
    let tx = conn.unchecked_transaction()?;
    // 首次同步在同一事务内判断：source 尚无 item。
    let first_sync = FeedRepository::count_items(&tx, &source.id)? == 0;
    let summary = FeedRepository::upsert_items(&tx, items)?;
    if first_sync && matches!(history, HistoryReadPolicy::MarkRead) {
        tx.execute(
            "UPDATE feed_items SET read_at = received_at
             WHERE source_id = ?1 AND read_at IS NULL",
            [&source.id],
        )?;
    }
    FeedRepository::update_source_sync_state(&tx, &source.id, state)?;
    tx.commit()?;
    Ok(summary)
}

// ── FeedSyncService（Task 2.6：调度器与手动刷新共用）─────────

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::Mutex;

/// 同步服务：`tokio::sync::Mutex<HashSet<String>>` 防止同源重复同步，
/// 不创建 job 表或通用任务状态机。手动刷新与自动刷新调用同一入口。
pub(crate) struct FeedSyncService<G: FeedNetGate> {
    db: Arc<Database>,
    gate: Arc<G>,
    in_flight: Arc<Mutex<HashSet<String>>>,
}

// 手写 Clone（Arc 克隆不需要 `G: Clone` 约束）。
impl<G: FeedNetGate> Clone for FeedSyncService<G> {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            gate: self.gate.clone(),
            in_flight: self.in_flight.clone(),
        }
    }
}

impl<G: FeedNetGate> FeedSyncService<G> {
    pub(crate) fn new(db: Arc<Database>, gate: Arc<G>) -> Self {
        Self {
            db,
            gate,
            in_flight: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// 单源同步（带互斥标记）：同源并发请求直接返回 `InFlight`。
    /// 手动刷新与自动刷新共用本入口；首次同步历史默认已读。
    pub(crate) async fn sync_source(
        &self,
        source_id: &str,
        mode: SyncMode,
    ) -> AppResult<SyncOutcome> {
        let mut in_flight = self.in_flight.lock().await;
        if !in_flight.insert(source_id.to_string()) {
            return Ok(SyncOutcome {
                status: SyncStatus::InFlight,
            });
        }
        drop(in_flight);

        let outcome = crate::feed::sync::sync_source(
            &self.db,
            self.gate.as_ref(),
            source_id,
            mode,
            HistoryReadPolicy::MarkRead,
        )
        .await;

        let mut in_flight = self.in_flight.lock().await;
        in_flight.remove(source_id);
        outcome
    }

    /// 自动同步一轮：取最多 2 个到期源并发同步；失败不阻断其他源。
    ///
    /// 使用 `join_all` 就地并发（AFIT future 非 Send，不能跨线程 spawn）。
    pub(crate) async fn sync_all(&self) -> AppResult<()> {
        let now = Utc::now();
        let due = self
            .db
            .with_read_conn(|conn| FeedRepository::list_due_sources(conn, &rfc3339(now), 2))?;
        let futures: Vec<_> = due
            .into_iter()
            .map(|source| {
                let service = self.clone();
                async move {
                    match service.sync_source(&source.id, SyncMode::Automatic).await {
                        Ok(outcome) => {
                            tracing::info!(
                                log_id = source.id.as_str(),
                                status = %outcome.status.status_label(),
                                "feed_sync_all_item"
                            );
                        }
                        Err(error) => {
                            tracing::warn!(
                                log_id = source.id.as_str(),
                                error_code = %stable_error_code(&error),
                                "feed_sync_all_item_failed"
                            );
                        }
                    }
                }
            })
            .collect();
        futures_util::future::join_all(futures).await;
        Ok(())
    }
}

impl SyncStatus {
    fn status_label(&self) -> &'static str {
        match self {
            SyncStatus::Succeeded { .. } => "succeeded",
            SyncStatus::NotModified => "not_modified",
            SyncStatus::Skipped => "skipped",
            SyncStatus::InFlight => "in_flight",
            SyncStatus::Failed { .. } => "failed",
        }
    }
}
