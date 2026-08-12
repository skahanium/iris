//! 订阅资料库仓储层。
//!
//! 所有方法接收 `&rusqlite::Connection`（由调用方从连接池取得），仓储自身
//! 不持有连接；批量 upsert 使用单个事务。摘要截断、FTS 转义与状态轴保护
//! 均为本层职责，详情见各方法文档。

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};

use crate::error::{AppError, AppResult};
use crate::feed::model::{
    FeedItemDetail, FeedItemInput, FeedItemQuery, FeedItemStatePatch, FeedItemSummary, FeedSource,
    FeedSourcePatch, FeedSourceSummary, FeedSourceSyncState, NewFeedSource, UpsertSummary,
};

/// 列表摘要从 `content_text` 截断的最大 Unicode scalar 数。
pub(crate) const EXCERPT_MAX_SCALARS: usize = 240;
/// 列表与搜索单页条数下界。
pub(crate) const ITEM_LIMIT_MIN: u32 = 1;
/// 列表与搜索单页条数上界。
pub(crate) const ITEM_LIMIT_MAX: u32 = 200;
/// `fetch_interval_minutes` 合法区间（对应 schema CHECK 约束）。
pub(crate) const FETCH_INTERVAL_MIN: i64 = 15;
pub(crate) const FETCH_INTERVAL_MAX: i64 = 10080;

/// 无状态仓储：方法全部以连接为参数。
pub struct FeedRepository;

