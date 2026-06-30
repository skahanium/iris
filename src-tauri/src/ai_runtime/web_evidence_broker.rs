//! Unified network evidence broker for research workflows.

use chrono::Utc;
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ai_runtime::{
    ContextPacket, SourceType, TrustLevel, WebEvidenceMeta, WebSearchBackend, WebSourceRank,
};
use crate::credentials::{self, MINIMAX_CREDENTIAL_SERVICE};
use crate::error::{AppError, AppResult};
use crate::llm::fetch_web_page::PageFetchResult;
use crate::llm::search_web::{fetch_native_provider_context, WebSearchFetchResult};
use crate::llm::web_search_config::WebSearchEffectiveBackend;
use crate::storage::db::Database;

const FETCH_EXCERPT_MAX_CHARS: usize = 12_000;

#[derive(Debug, Clone)]
pub struct WebEvidenceBrokerInput {
    pub query: String,
    pub urls: Vec<String>,
    pub enabled: bool,
    pub max_search_results: usize,
    pub max_fetches: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebEvidenceItem {
    pub url: String,
    pub canonical_url: String,
    pub title: String,
    pub domain: String,
    pub snippet: String,
    pub fetched_excerpt: Option<String>,
    pub provider_id: String,
    pub provider_kind: String,
    pub cost_class: String,
    pub raw_result_hash: String,
    pub extraction_method: String,
    pub trust_level: String,
    pub retrieval_reason: String,
    pub search_backend: WebSearchBackend,
    pub source_rank: WebSourceRank,
    pub freshness_label: Option<String>,
    pub failure_reason: Option<String>,
    pub conflict_group: Option<String>,
    pub conflict_note: Option<String>,
}

pub async fn collect_web_evidence(
    db: &Database,
    input: WebEvidenceBrokerInput,
) -> AppResult<Vec<WebEvidenceItem>> {
    if !input.enabled {
        return Ok(Vec::new());
    }

    let mut collected = Vec::new();
    if !input.query.trim().is_empty() {
        for planned_query in plan_search_queries(&input.query) {
            for fetch in collect_search_provider_fetches(db, &planned_query).await {
                match fetch {
                    Ok(fetch) => {
                        collected.extend(web_evidence_items_from_search_fetch(&fetch));
                    }
                    Err(error) => {
                        collected.push(failed_evidence_item(
                            "",
                            "web.provider",
                            "search",
                            format!("web_search_failed: {error}"),
                        ));
                    }
                }
            }
        }
    }

    collected.extend(input.urls.iter().map(|url| explicit_url_item(url)));
    let mut items = normalize_evidence_items(collected);
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
                search_backend: item.search_backend,
                source_rank: item.source_rank,
                provider_id: Some(item.provider_id.clone()),
                provider_kind: Some(item.provider_kind.clone()),
                raw_result_hash: Some(item.raw_result_hash.clone()),
                extraction_method: Some(item.extraction_method.clone()),
                conflict_group: item.conflict_group.clone(),
                conflict_note: item.conflict_note.clone(),
                failure_reason: item.failure_reason.clone(),
                fallback_from: None,
            }),
            corpus: None,
        })
        .collect()
}

fn plan_search_queries(raw_query: &str) -> Vec<String> {
    let sanitized = sanitize_search_query(raw_query);
    if sanitized.is_empty() {
        return Vec::new();
    }
    let mut planned = Vec::new();
    push_unique_query(&mut planned, sanitized.clone());

    for segment in sanitized.split(['。', '？', '?', '！', '!', '\n', ';', '；']) {
        let segment = segment.trim();
        if segment.len() >= 4 {
            push_unique_query(&mut planned, truncate_query(segment, 120));
        }
        if planned.len() >= 3 {
            break;
        }
    }

    if planned.len() < 3 {
        let keywords = keyword_query(&sanitized);
        if !keywords.is_empty() {
            push_unique_query(&mut planned, keywords);
        }
    }

    planned.truncate(3);
    planned
}

fn push_unique_query(planned: &mut Vec<String>, query: String) {
    let normalized = normalize_query_whitespace(&query);
    if normalized.is_empty() {
        return;
    }
    if !planned
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&normalized))
    {
        planned.push(normalized);
    }
}

