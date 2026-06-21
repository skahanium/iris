//! Unified network evidence broker for research workflows.

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::ai_runtime::{
    evidence_mixer, ContextPacket, SourceType, TrustLevel, WebEvidenceMeta, WebSearchBackend,
    WebSourceRank,
};
use crate::error::AppResult;
use crate::llm::fetch_web_page::PageFetchResult;
use crate::storage::db::Database;

const FETCH_EXCERPT_MAX_CHARS: usize = 12_000;

#[derive(Debug, Clone)]
pub struct WebEvidenceBrokerInput {
    pub query: String,
    pub enabled: bool,
    pub max_search_results: usize,
    pub max_fetches: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebEvidenceItem {
    pub url: String,
    pub title: String,
    pub domain: String,
    pub snippet: String,
    pub fetched_excerpt: Option<String>,
    pub source_rank: WebSourceRank,
    pub freshness_label: Option<String>,
    pub failure_reason: Option<String>,
}

pub async fn collect_web_evidence(
    db: &Database,
    input: WebEvidenceBrokerInput,
) -> AppResult<Vec<WebEvidenceItem>> {
    if !input.enabled {
        return Ok(Vec::new());
    }

    let fetch = match crate::llm::search_web::fetch_search_context_for_db(db, &input.query).await {
        Ok(fetch) => fetch,
        Err(error) => {
            return Ok(vec![failed_evidence_item(
                "",
                format!("web_search_failed: {error}"),
            )]);
        }
    };

    let packets = evidence_mixer::web_packets_from_fetch(&fetch, &input.query, None);
    let mut items = normalize_evidence_items(
        packets
            .iter()
            .filter_map(web_evidence_item_from_packet)
            .collect(),
    );
    items.truncate(input.max_search_results);
    enrich_with_page_fetches(db, items, input.max_fetches).await
}

pub fn web_evidence_items_to_packets(query: &str, items: &[WebEvidenceItem]) -> Vec<ContextPacket> {
    let fetched_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.failure_reason.is_none())
        .map(|(index, item)| ContextPacket {
            id: format!("web-broker-{index}-{}", query.len()),
            source_type: SourceType::Web,
            source_path: Some(item.url.clone()),
            title: item.title.clone(),
            heading_path: None,
            source_span: None,
            content_hash: String::new(),
            excerpt: item
                .fetched_excerpt
                .clone()
                .unwrap_or_else(|| item.snippet.clone()),
            retrieval_reason: "web_evidence_broker".into(),
            score: 0.7,
            trust_level: TrustLevel::ExternalWeb,
            citation_label: format!("[W{index}]"),
            stale: false,
            web: Some(WebEvidenceMeta {
                url: Some(item.url.clone()),
                domain: Some(item.domain.clone()),
                published_at: item.freshness_label.clone(),
                fetched_at: fetched_at.clone(),
                search_backend: WebSearchBackend::Duckduckgo,
                source_rank: item.source_rank,
                failure_reason: item.failure_reason.clone(),
                fallback_from: None,
            }),
            corpus: None,
        })
        .collect()
}

fn web_evidence_item_from_packet(packet: &ContextPacket) -> Option<WebEvidenceItem> {
    let web = packet.web.as_ref()?;
    let url = web.url.clone().or_else(|| packet.source_path.clone())?;
    if !is_https_url(&url) {
        return Some(failed_evidence_item(&url, "non_https_rejected".to_string()));
    }
    let domain = web
        .domain
        .clone()
        .unwrap_or_else(|| domain_from_url(&url).unwrap_or_default());
    Some(WebEvidenceItem {
        url,
        title: packet.title.clone(),
        domain,
        snippet: packet.excerpt.clone(),
        fetched_excerpt: None,
        source_rank: web.source_rank,
        freshness_label: web.published_at.clone(),
        failure_reason: web.failure_reason.clone(),
    })
}

fn normalize_evidence_items(items: Vec<WebEvidenceItem>) -> Vec<WebEvidenceItem> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let key = item.url.trim().to_lowercase();
        if !key.is_empty() && !seen.insert(key) {
            continue;
        }
        out.push(item);
    }
    out
}

