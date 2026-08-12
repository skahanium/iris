//! 订阅资料库容量、升级与故障回归（阶段 5.3）。
//!
//! - 100 个订阅源 / 10,000 篇文章：inbox 首屏、FTS 查询与详情读取正确；
//!   不保存机器相关的毫秒硬阈值。
//! - `EXPLAIN QUERY PLAN` 断言 inbox 与 source 列表走既有索引；只有查询
//!   确实全表扫描时才调整索引。
//! - 从加入 RSS 前的应用数据库副本启动：`063` 自动应用且现有笔记索引、
//!   AI 会话与设置不变。

use chrono::Utc;
use iris_lib::app::AppState;
use iris_lib::feed::model::{
    ConversionStatus, FeedItemInput, FeedItemQuery, FeedView, NewFeedSource, SourcePayloadKind,
};
use iris_lib::feed::repository::FeedRepository;
use iris_lib::storage::migrate::migrate_up;
use rusqlite::Connection;
use tempfile::tempdir;

/// 以文件库构造测试状态（与真实启动同路径：自动迁移到最新 schema）。
fn test_state() -> (tempfile::TempDir, std::sync::Arc<AppState>) {
    let dir = tempdir().expect("tempdir");
    let state = AppState::new(dir.path().join("data")).expect("app state");
    (dir, state)
}

fn seed_sources(state: &AppState, count: usize) -> Vec<String> {
    state
        .db
        .with_conn(|conn| {
            let mut ids = Vec::new();
            for i in 0..count {
                let id = format!("src-{i:03}");
                FeedRepository::create_source(
                    conn,
                    &NewFeedSource {
                        id: id.clone(),
                        feed_url: format!("https://example.com/feeds/{i:03}.xml"),
                        site_url: Some(format!("https://example.com/site/{i:03}")),
                        title: format!("订阅源 {i:03}"),
                        title_override: None,
                        description: None,
                        icon_url: None,
                        language: None,
                        folder_path: format!("容量/组{:02}", i % 10),
                        fetch_interval_minutes: 60,
                    },
                    Utc::now(),
                )
                .expect("create source");
                ids.push(id);
            }
            Ok(ids)
        })
        .expect("seed sources")
}

fn seed_items(state: &AppState, source_ids: &[String], per_source: usize) {
    let mut items = Vec::with_capacity(source_ids.len() * per_source);
    for (source_index, source_id) in source_ids.iter().enumerate() {
        for i in 0..per_source {
            let n = source_index * per_source + i;
            items.push(FeedItemInput {
                id: format!("item-{n:05}"),
                source_id: source_id.clone(),
                external_key: format!("key-{n:05}"),
                canonical_url: Some(format!("https://example.com/article/{n:05}")),
                title: format!("合成文章 {n:05}（容量回归专用）"),
                author_name: Some("容量测试作者".to_string()),
                published_at: Some("2026-08-01T08:00:00Z".to_string()),
                source_updated_at: None,
                received_at: format!("2026-08-{:02}T08:00:00Z", (n % 28) + 1),
                summary_markdown: format!("摘要 {n:05}"),
                content_markdown: format!("## 正文 {n:05}\n\n容量回归共用短语。"),
                content_text: format!("正文 {n:05} 容量回归共用短语"),
                source_payload: format!("<p>payload {n:05}</p>"),
                source_payload_kind: SourcePayloadKind::Html,
                content_hash: format!("hash-{n:05}"),
                conversion_version: 1,
                conversion_status: ConversionStatus::Ok,
            });
        }
    }
    state
        .db
        .with_conn(|conn| FeedRepository::upsert_items(conn, &items))
        .expect("seed items");
}

/// 返回查询计划文本（detail 列拼接），用于断言索引选择。
fn query_plan(conn: &Connection, sql: &str) -> String {
    let mut statement = conn
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .expect("explain");
    let mut plan = String::new();
    let rows = statement
        .query_map([], |row| row.get::<_, String>(3))
        .expect("plan rows");
    for row in rows {
        plan.push_str(&row.expect("plan row"));
        plan.push('\n');
    }
    plan
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get::<_, i64>(0),
    )
    .expect("table lookup")
        > 0
}

