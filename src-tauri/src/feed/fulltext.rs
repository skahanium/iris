//! RSS 摘要条目的有界网页正文提取。
//!
//! 本模块只持久化提取后的 Markdown 与纯文本，不保存 HTML、Cookie、代理地址或
//! 底层网络错误。调用方以最多两个任务并发运行；每项响应、提取与入库均有上限。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use scraper::{Html, Selector};
use tokio::sync::Mutex as AsyncMutex;

use crate::error::{AppError, AppResult};
use crate::feed::fetch::{FeedHttpClient, FeedNetGate, FetchPurpose};
use crate::feed::normalize::{html_to_markdown, markdown_to_text, rewrite_links};
use crate::feed::repository::FeedRepository;
use crate::storage::db::Database;

/// 转换后的正文 Markdown 上限（小于网络响应上限，限制常驻缓存与 FTS 负担）。
pub(crate) const FULLTEXT_MARKDOWN_MAX_BYTES: usize = 768 * 1024;

/// 从已验证且有界的 HTML 选择正文区域并转换为 Markdown。
///
/// 优先常见的语义/通用正文容器，并选择文字密度最高者；没有可靠容器时才
/// 降级到 `body`。规则不依赖任何站点或域名。
/// 解析失败或空正文返回稳定错误，阅读器继续显示 RSS 摘要。
pub(crate) fn extract_fulltext_markdown(
    html: String,
    base_url: &str,
) -> AppResult<(String, String)> {
    let fragment = {
        let document = Html::parse_document(&html);
        let selectors = [
            "article",
            "main",
            "[role=main]",
            "[role=article]",
            ".post-content",
            ".article-content",
            ".entry-content",
            ".main-content",
            "#content",
            "#main-content",
        ];
        // 先只保留 document 内的节点引用。相邻/嵌套的候选（例如 article 与 main）
        // 不能各自克隆一份 HTML；选定最大正文后才生成唯一一份片段字符串。
        let mut best = None;
        for selector in selectors
            .iter()
            .filter_map(|selector| Selector::parse(selector).ok())
        {
            for element in document.select(&selector) {
                // 不为每个候选拼接一份完整正文。网页响应已受 1 MiB 限制，但
                // 嵌套的 article/main 候选仍可能很多；逐段计数可避免额外峰值。
                let text_len: usize = element.text().map(|part| part.chars().count()).sum();
                if text_len >= 160
                    && best
                        .as_ref()
                        .is_none_or(|(best_len, _)| text_len > *best_len)
                {
                    best = Some((text_len, element));
                }
            }
        }
        best.map(|(_, element)| element.html())
            .or_else(|| {
                Selector::parse("body").ok().and_then(|selector| {
                    document
                        .select(&selector)
                        .next()
                        .map(|element| element.html())
                })
            })
            .ok_or_else(|| AppError::msg("feed_fulltext_extract_failed"))?
    };
    // `scraper` 解析树和输入缓冲都在此点释放，再进入 HTML → Markdown；
    // 这样单项峰值始终受 1 MiB 响应与 768 KiB 输出的双重上限约束。
    drop(html);

    let markdown =
        html_to_markdown(&fragment).map_err(|_| AppError::msg("feed_fulltext_extract_failed"))?;
    let markdown = rewrite_links(&markdown, Some(base_url));
    let markdown = truncate_utf8(markdown, FULLTEXT_MARKDOWN_MAX_BYTES);
    if markdown.trim().is_empty() {
        return Err(AppError::msg("feed_fulltext_extract_failed"));
    }
    let text = markdown_to_text(&markdown);
    if text.trim().chars().count() < 80 {
        return Err(AppError::msg("feed_fulltext_extract_failed"));
    }
    Ok((markdown, text))
}

fn truncate_utf8(mut value: String, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value
}

/// 受限的后台正文队列。没有 job 表：数据库状态列同时承担可恢复队列，进程
/// 重启时 `fetching` 会在启动维护中重新置回 pending。
pub(crate) struct FeedFulltextService<G: FeedNetGate> {
    db: Arc<Database>,
    gate: Arc<G>,
    running: Arc<AtomicBool>,
    /// drain 已运行时收到的新任务唤醒标记，避免其恰好退出时遗漏单篇请求。
    reschedule_requested: Arc<AtomicBool>,
    event_sink: Arc<AsyncMutex<Option<tauri::AppHandle>>>,
}

impl<G: FeedNetGate> Clone for FeedFulltextService<G> {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            gate: self.gate.clone(),
            running: self.running.clone(),
            reschedule_requested: self.reschedule_requested.clone(),
            event_sink: self.event_sink.clone(),
        }
    }
}

