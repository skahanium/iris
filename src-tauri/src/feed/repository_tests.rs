//! `FeedRepository` 契约测试（内存 SQLite，无网络、无 UI、无 Vault 写入）。
//!
//! 覆盖：source CRUD、收件箱派生、三个状态轴独立、keyset cursor 稳定、
//! source cascade、FTS 同步更新、详情 DTO 无 `source_payload`。

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::Connection;

use super::model::{
    ConversionStatus, FeedItemInput, FeedItemQuery, FeedItemStatePatch, FeedSourcePatch, FeedView,
    FulltextStatus, NewFeedSource, SourcePayloadKind,
};
use super::repository::{today_start_utc, FeedRepository};
use crate::storage::migrate::migrate_up;

fn test_conn() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON")
        .expect("enable foreign keys");
    migrate_up(&conn).expect("migrate up");
    conn
}

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-08-10T10:00:00Z")
        .expect("fixed now")
        .with_timezone(&Utc)
}

fn rfc3339(value: &str) -> String {
    DateTime::parse_from_rfc3339(value)
        .expect("valid timestamp")
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// 直接以 SQL 建立订阅源 fixture（被测试对象是仓储，不是 SQL 本身）。
fn insert_source(conn: &Connection, id: &str, title: &str, feed_url: &str) {
    conn.execute(
        "INSERT INTO feed_sources (id, feed_url, title, fulltext_enabled, created_at, updated_at)
         VALUES (?1, ?2, ?3, 1, '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
        rusqlite::params![id, feed_url, title],
    )
    .expect("insert source fixture");
}

fn item_input(
    source_id: &str,
    external_key: &str,
    title: &str,
    body: &str,
    received_at: &str,
) -> FeedItemInput {
    FeedItemInput {
        id: format!("item-{source_id}-{external_key}"),
        source_id: source_id.to_string(),
        external_key: external_key.to_string(),
        canonical_url: Some(format!("https://example.com/{external_key}")),
        title: title.to_string(),
        author_name: Some("Fixture Author".to_string()),
        published_at: Some("2026-07-01T08:00:00Z".to_string()),
        source_updated_at: None,
        received_at: rfc3339(received_at),
        summary_markdown: format!("*{title}*"),
        content_markdown: format!("## {title}\n\n{body}"),
        content_text: body.to_string(),
        source_payload: format!("<p>{body}</p>"),
        source_payload_kind: SourcePayloadKind::Html,
        content_hash: format!("hash-{external_key}-{body}"),
        conversion_version: 1,
        conversion_status: ConversionStatus::Ok,
        expires_at: "2026-08-17T10:00:00Z".to_string(),
        fulltext_status: FulltextStatus::NotRequested,
    }
}

#[test]
fn image_authorization_is_per_item_and_resets_when_content_changes() {
    let conn = test_conn();
    insert_source(
        &conn,
        "src-images",
        "Images",
        "https://example.com/images.xml",
    );
    let mut original = item_input(
        "src-images",
        "article",
        "Article",
        "![one](https://cdn.example/image.png)",
        "2026-08-01T00:00:00Z",
    );
    FeedRepository::upsert_items(&conn, &[original.clone()]).unwrap();
    let item_id = original.id.clone();

    let (markdown, article_url) = FeedRepository::authorize_item_images(&conn, &item_id, now())
        .unwrap()
        .expect("active item can be authorized");
    assert!(markdown.contains("image.png"));
    assert_eq!(article_url.as_deref(), Some("https://example.com/article"));
    assert!(
        FeedRepository::get_item_detail(&conn, &item_id)
            .unwrap()
            .unwrap()
            .images_authorized
    );

    original.content_hash = "changed-image-content".to_string();
    original.content_markdown = "new text".to_string();
    original.content_text = "new text".to_string();
    FeedRepository::upsert_items(&conn, &[original]).unwrap();
    assert!(
        !FeedRepository::get_item_detail(&conn, &item_id)
            .unwrap()
            .unwrap()
            .images_authorized,
        "更新文章内容必须撤销旧图片授权"
    );
}

fn list_ids(items: &[crate::feed::model::FeedItemSummary]) -> Vec<&str> {
    items.iter().map(|item| item.id.as_str()).collect()
}

// ── source CRUD ─────────────────────────────────────────────

#[test]
fn source_crud_roundtrip() {
    let conn = test_conn();
    let now = now();

    let created = FeedRepository::create_source(
        &conn,
        &NewFeedSource {
            id: "src-1".to_string(),
            feed_url: "https://example.com/feed.xml".to_string(),
            site_url: Some("https://example.com".to_string()),
            title: "Example Blog".to_string(),
            title_override: None,
            description: Some("desc".to_string()),
            icon_url: None,
            language: Some("en".to_string()),
            folder_path: "tech".to_string(),
            fetch_interval_minutes: 60,
        },
        now,
    )
    .expect("create source");
    assert_eq!(created.id, "src-1");
    assert!(created.is_enabled);
    assert!(
        created.fulltext_enabled,
        "new sources fetch web bodies by default"
    );
    assert_eq!(created.fetch_interval_minutes, 60);

    let fetched = FeedRepository::get_source(&conn, "src-1")
        .expect("get source")
        .expect("source exists");
    assert_eq!(fetched.feed_url, "https://example.com/feed.xml");
    assert_eq!(fetched.folder_path, "tech");
    assert_eq!(fetched.created_at, "2026-08-10T10:00:00Z");

    let updated = FeedRepository::update_source(
        &conn,
        "src-1",
        &FeedSourcePatch {
            title_override: Some("My Feed".to_string()),
            folder_path: Some("tech/rust".to_string()),
            fetch_interval_minutes: Some(120),
            is_enabled: Some(false),
            ..Default::default()
        },
        now,
    )
    .expect("update source");
    assert!(updated);

    let fetched = FeedRepository::get_source(&conn, "src-1")
        .expect("get source")
        .expect("source exists");
    assert_eq!(fetched.title_override.as_deref(), Some("My Feed"));
    assert_eq!(fetched.folder_path, "tech/rust");
    assert_eq!(fetched.fetch_interval_minutes, 120);
    assert!(!fetched.is_enabled);

    // 清除覆盖标题：恢复 feed 原标题。
    FeedRepository::update_source(
        &conn,
        "src-1",
        &FeedSourcePatch {
            clear_title_override: true,
            ..Default::default()
        },
        now,
    )
    .expect("clear title override");
    let fetched = FeedRepository::get_source(&conn, "src-1")
        .expect("get source")
        .expect("source exists");
    assert_eq!(fetched.title_override, None);

    // 不存在的 source 返回 None。
    assert!(FeedRepository::get_source(&conn, "missing")
        .expect("query")
        .is_none());
}

#[test]
fn opening_a_saved_summary_queues_only_that_item_and_promotes_it() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Example", "https://example.com/feed.xml");
    let items = [
        FeedItemInput {
            content_markdown: "summary".to_string(),
            summary_markdown: "summary".to_string(),
            ..item_input(
                "src-1",
                "opened",
                "Opened",
                "summary",
                "2026-08-01T00:00:00Z",
            )
        },
        FeedItemInput {
            fulltext_status: FulltextStatus::Pending,
            content_markdown: "summary".to_string(),
            summary_markdown: "summary".to_string(),
            ..item_input(
                "src-1",
                "background",
                "Background",
                "summary",
                "2026-08-02T00:00:00Z",
            )
        },
    ];
    FeedRepository::upsert_items(&conn, &items).expect("seed summaries");
    conn.execute(
        "UPDATE feed_items SET updated_at = '2026-08-01T00:00:00Z' WHERE source_id = 'src-1'",
        [],
    )
    .expect("normalize queue priority baseline");

    let opened_at = DateTime::parse_from_rfc3339("2026-08-10T10:00:01Z")
        .expect("fixed timestamp")
        .with_timezone(&Utc);
    let outcome = FeedRepository::enqueue_item_fulltext(&conn, "item-src-1-opened", opened_at)
        .expect("opening summary queues it");
    assert_eq!(outcome.as_str(), "queued");

    let claimed = FeedRepository::claim_pending_fulltext(&conn, 1, now()).expect("claim");
    assert_eq!(
        claimed[0].0, "item-src-1-opened",
        "opened item wins priority"
    );

    let repeated = FeedRepository::enqueue_item_fulltext(&conn, "item-src-1-opened", now())
        .expect("reopening reuses in-flight work");
    assert_eq!(repeated.as_str(), "already_queued");

    let failed =
        FeedRepository::fail_fulltext(&conn, "item-src-1-opened", now()).expect("mark failed");
    assert!(failed);
    let retried = FeedRepository::enqueue_item_fulltext(&conn, "item-src-1-opened", now())
        .expect("retry queues only the opened article again");
    assert_eq!(retried.as_str(), "queued");
}

#[test]
fn opening_full_feed_content_or_disabled_source_is_a_safe_noop() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Example", "https://example.com/feed.xml");
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-1",
            "full",
            "Full Feed Article",
            "full body",
            "2026-08-01T00:00:00Z",
        )],
    )
    .expect("seed full feed body");

    let full_feed = FeedRepository::enqueue_item_fulltext(&conn, "item-src-1-full", now())
        .expect("full feed article is not fetched again");
    assert_eq!(full_feed.as_str(), "not_eligible");

    FeedRepository::update_source(
        &conn,
        "src-1",
        &FeedSourcePatch {
            fulltext_enabled: Some(false),
            ..Default::default()
        },
        now(),
    )
    .expect("disable");
    let mut summary = item_input(
        "src-1",
        "summary-disabled",
        "Summary",
        "summary",
        "2026-08-01T00:00:00Z",
    );
    summary.content_markdown = "summary".to_string();
    summary.summary_markdown = "summary".to_string();
    FeedRepository::upsert_items(&conn, &[summary]).expect("seed disabled summary");
    let disabled =
        FeedRepository::enqueue_item_fulltext(&conn, "item-src-1-summary-disabled", now())
            .expect("disabled source remains a no-op");
    assert_eq!(disabled.as_str(), "not_eligible");
}