fn sanitize_search_query(raw_query: &str) -> String {
    let redacted_digits = redact_long_digit_runs(raw_query);
    let kept_tokens = redacted_digits
        .split_whitespace()
        .filter(|token| !is_sensitive_query_token(token))
        .collect::<Vec<_>>()
        .join(" ");
    truncate_query(&normalize_query_whitespace(&kept_tokens), 160)
}

fn redact_long_digit_runs(input: &str) -> String {
    let mut out = String::new();
    let mut digit_run = String::new();
    for ch in input.chars() {
        if ch.is_ascii_digit() {
            digit_run.push(ch);
            continue;
        }
        flush_digit_run(&mut out, &mut digit_run);
        out.push(ch);
    }
    flush_digit_run(&mut out, &mut digit_run);
    out
}

fn flush_digit_run(out: &mut String, digit_run: &mut String) {
    if digit_run.is_empty() {
        return;
    }
    if digit_run.len() < 7 {
        out.push_str(digit_run);
    } else {
        out.push(' ');
    }
    digit_run.clear();
}

fn is_sensitive_query_token(token: &str) -> bool {
    let trimmed = token.trim_matches(|ch: char| ch.is_ascii_punctuation());
    let lower = trimmed.to_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || (trimmed.contains('@') && trimmed.contains('.'))
}

fn keyword_query(query: &str) -> String {
    let stopwords = [
        "请", "帮我", "一下", "关于", "这个", "那个", "需要", "搜索", "查询", "总结", "核对",
        "please", "search", "about", "with", "from", "that", "this", "the", "and", "for",
    ];
    let mut words = Vec::new();
    for token in query.split(|ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                ',' | '.' | ':' | '：' | '，' | '。' | '?' | '？' | '!' | '！' | ';' | '；'
            )
    }) {
        let token = token.trim();
        if token.len() < 2 {
            continue;
        }
        if stopwords
            .iter()
            .any(|word| token.eq_ignore_ascii_case(word))
        {
            continue;
        }
        if !words
            .iter()
            .any(|word: &&str| word.eq_ignore_ascii_case(token))
        {
            words.push(token);
        }
        if words.len() >= 8 {
            break;
        }
    }
    truncate_query(&words.join(" "), 120)
}

fn truncate_query(query: &str, max_chars: usize) -> String {
    query.chars().take(max_chars).collect::<String>()
}

fn normalize_query_whitespace(query: &str) -> String {
    query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches([' ', '\n', '\t', '。', '，', ',', '.', '？', '?'])
        .to_string()
}

