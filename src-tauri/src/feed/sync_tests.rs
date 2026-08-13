//! `sync_source` 契约测试：首次历史默认已读/可选未读、后续新条目未读、
//! 304、内容更新保状态、重复 GUID、失败原子性（事务回滚）、稳定错误码、
//! 固定退避。全部在内存 SQLite + 本地测试服务器上完成。

use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};

use super::sync::{sync_source, HistoryReadPolicy, SyncMode, SyncStatus};
use super::test_http::{TestNetGate, TestResponse, TestServer};
use crate::feed::model::{FeedItemStatePatch, FeedSourcePatch, NewFeedSource};
use crate::feed::repository::FeedRepository;
use crate::storage::db::Database;

fn rss2_fixture() -> &'static str {
    include_str!("../../tests/fixtures/feeds/rss2-basic.xml")
}

fn atom_v1_fixture() -> &'static str {
    include_str!("../../tests/fixtures/feeds/item-update-v1.xml")
}

fn atom_v2_fixture() -> &'static str {
    include_str!("../../tests/fixtures/feeds/item-update-v2.xml")
}

fn malformed_fixture() -> &'static str {
    include_str!("../../tests/fixtures/feeds/malformed.xml")
}

fn create_db() -> Database {
    Database::open_in_memory().expect("in-memory db")
}

fn insert_source(db: &Database, id: &str, feed_url: &str) {
    db.with_conn(|conn| {
        FeedRepository::create_source(
            conn,
            &NewFeedSource {
                id: id.to_string(),
                feed_url: feed_url.to_string(),
                site_url: None,
                title: "Example Feed".to_string(),
                title_override: None,
                description: None,
                icon_url: None,
                language: None,
                folder_path: String::new(),
                fetch_interval_minutes: 60,
            },
            Utc::now(),
        )
        .map(|_| ())
    })
    .expect("insert source");
}

async fn sync_ok(
    db: &Database,
    gate: &TestNetGate,
    source_id: &str,
    mode: SyncMode,
    history: HistoryReadPolicy,
) -> SyncStatus {
    sync_source(db, gate, source_id, mode, history)
        .await
        .expect("sync completes")
        .status
}

fn with_write_conn<T>(
    db: &Database,
    f: impl FnOnce(&rusqlite::Connection) -> crate::error::AppResult<T>,
) -> T {
    db.with_conn(f).expect("write conn")
}

fn item_id_by_key(db: &Database, source_id: &str, external_key: &str) -> String {
    with_write_conn(db, |conn| {
        Ok(conn.query_row(
            "SELECT id FROM feed_items WHERE source_id = ?1 AND external_key = ?2",
            rusqlite::params![source_id, external_key],
            |row| row.get(0),
        )?)
    })
}

fn item_detail(db: &Database, item_id: &str) -> crate::feed::model::FeedItemDetail {
    db.with_read_conn(|conn| FeedRepository::get_item_detail(conn, item_id))
        .expect("detail query")
        .expect("item exists")
}

fn source_state(db: &Database, id: &str) -> crate::feed::model::FeedSource {
    db.with_read_conn(|conn| FeedRepository::get_source(conn, id))
        .expect("source query")
        .expect("source exists")
}

fn item_count(db: &Database, source_id: &str) -> i64 {
    with_write_conn(db, |conn| FeedRepository::count_items(conn, source_id))
}

fn minutes_between(after: &str, before: &str) -> i64 {
    let parse = |value: &str| {
        DateTime::parse_from_rfc3339(value)
            .expect("rfc3339")
            .with_timezone(&Utc)
    };
    (parse(after) - parse(before)).num_minutes()
}

