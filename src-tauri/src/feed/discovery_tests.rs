//! `discover` 契约测试：直接 Feed、HTML alternate、相对 href、重复候选、
//! 同源优先排序、跨协议/私网拒绝、无候选。全部在本地测试服务器上完成。

use super::discovery::{discover, FeedCandidate};
use super::test_http::{TestNetGate, TestResponse, TestServer};

fn html_page(candidates: &[(&str, &str, &str)]) -> String {
    // (href, type, title)
    let mut body = String::from("<!doctype html><html><head><title>Site</title>");
    for (href, feed_type, title) in candidates {
        body.push_str(&format!(
            r#"<link rel="alternate" type="{feed_type}" title="{title}" href="{href}"/>"#
        ));
    }
    body.push_str("</head><body><p>landing page</p></body></html>");
    body
}

fn rss2_fixture() -> &'static str {
    include_str!("../../tests/fixtures/feeds/rss2-basic.xml")
}

#[tokio::test]
async fn discover_direct_feed_returns_single_candidate() {
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(200, rss2_fixture()).header("Content-Type", "application/rss+xml"),
    );

    let candidates = discover(&TestNetGate::default(), &server.url("/feed.xml"))
        .await
        .expect("discover direct feed");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].url, server.url("/feed.xml"));
    assert_eq!(candidates[0].format.as_deref(), Some("rss"));
    assert_eq!(candidates[0].title.as_deref(), Some("Example Tech Blog"));
}

#[tokio::test]
async fn discover_html_alternate_parses_candidates() {
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(
            200,
            html_page(&[
                ("/feed.rss", "application/rss+xml", "RSS Feed"),
                ("/feed.atom", "application/atom+xml", "Atom Feed"),
                ("/feed.json", "application/feed+json", "JSON Feed"),
            ]),
        )
        .header("Content-Type", "text/html"),
    );

    let candidates = discover(&TestNetGate::default(), &server.url("/"))
        .await
        .expect("discover html");
    assert_eq!(candidates.len(), 3);
    assert_eq!(candidates[0].url, server.url("/feed.rss"));
    assert_eq!(candidates[0].format.as_deref(), Some("rss"));
    assert_eq!(candidates[0].title.as_deref(), Some("RSS Feed"));
    assert_eq!(candidates[1].format.as_deref(), Some("atom"));
    assert_eq!(candidates[2].format.as_deref(), Some("json"));
}

#[tokio::test]
async fn discover_ignores_non_feed_link_types() {
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(
            200,
            html_page(&[
                ("/feed.rss", "application/rss+xml", "RSS Feed"),
                ("/stylesheet.css", "text/css", "Stylesheet"),
                (
                    "/opensearch.xml",
                    "application/opensearchdescription+xml",
                    "Search",
                ),
            ]),
        )
        .header("Content-Type", "text/html"),
    );

    let candidates = discover(&TestNetGate::default(), &server.url("/"))
        .await
        .expect("discover html");
    assert_eq!(candidates.len(), 1, "只接受 RSS/Atom/JSON Feed 类型");
    assert_eq!(candidates[0].url, server.url("/feed.rss"));
}

#[tokio::test]
async fn discover_resolves_relative_and_absolute_hrefs() {
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(
            200,
            html_page(&[
                ("relative-feed.xml", "application/rss+xml", "Relative"),
                ("/root-feed.xml", "application/atom+xml", "Root"),
                (
                    "https://cdn.example.com/remote.xml",
                    "application/rss+xml",
                    "Remote",
                ),
            ]),
        )
        .header("Content-Type", "text/html"),
    );

    let candidates = discover(&TestNetGate::default(), &server.url("/blog/index.html"))
        .await
        .expect("discover html");
    let urls: Vec<&str> = candidates.iter().map(|c| c.url.as_str()).collect();
    assert!(
        urls.contains(&server.url("/blog/relative-feed.xml").as_str()),
        "相对 href 以最终 URL 为基准解析；got: {urls:?}"
    );
    assert!(urls.contains(&server.url("/root-feed.xml").as_str()));
    assert!(urls.contains(&"https://cdn.example.com/remote.xml"));
}

