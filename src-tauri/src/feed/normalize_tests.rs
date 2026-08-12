//! `normalize` 契约测试：由阶段 0 全部 Feed fixture 驱动。
//!
//! 断言：格式解析、稳定键、绝对链接、危险节点清理、远程图片标记、UTF-8、
//! 末尾换行、degraded 回退与稳定错误码。

use sha2::{Digest, Sha256};

use super::normalize::{normalize_feed, NormalizedFeed};
use crate::feed::model::{ConversionStatus, SourcePayloadKind};

fn fixture(name: &str) -> &'static str {
    match name {
        "rss2-basic.xml" => include_str!("../../tests/fixtures/feeds/rss2-basic.xml"),
        "atom-xhtml.xml" => include_str!("../../tests/fixtures/feeds/atom-xhtml.xml"),
        "rss1-rdf.xml" => include_str!("../../tests/fixtures/feeds/rss1-rdf.xml"),
        "json-feed.json" => include_str!("../../tests/fixtures/feeds/json-feed.json"),
        "duplicate-guid.xml" => include_str!("../../tests/fixtures/feeds/duplicate-guid.xml"),
        "item-update-v1.xml" => include_str!("../../tests/fixtures/feeds/item-update-v1.xml"),
        "item-update-v2.xml" => include_str!("../../tests/fixtures/feeds/item-update-v2.xml"),
        "malformed.xml" => include_str!("../../tests/fixtures/feeds/malformed.xml"),
        "xxe.xml" => include_str!("../../tests/fixtures/feeds/xxe.xml"),
        "unsafe-html.xml" => include_str!("../../tests/fixtures/feeds/unsafe-html.xml"),
        "relative-links.xml" => include_str!("../../tests/fixtures/feeds/relative-links.xml"),
        _ => panic!("unknown fixture {name}"),
    }
}

fn normalize(name: &str, source_id: &str) -> NormalizedFeed {
    normalize_feed(fixture(name).as_bytes(), source_id).unwrap_or_else(|e| {
        panic!("fixture {name} failed to normalize: {e}");
    })
}

// ── 格式 fixture ───────────────────────────────────────────

#[test]
fn rss2_basic_normalizes_all_items() {
    let feed = normalize("rss2-basic.xml", "src-1");
    assert_eq!(feed.title, "Example Tech Blog");
    assert_eq!(feed.site_url.as_deref(), Some("https://example.com/blog"));
    assert_eq!(
        feed.language.as_deref(),
        Some("en-us"),
        "feed-rs 规范化语言标签为小写"
    );
    assert_eq!(feed.items.len(), 3);

    let first = &feed.items[0];
    assert_eq!(first.external_key, "example-tech-blog-2026-08-01");
    assert_eq!(
        first.canonical_url.as_deref(),
        Some("https://example.com/blog/first")
    );
    assert_eq!(first.author_name.as_deref(), Some("alice@example.com"));
    assert_eq!(first.published_at.as_deref(), Some("2026-08-01T08:00:00Z"));
    assert_eq!(first.source_payload_kind, SourcePayloadKind::Html);
    assert!(first.content_markdown.contains("First paragraph"));
    assert!(
        first.content_markdown.contains("**emphasis**"),
        "HTML strong 转为粗体 markdown"
    );
    assert!(first.content_text.contains("First paragraph with emphasis"));
    assert!(
        first.content_markdown.ends_with('\n'),
        "末尾必须保留一个换行"
    );
    assert_eq!(first.conversion_status, ConversionStatus::Ok);
}

#[test]
fn atom_xhtml_converts_xhtml_content() {
    let feed = normalize("atom-xhtml.xml", "src-1");
    assert_eq!(feed.items.len(), 2);

    let first = &feed.items[0];
    assert_eq!(first.external_key, "https://example.com/atom/xhtml-entry");
    // feed-rs 将 xhtml 与 html 统一映射为 text/html；两者都经 htmd 转换。
    assert_eq!(first.source_payload_kind, SourcePayloadKind::Html);
    assert!(
        first.content_markdown.contains("XHTML body paragraph"),
        "xhtml content must convert to markdown"
    );
    assert!(first.content_markdown.contains("**bold**"));
    assert!(
        first.summary_markdown.contains("Summary with"),
        "summary 转 markdown"
    );
    assert_eq!(first.published_at.as_deref(), Some("2026-08-01T07:30:00Z"));

    let second = &feed.items[1];
    assert_eq!(second.source_payload_kind, SourcePayloadKind::Text);
    assert_eq!(second.content_markdown.trim(), "Plain text body only.");
}