#[tokio::test]
async fn first_sync_marks_history_read_by_default() {
    let db = create_db();
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(200, rss2_fixture()).header("Content-Type", "application/rss+xml"),
    );
    insert_source(&db, "src-1", &server.url("/feed.xml"));

    let status = sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;
    match status {
        SyncStatus::Succeeded { new_items, .. } => assert_eq!(new_items, 3),
        other => panic!("expected success, got {other:?}"),
    }

    let first_id = item_id_by_key(&db, "src-1", "example-tech-blog-2026-08-01");
    let first = item_detail(&db, &first_id);
    assert!(first.summary.is_read, "首次同步历史项目默认已读");
    let (received, read): (String, Option<String>) = with_write_conn(&db, |conn| {
        Ok(conn.query_row(
            "SELECT received_at, read_at FROM feed_items WHERE id = ?1",
            [&first_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    });
    assert_eq!(
        read.as_deref(),
        Some(received.as_str()),
        "历史已读必须 read_at=received_at"
    );
}

#[tokio::test]
async fn new_source_queues_summary_only_feed_for_default_web_fulltext() {
    let db = create_db();
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(200, rss2_fixture()).header("Content-Type", "application/rss+xml"),
    );
    insert_source(&db, "src-1", &server.url("/feed.xml"));

    sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;
    let statuses: Vec<String> = db
        .with_read_conn(|conn| {
            let mut statement =
                conn.prepare("SELECT fulltext_status FROM feed_items WHERE source_id = 'src-1'")?;
            let rows = statement
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<String>, _>>()?;
            Ok(rows)
        })
        .expect("statuses");
    assert!(
        statuses.iter().all(|status| status == "pending"),
        "新来源的仅摘要文章应自动进入正文补全队列"
    );
}

#[tokio::test]
async fn successful_sync_updates_feed_metadata_without_overwriting_title_override() {
    let db = create_db();
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(200, rss2_fixture()).header("Content-Type", "application/rss+xml"),
    );
    insert_source(&db, "src-1", &server.url("/feed.xml"));
    with_write_conn(&db, |conn| {
        FeedRepository::update_source(
            conn,
            "src-1",
            &FeedSourcePatch {
                title_override: Some("My title".to_string()),
                ..Default::default()
            },
            Utc::now(),
        )
    });

    sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;

    let source = source_state(&db, "src-1");
    assert_eq!(source.title, "Example Tech Blog");
    assert_eq!(source.title_override.as_deref(), Some("My title"));
    assert_eq!(source.site_url.as_deref(), Some("https://example.com/blog"));
    assert_eq!(source.language.as_deref(), Some("en-us"));
    assert!(source
        .description
        .as_deref()
        .is_some_and(|v| v.contains("synthetic")));
}

#[tokio::test]
async fn first_sync_can_leave_history_unread() {
    let db = create_db();
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(200, rss2_fixture()).header("Content-Type", "application/rss+xml"),
    );
    insert_source(&db, "src-1", &server.url("/feed.xml"));

    sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::LeaveUnread,
    )
    .await;

    let first_id = item_id_by_key(&db, "src-1", "example-tech-blog-2026-08-01");
    let first = item_detail(&db, &first_id);
    assert!(!first.summary.is_read, "选择历史未读时收件箱应包含历史项目");
}

#[tokio::test]
async fn first_sync_keeps_only_the_latest_fifty_feed_entries() {
    let db = create_db();
    let server = TestServer::start().await;
    let entries = (1..=60)
        .map(|day| {
            format!(
                "<item><guid>entry-{day}</guid><title>Entry {day}</title><link>https://example.com/{day}</link><description>summary</description><pubDate>Wed, {day:02} Aug 2026 10:00:00 GMT</pubDate></item>"
            )
        })
        .collect::<String>();
    let feed = format!("<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>Large</title>{entries}</channel></rss>");
    server.queue(TestResponse::new(200, feed).header("Content-Type", "application/rss+xml"));
    insert_source(&db, "src-1", &server.url("/feed.xml"));

    let status = sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;

    assert!(matches!(
        status,
        SyncStatus::Succeeded {
            new_items: 50,
            skipped_history: 10
        }
    ));
    assert_eq!(item_count(&db, "src-1"), 50);
}