fn rfc3339(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// 摘要截断：最多 `EXCERPT_MAX_SCALARS` 个 Unicode scalar，天然不切坏 UTF-8。
fn excerpt(content_text: &str) -> String {
    content_text.chars().take(EXCERPT_MAX_SCALARS).collect()
}

/// 转义 FTS 用户输入：每个空白分隔 token 包成双引号短语，内部引号翻倍，
/// 使 `*`、`OR`、`-` 等语法字符退化为字面 token，绝不拼接原始 SQL。
fn escape_fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// 列表/搜索共用的文章行映射（列序必须与查询 SQL 一致）。
fn map_item_summary(row: &Row) -> rusqlite::Result<FeedItemSummary> {
    let content_text: String = row.get(9)?;
    Ok(FeedItemSummary {
        row_id: row.get(0)?,
        id: row.get(1)?,
        source_id: row.get(2)?,
        source_title: row.get(3)?,
        title: row.get(4)?,
        author_name: row.get(5)?,
        canonical_url: row.get(6)?,
        published_at: row.get(7)?,
        received_at: row.get(8)?,
        excerpt: excerpt(&content_text),
        is_read: row.get::<_, Option<String>>(10)?.is_some(),
        is_starred: row.get::<_, Option<String>>(11)?.is_some(),
        is_archived: row.get::<_, Option<String>>(12)?.is_some(),
        conversion_status: row.get(13)?,
    })
}

const ITEM_SUMMARY_SELECT: &str = "SELECT i.row_id, i.id, i.source_id, s.title, i.title, \
     i.author_name, i.canonical_url, i.published_at, i.received_at, i.content_text, \
     i.read_at, i.starred_at, i.archived_at, i.conversion_status \
     FROM feed_items i JOIN feed_sources s ON s.id = i.source_id";

/// 向查询追加一个 `AND` 条件与对应位置参数（占位符按出现顺序递增）。
fn push_condition(sql: &mut String, values: &mut Vec<Value>, condition: &str, params: Vec<Value>) {
    if !condition.is_empty() {
        sql.push_str(" AND ");
        sql.push_str(condition);
        values.extend(params);
    }
}

/// 视图/来源/时间/游标条件生成器；`prefix` 为列名前缀（列表查询用 `i.`，
/// 批量 UPDATE 用空串）。
fn build_filters(query: &FeedItemQuery, now: DateTime<Utc>, prefix: &str) -> (String, Vec<Value>) {
    let mut sql = String::new();
    let mut values: Vec<Value> = Vec::new();
    match query.view {
        crate::feed::model::FeedView::Inbox => {
            push_condition(
                &mut sql,
                &mut values,
                &format!("{prefix}read_at IS NULL AND {prefix}archived_at IS NULL"),
                vec![],
            );
        }
        crate::feed::model::FeedView::Today => {
            push_condition(
                &mut sql,
                &mut values,
                &format!("{prefix}received_at >= ?"),
                vec![Value::Text(today_start_utc(now))],
            );
        }
        crate::feed::model::FeedView::All => {}
        crate::feed::model::FeedView::Starred => {
            push_condition(
                &mut sql,
                &mut values,
                &format!("{prefix}starred_at IS NOT NULL"),
                vec![],
            );
        }
        crate::feed::model::FeedView::Archived => {
            push_condition(
                &mut sql,
                &mut values,
                &format!("{prefix}archived_at IS NOT NULL"),
                vec![],
            );
        }
    }
    if let Some(source_id) = &query.source_id {
        push_condition(
            &mut sql,
            &mut values,
            &format!("{prefix}source_id = ?"),
            vec![Value::Text(source_id.clone())],
        );
    }
    if let Some(after) = &query.received_after {
        push_condition(
            &mut sql,
            &mut values,
            &format!("{prefix}received_at >= ?"),
            vec![Value::Text(after.clone())],
        );
    }
    if let Some(cursor) = &query.cursor {
        push_condition(
            &mut sql,
            &mut values,
            &format!(
                "({prefix}received_at < ? OR ({prefix}received_at = ? AND {prefix}row_id < ?))"
            ),
            vec![
                Value::Text(cursor.sort_at.clone()),
                Value::Text(cursor.sort_at.clone()),
                Value::Integer(cursor.row_id),
            ],
        );
    }
    (sql, values)
}

impl FeedRepository {
    // ── 订阅源 CRUD ─────────────────────────────────────────

    pub fn create_source(
        conn: &Connection,
        input: &NewFeedSource,
        now: DateTime<Utc>,
    ) -> AppResult<FeedSource> {
        if input.feed_url.trim().is_empty() {
            return Err(AppError::msg("feed_source_url_empty"));
        }
        if !(FETCH_INTERVAL_MIN..=FETCH_INTERVAL_MAX).contains(&input.fetch_interval_minutes) {
            return Err(AppError::msg("feed_fetch_interval_out_of_range"));
        }
        let now_str = rfc3339(now);
        conn.execute(
            "INSERT INTO feed_sources
             (id, feed_url, site_url, title, title_override, description, icon_url, language,
              folder_path, is_enabled, fetch_interval_minutes, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?11, ?11)",
            params![
                &input.id,
                &input.feed_url,
                &input.site_url,
                &input.title,
                &input.title_override,
                &input.description,
                &input.icon_url,
                &input.language,
                &input.folder_path,
                input.fetch_interval_minutes,
                now_str,
            ],
        )?;
        Self::get_source(conn, &input.id)?.ok_or_else(|| AppError::msg("feed_source_readback"))
    }

    pub fn get_source(conn: &Connection, id: &str) -> AppResult<Option<FeedSource>> {
        let source = conn
            .query_row(
                "SELECT id, feed_url, site_url, title, title_override, description, icon_url,
                        language, folder_path, is_enabled, fetch_interval_minutes, etag,
                        last_modified, last_checked_at, last_success_at, next_fetch_at,
                        consecutive_failures, last_error_code, last_error_at, created_at,
                        updated_at
                 FROM feed_sources WHERE id = ?1",
                [id],
                |row| {
                    Ok(FeedSource {
                        id: row.get(0)?,
                        feed_url: row.get(1)?,
                        site_url: row.get(2)?,
                        title: row.get(3)?,
                        title_override: row.get(4)?,
                        description: row.get(5)?,
                        icon_url: row.get(6)?,
                        language: row.get(7)?,
                        folder_path: row.get(8)?,
                        is_enabled: row.get::<_, i64>(9)? != 0,
                        fetch_interval_minutes: row.get(10)?,
                        etag: row.get(11)?,
                        last_modified: row.get(12)?,
                        last_checked_at: row.get(13)?,
                        last_success_at: row.get(14)?,
                        next_fetch_at: row.get(15)?,
                        consecutive_failures: row.get(16)?,
                        last_error_code: row.get(17)?,
                        last_error_at: row.get(18)?,
                        created_at: row.get(19)?,
                        updated_at: row.get(20)?,
                    })
                },
            )
            .optional()?;
        Ok(source)
    }

    pub fn list_sources(conn: &Connection) -> AppResult<Vec<FeedSourceSummary>> {
        let mut statement = conn.prepare(
            "SELECT s.id,
                    COALESCE(s.title_override, s.title) AS title,
                    s.feed_url, s.site_url, s.folder_path, s.is_enabled,
                    s.fetch_interval_minutes,
                    (SELECT COUNT(*) FROM feed_items i
                      WHERE i.source_id = s.id
                        AND i.read_at IS NULL AND i.archived_at IS NULL) AS unread_count,
                    s.last_checked_at, s.last_success_at, s.next_fetch_at,
                    s.consecutive_failures, s.last_error_code
             FROM feed_sources s
             ORDER BY s.folder_path, s.title",
        )?;
        let sources = statement
            .query_map([], |row| {
                Ok(FeedSourceSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    feed_url: row.get(2)?,
                    site_url: row.get(3)?,
                    folder_path: row.get(4)?,
                    is_enabled: row.get::<_, i64>(5)? != 0,
                    fetch_interval_minutes: row.get(6)?,
                    unread_count: row.get(7)?,
                    last_checked_at: row.get(8)?,
                    last_success_at: row.get(9)?,
                    next_fetch_at: row.get(10)?,
                    consecutive_failures: row.get(11)?,
                    last_error_code: row.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(sources)
    }

    pub fn update_source(
        conn: &Connection,
        id: &str,
        patch: &FeedSourcePatch,
        now: DateTime<Utc>,
    ) -> AppResult<bool> {
        if let Some(interval) = patch.fetch_interval_minutes {
            if !(FETCH_INTERVAL_MIN..=FETCH_INTERVAL_MAX).contains(&interval) {
                return Err(AppError::msg("feed_fetch_interval_out_of_range"));
            }
        }
        let now_str = rfc3339(now);
        let mut sets = vec!["updated_at = :updated_at"];
        let mut values: Vec<(&str, &dyn rusqlite::ToSql)> = vec![(":updated_at", &now_str)];
        // 先拷贝 Copy 型字段到外层作用域，避免 `if let` 块内借用提前结束。
        let interval = patch.fetch_interval_minutes;
        let enabled = patch.is_enabled;
        if patch.clear_title_override {
            sets.push("title_override = NULL");
        } else if let Some(title) = &patch.title_override {
            sets.push("title_override = :title_override");
            values.push((":title_override", title));
        }
        if let Some(folder) = &patch.folder_path {
            sets.push("folder_path = :folder_path");
            values.push((":folder_path", folder));
        }
        if let Some(value) = interval.as_ref() {
            sets.push("fetch_interval_minutes = :fetch_interval_minutes");
            values.push((":fetch_interval_minutes", value));
        }
        if let Some(value) = enabled.as_ref() {
            sets.push("is_enabled = :is_enabled");
            values.push((":is_enabled", value));
        }
        values.push((":id", &id));
        let sql = format!("UPDATE feed_sources SET {} WHERE id = :id", sets.join(", "));
        let changed = conn.execute(&sql, values.as_slice())?;
        Ok(changed > 0)
    }

    /// 删除订阅源及其全部文章（cascade）；返回被删除的文章数。
    pub fn delete_source(conn: &Connection, id: &str) -> AppResult<Option<i64>> {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM feed_sources WHERE id = ?1)",
            [id],
            |row| row.get(0),
        )?;
        if !exists {
            return Ok(None);
        }
        let item_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM feed_items WHERE source_id = ?1",
            [id],
            |row| row.get(0),
        )?;
        conn.execute("DELETE FROM feed_sources WHERE id = ?1", [id])?;
        Ok(Some(item_count))
    }

    pub fn count_items(conn: &Connection, source_id: &str) -> AppResult<i64> {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM feed_items WHERE source_id = ?1",
            [source_id],
            |row| row.get(0),
        )?)
    }

    /// 全量覆盖同步状态列（同步成功/失败/304 的唯一写入点）。
    pub fn update_source_sync_state(
        conn: &Connection,
        id: &str,
        state: &FeedSourceSyncState,
    ) -> AppResult<bool> {
        if state.consecutive_failures < 0 {
            return Err(AppError::msg("feed_sync_state_invalid"));
        }
        let changed = conn.execute(
            "UPDATE feed_sources
             SET etag = ?1, last_modified = ?2, last_checked_at = ?3,
                 last_success_at = ?4, next_fetch_at = ?5,
                 consecutive_failures = ?6, last_error_code = ?7,
                 last_error_at = ?8, updated_at = ?9
             WHERE id = ?10",
            params![
                state.etag,
                state.last_modified,
                state.last_checked_at,
                state.last_success_at,
                state.next_fetch_at,
                state.consecutive_failures,
                state.last_error_code,
                state.last_error_at,
                state.last_checked_at,
                id,
            ],
        )?;
        Ok(changed > 0)
    }

    // ── 文章 upsert 与列表 ───────────────────────────────────

    /// 批量 upsert：单个事务；仅当 `content_hash` 变化时替换内容字段，
    /// 绝不覆盖 `read_at`/`starred_at`/`archived_at`/`received_at`。
    ///
    /// 调用方已处于事务中（如同步短事务）时直接复用，不嵌套 BEGIN。
    pub fn upsert_items(conn: &Connection, items: &[FeedItemInput]) -> AppResult<UpsertSummary> {
        if items.is_empty() {
            return Ok(UpsertSummary {
                inserted: 0,
                updated: 0,
                unchanged: 0,
            });
        }
        let now_str = rfc3339(Utc::now());

        // 批量校验先行：任一非法条目整个批次拒绝（事务回滚，不留部分行）。
        for item in items {
            if item.external_key.trim().is_empty() {
                return Err(AppError::msg("feed_item_external_key_empty"));
            }
            if item.source_id.trim().is_empty() || item.id.trim().is_empty() {
                return Err(AppError::msg("feed_item_identity_invalid"));
            }
        }

        let execute = |target: &Connection| -> AppResult<UpsertSummary> {
            let mut inserted = 0usize;
            for item in items {
                let payload_kind = item.source_payload_kind.as_str();
                let conversion_status = item.conversion_status.as_str();
                let values: Vec<(&str, &dyn rusqlite::ToSql)> = vec![
                    (":id", &item.id),
                    (":source_id", &item.source_id),
                    (":external_key", &item.external_key),
                    (":canonical_url", &item.canonical_url),
                    (":title", &item.title),
                    (":author_name", &item.author_name),
                    (":published_at", &item.published_at),
                    (":source_updated_at", &item.source_updated_at),
                    (":received_at", &item.received_at),
                    (":summary_markdown", &item.summary_markdown),
                    (":content_markdown", &item.content_markdown),
                    (":content_text", &item.content_text),
                    (":source_payload", &item.source_payload),
                    (":source_payload_kind", &payload_kind),
                    (":content_hash", &item.content_hash),
                    (":conversion_version", &item.conversion_version),
                    (":conversion_status", &conversion_status),
                    (":created_at", &now_str),
                    (":updated_at", &now_str),
                ];
                inserted += target.execute(
                    "INSERT OR IGNORE INTO feed_items
                     (id, source_id, external_key, canonical_url, title, author_name,
                      published_at, source_updated_at, received_at, summary_markdown,
                      content_markdown, content_text, source_payload, source_payload_kind,
                      content_hash, conversion_version, conversion_status, created_at, updated_at)
                     VALUES (:id, :source_id, :external_key, :canonical_url, :title, :author_name,
                             :published_at, :source_updated_at, :received_at, :summary_markdown,
                             :content_markdown, :content_text, :source_payload,
                             :source_payload_kind, :content_hash, :conversion_version,
                             :conversion_status, :created_at, :updated_at)",
                    values.as_slice(),
                )?;
            }

            let mut updated = 0usize;
            for item in items {
                updated += target.execute(
                    "UPDATE feed_items
                     SET canonical_url = ?1, title = ?2, author_name = ?3, published_at = ?4,
                         source_updated_at = ?5, summary_markdown = ?6, content_markdown = ?7,
                         content_text = ?8, source_payload = ?9, source_payload_kind = ?10,
                         content_hash = ?11, conversion_version = ?12,
                         conversion_status = ?13, updated_at = ?14
                     WHERE source_id = ?15 AND external_key = ?16 AND content_hash != ?17",
                    params![
                        &item.canonical_url,
                        &item.title,
                        &item.author_name,
                        &item.published_at,
                        &item.source_updated_at,
                        &item.summary_markdown,
                        &item.content_markdown,
                        &item.content_text,
                        &item.source_payload,
                        &item.source_payload_kind.as_str(),
                        &item.content_hash,
                        item.conversion_version,
                        &item.conversion_status.as_str(),
                        &now_str,
                        &item.source_id,
                        &item.external_key,
                        &item.content_hash,
                    ],
                )?;
            }
            Ok(UpsertSummary {
                inserted,
                updated,
                unchanged: items.len() - inserted - updated,
            })
        };

        if conn.is_autocommit() {
            let tx = conn.unchecked_transaction()?;
            let summary = execute(&tx)?;
            tx.commit()?;
            Ok(summary)
        } else {
            // 调用方事务内：直接执行，由外层事务保证原子性。
            execute(conn)
        }
    }

    pub fn list_items(
        conn: &Connection,
        query: &FeedItemQuery,
        now: DateTime<Utc>,
    ) -> AppResult<Vec<FeedItemSummary>> {
        let limit = query.limit.clamp(ITEM_LIMIT_MIN, ITEM_LIMIT_MAX);
        let (filters, mut values) = build_filters(query, now, "i.");
        let mut sql = String::from(ITEM_SUMMARY_SELECT);
        sql.push_str(" WHERE 1=1");
        sql.push_str(&filters);
        sql.push_str(" ORDER BY i.received_at DESC, i.row_id DESC LIMIT ?");
        values.push(Value::Integer(i64::from(limit)));

        let mut statement = conn.prepare(&sql)?;
        let items = statement
            .query_map(params_from_iter(values.iter()), map_item_summary)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }

    pub fn get_item_detail(conn: &Connection, item_id: &str) -> AppResult<Option<FeedItemDetail>> {
        let detail = conn
            .query_row(
                "SELECT i.row_id, i.id, i.source_id, s.title, i.title, i.author_name,
                        i.canonical_url, i.published_at, i.received_at, i.content_text,
                        i.read_at, i.starred_at, i.archived_at, i.conversion_status,
                        i.content_markdown, i.summary_markdown
                 FROM feed_items i JOIN feed_sources s ON s.id = i.source_id
                 WHERE i.id = ?1",
                [item_id],
                |row| {
                    Ok(FeedItemDetail {
                        summary: map_item_summary(row)?,
                        content_markdown: row.get(14)?,
                        summary_markdown: row.get(15)?,
                    })
                },
            )
            .optional()?;
        Ok(detail)
    }

    pub fn set_item_state(
        conn: &Connection,
        item_id: &str,
        patch: &FeedItemStatePatch,
        now: DateTime<Utc>,
    ) -> AppResult<bool> {
        if patch.is_empty() {
            return Err(AppError::msg("feed_item_state_patch_empty"));
        }
        let now_str = rfc3339(now);
        let mut sets = vec!["updated_at = ?1".to_string()];
        let mut values: Vec<Value> = vec![Value::Text(now_str.clone())];
        for (option, column) in [
            (patch.is_read, "read_at"),
            (patch.is_starred, "starred_at"),
            (patch.is_archived, "archived_at"),
        ] {
            match option {
                Some(true) => {
                    sets.push(format!("{column} = ?{}", values.len() + 1));
                    values.push(Value::Text(now_str.clone()));
                }
                Some(false) => sets.push(format!("{column} = NULL")),
                None => {}
            }
        }
        values.push(Value::Text(item_id.to_string()));
        let id_index = values.len();
        let sql = format!(
            "UPDATE feed_items SET {} WHERE id = ?{id_index}",
            sets.join(", ")
        );
        let changed = conn.execute(&sql, params_from_iter(values.iter()))?;
        Ok(changed > 0)
    }

    /// 基于冻结查询条件批量标已读；返回实际影响行数。
    pub fn mark_items_read(
        conn: &Connection,
        query: &FeedItemQuery,
        now: DateTime<Utc>,
    ) -> AppResult<i64> {
        let now_str = rfc3339(now);
        let (filters, values) = build_filters(query, now, "");
        let mut sql = String::from("UPDATE feed_items SET read_at = ?1, updated_at = ?2");
        sql.push_str(" WHERE read_at IS NULL");
        sql.push_str(&filters);
        let mut all_values: Vec<Value> = vec![Value::Text(now_str.clone()), Value::Text(now_str)];
        all_values.extend(values);
        let affected = conn.execute(&sql, params_from_iter(all_values.iter()))?;
        Ok(affected as i64)
    }

    // ── 搜索 ────────────────────────────────────────────────

    pub fn search(
        conn: &Connection,
        query: &str,
        source_id: Option<&str>,
        limit: u32,
    ) -> AppResult<Vec<FeedItemSummary>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Err(AppError::msg("feed_search_query_empty"));
        }
        let escaped = escape_fts_query(trimmed);
        let limit = limit.clamp(ITEM_LIMIT_MIN, ITEM_LIMIT_MAX);

        let mut sql = String::from(
            "SELECT i.row_id, i.id, i.source_id, s.title, i.title, i.author_name,
                    i.canonical_url, i.published_at, i.received_at, i.content_text,
                    i.read_at, i.starred_at, i.archived_at, i.conversion_status
             FROM feed_items_fts f
             JOIN feed_items i ON i.row_id = f.rowid
             JOIN feed_sources s ON s.id = i.source_id
             WHERE feed_items_fts MATCH ?1",
        );
        let mut values: Vec<Value> = vec![Value::Text(escaped)];
        if let Some(source) = source_id {
            sql.push_str(" AND i.source_id = ?2");
            values.push(Value::Text(source.to_string()));
        }
        sql.push_str(&format!(
            " ORDER BY f.rank, i.received_at DESC LIMIT ?{}",
            values.len() + 1
        ));
        values.push(Value::Integer(i64::from(limit)));

        let mut statement = conn.prepare(&sql)?;
        let items = statement
            .query_map(params_from_iter(values.iter()), map_item_summary)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }
}