#[derive(Debug, Clone)]
struct SearchProviderFetch {
    body: String,
    backend: WebSearchEffectiveBackend,
    provider_id: String,
    provider_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SearchProviderCandidate {
    Mcp(String),
    Native(WebSearchEffectiveBackend),
}

fn search_provider_candidates(db: &Database) -> Vec<SearchProviderCandidate> {
    let mut candidates = Vec::new();
    for provider in crate::ai_runtime::mcp_runtime_registry::list_enabled_web_provider_mappings(db)
        .unwrap_or_default()
    {
        if provider.kind == "mcp" && provider.web_search_mapping_json.is_some() {
            candidates.push(SearchProviderCandidate::Mcp(provider.id));
        }
    }

    if credentials::api_key_configured(db, MINIMAX_CREDENTIAL_SERVICE).unwrap_or(false) {
        candidates.push(SearchProviderCandidate::Native(
            WebSearchEffectiveBackend::Minimax,
        ));
    }
    candidates.push(SearchProviderCandidate::Native(
        WebSearchEffectiveBackend::Duckduckgo,
    ));
    candidates.truncate(2);
    candidates
}

async fn collect_search_provider_fetches(
    db: &Database,
    query: &str,
) -> Vec<Result<SearchProviderFetch, String>> {
    let futures = search_provider_candidates(db)
        .into_iter()
        .map(|candidate| async move {
            match candidate {
                SearchProviderCandidate::Mcp(provider_id) => {
                    collect_mcp_search_provider_fetch(db, query, &provider_id).await
                }
                SearchProviderCandidate::Native(backend) => {
                    collect_native_search_provider_fetch(db, query, backend).await
                }
            }
            .map_err(|err| err.to_string())
        });
    join_all(futures).await
}

async fn collect_native_search_provider_fetch(
    db: &Database,
    query: &str,
    backend: WebSearchEffectiveBackend,
) -> AppResult<SearchProviderFetch> {
    let provider_id = provider_id_for_effective(backend);
    ensure_provider_circuit_allows(provider_id)?;
    match fetch_native_provider_context(db, query, backend).await {
        Ok(fetch) => {
            record_provider_success(provider_id);
            Ok(search_provider_fetch_from_native(fetch))
        }
        Err(error) => {
            record_provider_failure(provider_id);
            Err(error)
        }
    }
}

async fn collect_mcp_search_provider_fetch(
    db: &Database,
    query: &str,
    provider_id: &str,
) -> AppResult<SearchProviderFetch> {
    ensure_provider_circuit_allows(provider_id)?;
    let provider = resolve_mcp_provider_mapping(db, provider_id, "web.search")?;
    let call_result = crate::ai_runtime::mcp_host_runtime::call_provider_tool(
        db,
        &provider,
        serde_json::json!({
            "query": query,
            "q": query,
        }),
        crate::ai_runtime::mcp_host_runtime::McpHostRuntimeOptions {
            request_timeout: std::time::Duration::from_secs(20),
            max_stdout_line_bytes: 64 * 1024,
            max_stderr_bytes: 4 * 1024,
            cwd: None,
        },
    )
    .await;
    let call = match call_result {
        Ok(call) => {
            record_provider_success(provider_id);
            call
        }
        Err(error) => {
            record_provider_failure(provider_id);
            return Err(error);
        }
    };
    Ok(SearchProviderFetch {
        body: mcp_search_result_body(&call.result),
        backend: WebSearchEffectiveBackend::Duckduckgo,
        provider_id: call.provider_id,
        provider_kind: "mcp".into(),
    })
}

fn ensure_provider_circuit_allows(provider_id: &str) -> AppResult<()> {
    if crate::ai_runtime::circuit_breaker::is_request_allowed(provider_id) {
        Ok(())
    } else {
        Err(AppError::msg("provider_disabled: circuit_open"))
    }
}

fn record_provider_success(provider_id: &str) {
    crate::ai_runtime::circuit_breaker::record_success(provider_id);
}

fn record_provider_failure(provider_id: &str) {
    crate::ai_runtime::circuit_breaker::record_failure(provider_id);
}

fn resolve_mcp_provider_mapping(
    db: &Database,
    provider_id: &str,
    capability: &str,
) -> AppResult<crate::ai_runtime::capability_resolver::ResolvedCapabilityProvider> {
    let providers =
        crate::ai_runtime::mcp_runtime_registry::list_enabled_web_provider_mappings(db)?;
    for provider in providers {
        if provider.id != provider_id || provider.kind != "mcp" {
            continue;
        }
        let mapping_json = match capability {
            "web.search" => provider.web_search_mapping_json.as_deref(),
            "web.fetch" => provider.web_fetch_mapping_json.as_deref(),
            _ => None,
        };
        let Some(tool_name) = mapping_json.and_then(mapping_tool_name) else {
            break;
        };
        return Ok(
            crate::ai_runtime::capability_resolver::ResolvedCapabilityProvider {
                capability: capability.into(),
                provider_kind: "mcp".into(),
                profile_id: provider.id,
                tool_name,
                schema_hash: provider.provider_config_hash,
                requires_confirmation: true,
            },
        );
    }
    Err(AppError::msg(format!(
        "no enabled MCP provider mapping for {provider_id}:{capability}"
    )))
}

fn mapping_tool_name(mapping_json: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(mapping_json).ok()?;
    value
        .get("tool")
        .or_else(|| value.get("tool_name"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn search_provider_fetch_from_native(fetch: WebSearchFetchResult) -> SearchProviderFetch {
    SearchProviderFetch {
        body: fetch.body,
        backend: fetch.backend,
        provider_id: provider_id_for_effective(fetch.backend).into(),
        provider_kind: "native".into(),
    }
}

fn mcp_search_result_body(result: &serde_json::Value) -> String {
    if let Some(text) = result.as_str() {
        return text.to_string();
    }
    if let Some(items) = result.get("content").and_then(|value| value.as_array()) {
        let mut body = String::new();
        for item in items {
            if let Some(text) = item.get("text").and_then(|value| value.as_str()) {
                body.push_str(text);
                body.push('\n');
            }
        }
        if !body.trim().is_empty() {
            return body;
        }
    }
    result.to_string()
}

fn web_evidence_items_from_search_fetch(fetch: &SearchProviderFetch) -> Vec<WebEvidenceItem> {
    let search_backend = search_backend_for_effective(fetch.backend);
    parse_search_result_rows(&fetch.body)
        .into_iter()
        .map(|row| {
            if !is_https_url(&row.url) {
                return failed_evidence_item(
                    &row.url,
                    &fetch.provider_id,
                    "web.search",
                    "non_https_rejected".into(),
                );
            }
            let canonical_url = canonicalize_url(&row.url);
            WebEvidenceItem {
                domain: domain_from_url(&row.url).unwrap_or_default(),
                raw_result_hash: result_hash(&[&row.url, &row.title, &row.snippet]),
                url: row.url,
                canonical_url,
                title: row.title,
                snippet: row.snippet,
                fetched_excerpt: None,
                provider_id: fetch.provider_id.clone(),
                provider_kind: fetch.provider_kind.clone(),
                cost_class: "free".into(),
                extraction_method: "search_snippet".into(),
                trust_level: "external_untrusted".into(),
                retrieval_reason: "web.search".into(),
                search_backend,
                source_rank: WebSourceRank::Unknown,
                freshness_label: None,
                failure_reason: None,
                conflict_group: None,
                conflict_note: None,
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
struct SearchResultRow {
    title: String,
    url: String,
    snippet: String,
}

fn parse_search_result_rows(body: &str) -> Vec<SearchResultRow> {
    let mut rows = Vec::new();
    let mut current: Option<SearchResultRow> = None;
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some((_, title)) = trimmed.split_once("] 标题:") {
            if let Some(row) = current.take().filter(|row| !row.url.trim().is_empty()) {
                rows.push(row);
            }
            current = Some(SearchResultRow {
                title: title.trim().to_string(),
                url: String::new(),
                snippet: String::new(),
            });
            continue;
        }
        if let Some(row) = current.as_mut() {
            if let Some(url) = trimmed.strip_prefix("链接:") {
                row.url = url.trim().to_string();
            } else if let Some(snippet) = trimmed.strip_prefix("摘要:") {
                row.snippet = snippet.trim().to_string();
            }
        }
    }
    if let Some(row) = current.take().filter(|row| !row.url.trim().is_empty()) {
        rows.push(row);
    }
    rows
}

fn search_backend_for_effective(backend: WebSearchEffectiveBackend) -> WebSearchBackend {
    match backend {
        WebSearchEffectiveBackend::Minimax => WebSearchBackend::Minimax,
        WebSearchEffectiveBackend::Duckduckgo => WebSearchBackend::Duckduckgo,
    }
}

fn provider_id_for_effective(backend: WebSearchEffectiveBackend) -> &'static str {
    match backend {
        WebSearchEffectiveBackend::Minimax => "native.minimax",
        WebSearchEffectiveBackend::Duckduckgo => "native.duckduckgo",
    }
}

fn normalize_evidence_items(items: Vec<WebEvidenceItem>) -> Vec<WebEvidenceItem> {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut out = Vec::new();
    for item in items {
        let key = item.canonical_url.trim().to_lowercase();
        if let Some(existing_index) = seen.get(&key).copied() {
            let existing: &mut WebEvidenceItem = &mut out[existing_index];
            if existing.snippet.trim() != item.snippet.trim()
                && !existing.snippet.trim().is_empty()
                && !item.snippet.trim().is_empty()
            {
                let group = format!("url-{}", result_hash(&[&key]));
                existing.conflict_group = Some(group);
                existing.conflict_note =
                    Some("同一 URL 的不同 provider 摘要不完全一致，需按来源核对。".into());
            }
            continue;
        }
        if !key.is_empty() {
            seen.insert(key, out.len());
        }
        out.push(item);
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FetchProviderCandidate {
    Mcp(String),
    Native,
}

#[derive(Debug, Clone)]
struct PageProviderFetch {
    title: String,
    text: String,
    provider_id: String,
    provider_kind: String,
    extraction_method: String,
}

fn fetch_provider_candidates(db: &Database) -> Vec<FetchProviderCandidate> {
    let mut candidates = Vec::new();
    for provider in crate::ai_runtime::mcp_runtime_registry::list_enabled_web_provider_mappings(db)
        .unwrap_or_default()
    {
        if provider.kind == "mcp" && provider.web_fetch_mapping_json.is_some() {
            candidates.push(FetchProviderCandidate::Mcp(provider.id));
        }
    }
    candidates.push(FetchProviderCandidate::Native);
    candidates.truncate(2);
    candidates
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
            match fetch_url_with_providers(db, &item.url).await {
                Ok(page) => apply_page_provider_fetch(&mut item, page),
                Err(error) => item.failure_reason = Some(format!("fetch_failed: {error}")),
            }
        }
        enriched.push(item);
    }
    Ok(enriched)
}

async fn fetch_url_with_providers(db: &Database, url: &str) -> AppResult<PageProviderFetch> {
    let futures = fetch_provider_candidates(db)
        .into_iter()
        .map(|candidate| async move {
            match candidate {
                FetchProviderCandidate::Mcp(provider_id) => {
                    collect_mcp_page_fetch(db, url, &provider_id).await
                }
                FetchProviderCandidate::Native => collect_native_page_fetch(db, url).await,
            }
        });
    let results = join_all(futures).await;
    let mut failures = Vec::new();
    let mut successes = Vec::new();
    for result in results {
        match result {
            Ok(fetch) => successes.push(fetch),
            Err(error) => failures.push(error.to_string()),
        }
    }
    if !successes.is_empty() {
        return Ok(merge_page_provider_fetches(url, successes));
    }
    Err(AppError::msg(if failures.is_empty() {
        "no fetch providers available".into()
    } else {
        failures.join("; ")
    }))
}

fn merge_page_provider_fetches(url: &str, fetches: Vec<PageProviderFetch>) -> PageProviderFetch {
    let mut titles = Vec::new();
    let mut texts = Vec::new();
    let mut provider_ids = Vec::new();
    let mut provider_kinds = Vec::new();
    let mut methods = Vec::new();

    for fetch in fetches {
        push_unique_string(&mut titles, fetch.title);
        push_unique_string(&mut texts, fetch.text);
        push_unique_string(&mut provider_ids, fetch.provider_id);
        push_unique_string(&mut provider_kinds, fetch.provider_kind);
        push_unique_string(&mut methods, fetch.extraction_method);
    }

    PageProviderFetch {
        title: titles
            .into_iter()
            .find(|title| !title.trim().is_empty())
            .unwrap_or_else(|| url.to_string()),
        text: texts.join("\n\n---\n\n"),
        provider_id: provider_ids.join("+"),
        provider_kind: if provider_kinds.len() == 1 {
            provider_kinds.remove(0)
        } else {
            "mixed".into()
        },
        extraction_method: if methods.len() == 1 {
            methods.remove(0)
        } else {
            "merged_fetch".into()
        },
    }
}

fn push_unique_string(values: &mut Vec<String>, value: String) {
    if value.trim().is_empty() {
        return;
    }
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

async fn collect_native_page_fetch(db: &Database, url: &str) -> AppResult<PageProviderFetch> {
    let provider_id = "native.fetch";
    ensure_provider_circuit_allows(provider_id)?;
    match crate::llm::fetch_web_page::fetch_web_page(db, url, FETCH_EXCERPT_MAX_CHARS).await {
        Ok(page) => {
            record_provider_success(provider_id);
            Ok(page_provider_fetch_from_native(page))
        }
        Err(error) => {
            record_provider_failure(provider_id);
            Err(error)
        }
    }
}

async fn collect_mcp_page_fetch(
    db: &Database,
    url: &str,
    provider_id: &str,
) -> AppResult<PageProviderFetch> {
    crate::llm::fetch_web_page::validate_fetch_url(url)?;
    ensure_provider_circuit_allows(provider_id)?;
    let provider = resolve_mcp_provider_mapping(db, provider_id, "web.fetch")?;
    let call_result = crate::ai_runtime::mcp_host_runtime::call_provider_tool(
        db,
        &provider,
        serde_json::json!({
            "url": url,
            "max_chars": FETCH_EXCERPT_MAX_CHARS,
        }),
        crate::ai_runtime::mcp_host_runtime::McpHostRuntimeOptions {
            request_timeout: std::time::Duration::from_secs(20),
            max_stdout_line_bytes: 128 * 1024,
            max_stderr_bytes: 4 * 1024,
            cwd: None,
        },
    )
    .await;
    let call = match call_result {
        Ok(call) => {
            record_provider_success(provider_id);
            call
        }
        Err(error) => {
            record_provider_failure(provider_id);
            return Err(error);
        }
    };
    Ok(mcp_page_fetch_result(provider_id, url, &call.result))
}

fn page_provider_fetch_from_native(page: PageFetchResult) -> PageProviderFetch {
    PageProviderFetch {
        title: page.title,
        text: page.text,
        provider_id: "native.fetch".into(),
        provider_kind: "native".into(),
        extraction_method: "native_readability".into(),
    }
}

fn mcp_page_fetch_result(
    provider_id: &str,
    url: &str,
    result: &serde_json::Value,
) -> PageProviderFetch {
    let title = result
        .get("title")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(url)
        .to_string();
    let text = result
        .get("text")
        .or_else(|| result.get("body"))
        .or_else(|| result.get("content"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| mcp_search_result_body(result));
    PageProviderFetch {
        title,
        text,
        provider_id: provider_id.into(),
        provider_kind: "mcp".into(),
        extraction_method: "mcp_fetch".into(),
    }
}

fn apply_page_provider_fetch(item: &mut WebEvidenceItem, page: PageProviderFetch) {
    if item.title.trim().is_empty() && !page.title.trim().is_empty() {
        item.title = page.title;
    }
    if !page.text.trim().is_empty() {
        item.fetched_excerpt = Some(page.text);
        item.extraction_method = page.extraction_method;
        item.provider_id = page.provider_id;
        item.provider_kind = page.provider_kind;
        item.raw_result_hash = result_hash(&[
            &item.url,
            &item.title,
            item.fetched_excerpt.as_deref().unwrap_or_default(),
        ]);
    }
}

#[cfg(test)]
fn apply_page_fetch(item: &mut WebEvidenceItem, page: PageFetchResult) {
    apply_page_provider_fetch(item, page_provider_fetch_from_native(page));
}

fn explicit_url_item(url: &str) -> WebEvidenceItem {
    if !is_https_url(url) {
        return failed_evidence_item(
            url,
            "native.url",
            "explicit_url",
            "non_https_rejected".into(),
        );
    }
    let canonical_url = canonicalize_url(url);
    WebEvidenceItem {
        url: url.trim().to_string(),
        canonical_url: canonical_url.clone(),
        domain: domain_from_url(url).unwrap_or_default(),
        title: url.trim().to_string(),
        snippet: String::new(),
        fetched_excerpt: None,
        provider_id: "native.url".into(),
        provider_kind: "native".into(),
        cost_class: "free".into(),
        raw_result_hash: result_hash(&[url]),
        extraction_method: "explicit_url".into(),
        trust_level: "external_untrusted".into(),
        retrieval_reason: "explicit_url".into(),
        search_backend: WebSearchBackend::Duckduckgo,
        source_rank: WebSourceRank::Unknown,
        freshness_label: None,
        failure_reason: None,
        conflict_group: None,
        conflict_note: None,
    }
}

fn failed_evidence_item(
    url: impl Into<String>,
    provider_id: impl Into<String>,
    retrieval_reason: impl Into<String>,
    reason: String,
) -> WebEvidenceItem {
    let url = url.into();
    let canonical_url = canonicalize_url(&url);
    let raw_result_hash = result_hash(&[&url, &reason]);
    WebEvidenceItem {
        canonical_url,
        domain: domain_from_url(&url).unwrap_or_default(),
        title: if url.is_empty() {
            "网络证据代理".into()
        } else {
            url.clone()
        },
        url,
        snippet: String::new(),
        fetched_excerpt: None,
        provider_id: provider_id.into(),
        provider_kind: "native".into(),
        cost_class: "free".into(),
        raw_result_hash,
        extraction_method: "none".into(),
        trust_level: "external_untrusted".into(),
        retrieval_reason: retrieval_reason.into(),
        search_backend: WebSearchBackend::Duckduckgo,
        source_rank: WebSourceRank::Unknown,
        freshness_label: None,
        failure_reason: Some(reason),
        conflict_group: None,
        conflict_note: None,
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

fn canonicalize_url(url: &str) -> String {
    let mut trimmed = url.trim().to_lowercase();
    if let Some((before_fragment, _)) = trimmed.split_once('#') {
        trimmed = before_fragment.to_string();
    }
    while trimmed.ends_with('/') {
        trimmed.pop();
    }
    trimmed
}

fn result_hash(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(b"\0");
    }
    let digest = hasher.finalize();
    hex::encode(&digest[..12])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(url: &str) -> WebEvidenceItem {
        WebEvidenceItem {
            url: url.into(),
            canonical_url: canonicalize_url(url),
            title: "Title".into(),
            domain: domain_from_url(url).unwrap_or_default(),
            snippet: "Snippet".into(),
            fetched_excerpt: None,
            provider_id: "native.duckduckgo".into(),
            provider_kind: "native".into(),
            cost_class: "free".into(),
            raw_result_hash: result_hash(&[url]),
            extraction_method: "search_snippet".into(),
            trust_level: "external_untrusted".into(),
            retrieval_reason: "web.search".into(),
            search_backend: WebSearchBackend::Duckduckgo,
            source_rank: WebSourceRank::Unknown,
            freshness_label: None,
            failure_reason: None,
            conflict_group: None,
            conflict_note: None,
        }
    }

    #[tokio::test]
    async fn disabled_broker_returns_empty_without_search() {
        let db = Database::open_in_memory().unwrap();
        let items = collect_web_evidence(
            &db,
            WebEvidenceBrokerInput {
                query: "topic".into(),
                urls: Vec::new(),
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
    fn query_planner_strips_obvious_pii_and_limits_query_count() {
        let planned = plan_search_queries(
            "请帮我搜索 alice@example.com https://private.example/x \
             手机 13812345678 sqlite-vec 发布 2026 最新变化？再核对 Rust sqlite vec 用法。",
        );

        assert!(!planned.is_empty());
        assert!(planned.len() <= 3);
        let joined = planned.join("\n");
        assert!(!joined.contains("alice@example.com"));
        assert!(!joined.contains("https://private.example"));
        assert!(!joined.contains("13812345678"));
        assert!(joined.contains("sqlite-vec") || joined.contains("sqlite vec"));
        assert!(planned.iter().all(|query| query.chars().count() <= 160));
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
        let item = failed_evidence_item(
            "https://example.com/a",
            "native.fetch",
            "web.fetch",
            "fetch_failed".into(),
        );

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
        let rejected = failed_evidence_item(
            "http://example.com/a",
            "native.url",
            "explicit_url",
            "non_https_rejected".into(),
        );

        assert!(!is_https_url(&rejected.url));
        assert_eq!(
            rejected.failure_reason.as_deref(),
            Some("non_https_rejected")
        );
    }

    #[test]
    fn explicit_url_items_carry_provider_metadata() {
        let item = explicit_url_item("https://Example.com/a#section");

        assert_eq!(item.canonical_url, "https://example.com/a");
        assert_eq!(item.provider_id, "native.url");
        assert_eq!(item.retrieval_reason, "explicit_url");
        assert_eq!(item.trust_level, "external_untrusted");
        assert!(item.failure_reason.is_none());
    }

    #[test]
    fn search_provider_candidates_use_ddg_when_minimax_is_not_configured() {
        let db = Database::open_in_memory().unwrap();

        let candidates = search_provider_candidates(&db);

        assert_eq!(
            candidates,
            vec![SearchProviderCandidate::Native(
                WebSearchEffectiveBackend::Duckduckgo
            )]
        );
    }

    #[test]
    fn search_provider_candidates_select_top_two_with_mcp_priority() {
        let db = Database::open_in_memory().unwrap();
        crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
                id: "mcp-search".into(),
                name: "MCP Search".into(),
                kind: "mcp".into(),
                enabled: true,
                transport_kind: "stdio".into(),
                transport_config_json: "{}".into(),
                credential_refs_json: "{}".into(),
                web_search_mapping_json: Some(r#"{"tool":"search"}"#.into()),
                web_fetch_mapping_json: None,
            },
        )
        .unwrap();

        let candidates = search_provider_candidates(&db);

        assert_eq!(
            candidates,
            vec![
                SearchProviderCandidate::Mcp("mcp-search".into()),
                SearchProviderCandidate::Native(WebSearchEffectiveBackend::Duckduckgo),
            ]
        );
    }

    #[test]
    fn search_provider_candidates_include_minimax_only_when_configured() {
        let db = Database::open_in_memory().unwrap();
        crate::credentials::mark_api_key_configured(
            &db,
            crate::credentials::MINIMAX_CREDENTIAL_SERVICE,
        )
        .unwrap();

        let candidates = search_provider_candidates(&db);

        assert_eq!(
            candidates,
            vec![
                SearchProviderCandidate::Native(WebSearchEffectiveBackend::Minimax),
                SearchProviderCandidate::Native(WebSearchEffectiveBackend::Duckduckgo),
            ]
        );
    }

    #[test]
    fn provider_circuit_open_blocks_broker_provider_call() {
        let provider_id = "broker-circuit-open-test";
        for _ in 0..5 {
            record_provider_failure(provider_id);
        }

        let err = ensure_provider_circuit_allows(provider_id).unwrap_err();
        assert!(err.to_string().contains("provider_disabled"));
        assert!(err.to_string().contains("circuit_open"));
    }

    #[test]
    fn fetch_provider_candidates_use_mcp_then_native() {
        let db = Database::open_in_memory().unwrap();
        crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
                id: "mcp-fetch".into(),
                name: "MCP Fetch".into(),
                kind: "mcp".into(),
                enabled: true,
                transport_kind: "stdio".into(),
                transport_config_json: "{}".into(),
                credential_refs_json: "{}".into(),
                web_search_mapping_json: None,
                web_fetch_mapping_json: Some(r#"{"tool":"fetch"}"#.into()),
            },
        )
        .unwrap();

        let candidates = fetch_provider_candidates(&db);

        assert_eq!(
            candidates,
            vec![
                FetchProviderCandidate::Mcp("mcp-fetch".into()),
                FetchProviderCandidate::Native,
            ]
        );
    }

    #[test]
    fn mcp_page_fetch_result_extracts_text_and_provider_metadata() {
        let fetch = mcp_page_fetch_result(
            "mcp-fetch",
            "https://example.com/a",
            &serde_json::json!({
                "title": "Fetched title",
                "text": "Fetched body"
            }),
        );

        assert_eq!(fetch.provider_id, "mcp-fetch");
        assert_eq!(fetch.provider_kind, "mcp");
        assert_eq!(fetch.title, "Fetched title");
        assert_eq!(fetch.text, "Fetched body");
        assert_eq!(fetch.extraction_method, "mcp_fetch");
    }

    #[test]
    fn successful_fetch_provider_results_are_merged_not_raced() {
        let merged = merge_page_provider_fetches(
            "https://example.com/a",
            vec![
                PageProviderFetch {
                    title: "Native title".into(),
                    text: "Native body".into(),
                    provider_id: "native.fetch".into(),
                    provider_kind: "native".into(),
                    extraction_method: "native_readability".into(),
                },
                PageProviderFetch {
                    title: "MCP title".into(),
                    text: "MCP body".into(),
                    provider_id: "mcp-fetch".into(),
                    provider_kind: "mcp".into(),
                    extraction_method: "mcp_fetch".into(),
                },
            ],
        );

        assert!(merged.text.contains("Native body"));
        assert!(merged.text.contains("MCP body"));
        assert_eq!(merged.provider_id, "native.fetch+mcp-fetch");
        assert_eq!(merged.provider_kind, "mixed");
        assert_eq!(merged.extraction_method, "merged_fetch");
    }

    #[test]
    fn duplicate_url_conflicts_are_marked_without_adjudication() {
        let mut first = item("https://example.com/a");
        first.snippet = "One claim".into();
        let mut second = item("https://example.com/a#fragment");
        second.snippet = "Different claim".into();

        let items = normalize_evidence_items(vec![first, second]);

        assert_eq!(items.len(), 1);
        assert!(items[0].conflict_group.is_some());
        assert!(items[0]
            .conflict_note
            .as_deref()
            .unwrap_or_default()
            .contains("不完全一致"));
    }
}