#[test]
fn rss1_rdf_normalizes() {
    let feed = normalize("rss1-rdf.xml", "src-1");
    assert_eq!(feed.items.len(), 2);
    assert_eq!(
        feed.items[0].canonical_url.as_deref(),
        Some("https://example.com/rdf/one")
    );
    assert_eq!(
        feed.items[0].published_at.as_deref(),
        Some("2026-08-01T08:00:00Z")
    );
    // RSS1 无 atom:id 时 feed-rs 合成确定性 id：跨解析必须稳定（可去重）。
    let again = normalize("rss1-rdf.xml", "src-1");
    assert_eq!(feed.items[0].external_key, again.items[0].external_key);
    assert_ne!(feed.items[0].external_key, feed.items[1].external_key);
    assert!(
        feed.items[0].external_key.starts_with("53e0"),
        "got: {}",
        feed.items[0].external_key
    );
}

#[test]
fn json_feed_normalizes() {
    let feed = normalize("json-feed.json", "src-1");
    assert_eq!(feed.items.len(), 3);

    let first = &feed.items[0];
    assert_eq!(first.external_key, "https://example.com/json/one");
    assert_eq!(first.author_name.as_deref(), Some("Fixture Author"));
    assert!(first
        .content_markdown
        .contains("[a link](https://example.com/json/target)"));

    let second = &feed.items[1];
    assert_eq!(second.source_payload_kind, SourcePayloadKind::Text);

    let third = &feed.items[2];
    assert!(
        third
            .content_markdown
            .contains("![photo](https://cdn.example.com/json/photo.png)"),
        "远程图片保留标准 markdown 图片语法供前端占位"
    );
}

// ── 稳定键与更新对 ─────────────────────────────────────────

#[test]
fn duplicate_guid_preserves_external_keys() {
    let feed = normalize("duplicate-guid.xml", "src-1");
    assert_eq!(feed.items.len(), 3);
    assert_eq!(feed.items[0].external_key, feed.items[1].external_key);
    assert_eq!(feed.items[0].external_key, "duplicate-guid-2026-08-01");
    assert_ne!(feed.items[2].external_key, feed.items[0].external_key);
}

#[test]
fn item_update_pair_shares_external_key_but_changes_hash() {
    let v1 = normalize("item-update-v1.xml", "src-1");
    let v2 = normalize("item-update-v2.xml", "src-1");
    assert_eq!(v1.items.len(), 2);
    assert_eq!(v2.items.len(), 2);

    let a1 = &v1.items[0];
    let a2 = &v2.items[0];
    assert_eq!(a1.external_key, "https://example.com/updates/stable-id");
    assert_eq!(a2.external_key, a1.external_key, "GUID/ID 保持不变");
    assert_eq!(a1.published_at, a2.published_at, "published 不变");
    assert_ne!(a1.content_hash, a2.content_hash, "正文变化必须改变 hash");
    assert!(a1.content_markdown.contains("v1 body"));
    assert!(a2.content_markdown.contains("v2 body"));
    assert_eq!(
        a2.source_updated_at.as_deref(),
        Some("2026-08-02T08:00:00Z")
    );

    let c1 = &v1.items[1];
    let c2 = &v2.items[1];
    assert_eq!(c1.content_hash, c2.content_hash, "对照条目 v1/v2 完全一致");
    assert_eq!(c1.title, "Unchanged control entry");
}

#[test]
fn external_key_falls_back_to_sha256_when_no_id_or_link() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
  <title>NoKey Feed</title>
  <link>https://example.com/nokey</link>
  <item>
    <title>Falling Back</title>
    <description>body</description>
    <pubDate>Sat, 01 Aug 2026 08:00:00 GMT</pubDate>
  </item>
</channel></rss>"#;
    let feed = normalize_feed(xml.as_bytes(), "src-42").expect("no-key feed normalizes");
    let item = &feed.items[0];
    let mut hasher = Sha256::new();
    hasher.update(b"src-42");
    hasher.update(b"\0");
    hasher.update(b"Falling Back");
    hasher.update(b"\0");
    hasher.update(b"2026-08-01T08:00:00Z");
    let expected = hex::encode(hasher.finalize());
    assert_eq!(item.external_key, expected);
}

// ── 错误路径 ───────────────────────────────────────────────

#[test]
fn malformed_feed_is_parse_error() {
    let error = normalize_feed(fixture("malformed.xml").as_bytes(), "src-1")
        .expect_err("malformed xml must fail");
    assert!(
        error.to_string().contains("feed_parse_failed"),
        "got: {error}"
    );
}