/// 本地时区当日零点的 UTC 时间（RFC3339，秒精度），用于「今日」视图。
pub(crate) fn today_start_utc(now: DateTime<Utc>) -> String {
    let local = now.with_timezone(&chrono::Local);
    let local_midnight = local
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("valid midnight");
    chrono::Local
        .from_local_datetime(&local_midnight)
        .single()
        .unwrap_or_else(|| local_midnight.and_utc().with_timezone(&chrono::Local))
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

// ── 到期源查询（Task 2.6 调度器使用）────────────────────────

impl FeedRepository {
    /// 返回到期（`next_fetch_at` 为空或已到）且启用的订阅源，最多 `limit` 个；
    /// 从未同步的源优先。走 `idx_feed_sources_due` 索引。
    pub fn list_due_sources(
        conn: &Connection,
        now: &str,
        limit: i64,
    ) -> AppResult<Vec<FeedSource>> {
        let mut statement = conn.prepare(
            "SELECT id, feed_url, site_url, title, title_override, description, icon_url,
                    language, folder_path, is_enabled, fetch_interval_minutes, etag,
                    last_modified, last_checked_at, last_success_at, next_fetch_at,
                    consecutive_failures, last_error_code, last_error_at, created_at,
                    updated_at
             FROM feed_sources
             WHERE is_enabled = 1 AND (next_fetch_at IS NULL OR next_fetch_at <= ?1)
             ORDER BY next_fetch_at IS NULL DESC, next_fetch_at ASC
             LIMIT ?2",
        )?;
        let sources = statement
            .query_map(params![now, limit], |row| {
                Ok(FeedSource {
                    id: row.get(0)?,
                    feed_url: row.get(1)?,
                    site_url: row.get(2)?,
                    title: row.get(3)?,
                    title_override: row.get(4)?,
                    description: row.get(5)?,
                    icon_url: row.get(6)?,
                    language: row.get(7)?,
                    folder_path: row.get(8)?,
                    is_enabled: row.get::<_, i64>(9)? != 0,
                    fetch_interval_minutes: row.get(10)?,
                    etag: row.get(11)?,
                    last_modified: row.get(12)?,
                    last_checked_at: row.get(13)?,
                    last_success_at: row.get(14)?,
                    next_fetch_at: row.get(15)?,
                    consecutive_failures: row.get(16)?,
                    last_error_code: row.get(17)?,
                    last_error_at: row.get(18)?,
                    created_at: row.get(19)?,
                    updated_at: row.get(20)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(sources)
    }
}