impl<G: FeedNetGate + 'static> FeedFulltextService<G> {
    pub(crate) fn new(db: Arc<Database>, gate: Arc<G>) -> Self {
        Self {
            db,
            gate,
            running: Arc::new(AtomicBool::new(false)),
            reschedule_requested: Arc::new(AtomicBool::new(false)),
            event_sink: Arc::new(AsyncMutex::new(None)),
        }
    }

    pub(crate) fn attach_event_sink(&self, app: tauri::AppHandle) {
        if let Ok(mut sink) = self.event_sink.try_lock() {
            *sink = Some(app);
        }
    }

    /// 尝试启动一个 drain；已有 drain 只记录一次重调度，确保正文请求不会
    /// 在 worker 恰好退出的窗口丢失，同时仍最多两篇并发。
    pub(crate) fn schedule(&self) {
        if self
            .running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            self.reschedule_requested.store(true, Ordering::Release);
            return;
        }
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                service.reschedule_requested.store(false, Ordering::Release);
                if service.drain_pending().await.is_err() {
                    tracing::warn!(
                        result_code = "feed_fulltext_drain_failed",
                        "feed_fulltext_drain_failed"
                    );
                }
                if service.reschedule_requested.swap(false, Ordering::AcqRel) {
                    continue;
                }
                service.running.store(false, Ordering::Release);
                // `schedule` 可能恰好发生在上一次空队列检查之后。尝试重新
                // 认领 worker；若另一个调用已经启动新 worker，则安全退出。
                if service.reschedule_requested.swap(false, Ordering::AcqRel)
                    && service
                        .running
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                {
                    continue;
                }
                break;
            }
        });
    }

    async fn drain_pending(&self) -> AppResult<()> {
        loop {
            let pending = self.db.with_conn(|conn| {
                FeedRepository::claim_pending_fulltext(conn, 2, chrono::Utc::now())
            })?;
            if pending.is_empty() {
                return Ok(());
            }
            let outcomes = futures_util::future::join_all(pending.into_iter().map(|item| {
                let service = self.clone();
                async move { service.fetch_one(item).await }
            }))
            .await;
            for outcome in outcomes {
                if let Err(error) = outcome {
                    tracing::info!(result_code = %stable_code(&error), "feed_fulltext_failed");
                }
            }
        }
    }

    #[cfg(test)]
    async fn drain_pending_for_test(&self) -> AppResult<()> {
        self.drain_pending().await
    }

    async fn fetch_one(&self, item: (String, String, String)) -> AppResult<()> {
        let (item_id, source_id, url) = item;
        let result = async {
            let host = reqwest::Url::parse(&url)
                .ok()
                .and_then(|parsed| parsed.host_str().map(str::to_string))
                .ok_or_else(|| AppError::msg("feed_fulltext_url_invalid"))?;
            crate::llm::http_politeness::throttle_host(&host)
                .await
                .map_err(|_| AppError::msg("feed_fulltext_throttled"))?;
            let response = FeedHttpClient
                .fetch(
                    self.gate.as_ref(),
                    &url,
                    FetchPurpose::Article,
                    None,
                    None,
                    Some(&source_id),
                )
                .await?;
            if !response
                .content_type
                .as_deref()
                .is_none_or(|value| value.to_ascii_lowercase().contains("html"))
            {
                return Err(AppError::msg("feed_fulltext_content_type"));
            }
            // `String::from_utf8` 直接接管网络 Vec，避免额外的整份 HTML 拷贝。
            let html = String::from_utf8(response.bytes)
                .map_err(|_| AppError::msg("feed_fulltext_decode_failed"))?;
            let (markdown, text) = extract_fulltext_markdown(html, &response.final_url)?;
            let stored = self.db.with_conn(|conn| {
                FeedRepository::store_fulltext(conn, &item_id, &markdown, &text, chrono::Utc::now())
            })?;
            if stored {
                self.emit_changed(&source_id).await;
            }
            Ok(())
        }
        .await;
        if result.is_err() {
            let failed = self.db.with_conn(|conn| {
                FeedRepository::fail_fulltext(conn, &item_id, chrono::Utc::now())
            })?;
            if failed {
                self.emit_changed(&source_id).await;
            }
        }
        result
    }

    async fn emit_changed(&self, source_id: &str) {
        let Some(app) = self.event_sink.lock().await.as_ref().cloned() else {
            return;
        };
        use tauri::Emitter;
        let _ = app.emit(
            "feed:changed",
            serde_json::json!({
                "sourceId": source_id,
                "kind": "items_changed",
                "newItems": 0,
                "errorCode": null,
            }),
        );
    }
}

