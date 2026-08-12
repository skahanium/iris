//! OPML 导入导出契约测试（内存 SQLite，无网络、无 UI、无 Vault 写入）。
//!
//! 覆盖（阶段 5.1）：嵌套分组、重复 URL 去重、缺字段、XXE/DTD 拒绝、
//! 5 MiB 上限、UTF-8、标题截断、导入幂等、导出→导入往返、导出内容卫生
//! （不含 ETag/阅读状态/本地 ID）。

use chrono::Utc;
use rusqlite::Connection;

use super::opml::{
    export_opml, import_opml, parse_opml, OPML_MAX_BYTES, OPML_MAX_DEPTH, OPML_MAX_OUTLINES,
};
use super::repository::FeedRepository;
use crate::storage::migrate::migrate_up;

fn test_conn() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON")
        .expect("enable foreign keys");
    migrate_up(&conn).expect("migrate up");
    conn
}

fn insert_source(conn: &Connection, id: &str, title: &str, feed_url: &str, folder: &str) {
    conn.execute(
        "INSERT INTO feed_sources (id, feed_url, title, folder_path, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
        rusqlite::params![id, feed_url, title, folder],
    )
    .expect("insert source fixture");
}

fn nested_opml() -> &'static str {
    include_str!("../../tests/fixtures/opml/nested.opml")
}

fn duplicate_opml() -> &'static str {
    include_str!("../../tests/fixtures/opml/duplicate-urls.opml")
}

// ── 解析 ────────────────────────────────────────────────────

#[test]
fn parse_nested_folders_derive_folder_path() {
    let outlines = parse_opml(nested_opml().as_bytes()).expect("parse");
    assert_eq!(outlines.len(), 3, "三级嵌套 fixture 产出 3 个订阅大纲");

    let rust = outlines
        .iter()
        .find(|outline| outline.xml_url.as_deref() == Some("https://example.com/feeds/rust.xml"))
        .expect("rust feed");
    assert_eq!(rust.folder_path, "技术/Rust");
    assert_eq!(rust.title, "Example Rust Feed");

    let systems = outlines
        .iter()
        .find(|outline| outline.xml_url.as_deref() == Some("https://example.com/feeds/systems.xml"))
        .expect("systems feed");
    assert_eq!(systems.folder_path, "技术");

    let solo = outlines
        .iter()
        .find(|outline| outline.xml_url.as_deref() == Some("https://example.com/feeds/solo.xml"))
        .expect("solo feed");
    assert_eq!(solo.folder_path, "未分组");
    assert_eq!(solo.html_url.as_deref(), Some("https://example.com/solo"));
}

#[test]
fn parse_rejects_dtd_and_entity() {
    let xxe = r#"<?xml version="1.0"?>
<!DOCTYPE opml [<!ENTITY xxe SYSTEM "file:///etc/hosts">]>
<opml version="2.0"><body><outline text="x" xmlUrl="https://example.com/f.xml"/></body></opml>"#;
    let error = parse_opml(xxe.as_bytes()).expect_err("DTD/ENTITY 必须拒绝");
    assert!(error.to_string().contains("feed_xml_unsafe_declaration"));

    let entity = r#"<?xml version="1.0"?>
<!ENTITY copy "&#169;">
<opml version="2.0"><body><outline text="x" xmlUrl="https://example.com/f.xml"/></body></opml>"#;
    assert!(parse_opml(entity.as_bytes()).is_err(), "外部 ENTITY 拒绝");
}

#[test]
fn parse_truncates_oversized_title() {
    let bytes = include_bytes!("../../tests/fixtures/opml/oversized-title.opml");
    let outlines = parse_opml(bytes).expect("parse");
    assert_eq!(outlines.len(), 1);
    let title = &outlines[0].title;
    assert_eq!(title.chars().count(), 500, "标题截断到 500 scalar");
}