#[tokio::test]
async fn retained_history_boundary_prevents_old_feed_items_from_dripping_back_in() {
    let db = create_db();
    let server = TestServer::start().await;
    let feed = |start: u32, end: u32| {
        let entries = (start..=end)
            .rev()
            .map(|n| {
                let published = Utc
                    .with_ymd_and_hms(2026, 1, 1, 10, 0, 0)
                    .single()
                    .expect("date")
                    + chrono::Duration::days(i64::from(n));
                format!(
                    "<item><guid>entry-{n}</guid><title>Entry {n}</title><link>https://example.com/{n}</link><description>summary</description><pubDate>{}</pubDate></item>",
                    published.to_rfc2822()
                )
            })
            .collect::<String>();
        format!("<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>Large</title>{entries}</channel></rss>")
    };
    server.queue(TestResponse::new(200, feed(1, 60)).header("Content-Type", "application/rss+xml"));
    // 第二轮同时包含新条目 61–65 和所有旧历史；不能再渐进补入 1–10。
    server.queue(TestResponse::new(200, feed(1, 65)).header("Content-Type", "application/rss+xml"));
    insert_source(&db, "src-1", &server.url("/feed.xml"));

    sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;
    let boundary = source_state(&db, "src-1");
    assert_eq!(
        boundary.history_boundary_external_key.as_deref(),
        Some("entry-11")
    );

    let status = sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;
    assert!(matches!(status, SyncStatus::Succeeded { new_items: 5, .. }));
    assert_eq!(item_count(&db, "src-1"), 55);
    assert!(db
        .with_read_conn(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM feed_items WHERE source_id = 'src-1' AND external_key = 'entry-1'",
                [],
                |row| row.get::<_, i64>(0),
            )?)
        })
        .expect("count")
        == 0);
}

#[tokio::test]
async fn retained_history_boundary_keeps_new_undated_entries_before_the_boundary() {
    let db = create_db();
    let server = TestServer::start().await;
    let feed = |newest_undated: Option<&str>| {
        let entries = newest_undated
            .into_iter()
            .map(|id| {
                format!(
                    "<item><guid>{id}</guid><title>{id}</title><link>https://example.com/{id}</link><description>summary</description></item>"
                )
            })
            .chain((1..=60).rev().map(|n| {
                let published = Utc
                    .with_ymd_and_hms(2026, 1, 1, 10, 0, 0)
                    .single()
                    .expect("date")
                    + chrono::Duration::days(i64::from(n));
                format!(
                    "<item><guid>entry-{n}</guid><title>Entry {n}</title><link>https://example.com/{n}</link><description>summary</description><pubDate>{}</pubDate></item>",
                    published.to_rfc2822()
                )
            }))
            .collect::<String>();
        format!("<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>Large</title>{entries}</channel></rss>")
    };
    server.queue(TestResponse::new(200, feed(None)).header("Content-Type", "application/rss+xml"));
    server.queue(
        TestResponse::new(200, feed(Some("new-undated")))
            .header("Content-Type", "application/rss+xml"),
    );
    insert_source(&db, "src-1", &server.url("/feed.xml"));

    sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;
    let status = sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;

    assert!(matches!(status, SyncStatus::Succeeded { new_items: 1, .. }));
    assert_eq!(item_count(&db, "src-1"), 51);
    let id = item_id_by_key(&db, "src-1", "new-undated");
    assert!(!item_detail(&db, &id).summary.is_read);
}