#[test]
fn xxe_feed_rejected_before_parse() {
    let error = normalize_feed(fixture("xxe.xml").as_bytes(), "src-1")
        .expect_err("DOCTYPE/ENTITY must be rejected before parsing");
    assert!(
        error.to_string().contains("feed_xml_unsafe_declaration"),
        "got: {error}"
    );
}

// ── 安全转换 ───────────────────────────────────────────────

#[test]
fn unsafe_html_is_sanitized() {
    let feed = normalize("unsafe-html.xml", "src-1");
    assert_eq!(feed.items.len(), 2);

    let dangerous = &feed.items[0];
    let markdown = &dangerous.content_markdown;
    assert!(
        !markdown.contains("<script") && !markdown.contains("alert"),
        "script 必须清除"
    );
    assert!(!markdown.contains("<style"), "style 必须清除");
    assert!(!markdown.contains("iframe"), "iframe 必须清除");
    assert!(
        !markdown.contains("<form") && !markdown.contains("</form"),
        "表单必须清除"
    );
    assert!(
        !markdown.contains("javascript:"),
        "javascript: 链接必须清除"
    );
    assert!(
        !markdown.contains("onclick") && !markdown.contains("onerror"),
        "事件属性必须清除"
    );
    assert!(
        markdown.contains("Paragraph with an event attribute"),
        "节点文本保留"
    );

    let media = &feed.items[1];
    assert!(
        media
            .content_markdown
            .contains("![remote photo](https://cdn.example.com/unsafe/photo.png)"),
        "远程图片必须保留为 HTTPS 绝对链接的 markdown 图片"
    );
    assert!(
        media
            .content_markdown
            .contains("[relative link](https://example.com/unsafe/relative)"),
        "相对链接必须以文章 URL 为基准解析为绝对链接"
    );
}

#[test]
fn relative_links_resolved_against_canonical_url() {
    let feed = normalize("relative-links.xml", "src-1");

    let first = &feed.items[0];
    assert_eq!(
        first.canonical_url.as_deref(),
        Some("https://example.com/articles/one")
    );
    let md = &first.content_markdown;
    assert!(
        md.contains("[absolute-path link](https://example.com/articles/two)"),
        "/articles/two 解析为绝对链接"
    );
    assert!(
        md.contains("[relative-path link](https://example.com/articles/sibling)"),
        "相对路径以文章 URL 为基准"
    );
    assert!(
        md.contains("![parent-relative image](https://example.com/images/one.png)"),
        "父级相对图片解析"
    );

    let second = &feed.items[1];
    let md = &second.content_markdown;
    assert!(md.contains("[absolute https link](https://example.com/absolute)"));
    assert!(
        !md.contains("[mailto link](mailto:") && md.contains("mailto link"),
        "mailto 不安全链接必须降级为纯文本"
    );
}

// ── 截断与 degraded ────────────────────────────────────────

#[test]
fn title_truncated_to_500_scalars_and_degraded() {
    let long_title = "长".repeat(600);
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
  <title>Truncate Feed</title>
  <link>https://example.com/truncate</link>
  <item>
    <guid>truncate-1</guid>
    <title>{long_title}</title>
    <description>body</description>
    <pubDate>Sat, 01 Aug 2026 08:00:00 GMT</pubDate>
  </item>
</channel></rss>"#
    );
    let feed = normalize_feed(xml.as_bytes(), "src-1").expect("truncated title feed");
    assert_eq!(feed.items[0].title.chars().count(), 500);
    assert_eq!(feed.items[0].conversion_status, ConversionStatus::Degraded);
}

#[test]
fn content_truncated_to_4_mib_and_degraded() {
    let huge_body = "x".repeat(4 * 1024 * 1024 + 1024);
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
  <title>Big Feed</title>
  <link>https://example.com/big</link>
  <item>
    <guid>big-1</guid>
    <title>Big body</title>
    <description><![CDATA[<p>{huge_body}</p>]]></description>
    <pubDate>Sat, 01 Aug 2026 08:00:00 GMT</pubDate>
  </item>
</channel></rss>"#
    );
    let feed = normalize_feed(xml.as_bytes(), "src-1").expect("big feed");
    let item = &feed.items[0];
    assert!(
        item.content_markdown.len() <= 4 * 1024 * 1024,
        "content must be capped at 4 MiB"
    );
    assert_eq!(item.conversion_status, ConversionStatus::Degraded);
    assert!(item.content_markdown.ends_with('\n'));
    assert!(
        std::str::from_utf8(item.content_markdown.as_bytes()).is_ok(),
        "截断不得切坏 UTF-8"
    );
}