async fn enrich_with_page_fetches(
    db: &Database,
    items: Vec<WebEvidenceItem>,
    max_fetches: usize,
) -> AppResult<Vec<WebEvidenceItem>> {
    if max_fetches == 0 {
        return Ok(items);
    }

    let mut enriched = Vec::with_capacity(items.len());
    let mut fetched = 0usize;
    for mut item in items {
        if fetched < max_fetches && item.failure_reason.is_none() {
            fetched += 1;
            match crate::llm::fetch_web_page::fetch_web_page(db, &item.url, FETCH_EXCERPT_MAX_CHARS)
                .await
            {
                Ok(page) => apply_page_fetch(&mut item, page),
                Err(error) => item.failure_reason = Some(format!("fetch_failed: {error}")),
            }
        }
        enriched.push(item);
    }
    Ok(enriched)
}

fn apply_page_fetch(item: &mut WebEvidenceItem, page: PageFetchResult) {
    if item.title.trim().is_empty() && !page.title.trim().is_empty() {
        item.title = page.title;
    }
    if !page.text.trim().is_empty() {
        item.fetched_excerpt = Some(page.text);
    }
}

fn failed_evidence_item(url: impl Into<String>, reason: String) -> WebEvidenceItem {
    let url = url.into();
    WebEvidenceItem {
        domain: domain_from_url(&url).unwrap_or_default(),
        title: if url.is_empty() {
            "网络证据代理".into()
        } else {
            url.clone()
        },
        url,
        snippet: String::new(),
        fetched_excerpt: None,
        source_rank: WebSourceRank::Unknown,
        freshness_label: None,
        failure_reason: Some(reason),
    }
}

fn is_https_url(url: &str) -> bool {
    url.trim().to_lowercase().starts_with("https://")
}

fn domain_from_url(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    rest.split('/').next().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(url: &str) -> WebEvidenceItem {
        WebEvidenceItem {
            url: url.into(),
            title: "Title".into(),
            domain: domain_from_url(url).unwrap_or_default(),
            snippet: "Snippet".into(),
            fetched_excerpt: None,
            source_rank: WebSourceRank::Unknown,
            freshness_label: None,
            failure_reason: None,
        }
    }

    #[tokio::test]
    async fn disabled_broker_returns_empty_without_search() {
        let db = Database::open_in_memory().unwrap();
        let items = collect_web_evidence(
            &db,
            WebEvidenceBrokerInput {
                query: "topic".into(),
                enabled: false,
                max_search_results: 5,
                max_fetches: 0,
            },
        )
        .await
        .unwrap();

        assert!(items.is_empty());
    }

    #[test]
    fn deduplicates_urls() {
        let items = normalize_evidence_items(vec![
            item("https://example.com/a"),
            item("https://example.com/a"),
            item("https://example.com/b"),
        ]);

        assert_eq!(items.len(), 2);
    }

    #[test]
    fn records_fetch_failure_without_failing_whole_task() {
        let item = failed_evidence_item("https://example.com/a", "fetch_failed".into());

        assert_eq!(item.failure_reason.as_deref(), Some("fetch_failed"));
        assert_eq!(item.url, "https://example.com/a");
    }

    #[tokio::test]
    async fn broker_records_page_fetch_failure_without_dropping_other_evidence() {
        let db = Database::open_in_memory().unwrap();
        let items = enrich_with_page_fetches(
            &db,
            vec![item("https://localhost/a"), item("https://localhost/b")],
            1,
        )
        .await
        .unwrap();

        assert_eq!(items.len(), 2);
        assert!(items[0]
            .failure_reason
            .as_deref()
            .unwrap_or_default()
            .starts_with("fetch_failed:"));
        assert!(items[1].failure_reason.is_none());
    }

    #[test]
    fn broker_applies_successful_page_fetch_excerpt() {
        let mut item = item("https://example.com/a");

        apply_page_fetch(
            &mut item,
            PageFetchResult {
                url: "https://example.com/a".into(),
                title: "Fetched title".into(),
                text: "Fetched body".into(),
                truncated: false,
                from_cache: false,
                content_hash: "hash".into(),
            },
        );

        assert_eq!(item.fetched_excerpt.as_deref(), Some("Fetched body"));
    }

    #[test]
    fn rejects_non_https_fetch_targets() {
        let rejected = failed_evidence_item("http://example.com/a", "non_https_rejected".into());

        assert!(!is_https_url(&rejected.url));
        assert_eq!(
            rejected.failure_reason.as_deref(),
            Some("non_https_rejected")
        );
    }
}