#[tokio::test]
async fn subsequent_sync_new_items_are_unread_and_old_state_preserved() {
    let db = create_db();
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(200, rss2_fixture()).header("Content-Type", "application/rss+xml"),
    );
    // 第二次同步：追加一条新目的 feed。
    let second_feed = rss2_fixture().replace(
        "</channel>",
        "  <item>\n      <guid isPermaLink=\"false\">example-tech-blog-2026-08-04</guid>\n      <title>Fourth fixture post</title>\n      <link>https://example.com/blog/fourth</link>\n      <description>Fourth body.</description>\n      <pubDate>Tue, 04 Aug 2026 10:00:00 GMT</pubDate>\n    </item>\n  </channel>",
    );
    server.queue(TestResponse::new(200, second_feed).header("Content-Type", "application/rss+xml"));
    insert_source(&db, "src-1", &server.url("/feed.xml"));

    sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;

    // 用户把第一条标为未读。
    let first_id = item_id_by_key(&db, "src-1", "example-tech-blog-2026-08-01");
    with_write_conn(&db, |conn| {
        FeedRepository::set_item_state(
            conn,
            &first_id,
            &FeedItemStatePatch {
                is_read: Some(false),
                ..Default::default()
            },
            Utc::now(),
        )
    });

    let status = sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;
    match status {
        SyncStatus::Succeeded { new_items, .. } => assert_eq!(new_items, 1, "只有新条目计入"),
        other => panic!("expected success, got {other:?}"),
    }

    let fourth_id = item_id_by_key(&db, "src-1", "example-tech-blog-2026-08-04");
    let fourth = item_detail(&db, &fourth_id);
    assert!(!fourth.summary.is_read, "后续新条目保持未读");

    let first = item_detail(&db, &first_id);
    assert!(!first.summary.is_read, "旧条目状态不被覆盖");
}

#[tokio::test]
async fn not_modified_keeps_items_and_validators() {
    let db = create_db();
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(200, rss2_fixture())
            .header("Content-Type", "application/rss+xml")
            .header("ETag", "\"v1\""),
    );
    server.queue(
        TestResponse::new(304, "")
            .header("ETag", "\"v1\"")
            .header("Last-Modified", "Wed, 12 Aug 2026 08:00:00 GMT"),
    );
    insert_source(&db, "src-1", &server.url("/feed.xml"));

    sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;
    let after_first = source_state(&db, "src-1");
    assert_eq!(after_first.etag.as_deref(), Some("\"v1\""));
    assert_eq!(after_first.consecutive_failures, 0);

    let status = sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;
    assert!(matches!(status, SyncStatus::NotModified), "got {status:?}");

    let after_304 = source_state(&db, "src-1");
    assert_eq!(after_304.etag.as_deref(), Some("\"v1\""), "validators 保留");
    assert_eq!(
        after_304.last_modified.as_deref(),
        Some("Wed, 12 Aug 2026 08:00:00 GMT"),
        "304 响应头中的 Last-Modified 吸收"
    );
    assert_eq!(after_304.consecutive_failures, 0, "304 视为成功");
    assert!(after_304.last_success_at.is_some());
    assert!(
        after_304.next_fetch_at.as_deref().unwrap() > after_304.last_checked_at.as_deref().unwrap()
    );
    assert_eq!(item_count(&db, "src-1"), 3, "304 不触碰文章");
}

#[tokio::test]
async fn content_update_preserves_read_state_and_received_at() {
    let db = create_db();
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(200, atom_v1_fixture()).header("Content-Type", "application/atom+xml"),
    );
    server.queue(
        TestResponse::new(200, atom_v2_fixture()).header("Content-Type", "application/atom+xml"),
    );
    insert_source(&db, "src-1", &server.url("/feed.xml"));

    sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;
    let target_id = item_id_by_key(&db, "src-1", "https://example.com/updates/stable-id");
    let before = item_detail(&db, &target_id);
    assert!(before.summary.is_read);

    let status = sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;
    assert!(
        matches!(status, SyncStatus::Succeeded { new_items: 0, .. }),
        "更新不产生新条目；got {status:?}"
    );

    let after = item_detail(&db, &target_id);
    assert!(after.content_markdown.contains("v2 body"), "正文已更新");
    assert!(after.summary.is_read, "阅读状态不被内容更新覆盖");
    assert_eq!(
        after.summary.received_at, before.summary.received_at,
        "received_at 不可变"
    );
}

