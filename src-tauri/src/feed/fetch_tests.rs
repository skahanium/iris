//! `FeedHttpClient` 契约测试：有界 streaming、Content-Length 预拒绝、
//! 304、ETag/Last-Modified、重定向（5 跳/循环/目标再校验）、非 HTTPS、
//! 超时、系统代理策略（测试门不经过代理）与安全日志。
//!
//! 全部在本地 HTTP/1.1 测试服务器上完成，无外部网络。

use std::sync::Arc;

use tracing_subscriber::fmt::format::FmtSpan;

use super::fetch::{
    FeedFetchResult, FeedHttpClient, FetchPurpose, ProdNetGate, DISCOVERY_MAX_BYTES, FEED_MAX_BYTES,
};
use super::test_http::{TestNetGate, TestResponse, TestServer};
use crate::error::AppResult;

async fn fetch_ok(
    gate: &TestNetGate,
    server: &TestServer,
    path: &str,
    purpose: FetchPurpose,
) -> AppResult<FeedFetchResult> {
    FeedHttpClient
        .fetch(
            gate,
            &server.url(path),
            purpose,
            None,
            None,
            Some("src-test"),
        )
        .await
}

#[tokio::test]
async fn fetch_returns_status_headers_and_body() {
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(200, "<rss/>")
            .header("Content-Type", "application/rss+xml")
            .header("ETag", "\"abc123\"")
            .header("Last-Modified", "Wed, 12 Aug 2026 08:00:00 GMT"),
    );

    let result = fetch_ok(
        &TestNetGate::default(),
        &server,
        "/feed.xml",
        FetchPurpose::Feed,
    )
    .await
    .expect("fetch succeeds");
    assert_eq!(result.status, 200);
    assert_eq!(result.bytes, b"<rss/>");
    assert_eq!(result.content_type.as_deref(), Some("application/rss+xml"));
    assert_eq!(result.etag.as_deref(), Some("\"abc123\""));
    assert_eq!(
        result.last_modified.as_deref(),
        Some("Wed, 12 Aug 2026 08:00:00 GMT")
    );
    assert_eq!(result.final_url, server.url("/feed.xml"));
}

#[tokio::test]
async fn fetch_uses_minimal_user_agent() {
    let server = TestServer::start().await;
    server.queue(TestResponse::new(200, "ok"));

    fetch_ok(
        &TestNetGate::default(),
        &server,
        "/feed.xml",
        FetchPurpose::Feed,
    )
    .await
    .expect("fetch succeeds");

    let snapshot = server.requests_snapshot();
    assert_eq!(snapshot.len(), 1);
    let ua = snapshot[0]
        .headers
        .get("user-agent")
        .expect("user-agent header");
    assert!(
        ua.starts_with("Iris/") && ua.ends_with(" RSS Reader"),
        "UA must be exactly Iris/<version> RSS Reader, got {ua}"
    );
    assert!(!ua.contains("vault") && !ua.contains("darwin") && !ua.contains("Mac"));
}

#[tokio::test]
async fn fetch_sends_conditional_headers() {
    let server = TestServer::start().await;
    server.queue(TestResponse::new(200, "ok"));

    FeedHttpClient
        .fetch(
            &TestNetGate::default(),
            &server.url("/feed.xml"),
            FetchPurpose::Feed,
            Some("\"etag-1\""),
            Some("Wed, 12 Aug 2026 08:00:00 GMT"),
            None,
        )
        .await
        .expect("fetch succeeds");

    let snapshot = server.requests_snapshot();
    assert_eq!(
        snapshot[0].headers.get("if-none-match").map(String::as_str),
        Some("\"etag-1\"")
    );
    assert_eq!(
        snapshot[0]
            .headers
            .get("if-modified-since")
            .map(String::as_str),
        Some("Wed, 12 Aug 2026 08:00:00 GMT")
    );
}

#[tokio::test]
async fn fetch_does_not_forward_conditional_headers_across_authorities() {
    let origin = TestServer::start().await;
    let redirected = TestServer::start().await;
    origin.queue(TestResponse::new(302, "").header("Location", &redirected.url("/feed.xml")));
    redirected.queue(TestResponse::new(200, "<rss/>"));

    FeedHttpClient
        .fetch(
            &TestNetGate::default(),
            &origin.url("/start"),
            FetchPurpose::Feed,
            Some("\"origin-etag\""),
            Some("Wed, 12 Aug 2026 08:00:00 GMT"),
            None,
        )
        .await
        .expect("cross-authority redirect succeeds");

    let origin_request = origin.requests_snapshot();
    assert_eq!(
        origin_request[0]
            .headers
            .get("if-none-match")
            .map(String::as_str),
        Some("\"origin-etag\"")
    );
    let redirected_request = redirected.requests_snapshot();
    assert!(
        !redirected_request[0].headers.contains_key("if-none-match")
            && !redirected_request[0]
                .headers
                .contains_key("if-modified-since"),
        "validators from the original authority must not reach redirect targets"
    );
}