#[test]
fn source_update_validates_fetch_interval() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Example", "https://example.com/feed.xml");

    let too_small = FeedRepository::update_source(
        &conn,
        "src-1",
        &FeedSourcePatch {
            fetch_interval_minutes: Some(5),
            ..Default::default()
        },
        now(),
    );
    assert!(too_small.is_err(), "interval below 15 must be rejected");

    let too_large = FeedRepository::update_source(
        &conn,
        "src-1",
        &FeedSourcePatch {
            fetch_interval_minutes: Some(20000),
            ..Default::default()
        },
        now(),
    );
    assert!(too_large.is_err(), "interval above 10080 must be rejected");

    let boundary = FeedRepository::update_source(
        &conn,
        "src-1",
        &FeedSourcePatch {
            fetch_interval_minutes: Some(10080),
            ..Default::default()
        },
        now(),
    );
    assert!(boundary.expect("boundary accepted"));
}

#[test]
fn disabling_fulltext_cancels_only_not_started_source_items() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Example", "https://example.com/feed.xml");
    FeedRepository::upsert_items(
        &conn,
        &[FeedItemInput {
            fulltext_status: FulltextStatus::Pending,
            ..item_input(
                "src-1",
                "pending",
                "Pending",
                "summary",
                "2026-08-01T00:00:00Z",
            )
        }],
    )
    .expect("seed pending article");

    FeedRepository::update_source(
        &conn,
        "src-1",
        &FeedSourcePatch {
            fulltext_enabled: Some(false),
            ..Default::default()
        },
        now(),
    )
    .expect("disable fulltext");

    let status: String = conn
        .query_row(
            "SELECT fulltext_status FROM feed_items WHERE source_id = 'src-1'",
            [],
            |row| row.get(0),
        )
        .expect("status");
    assert_eq!(status, "not_requested");
}

#[test]
fn changed_feed_content_invalidates_stale_web_fulltext_and_requeues_when_enabled() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "One", "https://example.com/one.xml");
    FeedRepository::update_source(
        &conn,
        "src-1",
        &FeedSourcePatch {
            fulltext_enabled: Some(true),
            ..Default::default()
        },
        now(),
    )
    .expect("enable fulltext");
    let original = item_input(
        "src-1",
        "updated-entry",
        "Original",
        "old RSS summary",
        "2026-08-01T00:00:00Z",
    );
    FeedRepository::upsert_items(&conn, &[original]).expect("insert article");
    conn.execute(
        "UPDATE feed_items
         SET fulltext_markdown = 'old extracted page', content_text = 'old extracted page',
             content_origin = 'web', fulltext_status = 'ready',
             fulltext_extraction_version = 2,
             primary_document_kind = 'pdf',
             primary_document_url = 'https://example.com/old.pdf'
         WHERE source_id = 'src-1' AND external_key = 'updated-entry'",
        [],
    )
    .expect("seed prior web extraction");

    let replacement = FeedItemInput {
        title: "Updated".to_string(),
        content_markdown: "new RSS summary".to_string(),
        content_text: "new RSS summary".to_string(),
        content_hash: "hash-updated-entry-new".to_string(),
        fulltext_status: FulltextStatus::Pending,
        ..item_input(
            "src-1",
            "updated-entry",
            "Updated",
            "new RSS summary",
            "2026-08-01T00:00:00Z",
        )
    };
    FeedRepository::upsert_items(&conn, &[replacement]).expect("update article");

    let detail = FeedRepository::get_item_detail(&conn, "item-src-1-updated-entry")
        .expect("get detail")
        .expect("article exists");
    assert_eq!(detail.content_origin, "feed");
    assert_eq!(detail.fulltext_status, "pending");
    assert_eq!(detail.content_markdown, "new RSS summary");
    assert!(detail.primary_document.is_none());
    assert!(!detail.fulltext_needs_refresh);
}

#[test]
fn list_sources_reports_unread_count_and_display_title() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");
    insert_source(&conn, "src-2", "Feed Two", "https://example.com/two.xml");

    FeedRepository::upsert_items(
        &conn,
        &[
            item_input("src-1", "a", "A", "body a", "2026-08-01T08:00:00Z"),
            item_input("src-1", "b", "B", "body b", "2026-08-01T09:00:00Z"),
            item_input("src-2", "c", "C", "body c", "2026-08-01T10:00:00Z"),
        ],
    )
    .expect("upsert items");

    // src-1 的 b 标为已读。
    let summary_b = FeedRepository::list_items(
        &conn,
        &FeedItemQuery {
            view: FeedView::All,
            source_id: Some("src-1".to_string()),
            search: None,
            received_after: None,
            cursor: None,
            limit: 100,
        },
        now(),
    )
    .expect("list all")
    .into_iter()
    .find(|item| item.id == "item-src-1-b")
    .expect("item b");
    FeedRepository::set_item_state(
        &conn,
        &summary_b.id,
        &FeedItemStatePatch {
            is_read: Some(true),
            ..Default::default()
        },
        now(),
    )
    .expect("mark read");

    FeedRepository::update_source(
        &conn,
        "src-1",
        &FeedSourcePatch {
            title_override: Some("Renamed One".to_string()),
            ..Default::default()
        },
        now(),
    )
    .expect("rename source");

    let sources = FeedRepository::list_sources(&conn).expect("list sources");
    let one = sources.iter().find(|s| s.id == "src-1").expect("src-1");
    assert_eq!(
        one.title, "Renamed One",
        "display title must use title_override"
    );
    assert_eq!(one.unread_count, 1);
    let two = sources.iter().find(|s| s.id == "src-2").expect("src-2");
    assert_eq!(two.title, "Feed Two");
    assert_eq!(two.unread_count, 1);
}