#[test]
fn utf8_not_split_at_truncation_boundary() {
    // 中文字符 3 字节 + emoji 4 字节，跨 4 MiB 边界的多字节序列不得被切断。
    let unit = "长🧪";
    let repeat = (4 * 1024 * 1024) / 7 + 10;
    let huge_body = unit.repeat(repeat);
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
  <title>Utf8 Feed</title>
  <link>https://example.com/utf8</link>
  <item>
    <guid>utf8-1</guid>
    <title>Utf8 body</title>
    <description><![CDATA[<p>{huge_body}</p>]]></description>
    <pubDate>Sat, 01 Aug 2026 08:00:00 GMT</pubDate>
  </item>
</channel></rss>"#
    );
    let feed = normalize_feed(xml.as_bytes(), "src-1").expect("utf8 feed");
    let item = &feed.items[0];
    assert!(item.content_markdown.len() <= 4 * 1024 * 1024);
    assert_eq!(item.conversion_status, ConversionStatus::Degraded);
}

// ── 内容选择与纯文本 ───────────────────────────────────────

#[test]
fn summary_only_entry_uses_summary() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
  <title>Summary Only</title>
  <link>https://example.com/summary</link>
  <item>
    <guid>summary-1</guid>
    <title>Summary Item</title>
    <link>https://example.com/summary/one</link>
    <description><![CDATA[<p>Only a <em>summary</em> body.</p>]]></description>
    <pubDate>Sat, 01 Aug 2026 08:00:00 GMT</pubDate>
  </item>
</channel></rss>"#;
    let feed = normalize_feed(xml.as_bytes(), "src-1").expect("summary-only feed");
    let item = &feed.items[0];
    assert!(
        item.content_markdown.contains("Only a *summary* body."),
        "无 content 时必须回退到 summary；got: {}",
        item.content_markdown
    );
    assert_eq!(item.source_payload_kind, SourcePayloadKind::Html);
}

#[test]
fn missing_title_uses_fallback() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
  <title>No Title Feed</title>
  <link>https://example.com/notitle</link>
  <item>
    <guid>notitle-1</guid>
    <description>body</description>
    <pubDate>Sat, 01 Aug 2026 08:00:00 GMT</pubDate>
  </item>
</channel></rss>"#;
    let feed = normalize_feed(xml.as_bytes(), "src-1").expect("no-title feed");
    assert_eq!(feed.items[0].title, "（无标题）");
}

#[test]
fn content_text_is_deterministic_plain_text() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
  <title>Text Feed</title>
  <link>https://example.com/text</link>
  <item>
    <guid>text-1</guid>
    <title>Text Item</title>
    <link>https://example.com/text/one</link>
    <description><![CDATA[
      <h1>Heading</h1>
      <p>Body with <strong>bold</strong> and <a href="https://example.com/target">a link</a>.</p>
      <pre><code>let x = 1;</code></pre>
      <blockquote>Quoted text</blockquote>
      <ul><li>one</li><li>two</li></ul>
    ]]></description>
    <pubDate>Sat, 01 Aug 2026 08:00:00 GMT</pubDate>
  </item>
</channel></rss>"#;
    let feed = normalize_feed(xml.as_bytes(), "src-1").expect("text feed");
    let text = &feed.items[0].content_text;
    assert!(text.contains("Heading"));
    assert!(text.contains("Body with bold and a link"));
    assert!(text.contains("let x = 1;"), "代码块保留文本；got: {text}");
    assert!(text.contains("Quoted text"));
    assert!(text.contains("one"));
    assert!(text.contains("two"));
    assert!(!text.contains("**") && !text.contains("```") && !text.contains("[a link]"));
}

// ── 输入防抖 ───────────────────────────────────────────────

#[test]
fn xxe_declaration_detected_case_insensitively() {
    for prefix in [
        "<!DOCTYPE",
        "<!doctype",
        "<!DocType",
        "<!ENTITY",
        "<!entity",
    ] {
        let xml = format!(
            "{prefix} junk\n<rss version=\"2.0\"><channel><title>x</title></channel></rss>"
        );
        let error = normalize_feed(xml.as_bytes(), "src-1").expect_err("unsafe declaration");
        assert!(
            error.to_string().contains("feed_xml_unsafe_declaration"),
            "prefix {prefix} must be rejected; got: {error}"
        );
    }
}

#[test]
fn empty_input_is_parse_error() {
    let error = normalize_feed(b"", "src-1").expect_err("empty input fails");
    assert!(error.to_string().contains("feed_parse_failed"));
}
