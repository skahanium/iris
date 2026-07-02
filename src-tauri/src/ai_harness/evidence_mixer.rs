//! Evidence mixer — parse web results, dedupe, rank, and fuse with local packets.
//!
//! Local user notes outrank web; official/academic web sources outrank community/unknown.

use chrono::Utc;
use std::collections::HashSet;

use crate::ai_runtime::{
    ContextPacket, SourceType, TrustLevel, WebEvidenceMeta, WebSearchBackend, WebSourceRank,
};
use crate::llm::fetch_web_page::PageFetchResult;
use crate::llm::search_web::WebSearchFetchResult;
use crate::llm::web_search_config::WebSearchEffectiveBackend;

/// Parsed row from unified search body (`[n] 标题: …`).
#[derive(Debug, Clone)]
struct ParsedWebRow {
    title: String,
    url: String,
    snippet: String,
    date: Option<String>,
}

/// Classify domain trust for ranking (built-in rules per spec §4.3).
pub fn classify_source_rank(domain: &str) -> WebSourceRank {
    let d = domain.to_lowercase();
    if d.ends_with(".gov.cn")
        || d.ends_with(".gov")
        || d.ends_with(".edu.cn")
        || d.ends_with(".edu")
        || d.ends_with(".mil")
        || d.contains("court.gov")
        || d.contains("npc.gov")
        || d.contains("people.cn")
        || d.contains("people.com.cn")
        || d.contains("xinhua")
        || d.contains("cctv.com")
        || d.contains("mod.gov")
        || d.ends_with(".政务.cn")
    {
        return WebSourceRank::Official;
    }
    if d.contains("scholar")
        || d.contains("arxiv")
        || d.contains("doi.org")
        || d.contains("cnki")
        || d.contains("wanfangdata")
        || d.contains("webofscience")
        || d.contains("pubmed")
        || d.contains("springer")
        || d.contains("sciencedirect")
    {
        return WebSourceRank::Academic;
    }
    if d.contains("wikipedia")
        || d.contains("news.")
        || d.contains("bbc.")
        || d.contains("reuters")
        || d.contains("thepaper.cn")
        || d.contains("caixin.com")
        || (d.ends_with(".com.cn") && (d.contains("xinhua") || d.contains("people.com")))
    {
        return WebSourceRank::Media;
    }
    if d.contains("reddit")
        || d.contains("stackoverflow")
        || d.contains("zhihu.com")
        || d.contains("weibo")
        || d.contains("tieba")
        || d.contains("douban")
        || d.contains("bilibili")
    {
        return WebSourceRank::Community;
    }
    WebSourceRank::Unknown
}

fn rank_score(rank: WebSourceRank) -> f64 {
    match rank {
        WebSourceRank::Official => 0.95,
        WebSourceRank::Academic => 0.9,
        WebSourceRank::Media => 0.65,
        WebSourceRank::Community => 0.45,
        WebSourceRank::Unknown => 0.35,
    }
}

fn extract_domain(url: &str) -> Option<String> {
    let url = url.trim();
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split('/').next()?.split('?').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_lowercase())
    }
}

/// Parse a legacy unified web search text block into rows.
fn parse_search_body(body: &str) -> Vec<ParsedWebRow> {
    let mut rows = Vec::new();
    let mut current: Option<ParsedWebRow> = None;

    for line in body.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix('[') {
            if let Some(bracket) = rest.find(']') {
                let after = rest[bracket + 1..].trim();
                if let Some(title) = after
                    .strip_prefix("标题:")
                    .or_else(|| after.strip_prefix("标题："))
                {
                    if let Some(prev) = current.take() {
                        rows.push(prev);
                    }
                    current = Some(ParsedWebRow {
                        title: title.trim().to_string(),
                        url: String::new(),
                        snippet: String::new(),
                        date: None,
                    });
                    continue;
                }
            }
        }
        if let Some(ref mut row) = current {
            if let Some(link) = t.strip_prefix("链接:").or_else(|| t.strip_prefix("链接：")) {
                row.url = link.trim().to_string();
            } else if let Some(snippet) =
                t.strip_prefix("摘要:").or_else(|| t.strip_prefix("摘要："))
            {
                row.snippet = snippet.trim().to_string();
            } else if let Some(date) = t.strip_prefix("日期:").or_else(|| t.strip_prefix("日期："))
            {
                row.date = Some(date.trim().to_string());
            }
        }
    }
    if let Some(prev) = current {
        rows.push(prev);
    }
    rows
}