#[test]
fn item_summary_keeps_sort_time_separate_from_excerpt_content() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-1",
            "summary-columns",
            "Column order",
            "this is the article excerpt, not a timestamp",
            "2026-08-01T08:00:00Z",
        )],
    )
    .expect("upsert item");

    let item = FeedRepository::list_items(
        &conn,
        &FeedItemQuery {
            view: FeedView::All,
            source_id: None,
            search: None,
            received_after: None,
            cursor: None,
            limit: 50,
        },
        now(),
    )
    .expect("list items")
    .pop()
    .expect("item exists");

    assert_eq!(item.sort_at, "2026-07-01T08:00:00Z");
    assert_eq!(item.excerpt, "this is the article excerpt, not a timestamp");
}

#[test]
fn source_delete_cascades_items_and_fts() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");
    FeedRepository::upsert_items(
        &conn,
        &[
            item_input(
                "src-1",
                "a",
                "A",
                "unique-term-alpha",
                "2026-08-01T08:00:00Z",
            ),
            item_input(
                "src-1",
                "b",
                "B",
                "unique-term-beta",
                "2026-08-01T09:00:00Z",
            ),
        ],
    )
    .expect("upsert items");

    assert_eq!(
        FeedRepository::search(&conn, "unique-term", None, 50)
            .expect("search")
            .len(),
        2
    );

    let removed = FeedRepository::delete_source(&conn, "src-1")
        .expect("delete source")
        .expect("source existed");
    assert_eq!(removed, 2, "cascade must remove both items");

    assert!(FeedRepository::get_source(&conn, "src-1")
        .expect("query")
        .is_none());
    assert_eq!(
        FeedRepository::count_items(&conn, "src-1").expect("count"),
        0
    );
    assert!(
        FeedRepository::search(&conn, "unique-term", None, 50)
            .expect("search")
            .is_empty(),
        "FTS must be cleaned by delete trigger"
    );

    assert!(FeedRepository::delete_source(&conn, "missing")
        .expect("delete missing")
        .is_none());
}

#[test]
fn source_trash_hides_items_and_restore_keeps_source_paused() {
    let conn = test_conn();
    insert_source(
        &conn,
        "src-trash",
        "Trashable",
        "https://example.com/trash.xml",
    );
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-trash",
            "one",
            "Article",
            "retained body",
            "2026-08-10T08:00:00Z",
        )],
    )
    .expect("seed");

    let count = FeedRepository::trash_source(&conn, "src-trash", now())
        .expect("trash source")
        .expect("source exists");
    assert_eq!(count, 1);
    assert!(FeedRepository::list_sources(&conn).unwrap().is_empty());
    assert!(FeedRepository::get_item_detail(&conn, "item-src-trash-one")
        .unwrap()
        .is_none());
    let snapshot = FeedRepository::trash_snapshot(&conn, 200).unwrap();
    assert_eq!(snapshot.sources.len(), 1);
    assert_eq!(snapshot.sources[0].item_count, 1);
    assert!(
        snapshot.items.is_empty(),
        "source trash items are grouped once"
    );

    assert!(FeedRepository::restore_source(&conn, "src-trash", now()).unwrap());
    let restored = FeedRepository::get_source(&conn, "src-trash")
        .unwrap()
        .expect("restored source");
    assert!(
        !restored.is_enabled,
        "restore must not start network traffic"
    );
    assert!(FeedRepository::get_item_detail(&conn, "item-src-trash-one")
        .unwrap()
        .is_some());
}

#[test]
fn grouped_source_trash_cannot_be_cleared_or_restored_as_a_single_item() {
    let conn = test_conn();
    insert_source(
        &conn,
        "src-grouped-trash",
        "Grouped trash",
        "https://example.com/grouped-trash.xml",
    );
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-grouped-trash",
            "one",
            "Article",
            "body",
            "2026-08-10T08:00:00Z",
        )],
    )
    .unwrap();
    FeedRepository::trash_source(&conn, "src-grouped-trash", now()).unwrap();

    assert_eq!(FeedRepository::clear_deleted_items(&conn).unwrap(), 0);
    assert!(
        !FeedRepository::restore_deleted_item(&conn, "item-src-grouped-trash-one", now(),).unwrap()
    );
    assert_eq!(
        FeedRepository::purge_deleted_items(&conn, now()).unwrap(),
        0
    );

    let still_grouped: (bool, String) = conn
        .query_row(
            "SELECT deleted_at IS NOT NULL, deletion_reason
             FROM feed_items WHERE id = 'item-src-grouped-trash-one'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(still_grouped, (true, "source_removed".to_string()));
}

#[test]
fn restoring_source_does_not_revive_retention_deleted_items() {
    let conn = test_conn();
    insert_source(&conn, "src-mixed", "Mixed", "https://example.com/mixed.xml");
    FeedRepository::upsert_items(
        &conn,
        &[
            item_input(
                "src-mixed",
                "active",
                "Active",
                "active",
                "2026-08-10T08:00:00Z",
            ),
            item_input(
                "src-mixed",
                "expired",
                "Expired",
                "expired",
                "2026-08-10T08:00:00Z",
            ),
        ],
    )
    .unwrap();
    conn.execute(
        "UPDATE feed_items SET deleted_at = '2026-08-09T00:00:00Z',
         purge_after = '2026-09-08T00:00:00Z', deletion_reason = 'retention'
         WHERE external_key = 'expired'",
        [],
    )
    .unwrap();

    FeedRepository::trash_source(&conn, "src-mixed", now()).unwrap();
    FeedRepository::restore_source(&conn, "src-mixed", now()).unwrap();

    let expired_deleted: bool = conn
        .query_row(
            "SELECT deleted_at IS NOT NULL FROM feed_items WHERE external_key = 'expired'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(expired_deleted);
}

#[test]
fn expired_source_is_purged_with_its_grouped_articles() {
    let conn = test_conn();
    insert_source(
        &conn,
        "src-expired-trash",
        "Expired trash",
        "https://example.com/expired-trash.xml",
    );
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-expired-trash",
            "one",
            "Article",
            "body",
            "2026-08-10T08:00:00Z",
        )],
    )
    .unwrap();
    FeedRepository::trash_source(&conn, "src-expired-trash", now()).unwrap();
    conn.execute(
        "UPDATE feed_sources SET purge_after = '2026-08-09T00:00:00Z'
         WHERE id = 'src-expired-trash'",
        [],
    )
    .unwrap();

    assert_eq!(
        FeedRepository::purge_expired_sources(&conn, now()).unwrap(),
        1
    );
    let source_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM feed_sources WHERE id = 'src-expired-trash'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let item_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM feed_items WHERE source_id = 'src-expired-trash'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!((source_count, item_count), (0, 0));
}

#[test]
fn stale_web_fulltext_is_requeued_and_failure_falls_back_to_feed_content() {
    let conn = test_conn();
    insert_source(&conn, "src-stale", "Stale", "https://example.com/stale.xml");
    let mut input = item_input(
        "src-stale",
        "stale",
        "Stale article",
        "Feed summary body",
        "2026-08-10T08:00:00Z",
    );
    input.summary_markdown = input.content_markdown.clone();
    FeedRepository::upsert_items(&conn, &[input]).unwrap();
    conn.execute(
        "UPDATE feed_items SET content_markdown = 'old page shell',
         fulltext_markdown = 'old page shell',
         content_text = 'old page shell', content_origin = 'web', fulltext_status = 'ready',
         fulltext_extraction_version = 0 WHERE external_key = 'stale'",
        [],
    )
    .unwrap();

    let queued =
        FeedRepository::enqueue_item_fulltext(&conn, "item-src-stale-stale", now()).unwrap();
    assert_eq!(queued, super::model::FeedFulltextEnqueueOutcome::Queued);
    conn.execute(
        "UPDATE feed_items SET fulltext_status = 'fetching'
         WHERE id = 'item-src-stale-stale'",
        [],
    )
    .unwrap();
    FeedRepository::fail_fulltext(&conn, "item-src-stale-stale", now()).unwrap();
    let detail = FeedRepository::get_item_detail(&conn, "item-src-stale-stale")
        .unwrap()
        .unwrap();
    assert_eq!(detail.content_origin, "feed");
    assert!(!detail.content_markdown.contains("old page shell"));
}