#[tokio::test]
async fn fetch_handles_304_without_body() {
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(304, "")
            .header("ETag", "\"abc123\"")
            .header("Last-Modified", "Wed, 12 Aug 2026 08:00:00 GMT"),
    );

    let result = fetch_ok(
        &TestNetGate::default(),
        &server,
        "/feed.xml",
        FetchPurpose::Feed,
    )
    .await
    .expect("304 is not an error");
    assert_eq!(result.status, 304);
    assert!(result.bytes.is_empty());
    assert_eq!(result.etag.as_deref(), Some("\"abc123\""));
    assert_eq!(
        result.last_modified.as_deref(),
        Some("Wed, 12 Aug 2026 08:00:00 GMT")
    );
}

#[tokio::test]
async fn fetch_rejects_content_length_over_limit() {
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(200, vec![0u8; 1024])
            .header("Content-Length", (FEED_MAX_BYTES + 1).to_string().as_str()),
    );

    let error = fetch_ok(
        &TestNetGate::default(),
        &server,
        "/feed.xml",
        FetchPurpose::Feed,
    )
    .await
    .expect_err("content-length over limit must be rejected");
    assert!(
        error.to_string().contains("feed_response_too_large"),
        "got: {error}"
    );
}

#[tokio::test]
async fn fetch_aborts_stream_over_limit() {
    // 无 Content-Length：服务器持续流出 6 MiB，客户端必须在超限处停止。
    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(200, vec![0x41u8; FEED_MAX_BYTES + 1024 * 1024])
            .header("Content-Type", "application/rss+xml"),
    );

    let error = fetch_ok(
        &TestNetGate::default(),
        &server,
        "/feed.xml",
        FetchPurpose::Feed,
    )
    .await
    .expect_err("stream over limit must abort");
    assert!(
        error.to_string().contains("feed_response_too_large"),
        "got: {error}"
    );
}

#[tokio::test]
async fn fetch_discovery_purpose_uses_smaller_limit() {
    let server = TestServer::start().await;
    // 3 MiB：Feed 上限内、发现页上限外。两次请求各消费一份响应。
    let payload = vec![0x42u8; 3 * 1024 * 1024];
    server.queue(TestResponse::new(200, payload.clone()).header("Content-Type", "text/html"));
    server.queue(TestResponse::new(200, payload.clone()).header("Content-Type", "text/html"));

    let feed_ok = fetch_ok(
        &TestNetGate::default(),
        &server,
        "/feed",
        FetchPurpose::Feed,
    )
    .await
    .expect("3 MiB is within feed limit");
    assert_eq!(feed_ok.bytes.len(), 3 * 1024 * 1024);

    let discovery_err = fetch_ok(
        &TestNetGate::default(),
        &server,
        "/feed",
        FetchPurpose::Discovery,
    )
    .await
    .expect_err("3 MiB exceeds discovery limit");
    assert!(
        discovery_err
            .to_string()
            .contains("feed_response_too_large"),
        "got: {discovery_err}"
    );
    const _: () = assert!(DISCOVERY_MAX_BYTES < FEED_MAX_BYTES);
}

#[tokio::test]
async fn fetch_follows_redirects_up_to_five_hops() {
    let server = TestServer::start().await;
    server.queue(TestResponse::new(302, "").header("Location", "/hop1"));
    server.queue(TestResponse::new(302, "").header("Location", "/hop2"));
    server.queue(TestResponse::new(302, "").header("Location", "/hop3"));
    server.queue(TestResponse::new(302, "").header("Location", "/hop4"));
    server.queue(TestResponse::new(200, "<rss>final</rss>"));

    let result = fetch_ok(
        &TestNetGate::default(),
        &server,
        "/start",
        FetchPurpose::Feed,
    )
    .await
    .expect("five hops succeed");
    assert_eq!(result.status, 200);
    assert_eq!(result.bytes, b"<rss>final</rss>");
    assert_eq!(result.final_url, server.url("/hop4"));
    assert_eq!(server.requests_snapshot().len(), 5, "exactly five requests");
}

#[tokio::test]
async fn fetch_rejects_redirect_loop() {
    let server = TestServer::start().await;
    server.queue(TestResponse::new(302, "").header("Location", "/loop"));
    server.queue(TestResponse::new(302, "").header("Location", "/start"));

    let error = fetch_ok(
        &TestNetGate::default(),
        &server,
        "/start",
        FetchPurpose::Feed,
    )
    .await
    .expect_err("redirect loop must fail");
    assert!(
        error.to_string().contains("feed_redirect_loop"),
        "got: {error}"
    );
}

#[tokio::test]
async fn fetch_rejects_redirect_without_location() {
    let server = TestServer::start().await;
    server.queue(TestResponse::new(302, ""));

    let error = fetch_ok(
        &TestNetGate::default(),
        &server,
        "/start",
        FetchPurpose::Feed,
    )
    .await
    .expect_err("redirect without location must fail");
    assert!(
        error.to_string().contains("feed_redirect_missing_location"),
        "got: {error}"
    );
}