#[test]
fn parse_handles_missing_fields_and_invalid_urls() {
    let opml = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0">
  <body>
    <outline text="纯分组">
      <outline type="rss" xmlUrl="https://example.com/no-title.xml"/>
      <outline text="http 源" type="rss" xmlUrl="http://example.com/insecure.xml"/>
      <outline text="非法源" type="rss" xmlUrl="javascript:alert(1)"/>
      <outline type="rss" text="A&amp;B &lt;C&gt;" xmlUrl="https://example.com/escaped.xml"/>
    </outline>
  </body>
</opml>"#;
    let outlines = parse_opml(opml.as_bytes()).expect("parse");
    // 无标题源用 xmlUrl 兜底；http/javascript URL 归为 None（导入时跳过）。
    let no_title = outlines
        .iter()
        .find(|outline| outline.xml_url.as_deref() == Some("https://example.com/no-title.xml"))
        .expect("no-title feed");
    assert_eq!(no_title.title, "https://example.com/no-title.xml");
    assert_eq!(no_title.folder_path, "纯分组");

    let insecure = outlines
        .iter()
        .find(|outline| outline.title == "http 源")
        .expect("insecure feed");
    assert_eq!(insecure.xml_url, None, "非 HTTPS URL 不产出可订阅大纲");

    assert!(
        outlines.iter().any(|outline| outline.title == "A&B <C>"),
        "XML 实体按文本解码，不二次转义"
    );
}

#[test]
fn parse_preserves_utf8() {
    let opml = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0"><body>
  <outline text="中文订阅 🚀" type="rss" xmlUrl="https://example.com/cn.xml"/>
</body></opml>"#;
    let outlines = parse_opml(opml.as_bytes()).expect("parse");
    assert_eq!(outlines[0].title, "中文订阅 🚀");
}

#[test]
fn parse_ignores_comments_and_unknown_fields() {
    let opml = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0">
  <!-- 注释不产出大纲 -->
  <head><title>Ignored</title></head>
  <body>
    <outline text="未知属性源" category="ignored" created="2020-01-01"
             type="rss" xmlUrl="https://example.com/unknown.xml" extra="x"/>
  </body>
</opml>"#;
    let outlines = parse_opml(opml.as_bytes()).expect("parse");
    assert_eq!(outlines.len(), 1);
    assert_eq!(outlines[0].title, "未知属性源");
}

// ── 导入 ────────────────────────────────────────────────────

#[test]
fn import_is_idempotent_and_dedupes() {
    let conn = test_conn();
    let first = import_opml(&conn, duplicate_opml(), false).expect("first import");
    assert_eq!(first.added, 2, "重复 URL 只保留首个");
    assert_eq!(first.skipped, 1, "重复出现的大纲计 skipped");
    assert_eq!(first.added_ids.len(), 2);

    let second = import_opml(&conn, duplicate_opml(), false).expect("second import");
    assert_eq!(second.added, 0, "幂等：不重复新增");
    assert_eq!(second.updated, 0, "值无变化不计数");
    // 第二次：重复大纲 + 两个已存在且无变化的唯一 URL 均计入 skipped。
    assert_eq!(second.skipped, 3);

    let sources = FeedRepository::list_sources(&conn).expect("list");
    assert_eq!(sources.len(), 2, "库中只有 2 个唯一源");
    assert_eq!(sources[0].title, "First occurrence", "首个出现优先");
}