#[test]
fn expired_source_and_item_cannot_be_restored_during_cleanup_gap() {
    let conn = test_conn();
    insert_source(
        &conn,
        "src-expired-restore",
        "Expired restore",
        "https://example.com/expired-restore.xml",
    );
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-expired-restore",
            "expired-item",
            "Expired item",
            "body",
            "2026-08-10T08:00:00Z",
        )],
    )
    .unwrap();
    FeedRepository::trash_source(&conn, "src-expired-restore", now()).unwrap();
    conn.execute(
        "UPDATE feed_sources SET purge_after = '2026-08-09T23:59:59Z'
         WHERE id = 'src-expired-restore'",
        [],
    )
    .unwrap();
    assert!(!FeedRepository::restore_source(&conn, "src-expired-restore", now()).unwrap());

    insert_source(
        &conn,
        "src-expired-item",
        "Expired item source",
        "https://example.com/expired-item.xml",
    );
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-expired-item",
            "one",
            "One",
            "body",
            "2026-08-10T08:00:00Z",
        )],
    )
    .unwrap();
    conn.execute(
        "UPDATE feed_items SET deleted_at = '2026-07-01T00:00:00Z',
         purge_after = '2026-08-09T23:59:59Z', deletion_reason = 'retention'
         WHERE id = 'item-src-expired-item-one'",
        [],
    )
    .unwrap();
    assert!(
        !FeedRepository::restore_deleted_item(&conn, "item-src-expired-item-one", now(),).unwrap()
    );
}

// ── 收件箱派生与状态轴 ─────────────────────────────────────

#[test]
fn inbox_keeps_read_items_until_they_are_archived() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");
    FeedRepository::upsert_items(
        &conn,
        &[
            item_input("src-1", "fresh", "Fresh", "body", "2026-08-01T08:00:00Z"),
            item_input("src-1", "read", "Read", "body", "2026-08-01T09:00:00Z"),
            item_input("src-1", "arch", "Arch", "body", "2026-08-01T10:00:00Z"),
            item_input("src-1", "star", "Star", "body", "2026-08-01T11:00:00Z"),
            item_input("src-1", "both", "Both", "body", "2026-08-01T12:00:00Z"),
        ],
    )
    .expect("upsert items");

    FeedRepository::set_item_state(
        &conn,
        "item-src-1-read",
        &FeedItemStatePatch {
            is_read: Some(true),
            ..Default::default()
        },
        now(),
    )
    .expect("mark read");
    FeedRepository::set_item_state(
        &conn,
        "item-src-1-arch",
        &FeedItemStatePatch {
            is_archived: Some(true),
            ..Default::default()
        },
        now(),
    )
    .expect("archive");
    FeedRepository::set_item_state(
        &conn,
        "item-src-1-star",
        &FeedItemStatePatch {
            is_starred: Some(true),
            ..Default::default()
        },
        now(),
    )
    .expect("star");
    FeedRepository::set_item_state(
        &conn,
        "item-src-1-both",
        &FeedItemStatePatch {
            is_read: Some(true),
            is_archived: Some(true),
            ..Default::default()
        },
        now(),
    )
    .expect("read+archive");

    let inbox = FeedRepository::list_items(
        &conn,
        &FeedItemQuery {
            view: FeedView::Inbox,
            source_id: None,
            search: None,
            received_after: None,
            cursor: None,
            limit: 100,
        },
        now(),
    )
    .expect("list inbox");
    let ids = list_ids(&inbox);
    assert!(
        ids.contains(&"item-src-1-fresh"),
        "unread+unarchived must be in inbox"
    );
    assert!(
        ids.contains(&"item-src-1-star"),
        "starred-but-unread must remain in inbox"
    );
    assert!(
        ids.contains(&"item-src-1-read"),
        "read state must not remove from inbox"
    );
    assert!(!ids.contains(&"item-src-1-arch"));
    assert!(!ids.contains(&"item-src-1-both"));

    let starred = FeedRepository::list_items(
        &conn,
        &FeedItemQuery {
            view: FeedView::Starred,
            source_id: None,
            search: None,
            received_after: None,
            cursor: None,
            limit: 100,
        },
        now(),
    )
    .expect("list starred");
    let starred_ids = list_ids(&starred);
    assert_eq!(starred_ids.len(), 1);
    assert_eq!(starred_ids[0], "item-src-1-star");

    let archived = FeedRepository::list_items(
        &conn,
        &FeedItemQuery {
            view: FeedView::Archived,
            source_id: None,
            search: None,
            received_after: None,
            cursor: None,
            limit: 100,
        },
        now(),
    )
    .expect("list archived");
    let archived_ids = list_ids(&archived);
    assert_eq!(
        archived_ids.len(),
        2,
        "both explicitly archived items must appear"
    );
    assert!(archived_ids.contains(&"item-src-1-both"));
    assert!(archived_ids.contains(&"item-src-1-arch"));
    assert!(
        !archived_ids.contains(&"item-src-1-star"),
        "unarchived starred item must not appear"
    );
}

#[test]
fn expired_items_soft_delete_but_starred_items_remain() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");
    FeedRepository::upsert_items(
        &conn,
        &[
            item_input(
                "src-1",
                "expired",
                "Expired",
                "body",
                "2026-08-01T08:00:00Z",
            ),
            item_input(
                "src-1",
                "starred",
                "Starred",
                "body",
                "2026-08-01T09:00:00Z",
            ),
        ],
    )
    .expect("upsert items");
    conn.execute(
        "UPDATE feed_items SET expires_at = '2026-08-09T00:00:00Z'",
        [],
    )
    .expect("make items expired");
    FeedRepository::set_item_state(
        &conn,
        "item-src-1-starred",
        &FeedItemStatePatch {
            is_starred: Some(true),
            ..Default::default()
        },
        now(),
    )
    .expect("star item");

    assert_eq!(
        FeedRepository::soft_delete_expired_items(&conn, now()).expect("soft delete"),
        1
    );
    let all = FeedRepository::list_items(
        &conn,
        &FeedItemQuery {
            view: FeedView::All,
            source_id: Some("src-1".to_string()),
            search: None,
            received_after: None,
            cursor: None,
            limit: 50,
        },
        now(),
    )
    .expect("list live items");
    assert_eq!(list_ids(&all), vec!["item-src-1-starred"]);
}

#[test]
fn restoring_a_starred_trash_item_keeps_its_permanent_retention() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-1",
            "restore-starred",
            "Restore starred",
            "body",
            "2026-08-01T08:00:00Z",
        )],
    )
    .expect("upsert item");
    conn.execute(
        "UPDATE feed_items
         SET starred_at = '2026-08-10T00:00:00Z', deleted_at = '2026-08-10T00:00:00Z',
             purge_after = '2026-09-09T00:00:00Z', expires_at = '2026-08-17T00:00:00Z'
         WHERE id = 'item-src-1-restore-starred'",
        [],
    )
    .expect("seed recycle bin");

    assert!(
        FeedRepository::restore_deleted_item(&conn, "item-src-1-restore-starred", now(),)
            .expect("restore")
    );
    let (expires_at, deletion_reason): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT expires_at, deletion_reason
             FROM feed_items WHERE id = 'item-src-1-restore-starred'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("expiry");
    assert_eq!(expires_at, None, "收藏文章恢复后仍应永久保留");
    assert_eq!(deletion_reason, None, "恢复后必须清除删除原因");
}

