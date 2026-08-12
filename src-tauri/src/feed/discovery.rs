//! 订阅源自动发现：直接 Feed 或 HTML `<link rel=alternate>` 候选。
//!
//! 只返回安全 URL、候选标题与格式，不返回 HTML 正文；候选不自动订阅。
//! 阶段 2 Task 2.5 `feed::sync` 与阶段 3 IPC 将消费本模块；届时移除标注。
#![allow(dead_code)]

use std::collections::HashSet;

use crate::error::{AppError, AppResult};
use crate::feed::fetch::{FeedHttpClient, FeedNetGate, FetchPurpose};

/// 发现的候选订阅源（DTO：不含 HTML）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FeedCandidate {
    pub url: String,
    pub title: Option<String>,
    /// "rss" | "atom" | "json"
    pub format: Option<String>,
}

const MAX_CANDIDATES: usize = 10;

/// 发现流程：先尝试有界 Feed 解析；content-type/解析表明 HTML 时，
/// 只解析 `link[rel~=alternate]` 且 type 为 RSS/Atom/JSON Feed 的候选。
pub(crate) async fn discover<G: FeedNetGate>(gate: &G, url: &str) -> AppResult<Vec<FeedCandidate>> {
    let result = FeedHttpClient
        .fetch(gate, url, FetchPurpose::Discovery, None, None, None)
        .await?;
    if result.status != 200 {
        return Err(AppError::msg(format!(
            "feed_discovery_http_{}",
            result.status
        )));
    }
    let content_type = result
        .content_type
        .clone()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let is_html = content_type.contains("text/html") || content_type.contains("application/xhtml");

    // 1. 直接 Feed：非 HTML content-type 先尝试有界解析。
    if !is_html {
        if let Ok(feed) = feed_rs::parser::parse(result.bytes.as_slice()) {
            if !feed.entries.is_empty() {
                let title = feed.title.as_ref().map(|title| title.content.clone());
                return Ok(vec![FeedCandidate {
                    url: result.final_url.clone(),
                    title,
                    format: feed_format(&feed.feed_type),
                }]);
            }
        }
    }

    // 2. HTML alternate：content-type 或正文形态表明 HTML 时解析候选。
    if is_html || looks_like_html(&result.bytes) {
        return html_candidates(&result.bytes, &result.final_url, gate);
    }

    Err(AppError::msg("feed_discovery_unsupported"))
}

fn feed_format(feed_type: &feed_rs::model::FeedType) -> Option<String> {
    match feed_type {
        feed_rs::model::FeedType::JSON => Some("json".to_string()),
        feed_rs::model::FeedType::Atom => Some("atom".to_string()),
        feed_rs::model::FeedType::RSS0
        | feed_rs::model::FeedType::RSS1
        | feed_rs::model::FeedType::RSS2 => Some("rss".to_string()),
    }
}

fn looks_like_html(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let head = head.trim_start().to_ascii_lowercase();
    head.starts_with("<!doctype") || head.starts_with("<html")
}

/// 只解析 `link[rel~=alternate]`；候选去重、同源优先、最多 10 个；
/// 每个候选必须通过网门完整 URL 校验（跨协议/私网整体拒绝）。
fn html_candidates<G: FeedNetGate>(
    bytes: &[u8],
    base_url: &str,
    gate: &G,
) -> AppResult<Vec<FeedCandidate>> {
    let document = scraper::Html::parse_document(&String::from_utf8_lossy(bytes));
    let selector = scraper::Selector::parse(r#"link[rel~="alternate"]"#)
        .map_err(|_| AppError::msg("feed_discovery_selector"))?;
    let base_host = reqwest::Url::parse(base_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string));

    let mut seen: HashSet<String> = HashSet::new();
    let mut candidates: Vec<FeedCandidate> = Vec::new();
    for element in document.select(&selector) {
        let feed_type = element
            .value()
            .attr("type")
            .unwrap_or("")
            .to_ascii_lowercase();
        let format = if feed_type.contains("rss") {
            Some("rss")
        } else if feed_type.contains("atom") {
            Some("atom")
        } else if feed_type.contains("json") {
            Some("json")
        } else {
            continue;
        };
        let Some(href) = element.value().attr("href") else {
            continue;
        };
        let Ok(base) = reqwest::Url::parse(base_url) else {
            continue;
        };
        let Ok(absolute) = base.join(href) else {
            continue;
        };
        let absolute = absolute.to_string();
        // 跨协议/私网/危险 scheme：候选必须通过与请求同等的 URL 校验。
        if gate.validate_url(&absolute).is_err() {
            continue;
        }
        if !seen.insert(absolute.clone()) {
            continue;
        }
        let title = element.value().attr("title").map(str::to_string);
        candidates.push(FeedCandidate {
            url: absolute,
            title,
            format: format.map(str::to_string),
        });
    }

    // 同源 host 优先；稳定排序保持页内原顺序。
    candidates.sort_by_key(|candidate| {
        let same_host = reqwest::Url::parse(&candidate.url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            == base_host;
        std::cmp::Reverse(same_host)
    });
    candidates.truncate(MAX_CANDIDATES);
    Ok(candidates)
}