#[tokio::test]
async fn duplicate_guid_dedupes_within_batch() {
    let db = create_db();
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(
            200,
            include_str!("../../tests/fixtures/feeds/duplicate-guid.xml"),
        )
        .header("Content-Type", "application/rss+xml"),
    );
    insert_source(&db, "src-1", &server.url("/feed.xml"));

    let status = sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;
    match status {
        SyncStatus::Succeeded { new_items, .. } => {
            assert_eq!(new_items, 2, "重复 GUID 只入库一条")
        }
        other => panic!("expected success, got {other:?}"),
    }
    assert_eq!(item_count(&db, "src-1"), 2);
}

#[tokio::test]
async fn fetch_failure_is_atomic_and_records_backoff() {
    let db = create_db();
    let server = TestServer::start().await;
    // 第一次成功，第二次 500。
    server.queue(
        TestResponse::new(200, rss2_fixture())
            .header("Content-Type", "application/rss+xml")
            .header("ETag", "\"keep-me\""),
    );
    server.queue(TestResponse::new(500, "boom"));
    insert_source(&db, "src-1", &server.url("/feed.xml"));

    sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;
    let items_before = item_count(&db, "src-1");

    let status = sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;
    match status {
        SyncStatus::Failed { code } => assert_eq!(code, "feed_http_error_500"),
        other => panic!("expected failed, got {other:?}"),
    }

    let failed = source_state(&db, "src-1");
    assert_eq!(failed.consecutive_failures, 1);
    assert_eq!(
        failed.last_error_code.as_deref(),
        Some("feed_http_error_500")
    );
    assert!(failed.last_error_at.is_some());
    assert_eq!(
        failed.etag.as_deref(),
        Some("\"keep-me\""),
        "失败保留旧 validators"
    );
    assert_eq!(
        item_count(&db, "src-1"),
        items_before,
        "失败不触碰已有文章（原子性）"
    );
    let delta = minutes_between(
        failed.next_fetch_at.as_deref().expect("next fetch"),
        failed.last_checked_at.as_deref().expect("checked"),
    );
    assert!(
        (14..=16).contains(&delta),
        "首次失败退避 15 分钟；got {delta}"
    );
}

#[tokio::test]
async fn parse_failure_records_stable_code() {
    let db = create_db();
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(200, malformed_fixture()).header("Content-Type", "application/rss+xml"),
    );
    insert_source(&db, "src-1", &server.url("/feed.xml"));

    let status = sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;
    match status {
        SyncStatus::Failed { code } => assert_eq!(code, "feed_parse_failed"),
        other => panic!("expected failed, got {other:?}"),
    }
}

#[tokio::test]
async fn backoff_escalates_15m_1h_6h_24h() {
    let db = create_db();
    let server = TestServer::start().await;
    for _ in 0..4 {
        server.queue(TestResponse::new(500, "boom"));
    }
    insert_source(&db, "src-1", &server.url("/feed.xml"));

    let gate = TestNetGate::default();
    let mut expected = [15, 60, 360, 1440];
    for (index, expect_minutes) in expected.iter_mut().enumerate() {
        let status = sync_ok(
            &db,
            &gate,
            "src-1",
            SyncMode::Manual,
            HistoryReadPolicy::MarkRead,
        )
        .await;
        assert!(matches!(status, SyncStatus::Failed { .. }));
        let failed = source_state(&db, "src-1");
        assert_eq!(failed.consecutive_failures, (index + 1) as i64);
        let delta = minutes_between(
            failed.next_fetch_at.as_deref().expect("next fetch"),
            failed.last_checked_at.as_deref().expect("checked"),
        );
        assert!(
            (*expect_minutes - 1..=*expect_minutes + 1).contains(&delta),
            "第 {} 次失败退避 {expect_minutes} 分钟；got {delta}",
            index + 1
        );
    }
}