#[tokio::test]
async fn discover_deduplicates_candidates_by_url() {
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(
            200,
            html_page(&[
                ("/feed.xml", "application/rss+xml", "First"),
                ("/feed.xml", "application/atom+xml", "Second"),
                ("/feed.xml", "application/rss+xml", "Third"),
            ]),
        )
        .header("Content-Type", "text/html"),
    );

    let candidates = discover(&TestNetGate::default(), &server.url("/"))
        .await
        .expect("discover html");
    assert_eq!(candidates.len(), 1, "同 URL 候选必须去重");
}

#[tokio::test]
async fn discover_sorts_same_host_candidates_first() {
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(
            200,
            html_page(&[
                (
                    "https://example.com/feed.xml",
                    "application/rss+xml",
                    "External",
                ),
                ("/local.xml", "application/rss+xml", "Local"),
                (
                    "https://other.example.net/feed.xml",
                    "application/atom+xml",
                    "Other",
                ),
            ]),
        )
        .header("Content-Type", "text/html"),
    );

    let candidates = discover(&TestNetGate::default(), &server.url("/"))
        .await
        .expect("discover html");
    assert_eq!(candidates.len(), 3);
    assert_eq!(
        candidates[0].url,
        server.url("/local.xml"),
        "同源 host 候选优先；got: {:?}",
        candidates.iter().map(|c| c.url.clone()).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn discover_rejects_unsafe_candidates() {
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(
            200,
            html_page(&[
                (
                    "http://192.168.0.1/feed.xml",
                    "application/rss+xml",
                    "Private",
                ),
                ("javascript:alert(1)", "application/rss+xml", "Script"),
                ("/safe.xml", "application/rss+xml", "Safe"),
            ]),
        )
        .header("Content-Type", "text/html"),
    );

    let candidates = discover(&TestNetGate::default(), &server.url("/"))
        .await
        .expect("discover html");
    assert_eq!(candidates.len(), 1, "私网与 javascript: 候选必须被拒绝");
    assert_eq!(candidates[0].url, server.url("/safe.xml"));
}

#[tokio::test]
async fn discover_no_candidates_returns_empty() {
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(
            200,
            "<!doctype html><html><head><title>Plain</title></head><body>no feeds</body></html>",
        )
        .header("Content-Type", "text/html"),
    );

    let candidates = discover(&TestNetGate::default(), &server.url("/"))
        .await
        .expect("discover html without candidates");
    assert!(candidates.is_empty(), "无候选返回空列表，不报错");
}

#[tokio::test]
async fn discover_non_feed_non_html_errors() {
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(200, "not a feed or html")
            .header("Content-Type", "application/octet-stream"),
    );

    let error = discover(&TestNetGate::default(), &server.url("/blob"))
        .await
        .expect_err("unsupported content must error");
    assert!(
        error.to_string().contains("feed_discovery_unsupported"),
        "got: {error}"
    );
}

#[tokio::test]
async fn discover_html_without_content_type_still_parses_alternates() {
    // 无 content-type 的 HTML 页面（常见误配置）也能发现候选。
    let server = TestServer::start().await;
    server.queue(TestResponse::new(
        200,
        html_page(&[("/feed.xml", "application/rss+xml", "Feed")]),
    ));

    let candidates = discover(&TestNetGate::default(), &server.url("/"))
        .await
        .expect("discover html without content type");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].url, server.url("/feed.xml"));
}

#[tokio::test]
async fn discover_returns_at_most_ten_candidates() {
    let server = TestServer::start().await;
    let mut page = String::from("<!doctype html><html><head><title>Many</title>");
    for index in 0..15 {
        page.push_str(&format!(
            r#"<link rel="alternate" type="application/rss+xml" title="Feed {index}" href="/feed{index}.xml"/>"#
        ));
    }
    page.push_str("</head><body>many</body></html>");
    server.queue(TestResponse::new(200, page).header("Content-Type", "text/html"));

    let candidates = discover(&TestNetGate::default(), &server.url("/"))
        .await
        .expect("discover many");
    assert!(candidates.len() <= 10, "候选最多返回 10 个");
}

// 辅助：确保 FeedCandidate 可被外部消费（DTO 契约）。
#[allow(dead_code)]
fn candidate_contract(candidate: &FeedCandidate) -> (String, Option<String>, Option<String>) {
    (
        candidate.url.clone(),
        candidate.title.clone(),
        candidate.format.clone(),
    )
}