/// Convert a fetch result into structured web `ContextPacket`s.
pub fn web_packets_from_fetch(
    fetch: &WebSearchFetchResult,
    query_hint: &str,
    fallback_from: Option<WebSearchBackend>,
) -> Vec<ContextPacket> {
    let backend = match fetch.backend {
        WebSearchEffectiveBackend::Minimax => WebSearchBackend::Minimax,
        WebSearchEffectiveBackend::Duckduckgo => WebSearchBackend::Duckduckgo,
    };
    let fetched_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let rows = parse_search_body(&fetch.body);
    let mut seen_urls = HashSet::new();
    let mut packets = Vec::new();

    for (i, row) in rows.into_iter().enumerate() {
        if row.title.is_empty() && row.snippet.is_empty() {
            continue;
        }
        let url_key = row.url.to_lowercase();
        if !url_key.is_empty() && !seen_urls.insert(url_key.clone()) {
            continue;
        }
        let domain = extract_domain(&row.url);
        let source_rank = domain
            .as_deref()
            .map(classify_source_rank)
            .unwrap_or(WebSourceRank::Unknown);
        let score = rank_score(source_rank) * 0.85;

        packets.push(ContextPacket {
            id: format!("web-{i}-{}", query_hint.len()),
            source_type: SourceType::Web,
            source_path: if row.url.is_empty() {
                None
            } else {
                Some(row.url.clone())
            },
            title: if row.title.is_empty() {
                domain.clone().unwrap_or_else(|| "网页来源".to_string())
            } else {
                row.title.clone()
            },
            heading_path: None,
            source_span: None,
            content_hash: String::new(),
            excerpt: row.snippet.clone(),
            retrieval_reason: "web_search".into(),
            score,
            trust_level: TrustLevel::ExternalWeb,
            citation_label: format!("[W{i}]"),
            stale: false,
            web: Some(WebEvidenceMeta {
                url: if row.url.is_empty() {
                    None
                } else {
                    Some(row.url)
                },
                domain,
                published_at: row.date,
                fetched_at: fetched_at.clone(),
                search_backend: backend,
                source_rank,
                provider_id: None,
                provider_kind: None,
                raw_result_hash: None,
                extraction_method: None,
                conflict_group: None,
                conflict_note: None,
                failure_reason: None,
                fallback_from,
            }),
            corpus: None,
        });
    }

    packets
}

/// Convert a single-page fetch into one web `ContextPacket`.
pub fn web_packets_from_page_fetch(fetch: &PageFetchResult) -> Vec<ContextPacket> {
    let fetched_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let domain = extract_domain(&fetch.url);
    let source_rank = domain
        .as_deref()
        .map(classify_source_rank)
        .unwrap_or(WebSourceRank::Unknown);
    let title = if fetch.title.is_empty() {
        domain.clone().unwrap_or_else(|| "网页正文".to_string())
    } else {
        fetch.title.clone()
    };
    let score = rank_score(source_rank) * 0.9;

    vec![ContextPacket {
        id: format!(
            "web-page-{}",
            fetch.content_hash.chars().take(12).collect::<String>()
        ),
        source_type: SourceType::Web,
        source_path: Some(fetch.url.clone()),
        title,
        heading_path: None,
        source_span: None,
        content_hash: fetch.content_hash.clone(),
        excerpt: fetch.text.clone(),
        retrieval_reason: "web_page_fetch".into(),
        score,
        trust_level: TrustLevel::ExternalWeb,
        citation_label: "[Wp]".into(),
        stale: false,
        web: Some(WebEvidenceMeta {
            url: Some(fetch.url.clone()),
            domain,
            published_at: None,
            fetched_at,
            search_backend: WebSearchBackend::Duckduckgo,
            source_rank,
            provider_id: None,
            provider_kind: None,
            raw_result_hash: None,
            extraction_method: None,
            conflict_group: None,
            conflict_note: None,
            failure_reason: None,
            fallback_from: None,
        }),
        corpus: None,
    }]
}

