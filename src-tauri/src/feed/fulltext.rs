//! RSS 摘要条目的有界网页正文提取。
//!
//! 本模块只持久化提取后的 Markdown 与纯文本，不保存 HTML、Cookie、代理地址或
//! 底层网络错误。调用方以最多两个任务并发运行；每项响应、提取与入库均有上限。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use scraper::{Html, Selector};
use sha2::Digest;
use tokio::sync::Mutex as AsyncMutex;

use crate::error::{AppError, AppResult};
use crate::feed::fetch::{FeedHttpClient, FeedNetGate, FetchPurpose};
use crate::feed::normalize::{html_to_markdown, markdown_to_text, rewrite_links};
use crate::feed::repository::FeedRepository;
use crate::storage::db::Database;

/// 转换后的正文 Markdown 上限（小于网络响应上限，限制常驻缓存与 FTS 负担）。
pub(crate) const FULLTEXT_MARKDOWN_MAX_BYTES: usize = 768 * 1024;
/// 网页正文提取规则版本；旧版本只在用户再次打开单篇文章时重取。
pub(crate) const FULLTEXT_EXTRACTION_VERSION: i64 = 4;
const MAX_CANDIDATES: usize = 64;
const MAX_CANDIDATE_NODES: usize = 50_000;
const MAX_REMOVALS: usize = 256;
const MAX_REMOVAL_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExtractedPrimaryDocument {
    pub kind: &'static str,
    pub url: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ExtractedWebContent {
    pub markdown: String,
    pub text: String,
    pub quality_score: i64,
    pub extraction_version: i64,
    pub primary_document: Option<ExtractedPrimaryDocument>,
}

/// 从已验证且有界的 HTML 选择正文区域并转换为 Markdown。
///
/// 优先常见的语义/通用正文容器，并选择文字密度最高者；没有可靠容器时才
/// 降级到 `body`。规则不依赖任何站点或域名。
/// 解析失败或空正文返回稳定错误，阅读器继续显示 RSS 摘要。
#[cfg(test)]
pub(crate) fn extract_fulltext_markdown(
    html: String,
    base_url: &str,
) -> AppResult<(String, String)> {
    let extracted = extract_fulltext(html, base_url)?;
    Ok((extracted.markdown, extracted.text))
}

#[cfg(test)]
pub(crate) fn extract_fulltext(html: String, base_url: &str) -> AppResult<ExtractedWebContent> {
    extract_fulltext_for_item(html, base_url, None)
}

fn extract_fulltext_for_item(
    html: String,
    base_url: &str,
    expected_title: Option<&str>,
) -> AppResult<ExtractedWebContent> {
    let document = Html::parse_document(&html);
    let primary_document = extract_primary_document(&document, base_url);
    if let Some(scholarly) = extract_scholarly_metadata(&document) {
        let markdown = truncate_utf8(scholarly, FULLTEXT_MARKDOWN_MAX_BYTES);
        let text = markdown_to_text(&markdown);
        if text.chars().count() >= 80 {
            return Ok(ExtractedWebContent {
                quality_score: 10_000 + text.chars().count() as i64,
                markdown,
                text,
                extraction_version: FULLTEXT_EXTRACTION_VERSION,
                primary_document,
            });
        }
    }

    let selectors = [
        "article",
        "[itemprop=articleBody]",
        "[role=article]",
        ".post-content",
        ".article-content",
        ".entry-content",
        ".main-content",
        "[class*='article'][class*='content']",
        "[class*='article'][class*='body']",
        "[class*='post'][class*='content']",
        "[class*='post'][class*='body']",
        "[class*='entry'][class*='content']",
        "[class*='entry'][class*='body']",
        "[class~=abstract]",
        "main",
        "[role=main]",
        "#content",
        "#main-content",
    ];
    let mut best: Option<(i64, bool, usize, scraper::ElementRef<'_>)> = None;
    let mut seen = 0_usize;
    for selector in selectors
        .iter()
        .filter_map(|value| Selector::parse(value).ok())
    {
        for element in document.select(&selector) {
            if seen >= MAX_CANDIDATES {
                break;
            }
            seen += 1;
            let Some(score) = score_candidate(element, false) else {
                continue;
            };
            let depth = element.ancestors().take(64).count();
            let is_content = is_content_container(element);
            if best
                .as_ref()
                .is_none_or(|(best_score, best_is_content, best_depth, _)| {
                    if is_content != *best_is_content {
                        return is_content
                            && score.saturating_mul(100) >= best_score.saturating_mul(90);
                    }
                    score > *best_score
                        || (depth > *best_depth
                            && score.saturating_mul(100) >= best_score.saturating_mul(90))
                })
            {
                best = Some((score, is_content, depth, element));
            }
        }
    }
    if best.is_none() {
        let body = Selector::parse("body")
            .ok()
            .and_then(|selector| document.select(&selector).next());
        if let Some(body) = body {
            if let Some(score) = score_candidate(body, true) {
                best = Some((score, false, 0, body));
            }
        }
    }
    let (quality_score, _, _, element) =
        best.ok_or_else(|| AppError::msg("feed_fulltext_extract_failed"))?;
    let fragment = sanitize_fragment(element.html(), base_url);
    drop(document);
    drop(html);

    let markdown =
        html_to_markdown(&fragment).map_err(|_| AppError::msg("feed_fulltext_extract_failed"))?;
    let markdown = rewrite_links(&markdown, Some(base_url));
    let markdown = remove_duplicate_title(markdown, expected_title);
    let markdown = truncate_utf8(markdown, FULLTEXT_MARKDOWN_MAX_BYTES);
    let text = markdown_to_text(&markdown);
    if text.trim().chars().count() < 80 {
        return Err(AppError::msg("feed_fulltext_extract_failed"));
    }
    Ok(ExtractedWebContent {
        markdown,
        text,
        quality_score,
        extraction_version: FULLTEXT_EXTRACTION_VERSION,
        primary_document,
    })
}

fn remove_duplicate_title(markdown: String, expected_title: Option<&str>) -> String {
    let Some(expected_title) = expected_title else {
        return markdown;
    };
    let content = markdown.trim_start_matches(['\r', '\n']);
    let Some(first_line) = content.lines().next() else {
        return markdown;
    };
    let Some(heading) = first_line.strip_prefix("# ") else {
        return markdown;
    };
    let normalize = |value: &str| {
        value
            .chars()
            .filter(|character| character.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    if normalize(heading) != normalize(expected_title) {
        return markdown;
    }
    content[first_line.len()..]
        .trim_start_matches(['\r', '\n'])
        .to_string()
}

fn meta_contents(document: &Html, name: &str) -> Vec<String> {
    let Ok(selector) = Selector::parse("meta[name], meta[property]") else {
        return Vec::new();
    };
    document
        .select(&selector)
        .filter(|element| {
            element
                .value()
                .attr("name")
                .or_else(|| element.value().attr("property"))
                .is_some_and(|value| value.eq_ignore_ascii_case(name))
        })
        .filter_map(|element| element.value().attr("content"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .take(64)
        .map(ToOwned::to_owned)
        .collect()
}

fn json_ld_strings(document: &Html, keys: &[&str]) -> Vec<String> {
    let Ok(selector) = Selector::parse("script[type='application/ld+json']") else {
        return Vec::new();
    };
    let mut results = Vec::new();
    for script in document.select(&selector).take(8) {
        let raw = script.text().take(1).collect::<String>();
        if raw.len() > 256 * 1024 {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let mut stack = vec![&value];
        let mut visited = 0_usize;
        while let Some(value) = stack.pop() {
            visited += 1;
            if visited > 512 || results.len() >= 64 {
                break;
            }
            match value {
                serde_json::Value::Object(values) => {
                    for (key, value) in values {
                        if keys
                            .iter()
                            .any(|candidate| key.eq_ignore_ascii_case(candidate))
                        {
                            match value {
                                serde_json::Value::String(value) => {
                                    results.push(value.trim().to_string());
                                }
                                serde_json::Value::Object(author)
                                    if key.eq_ignore_ascii_case("author") =>
                                {
                                    if let Some(name) =
                                        author.get("name").and_then(|name| name.as_str())
                                    {
                                        results.push(name.trim().to_string());
                                    }
                                }
                                serde_json::Value::Array(authors)
                                    if key.eq_ignore_ascii_case("author") =>
                                {
                                    for author in authors.iter().take(32) {
                                        if let Some(name) = author
                                            .as_object()
                                            .and_then(|author| author.get("name"))
                                            .and_then(|name| name.as_str())
                                        {
                                            results.push(name.trim().to_string());
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        stack.push(value);
                    }
                }
                serde_json::Value::Array(values) => {
                    stack.extend(values.iter().rev().take(64));
                }
                _ => {}
            }
        }
    }
    results.retain(|value| !value.is_empty());
    results
}

fn json_ld_has_scholarly_type(document: &Html) -> bool {
    json_ld_strings(document, &["@type"])
        .iter()
        .any(|value| value.eq_ignore_ascii_case("ScholarlyArticle"))
}

fn extract_scholarly_metadata(document: &Html) -> Option<String> {
    let citation_signal = !meta_contents(document, "citation_abstract").is_empty()
        || !meta_contents(document, "citation_pdf_url").is_empty()
        || !meta_contents(document, "citation_doi").is_empty()
        || !meta_contents(document, "citation_author").is_empty();
    if !citation_signal && !json_ld_has_scholarly_type(document) {
        return None;
    }
    let abstract_text = meta_contents(document, "citation_abstract")
        .into_iter()
        .next()
        .or_else(|| meta_contents(document, "dc.description").into_iter().next())
        .or_else(|| meta_contents(document, "og:description").into_iter().next())
        .or_else(|| {
            json_ld_strings(document, &["abstract", "description"])
                .into_iter()
                .next()
        })?;
    if abstract_text.chars().count() < 80 {
        return None;
    }
    let mut authors = meta_contents(document, "citation_author");
    if authors.is_empty() {
        authors = meta_contents(document, "dc.creator");
    }
    if authors.is_empty() {
        authors = json_ld_strings(document, &["author"]);
    }
    let date = meta_contents(document, "citation_publication_date")
        .into_iter()
        .next()
        .or_else(|| meta_contents(document, "dc.date").into_iter().next())
        .or_else(|| {
            meta_contents(document, "article:published_time")
                .into_iter()
                .next()
        })
        .or_else(|| {
            json_ld_strings(document, &["datePublished"])
                .into_iter()
                .next()
        });
    let doi = meta_contents(document, "citation_doi")
        .into_iter()
        .next()
        .or_else(|| meta_contents(document, "dc.identifier").into_iter().next());
    let mut markdown = String::new();
    if !authors.is_empty() {
        markdown.push_str("**作者：** ");
        markdown.push_str(
            &authors
                .iter()
                .map(|author| escape_markdown_text(author))
                .collect::<Vec<_>>()
                .join("、"),
        );
        markdown.push_str("\n\n");
    }
    if let Some(date) = date {
        markdown.push_str("**日期：** ");
        markdown.push_str(&escape_markdown_text(&date));
        markdown.push_str("\n\n");
    }
    markdown.push_str("## 摘要\n\n");
    markdown.push_str(&escape_markdown_text(&abstract_text));
    if let Some(doi) = doi {
        markdown.push_str("\n\n**DOI：** ");
        markdown.push_str(&escape_markdown_text(&doi));
    }
    Some(markdown)
}

fn escape_markdown_text(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| {
            if matches!(
                character,
                '\\' | '`' | '*' | '_' | '[' | ']' | '#' | '>' | '|'
            ) {
                vec!['\\', character]
            } else {
                vec![character]
            }
        })
        .collect()
}

fn resolve_https_url(value: &str, base_url: &str) -> Option<String> {
    let base = reqwest::Url::parse(base_url).ok()?;
    let resolved = base.join(value.trim()).ok()?;
    crate::network::safe_https::validate_https_url(resolved.as_str()).ok()?;
    Some(resolved.to_string())
}

fn extract_primary_document(document: &Html, base_url: &str) -> Option<ExtractedPrimaryDocument> {
    let meta_url = meta_contents(document, "citation_pdf_url")
        .into_iter()
        .next();
    let link_url = Selector::parse("link[type='application/pdf'], a[type='application/pdf']")
        .ok()
        .and_then(|selector| {
            document.select(&selector).find_map(|element| {
                element
                    .value()
                    .attr("href")
                    .or_else(|| element.value().attr("content"))
                    .map(ToOwned::to_owned)
            })
        });
    let explicit_pdf_url = Selector::parse("a[href]").ok().and_then(|selector| {
        document.select(&selector).take(128).find_map(|element| {
            let href = element.value().attr("href")?;
            let resolved = resolve_https_url(href, base_url)?;
            let parsed = reqwest::Url::parse(&resolved).ok()?;
            let link_text = element
                .text()
                .take(4096)
                .collect::<String>()
                .to_ascii_lowercase();
            (parsed.path().to_ascii_lowercase().ends_with(".pdf")
                || link_text.split_whitespace().any(|word| word == "pdf"))
            .then_some(resolved)
        })
    });
    meta_url
        .or(link_url)
        .and_then(|url| resolve_https_url(&url, base_url))
        .or(explicit_pdf_url)
        .map(|url| ExtractedPrimaryDocument { kind: "pdf", url })
}

fn score_candidate(element: scraper::ElementRef<'_>, body_fallback: bool) -> Option<i64> {
    let mut nodes = element.descendants();
    let text = nodes
        .by_ref()
        .take(MAX_CANDIDATE_NODES)
        .filter_map(|node| node.value().as_text())
        .map(|text| text.to_string())
        .collect::<String>();
    if nodes.next().is_some() {
        return None;
    }
    let text_len = text.trim().chars().count();
    let minimum_text = if body_fallback { 160 } else { 80 };
    if text_len < minimum_text {
        return None;
    }
    let paragraphs = Selector::parse("p")
        .ok()
        .map_or(0, |selector| element.select(&selector).count().min(500));
    if body_fallback && paragraphs < 3 {
        return None;
    }
    let link_text_len = Selector::parse("a")
        .ok()
        .map(|selector| {
            element
                .select(&selector)
                .take(500)
                .flat_map(|link| link.text())
                .map(|part| part.chars().count())
                .sum::<usize>()
        })
        .unwrap_or(0);
    if link_text_len.saturating_mul(100) > text_len.saturating_mul(35) {
        return None;
    }
    let controls = Selector::parse("button, input, select, textarea, form, dialog")
        .ok()
        .map_or(0, |selector| element.select(&selector).take(101).count());
    if body_fallback && controls > paragraphs.saturating_mul(2).max(6) {
        return None;
    }
    let punctuation = text
        .chars()
        .filter(|character| matches!(character, '.' | '!' | '?' | '。' | '！' | '？'))
        .count();
    Some(
        text_len as i64 + paragraphs as i64 * 160 + punctuation as i64 * 12
            - link_text_len as i64 * 2
            - controls as i64 * 240,
    )
}

fn is_content_container(element: scraper::ElementRef<'_>) -> bool {
    if element.value().name() == "main" || element.value().attr("itemprop") == Some("articleBody") {
        return true;
    }
    ["class", "id"].iter().any(|attribute| {
        element.value().attr(attribute).is_some_and(|value| {
            let value = value.to_ascii_lowercase();
            value.contains("content")
                || value.contains("article-body")
                || value.contains("post-body")
        })
    })
}

fn sanitize_fragment(mut fragment: String, base_url: &str) -> String {
    let parsed = Html::parse_fragment(&fragment);
    let negative = [
        "nav",
        "header",
        "footer",
        "aside",
        "form",
        "dialog",
        "[hidden]",
        "[aria-hidden=true]",
        "[class*='comment']",
        "[class*='share']",
        "[class*='recommend']",
        "[class*='related']",
        "[class*='citation']",
        "[class*='reference']",
        "[class*='bookmark']",
        "[class*='toolbar']",
        "[class*='sidebar']",
        "[class*='advert']",
        "[class*='modal']",
        "[class*='pagination']",
    ];
    let mut removals = Vec::new();
    let mut removal_bytes = 0_usize;
    let mut removal_hashes = std::collections::HashSet::new();
    let mut push_removal = |removal: String| {
        if removals.len() >= MAX_REMOVALS
            || removal_bytes.saturating_add(removal.len()) > MAX_REMOVAL_BYTES
        {
            return false;
        }
        let digest = sha2::Sha256::digest(removal.as_bytes());
        if removal_hashes.insert(digest) {
            removal_bytes = removal_bytes.saturating_add(removal.len());
            removals.push(removal);
        }
        true
    };
    for selector in negative
        .iter()
        .filter_map(|value| Selector::parse(value).ok())
    {
        for element in parsed.select(&selector).take(128) {
            if !push_removal(element.html()) {
                break;
            }
        }
    }
    if let Ok(selector) = Selector::parse("img") {
        for image in parsed.select(&selector).take(256) {
            let alt = image.value().attr("alt").unwrap_or("").trim();
            let valid_source = image
                .value()
                .attr("src")
                .and_then(|source| resolve_https_url(source, base_url))
                .is_some();
            let class = image
                .value()
                .attr("class")
                .unwrap_or("")
                .to_ascii_lowercase();
            let inside_decorative_container = image
                .ancestors()
                .take(32)
                .filter_map(scraper::ElementRef::wrap)
                .filter_map(|ancestor| ancestor.value().attr("class"))
                .map(str::to_ascii_lowercase)
                .any(|class| {
                    ["avatar", "author", "profile", "byline", "badge"]
                        .iter()
                        .any(|token| class.contains(token))
                });
            let inside_figure = image
                .ancestors()
                .take(32)
                .filter_map(scraper::ElementRef::wrap)
                .any(|ancestor| ancestor.value().name() == "figure");
            let meaningful = valid_source
                && (inside_figure
                    || (alt.chars().count() >= 4
                        && !["icon", "logo", "avatar", "badge", "spinner"]
                            .iter()
                            .any(|token| class.contains(token))))
                && !inside_decorative_container;
            if !meaningful && !push_removal(image.html()) {
                break;
            }
        }
    }
    drop(parsed);
    for removal in removals {
        fragment = fragment.replace(&removal, "");
    }
    fragment
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

    async fn fetch_one(&self, item: (String, String, String, String)) -> AppResult<()> {
        let (item_id, source_id, url, expected_title) = item;
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
            let extracted =
                extract_fulltext_for_item(html, &response.final_url, Some(&expected_title))?;
            let stored = self.db.with_conn(|conn| {
                tracing::debug!(
                    quality_score = extracted.quality_score,
                    extraction_version = extracted.extraction_version,
                    "feed_fulltext_extracted"
                );
                FeedRepository::store_fulltext(
                    conn,
                    &item_id,
                    &extracted.markdown,
                    &extracted.text,
                    extracted.extraction_version,
                    extracted.primary_document.as_ref(),
                    chrono::Utc::now(),
                )
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

    #[test]
    fn ordinary_open_graph_description_never_replaces_a_full_article() {
        let article = "The complete article contains multiple detailed paragraphs. ".repeat(40);
        let html = format!(
            "<html><head><meta property='og:description' content='Short social teaser that is intentionally not the body.'></head><body><article><h1>Full story</h1><p>{article}</p><p>{article}</p><p>{article}</p></article></body></html>"
        );

        let extracted = extract_fulltext(html, "https://example.com/story").unwrap();
        assert!(extracted.text.contains("complete article"));
        assert!(!extracted.text.contains("Short social teaser"));
    }

    #[test]
    fn deeply_nested_repeated_noise_is_cleaned_with_bounded_work() {
        let mut noise = String::new();
        for _ in 0..400 {
            noise.push_str("<div class='related citation toolbar'>duplicate tool");
        }
        for _ in 0..400 {
            noise.push_str("</div>");
        }
        let html = format!(
            "<html><body><article><p>{}</p>{noise}<p>{}</p><p>{}</p></article></body></html>",
            "Useful article text. ".repeat(20),
            "More useful text. ".repeat(20),
            "Final useful text. ".repeat(20),
        );
        let extracted = extract_fulltext(html, "https://example.com/story").unwrap();
        assert!(extracted.text.contains("Useful article"));
        assert!(!extracted.text.contains("duplicate tool"));
    }

    #[test]
    fn extracts_scholarly_metadata_without_page_widgets() {
        let html = format!(
            r#"<html><head>
                <meta name="citation_title" content="A General Paper" />
                <meta name="citation_author" content="Ada Lovelace" />
                <meta name="citation_author" content="Grace Hopper" />
                <meta name="citation_abstract" content="{}" />
                <meta name="citation_pdf_url" content="https://papers.example/paper.pdf" />
              </head><body>
                <main><h1>A General Paper</h1><p>{}</p>
                  <section class="references-and-citations"><button>Export citation</button></section>
                  <section class="related-tools">Connected papers and recommendations</section>
                </main>
              </body></html>"#,
            "This abstract explains the complete research result. ".repeat(5),
            "This abstract explains the complete research result. ".repeat(5),
        );

        let extracted = extract_fulltext(html, "https://papers.example/record")
            .expect("scholarly metadata should be extracted");

        assert!(!extracted.markdown.contains("# A General Paper"));
        assert!(extracted.markdown.contains("Ada Lovelace"));
        assert!(extracted.markdown.contains("This abstract explains"));
        assert!(!extracted.markdown.contains("Export citation"));
        assert!(!extracted.markdown.contains("Connected papers"));
        assert_eq!(
            extracted
                .primary_document
                .as_ref()
                .map(|document| document.url.as_str()),
            Some("https://papers.example/paper.pdf")
        );
    }

    #[test]
    fn rejects_navigation_heavy_body_fallback() {
        let links = (0..30)
            .map(|index| format!("<a href='/tool/{index}'>related tool {index}</a>"))
            .collect::<String>();
        let html = format!(
            "<html><body><div><p>Short landing text.</p>{links}<button>Load more</button></div></body></html>"
        );

        let result = extract_fulltext(html, "https://example.com/landing");

        assert!(result.is_err(), "link-heavy page shell must not become正文");
    }

    #[test]
    fn extracts_generic_json_ld_scholarly_metadata_without_site_rules() {
        let abstract_text =
            "A bounded JSON-LD abstract describing a general research result. ".repeat(4);
        let json_ld = serde_json::json!({
            "@type": "ScholarlyArticle",
            "headline": "Generic JSON-LD Paper",
            "abstract": abstract_text,
            "author": [{ "name": "General Author" }],
            "datePublished": "2026-08-13"
        });
        let html = format!(
            "<html><head><script type='application/ld+json'>{json_ld}</script></head><body><main><p>fallback shell</p></main></body></html>"
        );

        let extracted = extract_fulltext(html, "https://papers.example/record").unwrap();
        assert!(extracted.markdown.contains("General Author"));
        assert!(extracted.markdown.contains("2026-08-13"));
        assert!(extracted.markdown.contains("JSON-LD abstract"));
    }

    #[test]
    fn removes_decorative_or_unsafe_images_but_keeps_https_figures() {
        let html = format!(
            "<html><body><article><h1>Figures</h1><p>{}</p><img src='http://unsafe.example/icon.png' alt='toolbar icon'><figure><img src='https://cdn.example/chart.png' alt='Research result chart'><figcaption>Figure 1</figcaption></figure></article></body></html>",
            "A complete article paragraph with punctuation. ".repeat(8)
        );
        let extracted = extract_fulltext(html, "https://papers.example/record").unwrap();
        assert!(!extracted.markdown.contains("unsafe.example"));
        assert!(extracted.markdown.contains("https://cdn.example/chart.png"));
        assert!(extracted.markdown.contains("Figure 1"));
    }

    #[test]
    fn prefers_a_deep_article_content_container_over_page_chrome() {
        let article = "The retained article body has enough complete prose. ".repeat(24);
        let html = format!(
            "<html><body><article><div class='article-header'><h1>页面标题</h1><div class='author'><img src='https://cdn.example/avatar.png' alt='Long Author Name'></div></div><div class='article-main-content'><p>{article}</p><p>{article}</p></div></article></body></html>"
        );

        let extracted = extract_fulltext(html, "https://example.com/post").unwrap();

        assert!(extracted.markdown.contains("retained article body"));
        assert!(!extracted.markdown.contains("页面标题"));
        assert!(!extracted.markdown.contains("avatar.png"));
    }

    #[test]
    fn removes_a_duplicate_title_after_leading_blank_lines() {
        let markdown = remove_duplicate_title(
            "\n\n# Same Article Title\n\n正文内容。".to_string(),
            Some("Same Article Title"),
        );

        assert_eq!(markdown, "正文内容。");
    }

    #[test]
    fn records_the_current_fulltext_extraction_version() {
        assert_eq!(FULLTEXT_EXTRACTION_VERSION, 4);
    }

    #[test]
    fn keeps_text_links_distinct_from_article_images() {
        let html = format!(
            "<html><body><article><p>{}</p><p>适合提前<a href='https://sspai.com/post/74169'>备餐</a>。</p><figure><img src='https://cdnfile.sspai.com/article/chart.png' alt='营养对比图'><figcaption>营养对比</figcaption></figure></article></body></html>",
            "完整正文内容。".repeat(30)
        );

        let extracted = extract_fulltext(html, "https://sspai.com/prime/story/example").unwrap();

        assert!(
            extracted
                .markdown
                .contains("[备餐](https://sspai.com/post/74169)"),
            "普通链接不得转换成图片语法；got: {}",
            extracted.markdown
        );
        assert!(
            !extracted.markdown.contains("![备餐]"),
            "got: {}",
            extracted.markdown
        );
        assert!(extracted.markdown.contains("![营养对比图]"));
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