#[test]
fn hundred_sources_and_ten_thousand_items_stay_correct() {
    let (_dir, state) = test_state();
    let source_ids = seed_sources(&state, 100);
    seed_items(&state, &source_ids, 100);

    // 源列表：100 个、按 folder/title 稳定排序、未读数正确。
    let sources = state
        .db
        .with_read_conn(FeedRepository::list_sources)
        .expect("list sources");
    assert_eq!(sources.len(), 100);
    assert!(sources.iter().all(|source| source.unread_count == 100));
    // 输出按 folder_path 稳定排序（同组内按标题）。
    let mut last: Option<(&str, &str)> = None;
    for source in &sources {
        let key = (source.folder_path.as_str(), source.title.as_str());
        if let Some(prev) = last {
            assert!(prev <= key, "list_sources 未按 folder/title 稳定排序");
        }
        last = Some(key);
    }

    // inbox 首屏：50 条、全部未读、无重复。
    let page = state
        .db
        .with_read_conn(|conn| {
            FeedRepository::list_items(
                conn,
                &FeedItemQuery {
                    view: FeedView::Inbox,
                    source_id: None,
                    received_after: None,
                    cursor: None,
                    limit: 50,
                },
                Utc::now(),
            )
        })
        .expect("inbox page");
    assert_eq!(page.len(), 50);
    assert!(page.iter().all(|item| !item.is_read));
    let mut ids: Vec<&str> = page.iter().map(|item| item.id.as_str()).collect();
    ids.dedup();
    assert_eq!(ids.len(), 50, "keyset 游标页内无重复");

    // FTS：共用短语命中上限 200 条；精确标题命中 1 条。
    let broad = state
        .db
        .with_read_conn(|conn| FeedRepository::search(conn, "容量回归共用短语", None, 200))
        .expect("broad search");
    assert_eq!(
        broad.len(),
        200,
        "FTS 上限 200 且命中全部 10k 文章的共用短语"
    );
    let exact = state
        .db
        .with_read_conn(|conn| FeedRepository::search(conn, "合成文章 00042", None, 50))
        .expect("exact search");
    assert_eq!(exact.len(), 1);
    assert_eq!(exact[0].id, "item-00042");

    // 详情读取：正文与来源正确。
    let detail = state
        .db
        .with_read_conn(|conn| FeedRepository::get_item_detail(conn, "item-00042"))
        .expect("detail query")
        .expect("item exists");
    assert!(detail.content_markdown.contains("## 正文 00042"));
    assert_eq!(detail.summary.source_id, "src-000");
    assert!(detail.summary.excerpt.contains("00042"));

    // 单源筛选与批量已读按 source 正确作用。
    let affected = state
        .db
        .with_conn(|conn| {
            FeedRepository::mark_items_read(
                conn,
                &FeedItemQuery {
                    view: FeedView::Inbox,
                    source_id: Some("src-000".to_string()),
                    received_after: None,
                    cursor: None,
                    limit: 50,
                },
                Utc::now(),
            )
        })
        .expect("mark read");
    assert_eq!(affected, 100);
    let after = state
        .db
        .with_read_conn(|conn| FeedRepository::count_items(conn, "src-000"))
        .expect("count");
    assert_eq!(after, 100, "标记已读不删除文章");
}