#[test]
fn state_axes_are_independent() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-1",
            "a",
            "A",
            "body",
            "2026-08-01T08:00:00Z",
        )],
    )
    .expect("upsert item");

    let patch = FeedItemStatePatch {
        is_read: Some(true),
        is_starred: Some(true),
        is_archived: Some(true),
    };
    FeedRepository::set_item_state(&conn, "item-src-1-a", &patch, now()).expect("set all");

    // 重新标未读只清 read_at，收藏与归档不受影响。
    FeedRepository::set_item_state(
        &conn,
        "item-src-1-a",
        &FeedItemStatePatch {
            is_read: Some(false),
            ..Default::default()
        },
        now(),
    )
    .expect("unread");
    let item = FeedRepository::get_item_detail(&conn, "item-src-1-a")
        .expect("detail")
        .expect("item exists");
    assert!(!item.summary.is_read, "unread clears only read_at");
    assert!(item.summary.is_starred, "unread must not clear starred_at");
    assert!(
        item.summary.is_archived,
        "unread must not clear archived_at"
    );

    // 取消收藏不影响归档。
    FeedRepository::set_item_state(
        &conn,
        "item-src-1-a",
        &FeedItemStatePatch {
            is_starred: Some(false),
            ..Default::default()
        },
        now(),
    )
    .expect("unstar");
    let item = FeedRepository::get_item_detail(&conn, "item-src-1-a")
        .expect("detail")
        .expect("item exists");
    assert!(item.summary.is_archived, "unstar must not clear archived");
    assert!(!item.summary.is_starred);
    assert!(!item.summary.is_read);

    // 取消归档不影响收藏与已读。
    FeedRepository::set_item_state(
        &conn,
        "item-src-1-a",
        &FeedItemStatePatch {
            is_archived: Some(false),
            ..Default::default()
        },
        now(),
    )
    .expect("unarchive");
    let item = FeedRepository::get_item_detail(&conn, "item-src-1-a")
        .expect("detail")
        .expect("item exists");
    assert!(
        !item.summary.is_archived,
        "unarchive clears only archived_at"
    );
    assert!(!item.summary.is_starred);
    assert!(!item.summary.is_read);

    // 空 patch 是验证错误。
    let empty = FeedRepository::set_item_state(
        &conn,
        "item-src-1-a",
        &FeedItemStatePatch::default(),
        now(),
    );
    assert!(empty.is_err(), "empty patch must be rejected");
}

// ── cursor 稳定与分页 ──────────────────────────────────────

#[test]
fn keyset_cursor_pages_stably_without_overlap() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");

    // 25 个条目：received_at 从 08-01 到 08-03 交错；fixture 的 published_at
    // 相同，因此实际排序完全由 row_id 决胜，覆盖 keyset 的并列路径。
    let mut inputs = Vec::new();
    for index in 0..25 {
        let day = 1 + (index % 3);
        let hour = 8 + (index % 10);
        let received = format!("2026-08-0{day}T{hour:02}:00:00Z");
        inputs.push(item_input(
            "src-1",
            &format!("k{index:02}"),
            &format!("Title {index}"),
            &format!("body {index}"),
            &received,
        ));
    }
    FeedRepository::upsert_items(&conn, &inputs).expect("upsert items");

    let mut collected = Vec::new();
    let mut cursor = None;
    loop {
        let page = FeedRepository::list_items(
            &conn,
            &FeedItemQuery {
                view: FeedView::All,
                source_id: None,
                search: None,
                received_after: None,
                cursor,
                limit: 10,
            },
            now(),
        )
        .expect("list page");
        assert!(page.len() <= 10, "limit must cap page size");
        collected.extend(page.iter().map(|item| item.id.clone()));
        if page.len() < 10 {
            break;
        }
        let last = page.last().expect("last item");
        cursor = Some(crate::feed::model::FeedPageCursor {
            // 游标必须沿用查询实际的 `published_at ?? received_at` 排序键；
            // 使用 received_at 会在有发布时间的条目上重复同一页。
            sort_at: last.sort_at.clone(),
            row_id: last.row_id,
        });
    }

    assert_eq!(collected.len(), 25, "all items must be reachable by cursor");
    let unique: std::collections::HashSet<_> = collected.iter().collect();
    assert_eq!(unique.len(), 25, "cursor pages must not overlap");

    // 分页中途插入更新条目不会造成重复或遗漏。
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-1",
            "k99",
            "Newest",
            "body newest",
            "2026-08-04T08:00:00Z",
        )],
    )
    .expect("insert newest item");
    let first_page = FeedRepository::list_items(
        &conn,
        &FeedItemQuery {
            view: FeedView::All,
            source_id: None,
            search: None,
            received_after: None,
            cursor: None,
            limit: 10,
        },
        now(),
    )
    .expect("first page after insert");
    assert_eq!(
        first_page[0].id, "item-src-1-k99",
        "newest item must lead the list"
    );
}

#[test]
fn keyset_cursor_tiebreak_uses_row_id_desc() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");
    FeedRepository::upsert_items(
        &conn,
        &[
            item_input("src-1", "a", "A", "body", "2026-08-01T08:00:00Z"),
            item_input("src-1", "b", "B", "body", "2026-08-01T08:00:00Z"),
            item_input("src-1", "c", "C", "body", "2026-08-01T09:00:00Z"),
        ],
    )
    .expect("upsert items");

    let items = FeedRepository::list_items(
        &conn,
        &FeedItemQuery {
            view: FeedView::All,
            source_id: None,
            search: None,
            received_after: None,
            cursor: None,
            limit: 10,
        },
        now(),
    )
    .expect("list");
    assert_eq!(items[0].id, "item-src-1-c");
    assert_eq!(
        items[1].id, "item-src-1-b",
        "row_id DESC breaks received_at ties"
    );
    assert_eq!(items[2].id, "item-src-1-a");
}

#[test]
fn list_limit_clamps_to_1_200() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");
    let inputs: Vec<_> = (0..250)
        .map(|index| {
            item_input(
                "src-1",
                &format!("k{index:03}"),
                &format!("Title {index}"),
                "body",
                // 小时范围 8..=21，避免越界时间导致 fixture 解析失败。
                &format!("2026-08-01T{:02}:00:00Z", 8 + index % 14),
            )
        })
        .collect();
    FeedRepository::upsert_items(&conn, &inputs).expect("upsert items");

    let zero = FeedRepository::list_items(
        &conn,
        &FeedItemQuery {
            view: FeedView::All,
            source_id: None,
            search: None,
            received_after: None,
            cursor: None,
            limit: 0,
        },
        now(),
    )
    .expect("limit 0 clamps to 1");
    assert_eq!(zero.len(), 1);

    let huge = FeedRepository::list_items(
        &conn,
        &FeedItemQuery {
            view: FeedView::All,
            source_id: None,
            search: None,
            received_after: None,
            cursor: None,
            limit: 500,
        },
        now(),
    )
    .expect("limit 500 clamps to 200");
    assert_eq!(huge.len(), 200);
}

#[test]
fn list_respects_received_after_and_source_filter() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");
    insert_source(&conn, "src-2", "Feed Two", "https://example.com/two.xml");
    FeedRepository::upsert_items(
        &conn,
        &[
            item_input("src-1", "a", "A", "body", "2026-08-01T08:00:00Z"),
            item_input("src-1", "b", "B", "body", "2026-08-02T08:00:00Z"),
            item_input("src-2", "c", "C", "body", "2026-08-03T08:00:00Z"),
        ],
    )
    .expect("upsert items");

    let after = FeedRepository::list_items(
        &conn,
        &FeedItemQuery {
            view: FeedView::All,
            source_id: None,
            search: None,
            received_after: Some("2026-08-02T00:00:00Z".to_string()),
            cursor: None,
            limit: 100,
        },
        now(),
    )
    .expect("list after");
    assert_eq!(list_ids(&after), vec!["item-src-2-c", "item-src-1-b"]);

    let scoped = FeedRepository::list_items(
        &conn,
        &FeedItemQuery {
            view: FeedView::All,
            source_id: Some("src-2".to_string()),
            search: None,
            received_after: None,
            cursor: None,
            limit: 100,
        },
        now(),
    )
    .expect("list scoped");
    assert_eq!(list_ids(&scoped), vec!["item-src-2-c"]);
}

// ── 今日视图 ───────────────────────────────────────────────

