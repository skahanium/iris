//! Unified network evidence broker for research workflows.

use chrono::Utc;
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

use crate::ai_runtime::{
    ContextPacket, SourceType, TrustLevel, WebEvidenceMeta, WebSearchBackend, WebSourceRank,
};
use crate::error::{AppError, AppResult};
use crate::llm::fetch_web_page::PageFetchResult;
use crate::storage::db::Database;

const FETCH_EXCERPT_MAX_CHARS: usize = 12_000;
const WEB_PACKET_EXCERPT_MAX_CHARS: usize = 4_000;
const MERGED_FETCH_MAX_CHARS: usize = 12_000;
const WEB_CONTEXT_TRUNCATION_MARKER: &str = "\n...（网页正文已按上下文预算截断）";
const WEB_FETCH_TURN_BUDGET: Duration = Duration::from_secs(8);

fn truncate_web_context_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let marker_chars = WEB_CONTEXT_TRUNCATION_MARKER.chars().count();
    let body_chars = max_chars.saturating_sub(marker_chars).max(1);
    let mut out = text.chars().take(body_chars).collect::<String>();
    out.push_str(WEB_CONTEXT_TRUNCATION_MARKER);
    out
}

#[derive(Debug, Clone)]
pub struct WebEvidenceBrokerInput {
    pub query: String,
    pub urls: Vec<String>,
    pub enabled: bool,
    pub max_search_results: usize,
    pub max_fetches: usize,
    /// Optional immutable Run-local provider/mapping snapshot. When supplied,
    /// the broker fails closed if that provider changes before a request.
    pub provider_snapshot:
        Option<crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WebEvidenceSearchRequestUsage {
    pub mcp: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebEvidenceProviderUsage {
    pub provider_id: String,
    pub provider_kind: String,
    pub successful_search_requests: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebEvidenceUsage {
    pub successful_search_requests: WebEvidenceSearchRequestUsage,
    pub providers: Vec<WebEvidenceProviderUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebEvidenceBrokerOutput {
    pub items: Vec<WebEvidenceItem>,
    pub usage: WebEvidenceUsage,
}

pub async fn collect_web_evidence(
    db: &Database,
    input: WebEvidenceBrokerInput,
) -> AppResult<Vec<WebEvidenceItem>> {
    Ok(collect_web_evidence_with_usage(db, input).await?.items)
}

pub async fn collect_web_evidence_with_usage(
    db: &Database,
    input: WebEvidenceBrokerInput,
) -> AppResult<WebEvidenceBrokerOutput> {
    let planned_queries = plan_search_queries(&input.query);
    collect_web_evidence_with_queries(db, input, planned_queries).await
}

/// Collect the first required Run evidence from exactly one original user query.
/// The normal Broker path may broaden an interactive tool call into bounded query variants;
/// the pre-answer stage must remain deterministic and low-latency instead.
pub async fn collect_initial_run_web_evidence_with_usage(
    db: &Database,
    input: WebEvidenceBrokerInput,
) -> AppResult<WebEvidenceBrokerOutput> {
    let planned_queries = initial_run_search_queries(&input.query);
    collect_web_evidence_with_queries(db, input, planned_queries).await
}

async fn collect_web_evidence_with_queries(
    db: &Database,
    input: WebEvidenceBrokerInput,
    planned_queries: Vec<String>,
) -> AppResult<WebEvidenceBrokerOutput> {
    if !input.enabled {
        return Ok(WebEvidenceBrokerOutput {
            items: Vec::new(),
            usage: WebEvidenceUsage::default(),
        });
    }

    let mut collected = Vec::new();
    let mut usage = WebEvidenceUsage::default();
    if !planned_queries.is_empty() {
        for fetch in collect_planned_query_fetches(
            db,
            planned_queries,
            input.max_search_results,
            input.provider_snapshot.as_ref(),
        )
        .await
        {
            match fetch {
                Ok(fetch) => {
                    let items = web_evidence_items_from_search_fetch(&fetch);
                    record_successful_search_usage(&mut usage, &fetch, &items);
                    collected.extend(items);
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

    suppress_search_provider_failures_if_success(&mut collected);
    collected.extend(input.urls.iter().map(|url| explicit_url_item(url)));
    let mut items = normalize_evidence_items(collected);
    items.truncate(input.max_search_results);
    let items = enrich_with_page_fetches(
        db,
        items,
        input.max_fetches,
        input.provider_snapshot.as_ref(),
    )
    .await?;
    Ok(WebEvidenceBrokerOutput { items, usage })
}

fn initial_run_search_queries(query: &str) -> Vec<String> {
    if !query.trim().is_empty() {
        vec![query.to_string()]
    } else {
        Vec::new()
    }
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
            excerpt: truncate_web_context_text(
                &item
                    .fetched_excerpt
                    .clone()
                    .unwrap_or_else(|| item.snippet.clone()),
                WEB_PACKET_EXCERPT_MAX_CHARS,
            ),
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
    search_backend: WebSearchBackend,
    provider_id: String,
    provider_kind: String,
    failure_reason: Option<String>,
    diagnostic_summary: Option<String>,
}

impl SearchProviderFetch {
    fn from_mcp_probe(probe: McpSearchProviderProbe) -> Self {
        Self {
            body: probe.diagnostic.body.clone(),
            search_backend: WebSearchBackend::Provider,
            provider_id: probe.provider_id.clone(),
            provider_kind: "mcp".into(),
            failure_reason: probe.diagnostic.failure_reason.clone(),
            diagnostic_summary: Some(probe.summary()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct McpSearchProviderProbe {
    pub provider_id: String,
    pub tool_name: String,
    pub argument_keys: Vec<String>,
    pub auth_header_present: bool,
    pub diagnostic: McpSearchResultDiagnostic,
}

impl McpSearchProviderProbe {
    pub(crate) fn summary(&self) -> String {
        self.diagnostic.message(
            &self.provider_id,
            &self.tool_name,
            &self.argument_keys,
            self.auth_header_present,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SearchProviderCandidate {
    Mcp(String),
}

fn search_provider_candidates(
    db: &Database,
    provider_snapshot: Option<
        &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary,
    >,
) -> AppResult<Vec<SearchProviderCandidate>> {
    let provider = match provider_snapshot {
        Some(snapshot) => snapshot.clone(),
        None => crate::ai_runtime::mcp_runtime_registry::resolve_selected_web_search_provider(db)?,
    };
    Ok(vec![SearchProviderCandidate::Mcp(provider.id)])
}

async fn collect_planned_query_fetches(
    db: &Database,
    planned_queries: Vec<String>,
    max_search_results: usize,
    provider_snapshot: Option<
        &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary,
    >,
) -> Vec<Result<SearchProviderFetch, String>> {
    let futures = planned_queries.iter().map(|query| {
        collect_search_provider_fetches(db, query, max_search_results, provider_snapshot)
    });
    flatten_planned_query_fetch_results(join_all(futures).await)
}

fn flatten_planned_query_fetch_results(
    batches: Vec<Vec<Result<SearchProviderFetch, String>>>,
) -> Vec<Result<SearchProviderFetch, String>> {
    batches.into_iter().flatten().collect()
}

async fn collect_search_provider_fetches(
    db: &Database,
    query: &str,
    max_search_results: usize,
    provider_snapshot: Option<
        &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary,
    >,
) -> Vec<Result<SearchProviderFetch, String>> {
    let candidates = match search_provider_candidates(db, provider_snapshot) {
        Ok(candidates) => candidates,
        Err(error) => return vec![Err(error.to_string())],
    };
    let futures = candidates.into_iter().map(|candidate| async move {
        match candidate {
            SearchProviderCandidate::Mcp(provider_id) => {
                collect_mcp_search_provider_fetch(
                    db,
                    query,
                    &provider_id,
                    max_search_results,
                    provider_snapshot,
                )
                .await
            }
        }
        .map_err(|err| err.to_string())
    });
    join_all(futures).await
}
async fn collect_mcp_search_provider_fetch(
    db: &Database,
    query: &str,
    provider_id: &str,
    max_search_results: usize,
    expected_snapshot: Option<
        &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary,
    >,
) -> AppResult<SearchProviderFetch> {
    ensure_provider_circuit_allows(provider_id)?;
    let (provider, mapping_json) =
        resolve_mcp_provider_mapping(db, provider_id, "web.search", expected_snapshot)?;
    let probe_result = call_mcp_search_provider(
        db,
        &provider,
        &mapping_json,
        query,
        max_search_results,
        Duration::from_secs(10),
        true,
    )
    .await;
    let probe = match probe_result {
        Ok(probe) => {
            match probe.diagnostic.application_failure {
                None => record_provider_success(provider_id),
                Some(failure) if failure.is_transient() => record_provider_failure(provider_id),
                Some(_) => {}
            }
            probe
        }
        Err(error) => {
            if is_transient_provider_error(&error) {
                record_provider_failure(provider_id);
            }
            return Err(error);
        }
    };
    Ok(SearchProviderFetch::from_mcp_probe(probe))
}

/// Execute the MCP search smoke request used by settings diagnostics without
/// altering the provider health record used by Run routing.
pub(crate) async fn probe_mcp_search_provider_without_recording(
    db: &Database,
    provider_id: &str,
    query: &str,
    max_results: usize,
    request_timeout: Duration,
) -> AppResult<McpSearchProviderProbe> {
    let (provider, mapping_json) =
        resolve_mcp_provider_mapping(db, provider_id, "web.search", None)?;
    call_mcp_search_provider(
        db,
        &provider,
        &mapping_json,
        query,
        max_results,
        request_timeout,
        false,
    )
    .await
}

async fn call_mcp_search_provider(
    db: &Database,
    provider: &crate::ai_runtime::capability_resolver::ResolvedCapabilityProvider,
    mapping_json: &str,
    query: &str,
    max_results: usize,
    request_timeout: Duration,
    record_health: bool,
) -> AppResult<McpSearchProviderProbe> {
    let effective_mapping = effective_mcp_search_mapping(db, &provider.profile_id, mapping_json);
    let arguments = build_mcp_search_arguments(&effective_mapping, query, max_results);
    let argument_keys = arguments
        .as_object()
        .map(|object| {
            let mut keys = object.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            keys
        })
        .unwrap_or_default();
    let auth_header_present =
        if mcp_provider_transport_kind(db, &provider.profile_id)?.eq_ignore_ascii_case("https") {
            crate::ai_runtime::mcp_host_runtime::provider_http_auth_header_present(
                db,
                &provider.profile_id,
            )
            .map_err(sanitize_mcp_runtime_error)?
        } else {
            false
        };
    let started = Instant::now();
    let call_result = crate::ai_runtime::mcp_host_runtime::call_provider_tool(
        db,
        provider,
        arguments,
        crate::ai_runtime::mcp_host_runtime::McpHostRuntimeOptions {
            request_timeout,
            max_stdout_line_bytes: 64 * 1024,
            max_stderr_bytes: 4 * 1024,
            cwd: None,
            stdio_session_pool: true,
            stdio_session_idle_timeout:
                crate::ai_runtime::mcp_host_runtime::DEFAULT_STDIO_SESSION_IDLE_TIMEOUT,
        },
    )
    .await;
    let call = match call_result {
        Ok(call) => call,
        Err(error) => {
            let failure_code = mcp_runtime_failure_code(&error);
            let error = sanitize_mcp_runtime_error(error);
            observe_mcp_search_provider_call(
                db,
                &provider.profile_id,
                false,
                started.elapsed(),
                Some(failure_code),
                record_health,
            );
            return Err(error);
        }
    };
    let diagnostic = diagnose_mcp_search_result(&call.provider_id, &call.result);
    observe_mcp_search_provider_call(
        db,
        &provider.profile_id,
        diagnostic.application_failure.is_none(),
        started.elapsed(),
        diagnostic
            .application_failure
            .map(McpApplicationFailureKind::failure_code),
        record_health,
    );
    Ok(McpSearchProviderProbe {
        provider_id: call.provider_id.clone(),
        tool_name: provider.tool_name.clone(),
        argument_keys,
        auth_header_present,
        diagnostic,
    })
}

fn observe_mcp_search_provider_call(
    db: &Database,
    provider_id: &str,
    success: bool,
    elapsed: Duration,
    failure_code: Option<&str>,
    record_health: bool,
) {
    if !record_health {
        return;
    }
    let _ = crate::ai_runtime::mcp_runtime_registry::record_web_evidence_provider_call(
        db,
        provider_id,
        success,
        elapsed.as_millis() as u64,
        failure_code,
    );
}

fn mcp_provider_transport_kind(db: &Database, provider_id: &str) -> AppResult<String> {
    db.with_read_conn(|conn| {
        Ok(conn.query_row(
            "SELECT transport_kind FROM web_evidence_providers WHERE id = ?1 AND kind = 'mcp'",
            [provider_id],
            |row| row.get::<_, String>(0),
        )?)
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

fn is_transient_provider_error(error: &AppError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("timeout")
        || message.contains("deadline")
        || message.contains("connection reset")
        || message.contains("connection refused")
        || message.contains("connection aborted")
        || message.contains("broken pipe")
        || message.contains("temporarily unavailable")
        || message.contains("service unavailable")
        || message.contains("network unreachable")
        || message.contains("mcp_provider_transport_error")
}

/// Classify a host-runtime failure before its details cross the evidence boundary.
/// The returned identifier is safe to persist in provider health state and contains
/// neither provider output nor request material.
fn mcp_runtime_failure_code(error: &AppError) -> &'static str {
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("auth_failed") || message.contains("auth_missing") {
        "mcp_provider_authentication"
    } else if message.contains("output_too_large") || message.contains("output too large") {
        "mcp_provider_output_too_large"
    } else if message.contains("timeout")
        || message.contains("timed out")
        || message.contains("deadline")
    {
        "mcp_provider_timeout"
    } else if message.contains("connection reset")
        || message.contains("connection refused")
        || message.contains("connection aborted")
        || message.contains("broken pipe")
        || message.contains("temporarily unavailable")
        || message.contains("service unavailable")
        || message.contains("network unreachable")
        || message.contains("transport")
    {
        "mcp_provider_transport_error"
    } else {
        "mcp_provider_runtime_error"
    }
}

fn sanitize_mcp_runtime_error(error: AppError) -> AppError {
    match mcp_runtime_failure_code(&error) {
        "mcp_provider_authentication" => AppError::msg("agent_run_web_provider_auth_failed"),
        "mcp_provider_output_too_large" => AppError::msg("mcp_provider_output_too_large"),
        "mcp_provider_timeout" => AppError::msg("agent_run_web_provider_timeout"),
        "mcp_provider_transport_error" => AppError::msg("mcp_provider_transport_error"),
        _ => AppError::msg("mcp_provider_runtime_error"),
    }
}

fn resolve_mcp_provider_mapping(
    db: &Database,
    provider_id: &str,
    capability: &str,
    expected_snapshot: Option<
        &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary,
    >,
) -> AppResult<(
    crate::ai_runtime::capability_resolver::ResolvedCapabilityProvider,
    String,
)> {
    let providers =
        crate::ai_runtime::mcp_runtime_registry::list_enabled_web_provider_mappings(db)?;
    for provider in providers {
        if provider.id != provider_id || provider.kind != "mcp" {
            continue;
        }
        if let Some(expected) = expected_snapshot {
            if provider != *expected {
                return Err(AppError::msg("web_search_provider_snapshot_changed"));
            }
        }
        let Some(mapping_json) = (match capability {
            "web.search" => provider.web_search_mapping_json.as_deref(),
            "web.fetch" => provider.web_fetch_mapping_json.as_deref(),
            _ => None,
        }) else {
            break;
        };
        let Some(tool_name) = mapping_tool_name(mapping_json) else {
            break;
        };
        return Ok((
            crate::ai_runtime::capability_resolver::ResolvedCapabilityProvider {
                capability: capability.into(),
                provider_kind: "mcp".into(),
                profile_id: provider.id,
                tool_name,
                schema_hash: provider.provider_config_hash,
                requires_confirmation: true,
            },
            mapping_json.to_string(),
        ));
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

fn mapping_json_value(mapping_json: &str) -> serde_json::Value {
    serde_json::from_str(mapping_json).unwrap_or_else(|_| serde_json::json!({}))
}

/// Keep saved custom mappings immutable while making legacy AnySearch mappings safe at runtime.
/// Earlier Iris builds saved only `queryArg`, which lets AnySearch choose an unbounded default and
/// can exceed the MCP response cap before evidence normalization begins.
fn effective_mcp_search_mapping(db: &Database, provider_id: &str, mapping_json: &str) -> String {
    if !is_anysearch_provider(db, provider_id) {
        return mapping_json.to_string();
    }
    let mut mapping = mapping_json_value(mapping_json);
    let Some(object) = mapping.as_object_mut() else {
        return mapping_json.to_string();
    };
    let is_search = object
        .get("tool")
        .or_else(|| object.get("tool_name"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|tool| tool.trim().eq_ignore_ascii_case("search"));
    if !is_search || object.contains_key("maxResultsArg") {
        return mapping_json.to_string();
    }
    object.insert(
        "maxResultsArg".into(),
        serde_json::Value::String("max_results".into()),
    );
    serde_json::to_string(&mapping).unwrap_or_else(|_| mapping_json.to_string())
}

fn is_anysearch_provider(db: &Database, provider_id: &str) -> bool {
    let transport = db.with_read_conn(|conn| {
        conn.query_row(
            "SELECT transport_config_json FROM web_evidence_providers WHERE id = ?1 AND kind = 'mcp'",
            [provider_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(Into::into)
    });
    transport
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|value| {
            value
                .get("url")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .and_then(|url| reqwest::Url::parse(&url).ok())
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| host.eq_ignore_ascii_case("api.anysearch.com"))
}

fn mapping_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
}

fn merge_mapping_extra_args(
    target: &mut serde_json::Map<String, serde_json::Value>,
    mapping: &serde_json::Value,
) {
    if let Some(extra) = mapping.get("extraArgs").and_then(|item| item.as_object()) {
        for (key, value) in extra {
            target.insert(key.clone(), value.clone());
        }
    }
}

pub(crate) fn build_mcp_search_arguments(
    mapping_json: &str,
    query: &str,
    max_results: usize,
) -> serde_json::Value {
    let mapping = mapping_json_value(mapping_json);
    let explicit = mapping.get("queryArg").is_some()
        || mapping.get("maxResultsArg").is_some()
        || mapping.get("extraArgs").is_some();
    let mut args = serde_json::Map::new();
    let query_arg = mapping_string(&mapping, "queryArg").unwrap_or_else(|| "query".into());
    args.insert(query_arg, serde_json::Value::String(query.to_string()));
    if !explicit {
        args.insert("q".into(), serde_json::Value::String(query.to_string()));
    }
    if let Some(max_results_arg) = mapping_string(&mapping, "maxResultsArg") {
        args.insert(max_results_arg, serde_json::json!(max_results));
    }
    merge_mapping_extra_args(&mut args, &mapping);
    serde_json::Value::Object(args)
}

fn build_mcp_fetch_arguments(mapping_json: &str, url: &str, max_chars: usize) -> serde_json::Value {
    let mapping = mapping_json_value(mapping_json);
    let explicit = mapping.get("urlArg").is_some()
        || mapping.get("urlListArg").is_some()
        || mapping.get("maxCharsArg").is_some()
        || mapping.get("extraArgs").is_some();
    let mut args = serde_json::Map::new();
    if let Some(url_list_arg) = mapping_string(&mapping, "urlListArg") {
        args.insert(url_list_arg, serde_json::json!([url]));
    } else {
        let url_arg = mapping_string(&mapping, "urlArg").unwrap_or_else(|| "url".into());
        args.insert(url_arg, serde_json::Value::String(url.to_string()));
    }
    if let Some(max_chars_arg) = mapping_string(&mapping, "maxCharsArg") {
        args.insert(max_chars_arg, serde_json::json!(max_chars));
    } else if !explicit {
        args.insert("max_chars".into(), serde_json::json!(max_chars));
    }
    merge_mapping_extra_args(&mut args, &mapping);
    serde_json::Value::Object(args)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct McpSearchResultDiagnostic {
    pub body: String,
    pub result_shape: String,
    pub content_text_length: usize,
    pub contains_url_marker: bool,
    pub parsed_row_count: usize,
    /// Rows that pass the same HTTPS safety gate used before evidence registration.
    pub usable_https_row_count: usize,
    /// Parsed rows rejected solely because their URL is not HTTPS.
    pub rejected_non_https_row_count: usize,
    pub first_url_domain: Option<String>,
    pub failure_reason: Option<String>,
    pub application_failure: Option<McpApplicationFailureKind>,
}

/// Bounded classification for MCP application-level errors. Provider text is only inspected in
/// memory to select a safe code; it is not persisted, logged, or passed to the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum McpApplicationFailureKind {
    AuthFailed,
    RateLimited,
    QuotaExceeded,
    InvalidArguments,
    ProviderFailed,
}

impl McpApplicationFailureKind {
    const fn failure_code(self) -> &'static str {
        match self {
            Self::AuthFailed => "agent_run_web_provider_auth_failed",
            Self::RateLimited => "mcp_provider_rate_limited",
            Self::QuotaExceeded => "mcp_provider_quota_exhausted",
            Self::InvalidArguments => "mcp_provider_invalid_arguments",
            Self::ProviderFailed => "mcp_search_provider_error",
        }
    }

    const fn is_transient(self) -> bool {
        matches!(self, Self::RateLimited | Self::ProviderFailed)
    }
}

impl McpSearchResultDiagnostic {
    pub(crate) fn message(
        &self,
        provider_id: &str,
        tool_name: &str,
        argument_keys: &[String],
        auth_header_present: bool,
    ) -> String {
        let first_domain = self.first_url_domain.as_deref().unwrap_or("none");
        format!(
            "provider {provider_id}; tool {tool_name}; argument keys [{}]; auth header present: {auth_header_present}; result shape: {}; content text length: {}; URL marker: {}; parsed rows: {}; usable HTTPS rows: {}; rejected non-HTTPS rows: {}; first domain: {first_domain}",
            argument_keys.join(","),
            self.result_shape,
            self.content_text_length,
            self.contains_url_marker,
            self.parsed_row_count,
            self.usable_https_row_count,
            self.rejected_non_https_row_count,
        )
    }
}

pub(crate) fn diagnose_mcp_search_result(
    _provider_id: &str,
    result: &serde_json::Value,
) -> McpSearchResultDiagnostic {
    let body = mcp_search_result_body(result);
    let rows = parse_search_result_rows(&body);
    let content_text_length = mcp_result_content_text_length(result);
    let contains_url_marker = body.contains("http://")
        || body.contains("https://")
        || body.to_ascii_lowercase().contains("url");
    let result_shape = mcp_result_shape(result);
    let first_url_domain = rows.first().and_then(|row| domain_from_url(&row.url));
    let usable_https_row_count = rows.iter().filter(|row| is_https_url(&row.url)).count();
    let rejected_non_https_row_count = rows.len().saturating_sub(usable_https_row_count);
    let application_failure =
        mcp_result_is_error(result).then(|| classify_mcp_application_failure(&body));
    let failure_reason = if let Some(failure) = application_failure {
        Some(failure.failure_code().into())
    } else if rows.is_empty() && result.get("content").is_some() && content_text_length == 0 {
        Some("mcp_search_parse_empty:empty_body".into())
    } else if rows.is_empty() {
        Some(classify_mcp_search_parse_empty(&body))
    } else if usable_https_row_count == 0 {
        Some("mcp_search_no_usable_https_results".into())
    } else {
        None
    };
    McpSearchResultDiagnostic {
        body,
        result_shape,
        content_text_length,
        contains_url_marker,
        parsed_row_count: rows.len(),
        usable_https_row_count,
        rejected_non_https_row_count,
        first_url_domain,
        failure_reason,
        application_failure,
    }
}

fn classify_mcp_application_failure(body: &str) -> McpApplicationFailureKind {
    let bounded = body
        .chars()
        .take(2048)
        .collect::<String>()
        .to_ascii_lowercase();
    if bounded.contains("invalid_api_key")
        || bounded.contains("invalid api key")
        || bounded.contains("missing api key")
        || bounded.contains("api key required")
        || bounded.contains("unauthorized")
        || bounded.contains("forbidden")
        || bounded.contains("authentication")
    {
        McpApplicationFailureKind::AuthFailed
    } else if bounded.contains("rate_limit")
        || bounded.contains("rate limit")
        || bounded.contains("too many requests")
    {
        McpApplicationFailureKind::RateLimited
    } else if bounded.contains("quota")
        || bounded.contains("credit")
        || bounded.contains("insufficient balance")
    {
        McpApplicationFailureKind::QuotaExceeded
    } else if bounded.contains("invalid argument")
        || bounded.contains("invalid_arguments")
        || bounded.contains("query is required")
        || bounded.contains("validation")
    {
        McpApplicationFailureKind::InvalidArguments
    } else {
        McpApplicationFailureKind::ProviderFailed
    }
}

fn mcp_result_is_error(result: &serde_json::Value) -> bool {
    result.get("isError").and_then(|value| value.as_bool()) == Some(true)
}

fn mcp_result_content_text_length(result: &serde_json::Value) -> usize {
    result
        .get("content")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("text")
                        .or_else(|| item.get("content"))
                        .and_then(|value| value.as_str())
                })
                .map(|text| text.trim().chars().count())
                .sum()
        })
        .unwrap_or_else(|| {
            result
                .as_str()
                .map(|text| text.chars().count())
                .unwrap_or_default()
        })
}

fn mcp_result_shape(result: &serde_json::Value) -> String {
    if result.as_str().is_some() {
        return "string".into();
    }
    let Some(object) = result.as_object() else {
        return match result {
            serde_json::Value::Array(_) => "array".into(),
            serde_json::Value::Null => "null".into(),
            serde_json::Value::Bool(_) => "bool".into(),
            serde_json::Value::Number(_) => "number".into(),
            serde_json::Value::String(_) => "string".into(),
            serde_json::Value::Object(_) => "object".into(),
        };
    };
    let mut keys = object.keys().map(String::as_str).collect::<Vec<_>>();
    keys.sort_unstable();
    format!("object:{}", keys.join(","))
}

fn classify_mcp_search_parse_empty(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "mcp_search_parse_empty:empty_body".into();
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return "mcp_search_parse_empty:unrecognized_schema".into();
    }
    if !(trimmed.contains("http://")
        || trimmed.contains("https://")
        || trimmed.to_ascii_lowercase().contains("url"))
    {
        return "mcp_search_parse_empty:text_without_url".into();
    }
    "mcp_search_parse_empty:unrecognized_schema".into()
}

fn mcp_search_result_body(result: &serde_json::Value) -> String {
    if let Some(text) = result.as_str() {
        if let Some(body) = json_text_search_rows_body(text) {
            return body;
        }
        return text.to_string();
    }
    if let Some(body) = structured_search_rows_body(result) {
        return body;
    }
    if let Some(items) = result.get("content").and_then(|value| value.as_array()) {
        let mut body = String::new();
        for item in items {
            if let Some(text) = item
                .get("text")
                .or_else(|| item.get("content"))
                .and_then(|value| value.as_str())
            {
                if let Some(normalized) = json_text_search_rows_body(text) {
                    body.push_str(&normalized);
                } else {
                    body.push_str(text);
                    body.push('\n');
                }
            }
        }
        if !body.trim().is_empty() {
            return body;
        }
    }
    result.to_string()
}

fn json_text_search_rows_body(text: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|value| structured_search_rows_body(&value))
}

fn structured_search_rows_body(result: &serde_json::Value) -> Option<String> {
    if let Some(items) = search_result_array_candidates(result)
        .into_iter()
        .find(|items| search_rows_body_from_items(items).is_some())
    {
        return search_rows_body_from_items(items);
    }

    if let Some(object) = result.as_object() {
        for child in object.values() {
            if let Some(body) = structured_search_rows_body(child) {
                return Some(body);
            }
        }
    }
    None
}

fn search_result_array_candidates(value: &serde_json::Value) -> Vec<&Vec<serde_json::Value>> {
    let mut candidates = Vec::new();
    for key in ["results", "items", "data", "web", "news", "images"] {
        if let Some(items) = value.get(key).and_then(|item| item.as_array()) {
            candidates.push(items);
        }
    }
    candidates
}

fn search_rows_body_from_items(items: &[serde_json::Value]) -> Option<String> {
    let mut body = String::new();
    for (index, item) in items.iter().enumerate() {
        let Some(object) = item.as_object() else {
            continue;
        };
        let title = object
            .get("title")
            .or_else(|| object.get("name"))
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim();
        let url = object
            .get("url")
            .or_else(|| object.get("link"))
            .or_else(|| object.get("source"))
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim();
        if url.is_empty() {
            continue;
        }
        let snippet = object
            .get("snippet")
            .or_else(|| object.get("content"))
            .or_else(|| object.get("description"))
            .or_else(|| object.get("markdown"))
            .or_else(|| object.get("text"))
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim();
        body.push_str(&format!(
            "[{}] title: {}
url: {}
snippet: {}
",
            index + 1,
            title,
            url,
            snippet
        ));
    }
    (!body.trim().is_empty()).then_some(body)
}

fn web_evidence_items_from_search_fetch(fetch: &SearchProviderFetch) -> Vec<WebEvidenceItem> {
    let search_backend = fetch.search_backend;
    if let Some(failure_reason) = fetch.failure_reason.as_ref() {
        let reason = fetch
            .diagnostic_summary
            .as_ref()
            .map(|summary| format!("{failure_reason}; diagnostic: {summary}"))
            .unwrap_or_else(|| failure_reason.clone());
        return vec![failed_evidence_item_with_kind(
            "",
            &fetch.provider_id,
            &fetch.provider_kind,
            "web.search",
            reason,
        )];
    }
    let rows = parse_search_result_rows(&fetch.body);
    if rows.is_empty() && fetch.provider_kind == "mcp" {
        let failure_reason = classify_mcp_search_parse_empty(&fetch.body);
        let reason = fetch
            .diagnostic_summary
            .as_ref()
            .map(|summary| format!("{failure_reason}; diagnostic: {summary}"))
            .unwrap_or(failure_reason);
        return vec![failed_evidence_item_with_kind(
            "",
            &fetch.provider_id,
            &fetch.provider_kind,
            "web.search",
            reason,
        )];
    }
    rows.into_iter()
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

#[cfg(test)]
fn web_evidence_usage_from_search_fetches<'a>(
    fetches: impl IntoIterator<Item = &'a SearchProviderFetch>,
) -> WebEvidenceUsage {
    let mut usage = WebEvidenceUsage::default();
    for fetch in fetches {
        let items = web_evidence_items_from_search_fetch(fetch);
        record_successful_search_usage(&mut usage, fetch, &items);
    }
    usage
}

fn record_successful_search_usage(
    usage: &mut WebEvidenceUsage,
    fetch: &SearchProviderFetch,
    items: &[WebEvidenceItem],
) {
    let has_successful_search_result = items.iter().any(|item| {
        item.failure_reason.is_none()
            && item.retrieval_reason == "web.search"
            && item.provider_id == fetch.provider_id
            && item.provider_kind == fetch.provider_kind
    });
    if !has_successful_search_result {
        return;
    }
    if fetch.provider_kind == "mcp" {
        usage.successful_search_requests.mcp += 1;
    } else {
        return;
    }

    if let Some(provider) = usage
        .providers
        .iter_mut()
        .find(|provider| provider.provider_id == fetch.provider_id)
    {
        provider.successful_search_requests += 1;
        return;
    }

    usage.providers.push(WebEvidenceProviderUsage {
        provider_id: fetch.provider_id.clone(),
        provider_kind: fetch.provider_kind.clone(),
        successful_search_requests: 1,
    });
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
        let lower = trimmed.to_ascii_lowercase();
        if let Some(title) = markdown_search_heading_title(trimmed) {
            if let Some(row) = current.take().filter(|row| !row.url.trim().is_empty()) {
                rows.push(row);
            }
            current = Some(SearchResultRow {
                title,
                url: String::new(),
                snippet: String::new(),
            });
            continue;
        }
        if let Some((_, title)) = trimmed
            .split_once("] title:")
            .or_else(|| trimmed.split_once("] 鏍囬:"))
        {
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
            if lower.starts_with("url:") {
                row.url = trimmed[4..].trim().to_string();
            } else if lower.starts_with("snippet:") {
                row.snippet = trimmed[8..].trim().to_string();
            } else if let Some(url) = markdown_bold_field_value(trimmed, "url") {
                row.url = url;
            } else if let Some(snippet) = markdown_bold_field_value(trimmed, "snippet")
                .or_else(|| markdown_bold_field_value(trimmed, "description"))
            {
                row.snippet = snippet;
            } else if let Some(url) = trimmed.strip_prefix("閾炬帴:") {
                row.url = url.trim().to_string();
            } else if let Some(snippet) = trimmed.strip_prefix("鎽樿:") {
                row.snippet = snippet.trim().to_string();
            } else if row.url.starts_with("http") && row.snippet.is_empty() {
                let snippet = trimmed
                    .trim_start_matches('-')
                    .trim()
                    .trim_start_matches('*')
                    .trim()
                    .to_string();
                if !snippet.is_empty() {
                    row.snippet = snippet;
                }
            }
        }
    }
    if let Some(row) = current.take().filter(|row| !row.url.trim().is_empty()) {
        rows.push(row);
    }
    rows
}

fn markdown_search_heading_title(line: &str) -> Option<String> {
    let title = line
        .strip_prefix("### ")
        .or_else(|| line.strip_prefix("#### "))?
        .trim();
    let title = title
        .split_once(". ")
        .map(|(_, rest)| rest)
        .or_else(|| title.split_once(") ").map(|(_, rest)| rest))
        .unwrap_or(title)
        .trim();
    (!title.is_empty()).then(|| title.to_string())
}

fn markdown_bold_field_value(line: &str, field: &str) -> Option<String> {
    let line = line.trim_start_matches('-').trim();
    let rest = line.strip_prefix("**")?;
    let (label, value) = rest.split_once("**:")?;
    if !label.trim().eq_ignore_ascii_case(field) {
        return None;
    }
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn suppress_search_provider_failures_if_success(items: &mut Vec<WebEvidenceItem>) {
    let has_search_success = items
        .iter()
        .any(|item| item.retrieval_reason == "web.search" && item.failure_reason.is_none());
    if has_search_success {
        items.retain(|item| {
            item.failure_reason.is_none()
                || !matches!(item.retrieval_reason.as_str(), "search" | "web.search")
                || !item.url.trim().is_empty()
        });
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

fn fetch_provider_candidates(
    db: &Database,
    provider_snapshot: Option<
        &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary,
    >,
) -> Vec<FetchProviderCandidate> {
    if let Some(snapshot) = provider_snapshot {
        return snapshot
            .web_fetch_mapping_json
            .as_ref()
            .map(|_| vec![FetchProviderCandidate::Mcp(snapshot.id.clone())])
            .unwrap_or_default();
    }
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
    provider_snapshot: Option<
        &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary,
    >,
) -> AppResult<Vec<WebEvidenceItem>> {
    if max_fetches == 0 {
        return Ok(items);
    }

    let mut enriched = Vec::with_capacity(items.len());
    let mut fetched = 0usize;
    let fetch_deadline = Instant::now() + WEB_FETCH_TURN_BUDGET;
    for mut item in items {
        if fetched < max_fetches && item.failure_reason.is_none() {
            if Instant::now() >= fetch_deadline {
                enriched.push(item);
                continue;
            }
            fetched += 1;
            let remaining = fetch_deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(
                remaining,
                fetch_url_with_providers(db, &item.url, provider_snapshot),
            )
            .await
            {
                Ok(Ok(page)) => apply_page_provider_fetch(&mut item, page),
                Ok(Err(_)) | Err(_) => {
                    // The search snippet remains usable low-grade evidence even when
                    // optional deep reading fails or exhausts the shared fetch budget.
                }
            }
        }
        enriched.push(item);
    }
    Ok(enriched)
}

async fn fetch_url_with_providers(
    db: &Database,
    url: &str,
    provider_snapshot: Option<
        &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary,
    >,
) -> AppResult<PageProviderFetch> {
    let futures = fetch_provider_candidates(db, provider_snapshot)
        .into_iter()
        .map(|candidate| async move {
            match candidate {
                FetchProviderCandidate::Mcp(provider_id) => {
                    collect_mcp_page_fetch(db, url, &provider_id, provider_snapshot).await
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

    let merged_text = texts.join("\n\n---\n\n");
    PageProviderFetch {
        title: titles
            .into_iter()
            .find(|title| !title.trim().is_empty())
            .unwrap_or_else(|| url.to_string()),
        text: truncate_web_context_text(&merged_text, MERGED_FETCH_MAX_CHARS),
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
    expected_snapshot: Option<
        &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary,
    >,
) -> AppResult<PageProviderFetch> {
    crate::llm::fetch_web_page::validate_fetch_url(url)?;
    ensure_provider_circuit_allows(provider_id)?;
    let (provider, mapping_json) =
        resolve_mcp_provider_mapping(db, provider_id, "web.fetch", expected_snapshot)?;
    let started = Instant::now();
    let call_result = crate::ai_runtime::mcp_host_runtime::call_provider_tool(
        db,
        &provider,
        build_mcp_fetch_arguments(&mapping_json, url, FETCH_EXCERPT_MAX_CHARS),
        crate::ai_runtime::mcp_host_runtime::McpHostRuntimeOptions {
            request_timeout: std::time::Duration::from_secs(20),
            max_stdout_line_bytes: 128 * 1024,
            max_stderr_bytes: 4 * 1024,
            cwd: None,
            stdio_session_pool: true,
            stdio_session_idle_timeout:
                crate::ai_runtime::mcp_host_runtime::DEFAULT_STDIO_SESSION_IDLE_TIMEOUT,
        },
    )
    .await;
    let call = match call_result {
        Ok(call) => call,
        Err(error) => {
            let failure_code = mcp_runtime_failure_code(&error);
            let error = sanitize_mcp_runtime_error(error);
            let _ = crate::ai_runtime::mcp_runtime_registry::record_web_evidence_provider_call(
                db,
                provider_id,
                false,
                started.elapsed().as_millis() as u64,
                Some(failure_code),
            );
            if is_transient_provider_error(&error) {
                record_provider_failure(provider_id);
            }
            return Err(error);
        }
    };
    let failure_kind = mcp_result_is_error(&call.result)
        .then(|| classify_mcp_application_failure(&mcp_search_result_body(&call.result)));
    let result = mcp_page_fetch_result(provider_id, url, &call.result);
    let _ = crate::ai_runtime::mcp_runtime_registry::record_web_evidence_provider_call(
        db,
        provider_id,
        result.is_ok(),
        started.elapsed().as_millis() as u64,
        failure_kind.map(McpApplicationFailureKind::failure_code),
    );
    match failure_kind {
        None => record_provider_success(provider_id),
        Some(kind) if kind.is_transient() => record_provider_failure(provider_id),
        Some(_) => {}
    }
    result
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
) -> AppResult<PageProviderFetch> {
    if mcp_result_is_error(result) {
        return Err(AppError::msg(
            classify_mcp_application_failure(&mcp_search_result_body(result)).failure_code(),
        ));
    }
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
    Ok(PageProviderFetch {
        title,
        text,
        provider_id: provider_id.into(),
        provider_kind: "mcp".into(),
        extraction_method: "mcp_fetch".into(),
    })
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
        search_backend: WebSearchBackend::Provider,
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
    failed_evidence_item_with_kind(url, provider_id, "native", retrieval_reason, reason)
}

fn failed_evidence_item_with_kind(
    url: impl Into<String>,
    provider_id: impl Into<String>,
    provider_kind: impl Into<String>,
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
        provider_kind: provider_kind.into(),
        cost_class: "free".into(),
        raw_result_hash,
        extraction_method: "none".into(),
        trust_level: "external_untrusted".into(),
        retrieval_reason: retrieval_reason.into(),
        search_backend: WebSearchBackend::Provider,
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

    #[test]
    fn initial_run_search_uses_the_single_original_query_without_replanning() {
        assert_eq!(
            initial_run_search_queries("最近世界杯战况如何？"),
            vec!["最近世界杯战况如何？".to_string()]
        );
    }

    fn item(url: &str) -> WebEvidenceItem {
        WebEvidenceItem {
            url: url.into(),
            canonical_url: canonicalize_url(url),
            title: "Title".into(),
            domain: domain_from_url(url).unwrap_or_default(),
            snippet: "Snippet".into(),
            fetched_excerpt: None,
            provider_id: "mcp.test".into(),
            provider_kind: "mcp".into(),
            cost_class: "free".into(),
            raw_result_hash: result_hash(&[url]),
            extraction_method: "search_snippet".into(),
            trust_level: "external_untrusted".into(),
            retrieval_reason: "web.search".into(),
            search_backend: WebSearchBackend::Provider,
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
                provider_snapshot: None,
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

    #[test]
    fn web_usage_counts_only_successful_mcp_search_providers() {
        let mcp_fetch = SearchProviderFetch {
            body: "[1] title: MCP result\nurl: https://example.com/mcp\nsnippet: ok".into(),
            search_backend: WebSearchBackend::Provider,
            provider_id: "anysearch".into(),
            provider_kind: "mcp".into(),
            failure_reason: None,
            diagnostic_summary: None,
        };
        let empty_mcp_fetch = SearchProviderFetch {
            body: "no parseable rows".into(),
            search_backend: WebSearchBackend::Provider,
            provider_id: "empty-mcp".into(),
            provider_kind: "mcp".into(),
            failure_reason: None,
            diagnostic_summary: None,
        };

        let usage = web_evidence_usage_from_search_fetches([&mcp_fetch, &empty_mcp_fetch]);

        assert_eq!(usage.successful_search_requests.mcp, 1);
        assert_eq!(usage.providers.len(), 1);
        assert!(usage.providers.iter().any(|provider| {
            provider.provider_id == "anysearch"
                && provider.provider_kind == "mcp"
                && provider.successful_search_requests == 1
        }));
    }

    #[test]
    fn planned_query_fetch_flatten_preserves_query_order() {
        let first = SearchProviderFetch {
            body: "[1] title: first\nurl: https://example.com/first\nsnippet: first".into(),
            search_backend: WebSearchBackend::Provider,
            provider_id: "provider-a".into(),
            provider_kind: "mcp".into(),
            failure_reason: None,
            diagnostic_summary: None,
        };
        let second = SearchProviderFetch {
            body: "[1] title: second\nurl: https://example.com/second\nsnippet: second".into(),
            search_backend: WebSearchBackend::Provider,
            provider_id: "provider-b".into(),
            provider_kind: "mcp".into(),
            failure_reason: None,
            diagnostic_summary: None,
        };

        let flattened = flatten_planned_query_fetch_results(vec![
            vec![Ok(first)],
            vec![Err("query-two-failed".into()), Ok(second)],
        ]);

        assert_eq!(flattened.len(), 3);
        assert_eq!(flattened[0].as_ref().unwrap().provider_id, "provider-a");
        assert_eq!(flattened[1].as_ref().unwrap_err(), "query-two-failed");
        assert_eq!(flattened[2].as_ref().unwrap().provider_id, "provider-b");
    }

    #[tokio::test]
    async fn broker_keeps_search_snippet_usable_when_page_fetch_fails() {
        let db = Database::open_in_memory().unwrap();
        let items = enrich_with_page_fetches(
            &db,
            vec![item("https://localhost/a"), item("https://localhost/b")],
            1,
            None,
        )
        .await
        .unwrap();

        assert_eq!(items.len(), 2);
        assert!(items[0].failure_reason.is_none());
        assert!(!items[0].snippet.is_empty());
        assert!(items[1].failure_reason.is_none());
    }

    #[test]
    fn broker_applies_successful_page_fetch_excerpt() {
        let mut item = item("https://example.com/a");

        apply_page_fetch(
            &mut item,
            PageFetchResult {
                title: "Fetched title".into(),
                text: "Fetched body".into(),
            },
        );

        assert_eq!(item.fetched_excerpt.as_deref(), Some("Fetched body"));
    }

    #[test]
    fn web_packet_excerpt_is_capped_for_large_fetched_pages() {
        let mut item = item("https://example.com/apple-watch");
        item.fetched_excerpt = Some("苹".repeat(87_000));

        let packets = web_evidence_items_to_packets("apple最新的手表是什么？", &[item]);

        assert_eq!(packets.len(), 1);
        assert!(packets[0].excerpt.chars().count() <= 4_000);
        assert!(packets[0].excerpt.contains("网页正文已按上下文预算截断"));
    }

    #[test]
    fn apple_latest_prefetch_packet_stays_under_fast_model_budget() {
        let mut item = item("https://www.apple.com/tw/apple-watch-series-11/");
        item.fetched_excerpt = Some("苹".repeat(87_521));

        let packets = web_evidence_items_to_packets("apple最新的手表是什么？", &[item]);
        let prompt =
            crate::ai_runtime::model_gateway::ModelGateway::format_evidence_packets(&packets);
        let estimated = crate::ai_runtime::text_support::estimate_tokens(&prompt);

        assert!(estimated < 55_808);
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
    fn search_provider_candidates_require_mcp_provider() {
        let db = Database::open_in_memory().unwrap();

        let err = search_provider_candidates(&db, None).unwrap_err();

        assert!(err.to_string().contains("web_search_provider_missing"));
    }

    #[test]
    fn diagnostic_search_smoke_observation_does_not_persist_provider_health() {
        let db = Database::open_in_memory().unwrap();
        crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
                id: "diagnostic-provider".into(),
                name: "Diagnostic provider".into(),
                kind: "mcp".into(),
                enabled: true,
                transport_kind: "stdio".into(),
                transport_config_json: r#"{"command":"mcp-server"}"#.into(),
                credential_refs_json: "{}".into(),
                web_search_mapping_json: Some(r#"{"tool":"search"}"#.into()),
                web_fetch_mapping_json: None,
            },
        )
        .unwrap();

        super::observe_mcp_search_provider_call(
            &db,
            "diagnostic-provider",
            true,
            Duration::from_millis(8),
            None,
            false,
        );

        assert!(
            crate::ai_runtime::mcp_runtime_registry::web_evidence_provider_runtime(
                &db,
                "diagnostic-provider"
            )
            .unwrap()
            .is_none()
        );
        assert!(
            crate::ai_runtime::mcp_runtime_registry::web_evidence_provider_health(
                &db,
                "diagnostic-provider"
            )
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn search_provider_candidates_use_selected_mcp_only() {
        let db = Database::open_in_memory().unwrap();
        crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
                id: "anysearch".into(),
                name: "AnySearch".into(),
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
        crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
                id: "brave".into(),
                name: "Brave Search".into(),
                kind: "mcp".into(),
                enabled: true,
                transport_kind: "stdio".into(),
                transport_config_json: "{}".into(),
                credential_refs_json: "{}".into(),
                web_search_mapping_json: Some(r#"{"tool":"brave_web_search"}"#.into()),
                web_fetch_mapping_json: None,
            },
        )
        .unwrap();
        crate::ai_runtime::mcp_runtime_registry::save_selected_web_search_provider_id(
            &db,
            Some("brave"),
        )
        .unwrap();

        let candidates = search_provider_candidates(&db, None).unwrap();

        assert_eq!(
            candidates,
            vec![SearchProviderCandidate::Mcp("brave".into())]
        );
    }

    #[test]
    fn search_provider_candidates_use_single_mcp_without_saved_choice() {
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

        let candidates = search_provider_candidates(&db, None).unwrap();

        assert_eq!(
            candidates,
            vec![SearchProviderCandidate::Mcp("mcp-search".into())]
        );
    }

    #[test]
    fn frozen_search_provider_snapshot_fails_closed_after_mapping_changes() {
        let db = Database::open_in_memory().unwrap();
        let provider = crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
            id: "frozen-search".into(),
            name: "Frozen Search".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json: "{}".into(),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: Some(r#"{"tool":"search_v1"}"#.into()),
            web_fetch_mapping_json: None,
        };
        crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(&db, &provider)
            .unwrap();
        let snapshot =
            crate::ai_runtime::mcp_runtime_registry::resolve_selected_web_search_provider(&db)
                .unwrap();

        let mut changed = provider;
        changed.web_search_mapping_json = Some(r#"{"tool":"search_v2"}"#.into());
        crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(&db, &changed)
            .unwrap();

        let error =
            resolve_mcp_provider_mapping(&db, "frozen-search", "web.search", Some(&snapshot))
                .unwrap_err();
        assert_eq!(error.to_string(), "web_search_provider_snapshot_changed");
    }

    #[test]
    fn mcp_mapping_builds_provider_specific_search_and_fetch_args() {
        let search = build_mcp_search_arguments(
            r#"{"tool":"tavily-search","queryArg":"query","maxResultsArg":"max_results","extraArgs":{"topic":"general"}}"#,
            "rust mcp",
            7,
        );
        assert_eq!(
            search,
            serde_json::json!({"query": "rust mcp", "max_results": 7, "topic": "general"})
        );

        let initial_search = build_mcp_search_arguments(
            r#"{"tool":"tavily-search","queryArg":"query","maxResultsArg":"max_results"}"#,
            "current news",
            5,
        );
        assert!(
            initial_search["max_results"]
                .as_u64()
                .is_some_and(|limit| limit <= 5),
            "the initial evidence request must preserve its bounded result limit"
        );

        let fetch = build_mcp_fetch_arguments(
            r#"{"tool":"tavily-extract","urlListArg":"urls","extraArgs":{"extract_depth":"basic"}}"#,
            "https://example.com/a",
            12000,
        );
        assert_eq!(
            fetch,
            serde_json::json!({"urls": ["https://example.com/a"], "extract_depth": "basic"})
        );
    }

    #[test]
    fn legacy_anysearch_mapping_gets_a_runtime_result_limit_without_mutation() {
        let db = Database::open_in_memory().unwrap();
        let legacy_mapping = r#"{"tool":"search","queryArg":"query"}"#;
        crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
                id: "anysearch-legacy".into(),
                name: "AnySearch".into(),
                kind: "mcp".into(),
                enabled: true,
                transport_kind: "https".into(),
                transport_config_json:
                    r#"{"url":"https://api.anysearch.com/mcp","allow_localhost_dev":false}"#.into(),
                credential_refs_json: "{}".into(),
                web_search_mapping_json: Some(legacy_mapping.into()),
                web_fetch_mapping_json: Some(r#"{"tool":"extract","urlArg":"url"}"#.into()),
            },
        )
        .unwrap();

        let effective = effective_mcp_search_mapping(&db, "anysearch-legacy", legacy_mapping);
        let arguments = build_mcp_search_arguments(&effective, "latest news", 5);

        assert_eq!(arguments["query"], "latest news");
        assert_eq!(arguments["max_results"], 5);
        assert_eq!(
            crate::ai_runtime::mcp_runtime_registry::list_web_evidence_providers(&db).unwrap()[0]
                .web_search_mapping_json
                .as_deref(),
            Some(legacy_mapping)
        );
    }

    #[test]
    fn mcp_search_body_normalizes_structured_result_arrays() {
        let body = mcp_search_result_body(&serde_json::json!({
            "results": [
                {
                    "title": "Iris MCP",
                    "url": "https://example.com/iris",
                    "content": "evidence snippet"
                }
            ]
        }));
        let rows = parse_search_result_rows(&body);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Iris MCP");
        assert_eq!(rows[0].url, "https://example.com/iris");
        assert_eq!(rows[0].snippet, "evidence snippet");
    }

    #[test]
    fn mcp_search_body_normalizes_anysearch_markdown_content_text() {
        let body = mcp_search_result_body(&serde_json::json!({
            "content": [
                {
                    "type": "text",
                    "text": "## Search Results\n\n### 1. Iris note app\n- **URL**: https://example.com/iris\n- Iris is a local-first note app.\n\n### 2. Iris docs\n- **URL**: https://docs.example.com/iris\n- Documentation snippet."
                }
            ]
        }));
        let rows = parse_search_result_rows(&body);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title, "Iris note app");
        assert_eq!(rows[0].url, "https://example.com/iris");
        assert_eq!(rows[0].snippet, "Iris is a local-first note app.");
        assert_eq!(rows[1].title, "Iris docs");
        assert_eq!(rows[1].url, "https://docs.example.com/iris");
    }

    #[test]
    fn mcp_search_body_normalizes_live_anysearch_multilingual_markdown() {
        let body = mcp_search_result_body(&serde_json::json!({
            "content": [
                {
                    "type": "text",
                    "text": "## Search Results (10 results, 4358ms)\n\n### 1. 高市総理がインドのモディ首相と会談へ　重要鉱物や半導体で連携確認\n- **URL**: https://www.fnn.jp/articles/-/1069028\n- 高市総理がインドのモディ首相と会談へ 重要鉱物や半導体で連携確認 対中国見据え協力深化へ協議...\n\n### 2. 中國正打擊日本的痛處，但高市早苗會屈服嗎？ - BBC News 中文\n- **URL**: https://www.bbc.com/zhongwen/articles/c178qrr29d1o/trad\n- 從日本首相高市早苗發表導致日中關係跌至多年來最低點的言論以來，北京方面一直在以各種方式加大對日本的施壓。"
                }
            ]
        }));
        let rows = parse_search_result_rows(&body);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].url, "https://www.fnn.jp/articles/-/1069028");
        assert_eq!(
            rows[1].url,
            "https://www.bbc.com/zhongwen/articles/c178qrr29d1o/trad"
        );
        assert!(rows[1].snippet.contains("北京方面"));
    }

    #[test]
    fn mcp_search_body_normalizes_firecrawl_text_json_results() {
        let body = mcp_search_result_body(&serde_json::json!({
            "content": [
                {
                    "type": "text",
                    "text": r#"{
                        "success": true,
                        "data": {
                            "web": [
                                {
                                    "url": "https://example.com/firecrawl",
                                    "title": "Firecrawl result",
                                    "description": "Firecrawl description"
                                }
                            ]
                        }
                    }"#
                }
            ]
        }));
        let rows = parse_search_result_rows(&body);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Firecrawl result");
        assert_eq!(rows[0].url, "https://example.com/firecrawl");
        assert_eq!(rows[0].snippet, "Firecrawl description");
    }

    #[test]
    fn mcp_search_parse_empty_returns_diagnostic_failure_item() {
        let items = web_evidence_items_from_search_fetch(&SearchProviderFetch {
            body: "MCP tool returned prose without links".into(),
            search_backend: WebSearchBackend::Provider,
            provider_id: "mcp.prose".into(),
            provider_kind: "mcp".into(),
            failure_reason: None,
            diagnostic_summary: None,
        });

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].provider_id, "mcp.prose");
        assert_eq!(
            items[0].failure_reason.as_deref(),
            Some("mcp_search_parse_empty:text_without_url")
        );
    }

    #[test]
    fn mcp_search_parse_empty_diagnostic_classifies_empty_and_schema_failures() {
        let empty = diagnose_mcp_search_result(
            "anysearch",
            &serde_json::json!({
                "content": [
                    {
                        "type": "text",
                        "text": "   "
                    }
                ]
            }),
        );
        assert_eq!(
            empty.failure_reason.as_deref(),
            Some("mcp_search_parse_empty:empty_body")
        );

        let schema = diagnose_mcp_search_result(
            "anysearch",
            &serde_json::json!({
                "content": [
                    {
                        "type": "text",
                        "text": "{\"items\":[{\"title\":\"No URL\"}]}"
                    }
                ]
            }),
        );
        assert_eq!(
            schema.failure_reason.as_deref(),
            Some("mcp_search_parse_empty:unrecognized_schema")
        );
    }

    #[test]
    fn mcp_search_diagnostic_rejects_http_only_rows_as_unusable_evidence() {
        let diagnostic = diagnose_mcp_search_result(
            "anysearch",
            &serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": "### 1. Insecure result\n- **URL**: http://example.com/news\n- **Snippet**: must not become evidence"
                }]
            }),
        );

        assert_eq!(diagnostic.parsed_row_count, 1);
        assert_eq!(diagnostic.usable_https_row_count, 0);
        assert_eq!(diagnostic.rejected_non_https_row_count, 1);
        assert_eq!(
            diagnostic.failure_reason.as_deref(),
            Some("mcp_search_no_usable_https_results")
        );
    }

    #[test]
    fn mcp_search_is_error_result_is_not_reported_as_parse_empty() {
        let diagnostic = diagnose_mcp_search_result(
            "anysearch",
            &serde_json::json!({
                "content": [
                    {
                        "type": "text",
                        "text": "invalid_api_key\nInvalid API key."
                    }
                ],
                "isError": true
            }),
        );
        assert_eq!(diagnostic.content_text_length, 32);
        assert_eq!(
            diagnostic.application_failure,
            Some(McpApplicationFailureKind::AuthFailed)
        );
        let fetch = SearchProviderFetch {
            body: diagnostic.body,
            search_backend: WebSearchBackend::Provider,
            provider_id: "anysearch".into(),
            provider_kind: "mcp".into(),
            failure_reason: diagnostic.failure_reason,
            diagnostic_summary: None,
        };
        let items = web_evidence_items_from_search_fetch(&fetch);

        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].failure_reason.as_deref(),
            Some("agent_run_web_provider_auth_failed")
        );
        assert!(!items[0].snippet.contains("Invalid API key"));
    }
    #[test]
    fn mcp_success_suppresses_mcp_search_failure_item() {
        let mut items = web_evidence_items_from_search_fetch(&SearchProviderFetch {
            body: "[1] title: MCP result\nurl: https://example.com/mcp\nsnippet: ok".into(),
            search_backend: WebSearchBackend::Provider,
            provider_id: "anysearch".into(),
            provider_kind: "mcp".into(),
            failure_reason: None,
            diagnostic_summary: None,
        });
        items.extend(web_evidence_items_from_search_fetch(&SearchProviderFetch {
            body: "unparseable mcp body".into(),
            search_backend: WebSearchBackend::Provider,
            provider_id: "empty-mcp".into(),
            provider_kind: "mcp".into(),
            failure_reason: None,
            diagnostic_summary: None,
        }));

        suppress_search_provider_failures_if_success(&mut items);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].provider_id, "anysearch");
        assert!(items[0].failure_reason.is_none());
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
    fn credential_and_mapping_errors_do_not_participate_in_provider_circuit_breaking() {
        assert_eq!(
            sanitize_mcp_runtime_error(AppError::msg(
                "auth_failed: bearer credential must contain the raw key only",
            ))
            .to_string(),
            "agent_run_web_provider_auth_failed"
        );
        assert_eq!(
            sanitize_mcp_runtime_error(AppError::msg("auth_missing: credential_unreadable",))
                .to_string(),
            "agent_run_web_provider_auth_failed"
        );
        assert!(!is_transient_provider_error(&AppError::msg(
            "agent_run_web_provider_auth_failed",
        )));
        assert!(!is_transient_provider_error(&AppError::msg(
            "auth_missing: credential_unreadable",
        )));
        assert!(is_transient_provider_error(&AppError::msg(
            "connection reset by peer",
        )));
    }

    #[test]
    fn host_runtime_failures_are_reduced_to_safe_distinct_health_codes() {
        assert_eq!(
            mcp_runtime_failure_code(&AppError::msg("output_too_large: response exceeded cap")),
            "mcp_provider_output_too_large"
        );
        assert_eq!(
            sanitize_mcp_runtime_error(AppError::msg("output_too_large: private provider body"))
                .to_string(),
            "mcp_provider_output_too_large"
        );
        assert_eq!(
            mcp_runtime_failure_code(&AppError::msg("connection reset by peer")),
            "mcp_provider_transport_error"
        );
        assert!(is_transient_provider_error(&AppError::msg(
            "mcp_provider_transport_error",
        )));
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

        let candidates = fetch_provider_candidates(&db, None);

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
        )
        .unwrap();

        assert_eq!(fetch.provider_id, "mcp-fetch");
        assert_eq!(fetch.provider_kind, "mcp");
        assert_eq!(fetch.title, "Fetched title");
        assert_eq!(fetch.text, "Fetched body");
        assert_eq!(fetch.extraction_method, "mcp_fetch");
    }

    #[test]
    fn mcp_page_fetch_rejects_application_error_without_exposing_error_body() {
        let error = mcp_page_fetch_result(
            "mcp-fetch",
            "https://example.com/a",
            &serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": "invalid_api_key\nInvalid API key."
                }],
                "isError": true
            }),
        )
        .unwrap_err()
        .to_string();

        assert_eq!(error, "agent_run_web_provider_auth_failed");
        assert!(!error.contains("Invalid API key"));
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
    fn merged_fetch_provider_text_has_total_cap() {
        let merged = merge_page_provider_fetches(
            "https://example.com/a",
            vec![
                PageProviderFetch {
                    title: "Native title".into(),
                    text: "苹".repeat(12_000),
                    provider_id: "native.fetch".into(),
                    provider_kind: "native".into(),
                    extraction_method: "native_readability".into(),
                },
                PageProviderFetch {
                    title: "MCP title".into(),
                    text: "表".repeat(12_000),
                    provider_id: "mcp-fetch".into(),
                    provider_kind: "mcp".into(),
                    extraction_method: "mcp_fetch".into(),
                },
            ],
        );

        assert!(merged.text.chars().count() <= 12_000);
        assert!(merged.text.contains("网页正文已按上下文预算截断"));
        assert_eq!(merged.provider_kind, "mixed");
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
