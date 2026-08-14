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
    FeedItemDetail, FeedItemInput, FeedItemQuery, FeedItemStatePatch, FeedItemSummary,
    FeedLibrarySummary, FeedPrimaryDocument, FeedSource, FeedSourcePatch, FeedSourceSummary,
    FeedSourceSyncState, FeedTrashItem, FeedTrashSnapshot, FeedTrashSource, NewFeedSource,
    UpsertSummary,
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
    let content_text: String = row.get(10)?;
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
        sort_at: row.get(9)?,
        excerpt: excerpt(&content_text),
        is_read: row.get::<_, Option<String>>(11)?.is_some(),
        is_starred: row.get::<_, Option<String>>(12)?.is_some(),
        is_archived: row.get::<_, Option<String>>(13)?.is_some(),
        conversion_status: row.get(14)?,
    })
}

const ITEM_SUMMARY_SELECT: &str = "SELECT i.row_id, i.id, i.source_id, \
     COALESCE(s.title_override, s.title), i.title, \
     i.author_name, i.canonical_url, i.published_at, i.received_at, \
     COALESCE(i.published_at, i.received_at), i.content_text, \
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
    push_condition(
        &mut sql,
        &mut values,
        &format!("{prefix}deleted_at IS NULL"),
        vec![],
    );
    match query.view {
        crate::feed::model::FeedView::Inbox => {
            push_condition(
                &mut sql,
                &mut values,
                &format!("{prefix}archived_at IS NULL"),
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
    if let Some(search) = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
    {
        let escaped = escape_fts_query(search);
        let like = format!("%{}%", escape_like_query(search));
        let mut condition = format!(
            "({prefix}row_id IN (SELECT rowid FROM feed_items_fts WHERE feed_items_fts MATCH ?) \
             OR COALESCE(s.title_override, s.title) LIKE ? ESCAPE '\\'"
        );
        let mut params = vec![Value::Text(escaped), Value::Text(like.clone())];
        if contains_cjk(search) {
            condition.push_str(&format!(
                " OR {prefix}title LIKE ? ESCAPE '\\' \
                 OR COALESCE({prefix}author_name, '') LIKE ? ESCAPE '\\' \
                 OR {prefix}content_text LIKE ? ESCAPE '\\'"
            ));
            params.extend([
                Value::Text(like.clone()),
                Value::Text(like.clone()),
                Value::Text(like),
            ]);
        }
        condition.push(')');
        push_condition(&mut sql, &mut values, &condition, params);
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
                "(COALESCE({prefix}published_at, {prefix}received_at) < ? \
                  OR (COALESCE({prefix}published_at, {prefix}received_at) = ? \
                      AND {prefix}row_id < ?))"
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

fn contains_cjk(value: &str) -> bool {
    value
        .chars()
        .any(|ch| matches!(ch as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF))
}

fn escape_like_query(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// 订阅源完整行映射（24 列；`get_source`/`list_due_sources`/`get_source_by_feed_url`
/// 的 SELECT 列序必须与此一致）。
fn map_source_row(row: &Row) -> rusqlite::Result<FeedSource> {
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
        history_boundary_external_key: row.get(21)?,
        history_boundary_published_at: row.get(22)?,
        fulltext_enabled: row.get::<_, i64>(23)? != 0,
    })
}

const SOURCE_SELECT: &str = "SELECT id, feed_url, site_url, title, title_override, description, \
     icon_url, language, folder_path, is_enabled, fetch_interval_minutes, etag, last_modified, \
     last_checked_at, last_success_at, next_fetch_at, consecutive_failures, last_error_code, \
     last_error_at, created_at, updated_at, history_boundary_external_key, \
     history_boundary_published_at, fulltext_enabled FROM feed_sources";

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
              folder_path, is_enabled, fetch_interval_minutes, fulltext_enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, 1, ?11, ?11)",
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
                &format!("{SOURCE_SELECT} WHERE id = ?1 AND deleted_at IS NULL"),
                [id],
                map_source_row,
            )
            .optional()?;
        Ok(source)
    }

    /// 按 `feed_url` 查询（列 UNIQUE）；OPML 导入合并复用。
    pub fn get_source_by_feed_url(
        conn: &Connection,
        feed_url: &str,
    ) -> AppResult<Option<FeedSource>> {
        let source = conn
            .query_row(
                &format!("{SOURCE_SELECT} WHERE feed_url = ?1 AND deleted_at IS NULL"),
                [feed_url],
                map_source_row,
            )
            .optional()?;
        Ok(source)
    }

    pub fn get_deleted_source_by_feed_url(
        conn: &Connection,
        feed_url: &str,
    ) -> AppResult<Option<String>> {
        conn.query_row(
            "SELECT id FROM feed_sources WHERE feed_url = ?1 AND deleted_at IS NOT NULL",
            [feed_url],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn find_deleted_source_by_feed_url(
        conn: &Connection,
        feed_url: &str,
    ) -> AppResult<Option<FeedTrashSource>> {
        conn.query_row(
            "SELECT s.id, COALESCE(s.title_override, s.title),
                    COUNT(i.id), SUM(CASE WHEN i.starred_at IS NOT NULL THEN 1 ELSE 0 END),
                    s.deleted_at, s.purge_after
             FROM feed_sources s
             LEFT JOIN feed_items i ON i.source_id = s.id
                                      AND i.deletion_reason = 'source_removed'
             WHERE s.feed_url = ?1 AND s.deleted_at IS NOT NULL
             GROUP BY s.id",
            [feed_url],
            |row| {
                Ok(FeedTrashSource {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    item_count: row.get(2)?,
                    starred_count: row.get(3)?,
                    deleted_at: row.get(4)?,
                    purge_after: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn list_sources(conn: &Connection) -> AppResult<Vec<FeedSourceSummary>> {
        let mut statement = conn.prepare(
            "SELECT s.id,
                    COALESCE(s.title_override, s.title) AS title,
                    s.feed_url, s.site_url, s.folder_path, s.is_enabled,
                    s.fetch_interval_minutes, s.fulltext_enabled,
                    (SELECT COUNT(*) FROM feed_items i
                      WHERE i.source_id = s.id
                        AND i.deleted_at IS NULL
                        AND i.read_at IS NULL AND i.archived_at IS NULL) AS unread_count,
                    s.last_checked_at, s.last_success_at, s.next_fetch_at,
                    s.consecutive_failures, s.last_error_code
             FROM feed_sources s
             WHERE s.deleted_at IS NULL
             ORDER BY s.folder_path, COALESCE(s.title_override, s.title)",
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
                    fulltext_enabled: row.get::<_, i64>(7)? != 0,
                    unread_count: row.get(8)?,
                    last_checked_at: row.get(9)?,
                    last_success_at: row.get(10)?,
                    next_fetch_at: row.get(11)?,
                    consecutive_failures: row.get(12)?,
                    last_error_code: row.get(13)?,
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
        let fulltext_enabled = patch.fulltext_enabled;
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
        if let Some(value) = fulltext_enabled.as_ref() {
            sets.push("fulltext_enabled = :fulltext_enabled");
            values.push((":fulltext_enabled", value));
        }
        values.push((":id", &id));
        let sql = format!(
            "UPDATE feed_sources SET {} WHERE id = :id AND deleted_at IS NULL",
            sets.join(", ")
        );
        let changed = conn.execute(&sql, values.as_slice())?;
        if changed > 0 && fulltext_enabled == Some(false) {
            // 关闭来源级补全后，不再启动尚未开始的网页请求；已在运行的请求
            // 由其自身安全边界完成或失败，避免强行中断连接。
            conn.execute(
                "UPDATE feed_items SET fulltext_status = 'not_requested'
                 WHERE source_id = ?1 AND fulltext_status = 'pending'",
                [id],
            )?;
        }
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

    pub fn source_trash_preview(
        conn: &Connection,
        source_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<Option<crate::feed::model::FeedSourceTrashPreview>> {
        conn.query_row(
            "SELECT COUNT(i.id),
                    SUM(CASE WHEN i.starred_at IS NOT NULL THEN 1 ELSE 0 END)
             FROM feed_sources s
             LEFT JOIN feed_items i ON i.source_id = s.id AND i.deleted_at IS NULL
             WHERE s.id = ?1 AND s.deleted_at IS NULL
             GROUP BY s.id",
            [source_id],
            |row| {
                Ok(crate::feed::model::FeedSourceTrashPreview {
                    item_count: row.get(0)?,
                    starred_count: row.get(1)?,
                    purge_after: rfc3339(now + chrono::Duration::days(30)),
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    /// 汇总资料库维护页所需的非敏感计数与最近成功同步时间。
    pub fn library_summary(conn: &Connection) -> AppResult<FeedLibrarySummary> {
        Ok(conn.query_row(
            "SELECT
                (SELECT COUNT(*) FROM feed_sources WHERE deleted_at IS NULL),
                (SELECT COUNT(*) FROM feed_sources WHERE deleted_at IS NULL AND is_enabled = 1),
                (SELECT COUNT(*) FROM feed_sources WHERE deleted_at IS NULL AND last_error_code IS NOT NULL),
                (SELECT COUNT(*) FROM feed_items WHERE deleted_at IS NULL),
                (SELECT COUNT(*) FROM feed_items
                 WHERE deleted_at IS NULL AND read_at IS NULL AND archived_at IS NULL),
                (SELECT MAX(last_success_at) FROM feed_sources WHERE deleted_at IS NULL)",
            [],
            |row| {
                Ok(FeedLibrarySummary {
                    source_count: row.get(0)?,
                    enabled_source_count: row.get(1)?,
                    failed_source_count: row.get(2)?,
                    item_count: row.get(3)?,
                    unread_count: row.get(4)?,
                    last_success_at: row.get(5)?,
                })
            },
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
             WHERE id = ?10 AND deleted_at IS NULL",
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

    /// 同步成功后更新 Feed 自身元数据；不触碰用户设置的 `title_override`。
    pub fn update_source_metadata(
        conn: &Connection,
        id: &str,
        title: &str,
        site_url: Option<&str>,
        description: Option<&str>,
        language: Option<&str>,
        now: &str,
    ) -> AppResult<bool> {
        let changed = conn.execute(
            "UPDATE feed_sources
             SET title = CASE WHEN TRIM(?1) = '' THEN title ELSE ?1 END,
                 site_url = COALESCE(?2, site_url),
                 description = COALESCE(?3, description),
                 language = COALESCE(?4, language), updated_at = ?5
             WHERE id = ?6 AND deleted_at IS NULL",
            params![title, site_url, description, language, now, id],
        )?;
        Ok(changed > 0)
    }

    /// 记录首次同步保留集的最旧条目，后续全量 Feed 不会继续回灌更早历史。
    pub fn set_history_boundary(
        conn: &Connection,
        source_id: &str,
        external_key: &str,
        published_at: Option<&str>,
        now: &str,
    ) -> AppResult<bool> {
        let changed = conn.execute(
            "UPDATE feed_sources
             SET history_boundary_external_key = ?1, history_boundary_published_at = ?2,
                 updated_at = ?3
             WHERE id = ?4 AND deleted_at IS NULL",
            params![external_key, published_at, now, source_id],
        )?;
        Ok(changed > 0)
    }

    // ── 文章 upsert 与列表 ───────────────────────────────────

    /// 批量 upsert：单个事务；元数据始终按稳定键更新，仅当 `content_hash`
    /// 变化时替换内容字段，
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
                let fulltext_status = item.fulltext_status.as_str();
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
                    (":expires_at", &item.expires_at),
                    (":fulltext_status", &fulltext_status),
                    (":created_at", &now_str),
                    (":updated_at", &now_str),
                ];
                inserted += target.execute(
                    "INSERT OR IGNORE INTO feed_items
                     (id, source_id, external_key, canonical_url, title, author_name,
                      published_at, source_updated_at, received_at, summary_markdown,
                      content_markdown, content_text, source_payload, source_payload_kind,
                      content_hash, conversion_version, conversion_status, expires_at, fulltext_status,
                      created_at, updated_at)
                     VALUES (:id, :source_id, :external_key, :canonical_url, :title, :author_name,
                             :published_at, :source_updated_at, :received_at, :summary_markdown,
                             :content_markdown, :content_text, :source_payload,
                             :source_payload_kind, :content_hash, :conversion_version,
                             :conversion_status, :expires_at, :fulltext_status, :created_at, :updated_at)",
                    values.as_slice(),
                )?;
            }

            let mut updated = 0usize;
            for item in items {
                updated += target.execute(
                    "UPDATE feed_items
                     SET canonical_url = ?1, title = ?2, author_name = ?3, published_at = ?4,
                         source_updated_at = ?5,
                         summary_markdown = CASE WHEN content_hash != ?11 THEN ?6 ELSE summary_markdown END,
                         content_markdown = CASE WHEN content_hash != ?11 THEN ?7 ELSE content_markdown END,
                         content_text = CASE WHEN content_hash != ?11 THEN ?8 ELSE content_text END,
                         source_payload = CASE WHEN content_hash != ?11 THEN ?9 ELSE source_payload END,
                         source_payload_kind = CASE WHEN content_hash != ?11 THEN ?10 ELSE source_payload_kind END,
                         fulltext_markdown = CASE WHEN content_hash != ?11 THEN NULL ELSE fulltext_markdown END,
                         content_origin = CASE WHEN content_hash != ?11 THEN 'feed' ELSE content_origin END,
                         fulltext_status = CASE WHEN content_hash != ?11 THEN ?14 ELSE fulltext_status END,
                         fulltext_extraction_version = CASE
                             WHEN content_hash != ?11 THEN 0 ELSE fulltext_extraction_version END,
                         primary_document_kind = CASE
                             WHEN content_hash != ?11 THEN NULL ELSE primary_document_kind END,
                         primary_document_url = CASE
                             WHEN content_hash != ?11 THEN NULL ELSE primary_document_url END,
                         images_authorized_at = CASE
                             WHEN content_hash != ?11 THEN NULL ELSE images_authorized_at END,
                         content_hash = ?11, conversion_version = ?12,
                         conversion_status = ?13, updated_at = ?15
                     WHERE source_id = ?16 AND external_key = ?17
                       AND (content_hash != ?11 OR canonical_url IS NOT ?1 OR title != ?2
                            OR author_name IS NOT ?3 OR published_at IS NOT ?4
                            OR source_updated_at IS NOT ?5)",
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
                        &item.fulltext_status.as_str(),
                        &now_str,
                        &item.source_id,
                        &item.external_key,
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
        sql.push_str(
            " ORDER BY COALESCE(i.published_at, i.received_at) DESC, i.row_id DESC LIMIT ?",
        );
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
                "SELECT i.row_id, i.id, i.source_id,
                        COALESCE(s.title_override, s.title), i.title, i.author_name,
                        i.canonical_url, i.published_at, i.received_at,
                        COALESCE(i.published_at, i.received_at), i.content_text,
                        i.read_at, i.starred_at, i.archived_at, i.conversion_status,
                        COALESCE(i.fulltext_markdown, i.content_markdown), i.summary_markdown, s.site_url,
                        i.content_origin, i.fulltext_status,
                        i.primary_document_kind, i.primary_document_url,
                        i.fulltext_extraction_version, i.images_authorized_at IS NOT NULL
                 FROM feed_items i JOIN feed_sources s ON s.id = i.source_id
                 WHERE i.id = ?1 AND i.deleted_at IS NULL AND s.deleted_at IS NULL",
                [item_id],
                |row| {
                    Ok(FeedItemDetail {
                        summary: map_item_summary(row)?,
                        content_markdown: row.get(15)?,
                        summary_markdown: row.get(16)?,
                        site_url: row.get(17)?,
                        content_origin: row.get(18)?,
                        fulltext_status: row.get(19)?,
                        primary_document: match (row.get::<_, Option<String>>(20)?, row.get::<_, Option<String>>(21)?) {
                            (Some(kind), Some(url)) => Some(FeedPrimaryDocument { kind, url }),
                            _ => None,
                        },
                        fulltext_needs_refresh: row.get::<_, String>(18)? == "web"
                            && row.get::<_, i64>(22)?
                                < crate::feed::fulltext::FULLTEXT_EXTRACTION_VERSION,
                        images_authorized: row.get(23)?,
                    })
                },
            )
            .optional()?;
        Ok(detail)
    }

    pub fn get_primary_document_url(conn: &Connection, item_id: &str) -> AppResult<Option<String>> {
        conn.query_row(
            "SELECT i.primary_document_url
             FROM feed_items i JOIN feed_sources s ON s.id = i.source_id
             WHERE i.id = ?1 AND i.deleted_at IS NULL AND s.deleted_at IS NULL
               AND i.primary_document_kind = 'pdf'",
            [item_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
    }

    /// 在文章仍可阅读时记录一次明确的单篇图片授权，并返回当前正文和公开原文 URL。
    pub fn authorize_item_images(
        conn: &Connection,
        item_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<Option<(String, Option<String>)>> {
        let item = conn
            .query_row(
                "SELECT COALESCE(i.fulltext_markdown, i.content_markdown), i.canonical_url
                 FROM feed_items i JOIN feed_sources s ON s.id = i.source_id
                 WHERE i.id = ?1 AND i.deleted_at IS NULL AND s.deleted_at IS NULL",
                [item_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if item.is_some() {
            conn.execute(
                "UPDATE feed_items SET images_authorized_at = ?1, updated_at = ?1
                 WHERE id = ?2 AND deleted_at IS NULL",
                params![rfc3339(now), item_id],
            )?;
        }
        Ok(item)
    }

    /// 读取已经明确授权的单篇图片上下文；不会因打开文章而扩大授权范围。
    pub fn authorized_item_images(
        conn: &Connection,
        item_id: &str,
    ) -> AppResult<Option<(String, Option<String>)>> {
        conn.query_row(
            "SELECT COALESCE(i.fulltext_markdown, i.content_markdown), i.canonical_url
             FROM feed_items i JOIN feed_sources s ON s.id = i.source_id
             WHERE i.id = ?1 AND i.deleted_at IS NULL AND s.deleted_at IS NULL
               AND i.images_authorized_at IS NOT NULL",
            [item_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn source_primary_document_urls(
        conn: &Connection,
        source_id: &str,
    ) -> AppResult<Vec<String>> {
        let mut statement = conn.prepare(
            "SELECT DISTINCT primary_document_url FROM feed_items
             WHERE source_id = ?1 AND primary_document_kind = 'pdf'
               AND primary_document_url IS NOT NULL",
        )?;
        let urls = statement
            .query_map([source_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(urls)
    }

    pub fn active_primary_document_reference_count(conn: &Connection, url: &str) -> AppResult<i64> {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM feed_items i JOIN feed_sources s ON s.id = i.source_id
             WHERE i.primary_document_url = ?1 AND i.deleted_at IS NULL
               AND s.deleted_at IS NULL",
            [url],
            |row| row.get(0),
        )?)
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
            "UPDATE feed_items SET {} WHERE id = ?{id_index} AND deleted_at IS NULL",
            sets.join(", ")
        );
        let changed = conn.execute(&sql, params_from_iter(values.iter()))?;
        if changed > 0 && (patch.is_starred.is_some() || patch.is_archived.is_some()) {
            conn.execute(
                "UPDATE feed_items SET expires_at = CASE
                    WHEN starred_at IS NOT NULL THEN NULL
                    WHEN archived_at IS NOT NULL THEN ?1
                    ELSE ?2 END
                 WHERE id = ?3 AND deleted_at IS NULL",
                params![
                    rfc3339(now + chrono::Duration::days(30)),
                    rfc3339(now + chrono::Duration::days(7)),
                    item_id
                ],
            )?;
        }
        Ok(changed > 0)
    }

    /// 基于冻结查询条件批量标已读；返回实际影响行数。
    pub fn mark_items_read(
        conn: &Connection,
        query: &FeedItemQuery,
        now: DateTime<Utc>,
    ) -> AppResult<i64> {
        let now_str = rfc3339(now);
        let (filters, values) = build_filters(query, now, "i.");
        let mut sql = String::from(
            "UPDATE feed_items SET read_at = ?1, updated_at = ?2
             WHERE read_at IS NULL AND id IN (
                 SELECT i.id FROM feed_items i JOIN feed_sources s ON s.id = i.source_id
                 WHERE s.deleted_at IS NULL",
        );
        sql.push_str(&filters);
        sql.push(')');
        let mut all_values: Vec<Value> = vec![Value::Text(now_str.clone()), Value::Text(now_str)];
        all_values.extend(values);
        let affected = conn.execute(&sql, params_from_iter(all_values.iter()))?;
        Ok(affected as i64)
    }

    /// 将到期且未收藏文章移入 RSS 回收站；收藏项永不自动清理。
    pub fn soft_delete_expired_items(conn: &Connection, now: DateTime<Utc>) -> AppResult<u32> {
        let now_str = rfc3339(now);
        let purge_after = rfc3339(now + chrono::Duration::days(30));
        let changed = conn.execute(
            "UPDATE feed_items
             SET deleted_at = ?1, purge_after = ?2, deletion_reason = 'retention',
                 images_authorized_at = NULL, updated_at = ?1
             WHERE deleted_at IS NULL AND starred_at IS NULL
               AND expires_at IS NOT NULL AND expires_at <= ?1",
            params![now_str, purge_after],
        )?;
        Ok(changed as u32)
    }

    /// 物理清理已过 RSS 回收站保留期的缓存文章；调用方可随后显式 optimize。
    pub fn purge_deleted_items(conn: &Connection, now: DateTime<Utc>) -> AppResult<u32> {
        let changed = conn.execute(
            "DELETE FROM feed_items
             WHERE deleted_at IS NOT NULL AND purge_after <= ?1
               AND COALESCE(deletion_reason, 'retention') != 'source_removed'",
            [rfc3339(now)],
        )?;
        Ok(changed as u32)
    }

    /// 物理清理已超过 30 天恢复窗口的来源；外键级联只作用于该来源文章。
    pub fn purge_expired_sources(conn: &Connection, now: DateTime<Utc>) -> AppResult<u32> {
        let changed = conn.execute(
            "DELETE FROM feed_sources
             WHERE deleted_at IS NOT NULL AND purge_after IS NOT NULL AND purge_after <= ?1",
            [rfc3339(now)],
        )?;
        Ok(changed as u32)
    }

    pub(crate) fn expired_source_ids(
        conn: &Connection,
        now: DateTime<Utc>,
    ) -> AppResult<Vec<String>> {
        let mut statement = conn.prepare(
            "SELECT id FROM feed_sources
             WHERE deleted_at IS NOT NULL AND purge_after IS NOT NULL AND purge_after <= ?1
             ORDER BY purge_after, id",
        )?;
        let ids = statement
            .query_map([rfc3339(now)], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    /// 将 RSS 回收站内容恢复，并按当前状态重新赋予 7/30 天保留期。
    pub fn restore_deleted_item(
        conn: &Connection,
        item_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<bool> {
        let now_str = rfc3339(now);
        let changed = conn.execute(
            "UPDATE feed_items
             SET deleted_at = NULL, purge_after = NULL,
                 deletion_reason = NULL,
                 expires_at = CASE
                    WHEN starred_at IS NOT NULL THEN NULL
                    WHEN archived_at IS NULL THEN ?2
                    ELSE ?3
                 END,
                 updated_at = ?1
             WHERE id = ?4 AND deleted_at IS NOT NULL
               AND purge_after IS NOT NULL AND purge_after > ?1
               AND COALESCE(deletion_reason, 'retention') != 'source_removed'",
            params![
                now_str,
                rfc3339(now + chrono::Duration::days(7)),
                rfc3339(now + chrono::Duration::days(30)),
                item_id
            ],
        )?;
        Ok(changed > 0)
    }

    /// 原子认领待抓取正文；状态先改为 `fetching`，并发 worker 不会重复处理。
    /// 返回值仅含内部 ID、来源 ID 与已规范化 HTTPS 地址。
    pub fn claim_pending_fulltext(
        conn: &Connection,
        limit: u32,
        now: DateTime<Utc>,
    ) -> AppResult<Vec<(String, String, String, String)>> {
        let tx = conn.unchecked_transaction()?;
        let mut statement = tx.prepare(
            "SELECT i.id, i.source_id, i.canonical_url, i.title
             FROM feed_items i JOIN feed_sources s ON s.id = i.source_id
             WHERE i.deleted_at IS NULL AND i.fulltext_status = 'pending'
               AND s.fulltext_enabled = 1 AND s.deleted_at IS NULL
               AND i.canonical_url IS NOT NULL
             ORDER BY i.updated_at DESC, i.row_id ASC
             LIMIT ?1",
        )?;
        let candidates = statement
            .query_map([i64::from(limit.clamp(1, 2))], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<Result<Vec<(String, String, String, String)>, _>>()?;
        drop(statement);
        let now_str = rfc3339(now);
        for (item_id, _, _, _) in &candidates {
            tx.execute(
                "UPDATE feed_items SET fulltext_status = 'fetching', updated_at = ?1
                 WHERE id = ?2 AND deleted_at IS NULL AND fulltext_status = 'pending'",
                params![now_str, item_id],
            )?;
        }
        tx.commit()?;
        Ok(candidates)
    }

    /// 写入已提取的正文；原 RSS 内容和源载荷保持不变，以便安全降级与审计。
    /// `content_text` 是 FTS 的单一文本投影，因此改为正文纯文本，不再复制一份。
    pub(crate) fn store_fulltext(
        conn: &Connection,
        item_id: &str,
        markdown: &str,
        text: &str,
        extraction_version: i64,
        primary_document: Option<&crate::feed::fulltext::ExtractedPrimaryDocument>,
        now: DateTime<Utc>,
    ) -> AppResult<bool> {
        let changed = conn.execute(
            "UPDATE feed_items
             SET fulltext_markdown = ?1, content_text = ?2, content_origin = 'web',
                 fulltext_status = 'ready', fulltext_extraction_version = ?3,
                 primary_document_kind = ?4, primary_document_url = ?5,
                 images_authorized_at = NULL, updated_at = ?6
             WHERE id = ?7 AND deleted_at IS NULL AND fulltext_status = 'fetching'",
            params![
                markdown,
                text,
                extraction_version,
                primary_document.map(|document| document.kind),
                primary_document.map(|document| document.url.as_str()),
                rfc3339(now),
                item_id
            ],
        )?;
        Ok(changed > 0)
    }

    /// 正文抓取失败只记录稳定状态，不保存底层错误或 URL；阅读器继续展示摘要。
    pub fn fail_fulltext(conn: &Connection, item_id: &str, now: DateTime<Utc>) -> AppResult<bool> {
        let changed = conn.execute(
            "UPDATE feed_items
             SET fulltext_status = 'failed',
                 fulltext_markdown = CASE
                     WHEN fulltext_extraction_version < ?1 THEN NULL ELSE fulltext_markdown END,
                 content_markdown = CASE
                     WHEN fulltext_extraction_version < ?1 THEN summary_markdown ELSE content_markdown END,
                 content_origin = CASE
                     WHEN fulltext_extraction_version < ?1 THEN 'feed' ELSE content_origin END,
                 content_text = CASE
                     WHEN fulltext_extraction_version < ?1 THEN summary_markdown ELSE content_text END,
                 primary_document_kind = CASE
                     WHEN fulltext_extraction_version < ?1 THEN NULL ELSE primary_document_kind END,
                 primary_document_url = CASE
                     WHEN fulltext_extraction_version < ?1 THEN NULL ELSE primary_document_url END,
                 images_authorized_at = NULL,
                 updated_at = ?2
             WHERE id = ?3 AND deleted_at IS NULL AND fulltext_status = 'fetching'",
            params![
                crate::feed::fulltext::FULLTEXT_EXTRACTION_VERSION,
                rfc3339(now),
                item_id
            ],
        )?;
        Ok(changed > 0)
    }

    /// 管理中心的 RSS 回收站列表；与 Markdown 回收站完全隔离。
    pub fn list_deleted_items(conn: &Connection, limit: u32) -> AppResult<Vec<FeedTrashItem>> {
        let select = ITEM_SUMMARY_SELECT.replacen(
            " FROM feed_items",
            " , i.deleted_at, i.purge_after FROM feed_items",
            1,
        );
        let mut statement = conn.prepare(&format!(
            "{select} WHERE i.deleted_at IS NOT NULL
               AND COALESCE(i.deletion_reason, 'retention') != 'source_removed'
             ORDER BY i.deleted_at DESC, i.row_id DESC LIMIT ?1"
        ))?;
        let rows = statement
            .query_map([i64::from(limit.clamp(1, ITEM_LIMIT_MAX))], |row| {
                Ok(FeedTrashItem {
                    item: map_item_summary(row)?,
                    deleted_at: row.get(15)?,
                    purge_after: row.get(16)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn trash_snapshot(conn: &Connection, limit: u32) -> AppResult<FeedTrashSnapshot> {
        let mut statement = conn.prepare(
            "SELECT s.id, COALESCE(s.title_override, s.title),
                    COUNT(i.id), SUM(CASE WHEN i.starred_at IS NOT NULL THEN 1 ELSE 0 END),
                    s.deleted_at, s.purge_after
             FROM feed_sources s
             LEFT JOIN feed_items i ON i.source_id = s.id AND i.deletion_reason = 'source_removed'
             WHERE s.deleted_at IS NOT NULL
             GROUP BY s.id
             ORDER BY s.deleted_at DESC LIMIT ?1",
        )?;
        let sources = statement
            .query_map([i64::from(limit.clamp(1, 200))], |row| {
                Ok(FeedTrashSource {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    item_count: row.get(2)?,
                    starred_count: row.get(3)?,
                    deleted_at: row.get(4)?,
                    purge_after: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(FeedTrashSnapshot {
            sources,
            items: Self::list_deleted_items(conn, limit)?,
        })
    }

    pub fn trash_source(
        conn: &Connection,
        source_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<Option<i64>> {
        let tx = conn.unchecked_transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM feed_sources WHERE id = ?1 AND deleted_at IS NULL)",
            [source_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Ok(None);
        }
        let deleted_at = rfc3339(now);
        let purge_after = rfc3339(now + chrono::Duration::days(30));
        let item_count = tx.execute(
            "UPDATE feed_items
             SET deleted_at = ?1, purge_after = ?2, deletion_reason = 'source_removed',
                 images_authorized_at = NULL,
                 fulltext_status = CASE WHEN fulltext_status IN ('pending', 'fetching')
                                        THEN 'not_requested' ELSE fulltext_status END,
                 updated_at = ?1
             WHERE source_id = ?3 AND deleted_at IS NULL",
            params![deleted_at, purge_after, source_id],
        )?;
        tx.execute(
            "UPDATE feed_sources
             SET is_enabled = 0, deleted_at = ?1, purge_after = ?2, updated_at = ?1
             WHERE id = ?3 AND deleted_at IS NULL",
            params![deleted_at, purge_after, source_id],
        )?;
        tx.commit()?;
        Ok(Some(item_count as i64))
    }

    pub fn restore_source(
        conn: &Connection,
        source_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<bool> {
        let tx = conn.unchecked_transaction()?;
        let restored = tx.execute(
            "UPDATE feed_sources
             SET deleted_at = NULL, purge_after = NULL, is_enabled = 0, updated_at = ?1
             WHERE id = ?2 AND deleted_at IS NOT NULL
               AND purge_after IS NOT NULL AND purge_after > ?1",
            params![rfc3339(now), source_id],
        )?;
        if restored == 0 {
            return Ok(false);
        }
        tx.execute(
            "UPDATE feed_items
             SET deleted_at = NULL, purge_after = NULL, deletion_reason = NULL,
                 expires_at = CASE WHEN starred_at IS NOT NULL THEN NULL
                                   WHEN archived_at IS NOT NULL THEN ?2 ELSE ?3 END,
                 updated_at = ?1
             WHERE source_id = ?4 AND deletion_reason = 'source_removed'",
            params![
                rfc3339(now),
                rfc3339(now + chrono::Duration::days(30)),
                rfc3339(now + chrono::Duration::days(7)),
                source_id
            ],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn purge_source(conn: &Connection, source_id: &str) -> AppResult<Option<i64>> {
        let item_count = conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM feed_items WHERE source_id = s.id)
                 FROM feed_sources s WHERE s.id = ?1 AND s.deleted_at IS NOT NULL",
                [source_id],
                |row| row.get(0),
            )
            .optional()?;
        if item_count.is_some() {
            conn.execute(
                "DELETE FROM feed_sources WHERE id = ?1 AND deleted_at IS NOT NULL",
                [source_id],
            )?;
        }
        Ok(item_count)
    }

    /// 清空 RSS 回收站；仅由显式用户操作调用。
    pub fn clear_deleted_items(conn: &Connection) -> AppResult<u32> {
        let changed = conn.execute(
            "DELETE FROM feed_items
             WHERE deleted_at IS NOT NULL
               AND COALESCE(deletion_reason, 'retention') != 'source_removed'",
            [],
        )?;
        Ok(changed as u32)
    }

    /// 进程中断时遗留的 `fetching` 工作重新入队，不保留半成品正文。
    pub fn recover_interrupted_fulltext(conn: &Connection) -> AppResult<u32> {
        let changed = conn.execute(
            "UPDATE feed_items
             SET fulltext_status = CASE
                 WHEN EXISTS (
                    SELECT 1 FROM feed_sources s
                    WHERE s.id = feed_items.source_id AND s.fulltext_enabled = 1
                      AND s.deleted_at IS NULL
                 ) THEN 'pending'
                 ELSE 'not_requested'
             END
             WHERE deleted_at IS NULL AND fulltext_status = 'fetching'",
            [],
        )?;
        Ok(changed as u32)
    }

    /// 将用户刚打开的单篇摘要加入正文补全队列。不会扫描来源历史，且重复
    /// 请求复用既有状态；返回稳定结果供阅读器展示，而不暴露数据库细节。
    pub fn enqueue_item_fulltext(
        conn: &Connection,
        item_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<crate::feed::model::FeedFulltextEnqueueOutcome> {
        use crate::feed::model::FeedFulltextEnqueueOutcome;

        let row = conn
            .query_row(
                "SELECT i.fulltext_status, i.deleted_at IS NULL, s.fulltext_enabled = 1,
                        i.canonical_url IS NOT NULL,
                        i.content_markdown = i.summary_markdown,
                        s.deleted_at IS NULL
                 FROM feed_items i JOIN feed_sources s ON s.id = i.source_id
                 WHERE i.id = ?1",
                [item_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, bool>(1)?,
                        row.get::<_, bool>(2)?,
                        row.get::<_, bool>(3)?,
                        row.get::<_, bool>(4)?,
                        row.get::<_, bool>(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((status, retained, enabled, has_url, summary_only, source_active)) = row else {
            return Err(AppError::msg("feed_item_not_found"));
        };
        let needs_refresh: bool = conn.query_row(
            "SELECT content_origin = 'web' AND fulltext_extraction_version < ?1
             FROM feed_items WHERE id = ?2",
            params![crate::feed::fulltext::FULLTEXT_EXTRACTION_VERSION, item_id],
            |row| row.get(0),
        )?;
        if status == "ready" && !needs_refresh {
            return Ok(FeedFulltextEnqueueOutcome::AlreadyReady);
        }
        if status == "pending" || status == "fetching" {
            return Ok(FeedFulltextEnqueueOutcome::AlreadyQueued);
        }
        if !retained || !enabled || !has_url || (!summary_only && !needs_refresh) || !source_active
        {
            return Ok(FeedFulltextEnqueueOutcome::NotEligible);
        }
        let changed = conn.execute(
            "UPDATE feed_items
             SET fulltext_status = 'pending', updated_at = ?1
             WHERE id = ?2 AND deleted_at IS NULL
               AND (fulltext_status IN ('not_requested', 'failed')
                    OR (fulltext_status = 'ready' AND fulltext_extraction_version < ?3))",
            params![
                rfc3339(now),
                item_id,
                crate::feed::fulltext::FULLTEXT_EXTRACTION_VERSION
            ],
        )?;
        Ok(if changed > 0 {
            FeedFulltextEnqueueOutcome::Queued
        } else {
            FeedFulltextEnqueueOutcome::AlreadyQueued
        })
    }

    /// 用户显式请求的资料库空间收缩；后台维护不会调用 VACUUM。
    pub fn vacuum_feed_library(conn: &Connection) -> AppResult<()> {
        conn.execute_batch("PRAGMA optimize; VACUUM;")?;
        Ok(())
    }

    // ── 搜索 ────────────────────────────────────────────────

    pub fn search(
        conn: &Connection,
        query: &str,
        source_id: Option<&str>,
        limit: u32,
    ) -> AppResult<Vec<FeedItemSummary>> {
        if query.trim().is_empty() {
            return Err(AppError::msg("feed_search_query_empty"));
        }
        Self::list_items(
            conn,
            &FeedItemQuery {
                view: crate::feed::model::FeedView::All,
                source_id: source_id.map(ToOwned::to_owned),
                search: Some(query.to_string()),
                received_after: None,
                cursor: None,
                limit,
            },
            Utc::now(),
        )
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
                    updated_at, history_boundary_external_key, history_boundary_published_at,
                    fulltext_enabled
             FROM feed_sources
             WHERE deleted_at IS NULL AND is_enabled = 1
               AND (next_fetch_at IS NULL OR next_fetch_at <= ?1)
             ORDER BY next_fetch_at IS NULL DESC, next_fetch_at ASC
             LIMIT ?2",
        )?;
        let sources = statement
            .query_map(params![now, limit], map_source_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(sources)
    }

    /// 返回全部启用源，供用户明确触发的全量同步使用。
    pub fn list_enabled_sources(conn: &Connection) -> AppResult<Vec<FeedSource>> {
        let mut statement = conn.prepare(&format!(
            "{SOURCE_SELECT} WHERE deleted_at IS NULL AND is_enabled = 1 ORDER BY created_at, id"
        ))?;
        let sources = statement
            .query_map([], map_source_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(sources)
    }
}