#[test]
fn today_view_uses_local_midnight_boundary() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");

    let boundary = today_start_utc(now());
    // 边界前 1 秒（从边界推导，保证任何本地时区下都落在“昨天”）。
    let before = (DateTime::parse_from_rfc3339(&boundary)
        .expect("valid boundary")
        .with_timezone(&Utc)
        - chrono::Duration::seconds(1))
    .to_rfc3339_opts(SecondsFormat::Secs, true);
    FeedRepository::upsert_items(
        &conn,
        &[
            item_input("src-1", "old", "Old", "body", &before),
            item_input("src-1", "fresh", "Fresh", "body", &boundary),
        ],
    )
    .expect("upsert items");

    let today = FeedRepository::list_items(
        &conn,
        &FeedItemQuery {
            view: FeedView::Today,
            source_id: None,
            search: None,
            received_after: None,
            cursor: None,
            limit: 100,
        },
        now(),
    )
    .expect("list today");
    let ids = list_ids(&today);
    assert!(
        ids.contains(&"item-src-1-fresh"),
        "item at local midnight must be in today"
    );
    assert!(
        !ids.contains(&"item-src-1-old"),
        "item before local midnight must be excluded"
    );

    // 今日视图不受已读影响（只按本地时区当天收到筛选）。
    FeedRepository::set_item_state(
        &conn,
        "item-src-1-fresh",
        &FeedItemStatePatch {
            is_read: Some(true),
            ..Default::default()
        },
        now(),
    )
    .expect("mark read");
    let today = FeedRepository::list_items(
        &conn,
        &FeedItemQuery {
            view: FeedView::Today,
            source_id: None,
            search: None,
            received_after: None,
            cursor: None,
            limit: 100,
        },
        now(),
    )
    .expect("list today after read");
    assert!(list_ids(&today).contains(&"item-src-1-fresh"));
}

// ── upsert 语义 ────────────────────────────────────────────

#[test]
fn upsert_counts_insert_update_unchanged() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");

    let first = FeedRepository::upsert_items(
        &conn,
        &[
            item_input("src-1", "a", "A", "body v1", "2026-08-01T08:00:00Z"),
            item_input("src-1", "b", "B", "body v1", "2026-08-01T09:00:00Z"),
        ],
    )
    .expect("first upsert");
    assert_eq!(
        first,
        crate::feed::model::UpsertSummary {
            inserted: 2,
            updated: 0,
            unchanged: 0
        }
    );

    let same = FeedRepository::upsert_items(
        &conn,
        &[
            item_input("src-1", "a", "A", "body v1", "2026-08-01T08:00:00Z"),
            item_input("src-1", "b", "B", "body v1", "2026-08-01T09:00:00Z"),
        ],
    )
    .expect("same upsert");
    assert_eq!(
        same,
        crate::feed::model::UpsertSummary {
            inserted: 0,
            updated: 0,
            unchanged: 2
        }
    );

    let changed = FeedRepository::upsert_items(
        &conn,
        &[
            item_input("src-1", "a", "A", "body v2", "2026-08-01T08:00:00Z"),
            item_input("src-1", "b", "B", "body v1", "2026-08-01T09:00:00Z"),
        ],
    )
    .expect("changed upsert");
    assert_eq!(
        changed,
        crate::feed::model::UpsertSummary {
            inserted: 0,
            updated: 1,
            unchanged: 1
        }
    );

    // 空批量直接返回零计数。
    let empty = FeedRepository::upsert_items(&conn, &[]).expect("empty upsert");
    assert_eq!(empty.inserted + empty.updated + empty.unchanged, 0);
}

#[test]
fn content_update_preserves_state_and_received_at() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-1",
            "a",
            "A",
            "body v1",
            "2026-08-01T08:00:00Z",
        )],
    )
    .expect("initial upsert");

    let id = "item-src-1-a".to_string();
    FeedRepository::set_item_state(
        &conn,
        &id,
        &FeedItemStatePatch {
            is_read: Some(true),
            is_starred: Some(true),
            is_archived: Some(true),
        },
        now(),
    )
    .expect("set all states");

    // 内容更新（hash 变化）：只替换内容字段。
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-1",
            "a",
            "A",
            "body v2",
            "2026-08-01T08:00:00Z",
        )],
    )
    .expect("content update");

    let detail = FeedRepository::get_item_detail(&conn, &id)
        .expect("detail")
        .expect("item exists");
    assert!(
        detail.content_markdown.contains("body v2"),
        "content must be replaced"
    );
    assert!(
        detail.summary.is_read,
        "read_at must survive content update"
    );
    assert!(
        detail.summary.is_starred,
        "starred_at must survive content update"
    );
    assert!(
        detail.summary.is_archived,
        "archived_at must survive content update"
    );
    assert_eq!(
        detail.summary.received_at, "2026-08-01T08:00:00Z",
        "received_at is immutable"
    );

    // hash 未变时正文不替换。
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-1",
            "a",
            "A",
            "body v2",
            "2026-08-01T08:00:00Z",
        )],
    )
    .expect("no-op upsert");
    let detail = FeedRepository::get_item_detail(&conn, &id)
        .expect("detail")
        .expect("item exists");
    assert!(detail.summary.is_read, "no-op must keep state untouched");
}

#[test]
fn metadata_update_with_same_hash_preserves_content_and_state() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");
    let mut original = item_input(
        "src-1",
        "a",
        "Old title",
        "stable body",
        "2026-08-01T08:00:00Z",
    );
    original.canonical_url = Some("https://example.com/old".to_string());
    FeedRepository::upsert_items(&conn, &[original.clone()]).expect("initial upsert");
    FeedRepository::set_item_state(
        &conn,
        "item-src-1-a",
        &FeedItemStatePatch {
            is_read: Some(true),
            ..Default::default()
        },
        now(),
    )
    .expect("mark read");

    let mut changed = original;
    changed.title = "New title".to_string();
    changed.author_name = Some("New author".to_string());
    changed.canonical_url = Some("https://example.com/new".to_string());
    changed.content_markdown = "must not replace stable body".to_string();
    let summary = FeedRepository::upsert_items(&conn, &[changed]).expect("metadata upsert");
    assert_eq!(summary.updated, 1);

    let detail = FeedRepository::get_item_detail(&conn, "item-src-1-a")
        .expect("detail")
        .expect("exists");
    assert_eq!(detail.summary.title, "New title");
    assert_eq!(detail.summary.author_name.as_deref(), Some("New author"));
    assert_eq!(
        detail.summary.canonical_url.as_deref(),
        Some("https://example.com/new")
    );
    assert!(detail.content_markdown.contains("stable body"));
    assert!(detail.summary.is_read);
}

// ── 详情 DTO ───────────────────────────────────────────────

#[test]
fn item_detail_omits_source_payload() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-1",
            "a",
            "A",
            "body",
            "2026-08-01T08:00:00Z",
        )],
    )
    .expect("upsert item");

    let detail = FeedRepository::get_item_detail(&conn, "item-src-1-a")
        .expect("detail")
        .expect("item exists");
    assert_eq!(detail.summary.source_title, "Feed One");
    assert!(detail.content_markdown.contains("body"));
    assert_eq!(detail.summary.conversion_status, "ok");

    let json = serde_json::to_value(&detail).expect("serialize detail");
    let map = json.as_object().expect("detail is object");
    assert!(
        !json.to_string().contains("sourcePayload"),
        "detail DTO must never expose source_payload"
    );
    assert!(map.contains_key("contentMarkdown"));
    assert!(map.contains_key("summaryMarkdown"));
    assert!(map.contains_key("summary"));
}

// ── 批量已读 ───────────────────────────────────────────────