#[tokio::test]
async fn fetch_rejects_redirect_to_private_network() {
    // 初始请求允许（127.0.0.1），重定向目标指向私网段必须被门拒绝。
    let server = TestServer::start().await;
    server.queue(TestResponse::new(302, "").header("Location", "http://192.168.0.1/steal"));

    let error = fetch_ok(
        &TestNetGate::default(),
        &server,
        "/start",
        FetchPurpose::Feed,
    )
    .await
    .expect_err("redirect to private network must fail");
    assert_eq!(error.to_string(), "feed_url_rejected");
}

#[tokio::test]
async fn fetch_enforces_one_deadline_across_redirect_hops() {
    let server = TestServer::start_with_delay(180).await;
    server.queue(TestResponse::new(302, "").header("Location", "/final"));
    server.queue(TestResponse::new(200, "ok"));
    let gate = TestNetGate {
        timeout: std::time::Duration::from_millis(300),
    };

    let error = fetch_ok(&gate, &server, "/start", FetchPurpose::Feed)
        .await
        .expect_err("two individually-fast hops must still share one total deadline");
    assert_eq!(error.to_string(), "feed_fetch_timeout");
}

#[tokio::test]
async fn fetch_rejects_oversized_response_headers() {
    let server = TestServer::start().await;
    server.queue(TestResponse::new(200, "ok").header("X-Large", &"a".repeat(70 * 1024)));

    let error = fetch_ok(
        &TestNetGate::default(),
        &server,
        "/feed.xml",
        FetchPurpose::Feed,
    )
    .await
    .expect_err("oversized response headers must be rejected before reading the body");
    assert_eq!(error.to_string(), "feed_response_headers_too_large");
}

#[tokio::test]
async fn fetch_rejects_non_https_with_production_gate() {
    // 生产网门：非 HTTPS 在发起任何请求前被拒绝。
    let server = TestServer::start().await;
    let error = FeedHttpClient
        .fetch(
            &ProdNetGate,
            &server.url("/feed.xml"),
            FetchPurpose::Feed,
            None,
            None,
            None,
        )
        .await
        .expect_err("production gate must reject plain http");
    assert!(!error.to_string().is_empty());
    assert!(
        server.requests_snapshot().is_empty(),
        "no request must be sent"
    );
}

#[tokio::test]
async fn fetch_times_out_when_server_stalls() {
    let server = TestServer::start_with_delay(5_000).await;
    server.queue(TestResponse::new(200, "too slow"));

    let gate = TestNetGate {
        timeout: std::time::Duration::from_millis(300),
    };
    let error = fetch_ok(&gate, &server, "/slow", FetchPurpose::Feed)
        .await
        .expect_err("stalled server must time out");
    assert_eq!(error.to_string(), "feed_fetch_timeout");
}

/// 捕获 tracing 事件，证明日志只含安全字段。
type CaptureStore = Arc<std::sync::Mutex<Vec<String>>>;

struct CaptureLine(CaptureStore);

impl std::io::Write for CaptureLine {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("capture lock")
            .push(String::from_utf8_lossy(buf).to_string());
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct CaptureMakeWriter(CaptureStore);

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CaptureMakeWriter {
    type Writer = CaptureLine;

    fn make_writer(&'a self) -> Self::Writer {
        CaptureLine(Arc::clone(&self.0))
    }
}

/// 进程级日志捕获：测试进程内唯一的全局 fmt subscriber，规避线程本地
/// 调度器在并发下的竞态；`try_init` 失败（已被其他测试初始化）时静默降级。
static LOG_CAPTURE: std::sync::OnceLock<CaptureStore> = std::sync::OnceLock::new();

fn global_log_capture() -> CaptureStore {
    LOG_CAPTURE
        .get_or_init(|| {
            let store: CaptureStore = Arc::new(std::sync::Mutex::new(Vec::new()));
            let _ = tracing_subscriber::fmt()
                .with_writer(CaptureMakeWriter(Arc::clone(&store)))
                .with_span_events(FmtSpan::NONE)
                .with_max_level(tracing::Level::INFO)
                .try_init();
            store
        })
        .clone()
}

#[tokio::test]
async fn fetch_logs_only_safe_fields() {
    let body = "BODY-FIXTURE-SECRET-7f3a";
    let path = "/private/feed.xml";
    let store = global_log_capture();

    let server = TestServer::start().await;
    server.queue(
        TestResponse::new(200, body)
            .header("Content-Type", "application/rss+xml")
            .header("ETag", "\"etag-secret\""),
    );

    let result = FeedHttpClient
        .fetch(
            &TestNetGate::default(),
            &server.url(path),
            FetchPurpose::Feed,
            None,
            None,
            Some("src-42"),
        )
        .await
        .expect("fetch succeeds");
    assert_eq!(result.status, 200);

    // 事件在 fetch 返回前已同步写出（同一线程），读取不会竞态。
    let logs = store.lock().expect("logs").join("\n");
    assert!(
        logs.contains("src-42"),
        "log must carry the stable source id; captured: {logs}"
    );
    assert!(
        !logs.contains(path)
            && !logs.contains("BODY-FIXTURE-SECRET")
            && !logs.contains("etag-secret"),
        "log must not contain URL, body or validator values; got: {logs}"
    );
}