#[test]
fn inbox_and_source_list_use_existing_indexes() {
    let (_dir, state) = test_state();
    let source_ids = seed_sources(&state, 100);
    seed_items(&state, &source_ids, 50);

    state
        .db
        .with_read_conn(|conn| {
            // inbox 首屏查询必须走 idx_feed_items_inbox。
            let inbox_plan = query_plan(
                conn,
                "SELECT i.row_id, i.id FROM feed_items i
             WHERE i.read_at IS NULL AND i.archived_at IS NULL
             ORDER BY i.received_at DESC, i.row_id DESC LIMIT 50",
            );
            assert!(
                inbox_plan.contains("idx_feed_items_inbox"),
                "inbox 查询未走 idx_feed_items_inbox：\n{inbox_plan}"
            );

            // source 列表（folder/title 排序）必须走 idx_feed_sources_folder。
            let source_plan = query_plan(
                conn,
                "SELECT s.id FROM feed_sources s ORDER BY s.folder_path, s.title",
            );
            assert!(
                source_plan.contains("idx_feed_sources_folder"),
                "source 列表未走 idx_feed_sources_folder：\n{source_plan}"
            );

            // 单源 inbox 筛选由优化器选择索引（inbox 或 source_time 均正确），
            // 但不得出现 feed_items 全表扫描。
            let scoped_plan = query_plan(
                conn,
                "SELECT i.row_id FROM feed_items i
             WHERE i.read_at IS NULL AND i.archived_at IS NULL AND i.source_id = 'src-001'
             ORDER BY i.received_at DESC, i.row_id DESC LIMIT 50",
            );
            assert!(
                !scoped_plan.contains("SCAN feed_items"),
                "单源 inbox 全表扫描：\n{scoped_plan}"
            );
            assert!(
                scoped_plan.contains("USING INDEX"),
                "单源 inbox 未使用任何索引：\n{scoped_plan}"
            );
            Ok(())
        })
        .expect("read plan");
}

#[test]
fn upgrade_from_pre_rss_library_applies_063_without_touching_existing_state() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("iris.db");
    let conn = Connection::open(&db_path).expect("open db");

    // 建立最新库后回滚 063，模拟「加入 RSS 前」的应用数据库副本：
    // 其余 62 个迁移与既有表全部保留，只有 feed 相关对象被移除。
    migrate_up(&conn).expect("migrate up");
    let down = include_str!("../migrations/063_feed_library.down.sql");
    conn.execute_batch(down).expect("rollback 063");
    conn.execute(
        "DELETE FROM _migrations WHERE name = '063_feed_library'",
        [],
    )
    .expect("unregister 063");
    assert!(!table_exists(&conn, "feed_sources"), "旧库无 feed_sources");
    assert!(table_exists(&conn, "files"), "旧库保留 files");

    // 旧数据：笔记索引、AI 会话、设置（应用状态代表）。
    conn.execute(
        "INSERT INTO files (path, title, content_hash, word_count, created_at, updated_at)
         VALUES ('notes/既有笔记.md', '既有笔记', 'h1', 12,
                 '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
        [],
    )
    .expect("seed file");
    conn.execute(
        "INSERT INTO sessions (session_key, retention_policy, created_at, updated_at)
         VALUES ('sess-1', 'user_clearable', '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
        [],
    )
    .expect("seed session");
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('theme', 'dark')",
        [],
    )
    .expect("seed setting");

    // 重新启动升级：063 自动应用。
    migrate_up(&conn).expect("re-upgrade");
    assert!(table_exists(&conn, "feed_sources"));
    assert!(table_exists(&conn, "feed_items"));
    assert!(table_exists(&conn, "feed_items_fts"));

    // 旧数据不变。
    let files: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM files WHERE path = 'notes/既有笔记.md'",
            [],
            |row| row.get(0),
        )
        .expect("count files");
    assert_eq!(files, 1, "笔记索引不被 063 触碰");
    let sessions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE session_key = 'sess-1'",
            [],
            |row| row.get(0),
        )
        .expect("count sessions");
    assert_eq!(sessions, 1, "AI 会话不被 063 触碰");
    let theme: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'theme'",
            [],
            |row| row.get(0),
        )
        .expect("theme setting");
    assert_eq!(theme, "dark", "设置不被 063 触碰");

    // 063 恰好注册一次（幂等）。
    let registered: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM _migrations WHERE name = '063_feed_library'",
            [],
            |row| row.get(0),
        )
        .expect("migration registry");
    assert_eq!(registered, 1);
}