#[test]
fn mark_items_read_respects_frozen_view_and_counts() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");
    insert_source(&conn, "src-2", "Feed Two", "https://example.com/two.xml");
    FeedRepository::upsert_items(
        &conn,
        &[
            item_input("src-1", "a", "A", "body", "2026-08-01T08:00:00Z"),
            item_input("src-1", "b", "B", "body", "2026-08-01T09:00:00Z"),
            item_input("src-1", "c", "C", "body", "2026-08-01T10:00:00Z"),
            item_input("src-2", "d", "D", "body", "2026-08-01T11:00:00Z"),
        ],
    )
    .expect("upsert items");
    // c 已读、d 归档。
    FeedRepository::set_item_state(
        &conn,
        "item-src-1-c",
        &FeedItemStatePatch {
            is_read: Some(true),
            ..Default::default()
        },
        now(),
    )
    .expect("mark c read");
    FeedRepository::set_item_state(
        &conn,
        "item-src-2-d",
        &FeedItemStatePatch {
            is_archived: Some(true),
            ..Default::default()
        },
        now(),
    )
    .expect("archive d");

    // 冻结收件箱条件（src-1）：a、b 应被标为已读；c 已读、d 归档不受影响。
    let affected = FeedRepository::mark_items_read(
        &conn,
        &FeedItemQuery {
            view: FeedView::Inbox,
            source_id: Some("src-1".to_string()),
            search: None,
            received_after: None,
            cursor: None,
            limit: 100,
        },
        now(),
    )
    .expect("mark read");
    assert_eq!(affected, 2);

    let detail_b = FeedRepository::get_item_detail(&conn, "item-src-1-b")
        .expect("detail")
        .expect("item b");
    assert!(detail_b.summary.is_read);

    let reapply = FeedRepository::mark_items_read(
        &conn,
        &FeedItemQuery {
            view: FeedView::Inbox,
            source_id: Some("src-1".to_string()),
            search: None,
            received_after: None,
            cursor: None,
            limit: 100,
        },
        now(),
    )
    .expect("reapply");
    assert_eq!(reapply, 0, "already-read rows must not be recounted");
}

// ── 搜索 ───────────────────────────────────────────────────

#[test]
fn search_finds_matches_and_tracks_fts_updates() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");
    insert_source(&conn, "src-2", "Feed Two", "https://example.com/two.xml");
    FeedRepository::upsert_items(
        &conn,
        &[
            item_input("src-1", "a", "A", "hello world", "2026-08-01T08:00:00Z"),
            item_input("src-1", "b", "B", "goodbye moon", "2026-08-01T09:00:00Z"),
            item_input("src-2", "c", "C", "hello again", "2026-08-01T10:00:00Z"),
        ],
    )
    .expect("upsert items");

    let hits = FeedRepository::search(&conn, "hello", None, 50).expect("search");
    assert_eq!(hits.len(), 2);

    let scoped = FeedRepository::search(&conn, "hello", Some("src-2"), 50).expect("search");
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0].id, "item-src-2-c");
    assert_eq!(scoped[0].source_title, "Feed Two");

    // 内容更新后 FTS 同步：旧词消失、新词出现。
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-1",
            "a",
            "A",
            "hello world updated",
            "2026-08-01T08:00:00Z",
        )],
    )
    .expect("update content");
    assert_eq!(
        FeedRepository::search(&conn, "goodbye", None, 50)
            .expect("search goodbye")
            .len(),
        1
    );
    assert_eq!(
        FeedRepository::search(&conn, "updated", None, 50)
            .expect("search updated")
            .len(),
        1
    );

    // 删除条目后 FTS 清空。
    FeedRepository::delete_source(&conn, "src-2").expect("delete src-2");
    assert!(FeedRepository::search(&conn, "again", None, 50)
        .expect("search after delete")
        .is_empty());
}

#[test]
fn unified_search_respects_view_source_cursor_and_bulk_read() {
    let conn = test_conn();
    insert_source(
        &conn,
        "src-1",
        "Rust 中文周刊",
        "https://example.com/one.xml",
    );
    insert_source(&conn, "src-2", "Other Feed", "https://example.com/two.xml");
    FeedRepository::upsert_items(
        &conn,
        &[
            item_input(
                "src-1",
                "a",
                "A",
                "这是中文短语测试",
                "2026-08-01T08:00:00Z",
            ),
            item_input("src-1", "b", "B", "ordinary", "2026-08-01T09:00:00Z"),
            item_input("src-2", "c", "C", "中文短语", "2026-08-01T10:00:00Z"),
        ],
    )
    .expect("upsert items");

    let query = FeedItemQuery {
        view: FeedView::Inbox,
        source_id: Some("src-1".to_string()),
        search: Some("中文".to_string()),
        received_after: None,
        cursor: None,
        limit: 50,
    };
    let hits = FeedRepository::list_items(&conn, &query, now()).expect("unified search");
    assert_eq!(hits.len(), 2, "正文子串与来源标题都应匹配");
    assert!(hits.iter().all(|item| item.source_id == "src-1"));

    let affected = FeedRepository::mark_items_read(&conn, &query, now()).expect("bulk read");
    assert_eq!(affected, 2, "批量操作必须复用完全相同的搜索过滤条件");
    let after_read =
        FeedRepository::list_items(&conn, &query, now()).expect("inbox after bulk read");
    assert_eq!(after_read.len(), 2, "已读不应自动离开未归档收件箱");
    assert!(after_read.iter().all(|item| item.is_read));
}

#[test]
fn item_summaries_and_search_use_source_display_title() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Original", "https://example.com/one.xml");
    FeedRepository::update_source(
        &conn,
        "src-1",
        &FeedSourcePatch {
            title_override: Some("Display title".to_string()),
            ..Default::default()
        },
        now(),
    )
    .expect("override title");
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-1",
            "a",
            "Article",
            "ordinary body",
            "2026-08-01T08:00:00Z",
        )],
    )
    .expect("upsert");

    let query = FeedItemQuery {
        view: FeedView::All,
        source_id: None,
        search: Some("Display".to_string()),
        received_after: None,
        cursor: None,
        limit: 50,
    };
    let hits = FeedRepository::list_items(&conn, &query, now()).expect("search title");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].source_title, "Display title");
    let detail = FeedRepository::get_item_detail(&conn, &hits[0].id)
        .expect("detail")
        .expect("exists");
    assert_eq!(detail.summary.source_title, "Display title");
}

#[test]
fn search_escapes_special_characters_and_rejects_empty() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");
    FeedRepository::upsert_items(
        &conn,
        &[
            item_input(
                "src-1",
                "cpp",
                "C++ Guide",
                "learning C++ fast",
                "2026-08-01T08:00:00Z",
            ),
            item_input(
                "src-1",
                "dash",
                "Dash Note",
                "foo-bar wiring",
                "2026-08-01T09:00:00Z",
            ),
            item_input(
                "src-1",
                "quote",
                "Quoted",
                "a \"quoted\" phrase",
                "2026-08-01T10:00:00Z",
            ),
        ],
    )
    .expect("upsert items");

    // 特殊字符查询不得触发 MATCH 语法错误，按分词匹配。
    assert_eq!(
        FeedRepository::search(&conn, "C++", None, 50)
            .expect("search C++")
            .len(),
        1
    );
    assert_eq!(
        FeedRepository::search(&conn, "foo-bar", None, 50)
            .expect("search foo-bar")
            .len(),
        1
    );
    assert_eq!(
        FeedRepository::search(&conn, "\"quoted\"", None, 50)
            .expect("search quoted")
            .len(),
        1
    );
    assert!(
        FeedRepository::search(&conn, "hello* OR world", None, 50)
            .expect("search with operators")
            .is_empty(),
        "operators must be escaped into literal tokens, not executed"
    );

    assert!(
        FeedRepository::search(&conn, "", None, 50).is_err(),
        "empty query is a validation error"
    );
    assert!(
        FeedRepository::search(&conn, "   ", None, 50).is_err(),
        "whitespace-only query is a validation error"
    );
}

// ── 摘要截断 ───────────────────────────────────────────────