#[tokio::test]
async fn automatic_mode_skips_disabled_source_but_manual_syncs() {
    let db = create_db();
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(200, rss2_fixture()).header("Content-Type", "application/rss+xml"),
    );
    insert_source(&db, "src-1", &server.url("/feed.xml"));
    // 暂停源。
    with_write_conn(&db, |conn| {
        FeedRepository::update_source(
            conn,
            "src-1",
            &FeedSourcePatch {
                is_enabled: Some(false),
                ..Default::default()
            },
            Utc::now(),
        )
    });

    let status = sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Automatic,
        HistoryReadPolicy::MarkRead,
    )
    .await;
    assert!(matches!(status, SyncStatus::Skipped), "自动同步跳过暂停源");
    assert_eq!(item_count(&db, "src-1"), 0);

    let status = sync_ok(
        &db,
        &TestNetGate::default(),
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;
    assert!(
        matches!(status, SyncStatus::Succeeded { .. }),
        "手动刷新可同步暂停源；got {status:?}"
    );
}

#[tokio::test]
async fn missing_source_is_hard_error() {
    let db = create_db();
    let error = sync_source(
        &db,
        &TestNetGate::default(),
        "missing",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await
    .expect_err("missing source must error");
    assert!(
        error.to_string().contains("feed_source_not_found"),
        "got: {error}"
    );
}

#[tokio::test]
async fn sync_waits_for_slow_response_within_timeout() {
    let db = create_db();
    let server = TestServer::start_with_delay(400).await;
    server.queue(
        TestResponse::new(200, rss2_fixture()).header("Content-Type", "application/rss+xml"),
    );
    insert_source(&db, "src-1", &server.url("/feed.xml"));

    let status = sync_ok(
        &db,
        &TestNetGate {
            timeout: Duration::from_secs(5),
        },
        "src-1",
        SyncMode::Manual,
        HistoryReadPolicy::MarkRead,
    )
    .await;
    assert!(
        matches!(status, SyncStatus::Succeeded { .. }),
        "got {status:?}"
    );
}

// ── FeedSyncService（Task 2.6）─────────────────────────────

use std::sync::Arc;

use super::sync::FeedSyncService;

#[tokio::test]
async fn same_source_cannot_sync_concurrently() {
    let db = Arc::new(create_db());
    let server = TestServer::start_with_delay(800).await;
    server.queue(
        TestResponse::new(200, rss2_fixture()).header("Content-Type", "application/rss+xml"),
    );
    insert_source(&db, "src-1", &server.url("/feed.xml"));

    let service = Arc::new(FeedSyncService::new(
        db.clone(),
        Arc::new(TestNetGate::default()),
    ));

    let (a, b) = tokio::join!(
        service.sync_source("src-1", SyncMode::Manual),
        service.sync_source("src-1", SyncMode::Manual),
    );
    let outcomes = [a.expect("a"), b.expect("b")];
    let succeeded = outcomes
        .iter()
        .filter(|o| matches!(o.status, SyncStatus::Succeeded { .. }))
        .count();
    let in_flight = outcomes
        .iter()
        .filter(|o| matches!(o.status, SyncStatus::InFlight))
        .count();
    assert_eq!(succeeded, 1, "恰好一次同步成功");
    assert_eq!(in_flight, 1, "并发请求返回 InFlight");
    assert_eq!(server.requests_snapshot().len(), 1, "服务器只收到一次请求");
}

#[tokio::test]
async fn failure_releases_inflight_marker() {
    let db = Arc::new(create_db());
    let server = TestServer::start().await;
    server.queue(TestResponse::new(500, "boom"));
    server.queue(
        TestResponse::new(200, rss2_fixture()).header("Content-Type", "application/rss+xml"),
    );
    insert_source(&db, "src-1", &server.url("/feed.xml"));

    let service = FeedSyncService::new(db.clone(), Arc::new(TestNetGate::default()));

    let first = service
        .sync_source("src-1", SyncMode::Manual)
        .await
        .expect("first sync");
    assert!(
        matches!(first.status, SyncStatus::Failed { .. }),
        "got {:?}",
        first.status
    );

    let second = service
        .sync_source("src-1", SyncMode::Manual)
        .await
        .expect("second sync");
    assert!(
        matches!(second.status, SyncStatus::Succeeded { .. }),
        "失败后互斥标记必须释放；got {:?}",
        second.status
    );
}

#[tokio::test]
async fn cancelled_sync_releases_inflight_marker() {
    let db = Arc::new(create_db());
    let server = TestServer::start_with_delay(150).await;
    server.queue(
        TestResponse::new(200, rss2_fixture()).header("Content-Type", "application/rss+xml"),
    );
    server.queue(
        TestResponse::new(200, rss2_fixture()).header("Content-Type", "application/rss+xml"),
    );
    insert_source(&db, "src-1", &server.url("/feed.xml"));
    let service = FeedSyncService::new(db, Arc::new(TestNetGate::default()));

    let timed_out = tokio::time::timeout(
        Duration::from_millis(20),
        service.sync_source("src-1", SyncMode::Manual),
    )
    .await;
    assert!(
        timed_out.is_err(),
        "first future must be cancelled by timeout"
    );
    tokio::time::sleep(Duration::from_millis(180)).await;

    let retry = service
        .sync_source("src-1", SyncMode::Manual)
        .await
        .expect("retry");
    assert!(
        matches!(retry.status, SyncStatus::Succeeded { .. }),
        "取消 future 后不能永久卡在 InFlight"
    );
}

#[tokio::test]
async fn sync_due_batch_fetches_at_most_two_due_sources_concurrently() {
    let db = Arc::new(create_db());
    let server = TestServer::start_with_delay(200).await;
    // 5 个到期源 → 每轮最多 2 个。
    for index in 0..5 {
        insert_source(
            &db,
            &format!("src-{index}"),
            &server.url(&format!("/feed{index}.xml")),
        );
        server.queue(
            TestResponse::new(200, rss2_fixture()).header("Content-Type", "application/rss+xml"),
        );
    }
    let service = FeedSyncService::new(db.clone(), Arc::new(TestNetGate::default()));

    service.sync_due_batch().await.expect("sync due batch");

    assert_eq!(server.requests_snapshot().len(), 2, "每轮最多取 2 个到期源");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let remaining: Vec<String> = db
        .with_read_conn(|conn| {
            Ok(FeedRepository::list_due_sources(conn, &now, 10)?
                .iter()
                .map(|source| source.id.clone())
                .collect())
        })
        .expect("list due");
    assert_eq!(remaining.len(), 3, "其余到期源留到下一轮");
}

#[tokio::test]
async fn sync_due_batch_skips_paused_sources() {
    let db = Arc::new(create_db());
    let server = TestServer::start().await;
    insert_source(&db, "src-1", &server.url("/feed.xml"));
    with_write_conn(&db, |conn| {
        FeedRepository::update_source(
            conn,
            "src-1",
            &FeedSourcePatch {
                is_enabled: Some(false),
                ..Default::default()
            },
            Utc::now(),
        )
    });
    // 暂停源不会出现在 due 查询中，也不会发起任何请求。
    let service = FeedSyncService::new(db.clone(), Arc::new(TestNetGate::default()));
    service.sync_due_batch().await.expect("sync due batch");
    assert!(
        server.requests_snapshot().is_empty(),
        "暂停源不得触发任何请求"
    );
}

#[tokio::test]
async fn manual_sync_all_fetches_every_enabled_source_with_concurrency_two() {
    let db = Arc::new(create_db());
    let server = TestServer::start_with_delay(80).await;
    for index in 0..5 {
        insert_source(
            &db,
            &format!("src-{index}"),
            &server.url(&format!("/feed{index}.xml")),
        );
        server.queue(
            TestResponse::new(200, rss2_fixture()).header("Content-Type", "application/rss+xml"),
        );
    }
    let service = FeedSyncService::new(db, Arc::new(TestNetGate::default()));
    let started = std::time::Instant::now();
    let outcome = service.sync_all().await.expect("manual sync all");

    assert_eq!(server.requests_snapshot().len(), 5);
    assert_eq!(outcome.total, 5);
    assert_eq!(outcome.succeeded, 5);
    assert_eq!(outcome.failed, 0);
    assert!(
        started.elapsed() >= Duration::from_millis(200),
        "5 个请求按宽度 2 分三批执行"
    );
}