#[test]
fn import_updates_folder_and_override_without_resetting_state() {
    let conn = test_conn();
    // 预置「未分组」组里的 solo 源：旧分组、自定义标题、暂停中、带 etag。
    conn.execute(
        "INSERT INTO feed_sources
         (id, feed_url, title, title_override, folder_path, is_enabled, etag, next_fetch_at,
          created_at, updated_at)
         VALUES ('solo', 'https://example.com/feeds/solo.xml', 'Feed 原标题', '用户自定义标题',
                 '旧分组', 0, 'etag-abc', '2030-01-01T00:00:00Z',
                 '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
        [],
    )
    .expect("seed existing source");

    let result = import_opml(&conn, nested_opml(), false).expect("import");
    assert_eq!(result.added, 2, "rust/systems 新增");
    assert_eq!(result.updated, 1, "solo 更新 folder 与 override");

    let solo = FeedRepository::get_source(&conn, "solo")
        .expect("get")
        .expect("exists");
    assert_eq!(solo.folder_path, "未分组", "folder 以 OPML 为准");
    assert_eq!(
        solo.title_override.as_deref(),
        Some("Example Solo Feed"),
        "title override 以 OPML 为准"
    );
    assert!(!solo.is_enabled, "暂停状态不被重置");
    assert_eq!(solo.etag.as_deref(), Some("etag-abc"), "etag 不被重置");
    assert_eq!(
        solo.next_fetch_at.as_deref(),
        Some("2030-01-01T00:00:00Z"),
        "调度状态不被重置"
    );
}

#[test]
fn import_dry_run_does_not_write() {
    let conn = test_conn();
    let result = import_opml(&conn, nested_opml(), true).expect("dry run");
    assert_eq!(result.added, 3);
    assert_eq!(result.skipped, 0);
    assert!(FeedRepository::list_sources(&conn)
        .expect("list")
        .is_empty());
}

#[test]
fn import_skips_invalid_and_missing_urls() {
    let conn = test_conn();
    let opml = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0"><body>
  <outline text="http 源" type="rss" xmlUrl="http://example.com/f.xml"/>
  <outline text="相对路径" type="rss" xmlUrl="/feeds/local.xml"/>
  <outline text="内网地址" type="rss" xmlUrl="https://192.168.1.5/f.xml"/>
  <outline text="合法源" type="rss" xmlUrl="https://example.com/ok.xml"/>
</body></opml>"#;
    let result = import_opml(&conn, opml, false).expect("import");
    assert_eq!(result.added, 1, "只有 HTTPS 公网源可导入");
    assert_eq!(result.skipped, 3);
}

#[test]
fn import_requires_valid_utf8() {
    // 非法 UTF-8 序列：0xFF 不是合法 XML 文本。
    let bytes = b"<?xml version=\"1.0\"?><opml version=\"2.0\"><body>\xff</body></opml>";
    assert!(
        parse_opml(bytes).is_err(),
        "非 UTF-8 内容必须报错而不是静默损坏"
    );
}

#[test]
fn import_large_input_is_bounded_by_command_layer() {
    // 命令层（feed_commands）负责 5 MiB 检查；这里只验证常量与传输语义。
    assert_eq!(OPML_MAX_BYTES, 5 * 1024 * 1024);
}

#[test]
fn parse_bounds_outline_count_depth_and_folder_length() {
    let many = format!(
        "<opml><body>{}</body></opml>",
        (0..=OPML_MAX_OUTLINES)
            .map(|i| format!(r#"<outline text="{i}" xmlUrl="https://example.com/{i}.xml"/>"#))
            .collect::<String>()
    );
    assert!(parse_opml(many.as_bytes())
        .expect_err("outline count bound")
        .to_string()
        .contains("feed_opml_too_many_outlines"));

    let nested = format!(
        "<opml><body>{}<outline xmlUrl=\"https://example.com/x.xml\"/>{}</body></opml>",
        "<outline text=\"x\">".repeat(OPML_MAX_DEPTH + 1),
        "</outline>".repeat(OPML_MAX_DEPTH + 1)
    );
    assert!(parse_opml(nested.as_bytes())
        .expect_err("depth bound")
        .to_string()
        .contains("feed_opml_too_deep"));

    let folder = "分".repeat(500);
    let long_folder = format!(
        r#"<opml><body><outline text="{folder}"><outline text="{folder}"><outline text="{folder}"><outline xmlUrl="https://example.com/x.xml"/></outline></outline></outline></body></opml>"#
    );
    assert!(parse_opml(long_folder.as_bytes())
        .expect_err("folder bound")
        .to_string()
        .contains("feed_opml_folder_too_long"));
}

#[test]
fn import_canonicalizes_urls_before_deduplication() {
    let conn = test_conn();
    let xml = r#"<opml><body>
      <outline text="One" xmlUrl="https://EXAMPLE.com:443/feed.xml#fragment"/>
      <outline text="Two" xmlUrl="https://example.com/feed.xml"/>
    </body></opml>"#;
    let result = import_opml(&conn, xml, false).expect("import");
    assert_eq!(result.added, 1);
    assert_eq!(result.skipped, 1);
    let source = FeedRepository::list_sources(&conn)
        .expect("sources")
        .remove(0);
    assert_eq!(source.feed_url, "https://example.com/feed.xml");
}

// ── 导出与往返 ──────────────────────────────────────────────

#[test]
fn export_is_sorted_and_leaks_no_internal_state() {
    let conn = test_conn();
    insert_source(&conn, "s1", "Beta 源", "https://example.com/beta.xml", "");
    conn.execute(
        "UPDATE feed_sources SET site_url = 'https://example.com/beta-site' WHERE id = 's1'",
        [],
    )
    .expect("set site url");
    insert_source(&conn, "s2", "Amp 源", "https://example.com/amp.xml", "技术");
    insert_source(
        &conn,
        "s3",
        "Rust 源",
        "https://example.com/rust.xml",
        "技术/Rust",
    );
    // 同步/阅读状态字段存在但绝不导出。
    conn.execute(
        "UPDATE feed_sources SET etag = 'e', last_error_code = 'feed_http_error_500',
                consecutive_failures = 3, is_enabled = 0
         WHERE id = 's1'",
        [],
    )
    .expect("mark internal state");

    let xml = export_opml(&conn).expect("export");
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(xml.contains("<opml version=\"2.0\">"));
    assert!(xml.contains("xmlUrl=\"https://example.com/rust.xml\""));
    assert!(xml.contains("htmlUrl=\"https://example.com/beta-site\""));

    // 稳定排序：技术 < 技术/Rust < 未分组（空串最先），组内按标题。
    let tech = xml.find("text=\"技术\"").expect("技术组");
    let tech_rust = xml.find("text=\"Rust\"").expect("Rust 组");
    let beta = xml.find("text=\"Beta 源\"").expect("空分组源");
    assert!(beta < tech && tech < tech_rust, "空分组最先，嵌套在后");

    // 不泄漏内部状态：无 etag/错误/阅读状态/本地 ID/时间戳。
    for leak in [
        "etag",
        "last_error",
        "consecutive_failures",
        "read_at",
        "starred_at",
        "archived_at",
        "is_enabled",
        "created_at",
        "\"id\"",
    ] {
        assert!(!xml.contains(leak), "导出不得包含内部字段：{leak}");
    }
}

#[test]
fn export_escapes_xml_special_characters() {
    let conn = test_conn();
    conn.execute(
        "INSERT INTO feed_sources (id, feed_url, title, folder_path, created_at, updated_at)
         VALUES ('s1', 'https://example.com/a&b.xml', '标题 & <引号> \"q\"', '组/子&组',
                 '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
        [],
    )
    .expect("insert source");

    let xml = export_opml(&conn).expect("export");
    assert!(xml.contains("text=\"标题 &amp; &lt;引号&gt; &quot;q&quot;\""));
    assert!(xml.contains("xmlUrl=\"https://example.com/a&amp;b.xml\""));
    assert!(xml.contains("text=\"子&amp;组\""));
}

#[test]
fn export_import_roundtrip_preserves_folders() {
    let conn = test_conn();
    insert_source(&conn, "s1", "Alpha 源", "https://example.com/alpha.xml", "");
    insert_source(
        &conn,
        "s2",
        "Beta 源",
        "https://example.com/beta.xml",
        "技术",
    );
    insert_source(
        &conn,
        "s3",
        "Rust 源",
        "https://example.com/rust.xml",
        "技术/Rust",
    );
    insert_source(
        &conn,
        "s4",
        "阅读源",
        "https://example.com/read.xml",
        "阅读/深/嵌套",
    );

    let xml = export_opml(&conn).expect("export");

    // 清空后导入：订阅关系与分组完整往返。
    conn.execute("DELETE FROM feed_sources", []).expect("wipe");
    let result = import_opml(&conn, &xml, false).expect("reimport");
    assert_eq!(result.added, 4);
    assert_eq!(result.skipped, 0);

    let sources = FeedRepository::list_sources(&conn).expect("list");
    let by_url = |url: &str| {
        sources
            .iter()
            .find(|source| source.feed_url == url)
            .expect("source")
    };
    assert_eq!(by_url("https://example.com/alpha.xml").folder_path, "");
    assert_eq!(by_url("https://example.com/beta.xml").folder_path, "技术");
    assert_eq!(
        by_url("https://example.com/rust.xml").folder_path,
        "技术/Rust"
    );
    assert_eq!(
        by_url("https://example.com/read.xml").folder_path,
        "阅读/深/嵌套"
    );
    assert_eq!(by_url("https://example.com/beta.xml").title, "Beta 源");
    // 往返后源处于启用默认状态，等待首次同步。
    assert!(by_url("https://example.com/rust.xml").is_enabled);
    assert_eq!(by_url("https://example.com/rust.xml").unread_count, 0);
}

#[test]
fn export_empty_library_emits_valid_empty_opml() {
    let conn = test_conn();
    let xml = export_opml(&conn).expect("export");
    assert!(xml.contains("<body>"));
    assert!(xml.contains("</body>"));
    assert!(xml.contains("</opml>"));
    // 空库导出可被解析且不产出大纲。
    let outlines = parse_opml(xml.as_bytes()).expect("parse");
    assert!(outlines.is_empty());
}

#[test]
fn source_by_url_finds_existing_source() {
    let conn = test_conn();
    insert_source(&conn, "s1", "标题", "https://example.com/a.xml", "组");
    let found = FeedRepository::get_source_by_feed_url(&conn, "https://example.com/a.xml")
        .expect("lookup")
        .expect("exists");
    assert_eq!(found.id, "s1");
    assert!(
        FeedRepository::get_source_by_feed_url(&conn, "https://example.com/missing.xml")
            .expect("lookup")
            .is_none()
    );
}

#[test]
fn import_uses_deterministic_ids_but_never_duplicates() {
    let conn = test_conn();
    let first = import_opml(&conn, duplicate_opml(), false).expect("first");
    let second = import_opml(&conn, duplicate_opml(), false).expect("second");
    assert_eq!(first.added_ids.len(), 2);
    assert!(second.added_ids.is_empty());
    // 新增 ID 是稳定 UUID，可被前端用于首次同步。
    let ids = first.added_ids.clone();
    let sources = FeedRepository::list_sources(&conn).expect("list");
    assert_eq!(sources.len(), 2);
    assert!(sources.iter().any(|s| s.id == ids[0]));
    assert!(sources.iter().any(|s| s.id == ids[1]));

    // 导入的源带 created_at/updated_at 且处于启用、等待首次同步状态。
    for id in &ids {
        let source = FeedRepository::get_source(&conn, id)
            .expect("get")
            .expect("exists");
        let created_at = chrono::DateTime::parse_from_rfc3339(&source.created_at)
            .expect("valid timestamp")
            .with_timezone(&Utc);
        assert!(created_at <= Utc::now(), "created_at 不晚于导入时刻");
        assert!(source.is_enabled);
        assert_eq!(source.consecutive_failures, 0);
    }
}