fn stable_code(_: &AppError) -> &'static str {
    "feed_fulltext_failed"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::model::{
        ConversionStatus, FeedItemInput, FulltextStatus, NewFeedSource, SourcePayloadKind,
    };
    use crate::feed::repository::FeedRepository;
    use crate::feed::test_http::{TestNetGate, TestResponse, TestServer};
    use crate::storage::db::Database;

    #[test]
    fn extracts_article_and_discards_navigation_markup() {
        let html = format!(
            "<html><body><nav>不要保留</nav><article><h1>标题</h1><p>{}</p><script>bad()</script></article></body></html>",
            "正文内容 ".repeat(30)
        );

        let (markdown, text) = extract_fulltext_markdown(html, "https://example.com/post")
            .expect("article should be extracted");

        assert!(markdown.contains("标题"));
        assert!(text.contains("正文"));
        assert!(!text.contains("不要保留"));
        assert!(!text.contains("bad()"));
    }

    #[test]
    fn extracts_semantic_main_without_domain_specific_rules() {
        let html = format!(
            "<html><body><header>站点标题</header><main><h1>通用正文</h1><p>{}</p></main><footer>页脚</footer></body></html>",
            "任何站点的静态主内容。".repeat(20)
        );

        let (_, text) = extract_fulltext_markdown(html, "https://example.net/post")
            .expect("semantic main should be extracted");

        assert!(text.contains("通用正文"));
        assert!(!text.contains("站点标题"));
        assert!(!text.contains("页脚"));
    }

    #[tokio::test]
    async fn pending_summary_is_replaced_with_web_fulltext_without_persisting_html() {
        let db = Arc::new(Database::open_in_memory().expect("db"));
        let server = TestServer::start().await;
        server.queue(
            TestResponse::new(
                200,
                format!(
                    "<html><body><article><h1>网页标题</h1><p>{}</p></article></body></html>",
                    "完整正文。".repeat(30)
                ),
            )
            .header("Content-Type", "text/html"),
        );
        let source_id = "source-fulltext";
        db.with_conn(|conn| {
            FeedRepository::create_source(
                conn,
                &NewFeedSource {
                    id: source_id.to_string(),
                    feed_url: server.url("/rss.xml"),
                    site_url: None,
                    title: "Test".to_string(),
                    title_override: None,
                    description: None,
                    icon_url: None,
                    language: None,
                    folder_path: String::new(),
                    fetch_interval_minutes: 60,
                },
                chrono::Utc::now(),
            )?;
            FeedRepository::upsert_items(
                conn,
                &[FeedItemInput {
                    id: "item-fulltext".to_string(),
                    source_id: source_id.to_string(),
                    external_key: "external".to_string(),
                    canonical_url: Some(server.url("/article")),
                    title: "摘要标题".to_string(),
                    author_name: None,
                    published_at: None,
                    source_updated_at: None,
                    received_at: "2026-08-12T00:00:00Z".to_string(),
                    summary_markdown: "摘要。".to_string(),
                    content_markdown: "摘要。".to_string(),
                    content_text: "摘要。".to_string(),
                    source_payload: "摘要。".to_string(),
                    source_payload_kind: SourcePayloadKind::Text,
                    content_hash: "fixture".to_string(),
                    conversion_version: 1,
                    conversion_status: ConversionStatus::Ok,
                    expires_at: "2026-08-19T00:00:00Z".to_string(),
                    fulltext_status: FulltextStatus::Pending,
                }],
            )?;
            FeedRepository::update_source(
                conn,
                source_id,
                &crate::feed::model::FeedSourcePatch {
                    fulltext_enabled: Some(true),
                    ..Default::default()
                },
                chrono::Utc::now(),
            )
        })
        .expect("seed");
        let service = FeedFulltextService::new(db.clone(), Arc::new(TestNetGate::default()));

        service.drain_pending_for_test().await.expect("drain");

        let detail = db
            .with_read_conn(|conn| FeedRepository::get_item_detail(conn, "item-fulltext"))
            .expect("detail")
            .expect("exists");
        assert_eq!(detail.content_origin, "web");
        assert_eq!(detail.fulltext_status, "ready");
        assert!(detail.content_markdown.contains("完整正文"));
        let raw: String = db
            .with_read_conn(|conn| {
                conn.query_row(
                    "SELECT source_payload FROM feed_items WHERE id = 'item-fulltext'",
                    [],
                    |row| row.get(0),
                )
                .map_err(Into::into)
            })
            .expect("payload");
        assert_eq!(raw, "摘要。", "原始网页 HTML 不得进入数据库");
    }
}