#[test]
fn excerpt_truncates_to_240_unicode_scalars_without_splitting_utf8() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");

    // 250 个中文字符 + 尾部 emoji：验证 scalar 计数与 UTF-8 安全截断。
    let long_body = format!("{}🧪", "长".repeat(250));
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-1",
            "a",
            "A",
            &long_body,
            "2026-08-01T08:00:00Z",
        )],
    )
    .expect("upsert item");

    let items = FeedRepository::list_items(
        &conn,
        &FeedItemQuery {
            view: FeedView::All,
            source_id: None,
            search: None,
            received_after: None,
            cursor: None,
            limit: 10,
        },
        now(),
    )
    .expect("list");
    let excerpt = &items[0].excerpt;
    assert_eq!(
        excerpt.chars().count(),
        240,
        "excerpt must be exactly 240 Unicode scalars"
    );
    assert!(
        excerpt.chars().all(|c| c == '长'),
        "excerpt must not contain a split surrogate"
    );

    // 短正文原样返回。
    FeedRepository::upsert_items(
        &conn,
        &[item_input(
            "src-1",
            "b",
            "B",
            "short body",
            "2026-08-01T09:00:00Z",
        )],
    )
    .expect("upsert short");
    let items = FeedRepository::list_items(
        &conn,
        &FeedItemQuery {
            view: FeedView::All,
            source_id: None,
            search: None,
            received_after: None,
            cursor: None,
            limit: 10,
        },
        now(),
    )
    .expect("list");
    let short = items
        .iter()
        .find(|item| item.id == "item-src-1-b")
        .expect("short item");
    assert_eq!(short.excerpt, "short body");
}

// ── DTO 序列化 ─────────────────────────────────────────────

#[test]
fn feed_view_serde_roundtrip_uses_snake_case() {
    for (expected, view) in [
        ("inbox", FeedView::Inbox),
        ("today", FeedView::Today),
        ("all", FeedView::All),
        ("starred", FeedView::Starred),
        ("archived", FeedView::Archived),
    ] {
        let json = serde_json::to_value(view).expect("serialize view");
        assert_eq!(json.as_str().expect("string"), expected);
        let parsed: FeedView = serde_json::from_value(json).expect("deserialize view");
        assert_eq!(parsed, view);
    }
}

#[test]
fn item_query_roundtrip_uses_camel_case() {
    let query = FeedItemQuery {
        view: FeedView::Inbox,
        source_id: Some("src-1".to_string()),
        search: Some("rust".to_string()),
        received_after: None,
        cursor: Some(crate::feed::model::FeedPageCursor {
            sort_at: "2026-08-01T08:00:00Z".to_string(),
            row_id: 7,
        }),
        limit: 50,
    };
    let json = serde_json::to_value(&query).expect("serialize query");
    let map = json.as_object().expect("object");
    assert!(map.contains_key("receivedAfter"));
    assert!(map.contains_key("sourceId"));
    assert!(map.contains_key("search"));
    assert!(map.contains_key("cursor"));
    assert!(!map.contains_key("received_after"));

    let parsed: FeedItemQuery = serde_json::from_value(json).expect("deserialize query");
    assert_eq!(parsed.view, FeedView::Inbox);
    assert_eq!(parsed.cursor.expect("cursor").row_id, 7);
}

// ── 同步状态与事务回滚 ─────────────────────────────────────

#[test]
fn upsert_rolls_back_entire_batch_on_invalid_item() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");

    let mut valid = item_input("src-1", "a", "A", "body", "2026-08-01T08:00:00Z");
    let mut invalid = item_input("src-1", "b", "B", "body", "2026-08-01T09:00:00Z");
    invalid.external_key = String::new();
    valid.source_id = "src-1".to_string();

    let error = FeedRepository::upsert_items(&conn, &[valid, invalid])
        .expect_err("batch with invalid item must fail");
    assert!(error.to_string().contains("feed_item_external_key_empty"));

    assert_eq!(
        FeedRepository::count_items(&conn, "src-1").expect("count"),
        0,
        "整个批次必须回滚，不得留下部分行"
    );
}

#[test]
fn update_source_sync_state_overwrites_sync_columns() {
    let conn = test_conn();
    insert_source(&conn, "src-1", "Feed One", "https://example.com/one.xml");

    let ok = FeedRepository::update_source_sync_state(
        &conn,
        "src-1",
        &crate::feed::model::FeedSourceSyncState {
            etag: Some("\"abc\"".to_string()),
            last_modified: Some("Wed, 12 Aug 2026 08:00:00 GMT".to_string()),
            last_checked_at: "2026-08-12T08:00:00Z".to_string(),
            last_success_at: Some("2026-08-12T08:00:00Z".to_string()),
            next_fetch_at: "2026-08-12T09:00:00Z".to_string(),
            consecutive_failures: 0,
            last_error_code: None,
            last_error_at: None,
        },
    )
    .expect("write sync state");
    assert!(ok);

    let source = FeedRepository::get_source(&conn, "src-1")
        .expect("get source")
        .expect("exists");
    assert_eq!(source.etag.as_deref(), Some("\"abc\""));
    assert_eq!(source.consecutive_failures, 0);
    assert_eq!(
        source.next_fetch_at.as_deref(),
        Some("2026-08-12T09:00:00Z")
    );

    // 失败态覆盖：保留 validators，写入错误码与失败计数。
    FeedRepository::update_source_sync_state(
        &conn,
        "src-1",
        &crate::feed::model::FeedSourceSyncState {
            etag: source.etag.clone(),
            last_modified: source.last_modified.clone(),
            last_checked_at: "2026-08-12T08:30:00Z".to_string(),
            last_success_at: None,
            next_fetch_at: "2026-08-12T08:45:00Z".to_string(),
            consecutive_failures: 1,
            last_error_code: Some("feed_http_error_500".to_string()),
            last_error_at: Some("2026-08-12T08:30:00Z".to_string()),
        },
    )
    .expect("write failure state");

    let source = FeedRepository::get_source(&conn, "src-1")
        .expect("get source")
        .expect("exists");
    assert_eq!(source.consecutive_failures, 1);
    assert_eq!(
        source.last_error_code.as_deref(),
        Some("feed_http_error_500")
    );
    assert_eq!(
        source.etag.as_deref(),
        Some("\"abc\""),
        "失败不得清除 validators"
    );
    assert_eq!(source.last_success_at, None);

    let negative = FeedRepository::update_source_sync_state(
        &conn,
        "src-1",
        &crate::feed::model::FeedSourceSyncState {
            etag: None,
            last_modified: None,
            last_checked_at: "2026-08-12T08:30:00Z".to_string(),
            last_success_at: None,
            next_fetch_at: "2026-08-12T08:45:00Z".to_string(),
            consecutive_failures: -1,
            last_error_code: None,
            last_error_at: None,
        },
    );
    assert!(negative.is_err(), "负失败计数必须被拒绝");
}

// ── 到期源查询 ─────────────────────────────────────────────

#[test]
fn list_due_sources_returns_only_enabled_due_sources_with_limit() {
    let conn = test_conn();
    // 5 个源：1 个禁用、2 个到期、1 个未到期、1 个从未同步。
    for (id, enabled, next_fetch) in [
        ("src-disabled", "0", "2026-08-01T00:00:00Z"),
        ("src-due-1", "1", "2026-08-01T00:00:00Z"),
        ("src-due-2", "1", "2026-08-01T00:00:00Z"),
        ("src-future", "1", "2099-01-01T00:00:00Z"),
        ("src-never", "1", ""), // next_fetch_at NULL
    ] {
        conn.execute(
            "INSERT INTO feed_sources
             (id, feed_url, title, is_enabled, next_fetch_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, NULLIF(?5, ''), '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
            rusqlite::params![id, format!("https://example.com/{id}.xml"), id, enabled, next_fetch],
        )
        .expect("insert source");
    }

    let due = FeedRepository::list_due_sources(&conn, "2026-08-10T00:00:00Z", 2).expect("list due");
    assert_eq!(due.len(), 2, "limit 2");
    let ids: Vec<&str> = due.iter().map(|s| s.id.as_str()).collect();
    assert!(!ids.contains(&"src-disabled"), "禁用源不返回");
    assert!(!ids.contains(&"src-future"), "未到期源不返回");
    assert!(
        ids.contains(&"src-never"),
        "从未同步的源（next_fetch_at NULL）优先返回"
    );

    let all =
        FeedRepository::list_due_sources(&conn, "2026-08-10T00:00:00Z", 10).expect("list all due");
    assert_eq!(all.len(), 3, "到期源共 3 个（含 never）");
}