/// Fuse local and web packets: local first, web supplements, dedupe by id/url.
pub fn mix_and_rank(
    local: Vec<ContextPacket>,
    web: Vec<ContextPacket>,
    max_results: usize,
) -> Vec<ContextPacket> {
    let mut out = local;
    for mut p in web {
        // Boost official/academic slightly within web tier
        if let Some(ref w) = p.web {
            p.score = rank_score(w.source_rank) * 0.85;
        }
        out.push(p);
    }

    // Local notes beat web (spec: 本地用户笔记优先级高于网页)
    for p in out.iter_mut() {
        if matches!(
            p.source_type,
            SourceType::Note | SourceType::Regulation | SourceType::Anchor
        ) {
            p.score = (p.score + 0.35).min(1.0);
        }
    }

    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Dedupe by id and web url
    let mut seen_ids = HashSet::new();
    let mut seen_urls = HashSet::new();
    out.retain(|p| {
        if !seen_ids.insert(p.id.clone()) {
            return false;
        }
        if let Some(ref w) = p.web {
            if let Some(ref url) = w.url {
                if !url.is_empty() && !seen_urls.insert(url.to_lowercase()) {
                    return false;
                }
            }
        }
        true
    });

    out.truncate(max_results);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::web_search_config::WebSearchEffectiveBackend;

    #[test]
    fn parses_search_body_rows() {
        let body = "以下是与问题相关的网页搜索结果：\n\n\
            [1] 标题: 示例\n    链接: https://www.gov.cn/a\n    摘要: 摘要文本\n\n\
            [2] 标题: B\n    链接: https://example.com/b\n    摘要: y\n\n";
        let rows = parse_search_body(body);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].url, "https://www.gov.cn/a");
    }

    #[test]
    fn official_domain_ranks_higher() {
        assert!(matches!(
            classify_source_rank("www.gov.cn"),
            WebSourceRank::Official
        ));
        assert!(matches!(
            classify_source_rank("arxiv.org"),
            WebSourceRank::Academic
        ));
    }

    #[test]
    fn classifies_chinese_domains() {
        assert!(matches!(
            classify_source_rank("people.cn"),
            WebSourceRank::Official
        ));
        assert!(matches!(
            classify_source_rank("thepaper.cn"),
            WebSourceRank::Media
        ));
        assert!(matches!(
            classify_source_rank("cnki.net"),
            WebSourceRank::Academic
        ));
        assert!(matches!(
            classify_source_rank("bilibili.com"),
            WebSourceRank::Community
        ));
        assert!(matches!(
            classify_source_rank("sciencedirect.com"),
            WebSourceRank::Academic
        ));
    }

    #[test]
    fn mix_prefers_local_over_web() {
        let local = vec![ContextPacket {
            id: "note-1".into(),
            source_type: SourceType::Note,
            source_path: Some("a.md".into()),
            title: "本地".into(),
            heading_path: None,
            source_span: None,
            content_hash: String::new(),
            excerpt: "local".into(),
            retrieval_reason: "fts".into(),
            score: 0.5,
            trust_level: TrustLevel::UserNote,
            citation_label: "[L0]".into(),
            stale: false,
            web: None,
            corpus: None,
        }];
        let web = vec![ContextPacket {
            id: "web-1".into(),
            source_type: SourceType::Web,
            source_path: Some("https://x.com".into()),
            title: "Web".into(),
            heading_path: None,
            source_span: None,
            content_hash: String::new(),
            excerpt: "web".into(),
            retrieval_reason: "web".into(),
            score: 0.55,
            trust_level: TrustLevel::ExternalWeb,
            citation_label: "[W0]".into(),
            stale: false,
            web: None,
            corpus: None,
        }];
        let mixed = mix_and_rank(local, web, 10);
        assert_eq!(mixed.first().map(|p| p.source_type), Some(SourceType::Note));
    }

    #[test]
    fn web_packets_from_empty_body() {
        let fetch = WebSearchFetchResult {
            body: "(未找到搜索结果)\n".into(),
            backend: WebSearchEffectiveBackend::Duckduckgo,
        };
        let packets = web_packets_from_fetch(&fetch, "q", None);
        assert!(packets.is_empty());
    }

    #[test]
    fn web_packets_from_page_fetch_sets_reason() {
        use crate::llm::fetch_web_page::PageFetchResult;

        let page = PageFetchResult {
            url: "https://example.com/doc".into(),
            title: "Doc".into(),
            text: "正文内容".into(),
            truncated: false,
            from_cache: false,
            content_hash: "abc123".into(),
        };
        let packets = web_packets_from_page_fetch(&page);
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].retrieval_reason, "web_page_fetch");
        assert!(packets[0].excerpt.contains("正文"));
    }
}
