//! K12 native search subrequest constructor and fixture credential parser.
//!
//! This module constructs an isolated native-search payload and maps protocol
//! fixtures onto K11 route-attempt fields. Production adapters are registered
//! per model in `native_search_adapter`. The production subrequest stays
//! `stream:false` and does not travel through `ModelGateway`. Main-dialogue SSE
//! may still carry leaked search events; those become `MainStreamLeak`
//! observations and never mark the K11 native route succeeded.

use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::json;
use serde_json::Value;

use crate::ai_runtime::dual_path_search::{
    NativeSearchSupport, RouteAttemptOutcome, RouteFailureClass, SearchActionIdentity, SearchHit,
};
use crate::ai_types::EndpointFamily;

/// Frozen model/endpoint identity used by the C10 native-search probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSearchEndpointRef {
    pub model_id: String,
    pub endpoint_family: EndpointFamily,
    /// Chat API base of the selected candidate, if known. Native search may
    /// derive a sibling path from it; it is not sent as vendor JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_base: Option<String>,
    /// Encrypted-store service for the selected candidate, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_service: Option<String>,
}

impl NativeSearchEndpointRef {
    pub(crate) fn new(model_id: impl Into<String>, endpoint_family: EndpointFamily) -> Self {
        Self {
            model_id: model_id.into(),
            endpoint_family,
            api_base: None,
            credential_service: None,
        }
    }
}

/// Public retrieval scope that may travel with the approved query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSearchPublicScope {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// Request identity that must stay attached to results and billing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSearchRequestIdentity {
    pub run_id: String,
    pub input_revision: String,
    pub parent_call_id: String,
    #[serde(default)]
    pub child_run_id: Option<String>,
    #[serde(default)]
    pub model_turn: u32,
    #[serde(default)]
    pub tool_surface_version: String,
    pub attempt: u32,
}

/// Where retrieval credentials were observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RetrievalOrigin {
    #[default]
    IsolatedSubrequest,
    MainStreamLeak,
}

/// Protocol-family-agnostic retrieval observation for C11/C12/C26.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RetrievalObservation {
    pub origin: RetrievalOrigin,
    pub identity: SearchActionIdentity,
    pub has_retrieval_credentials: bool,
    pub candidates: Vec<SearchHit>,
    pub event_kinds: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u32>,
}

/// Draft accepted by the constructor. Private material is never forwarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeSearchSubrequestDraft {
    pub query: String,
    pub public_scope: NativeSearchPublicScope,
    pub identity: NativeSearchRequestIdentity,
    pub endpoint: NativeSearchEndpointRef,
    pub private_material: Option<String>,
}

/// Why constructing a native subrequest failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NativeSearchConstructError {
    EmptyQuery,
}

/// K06 claim class for a native auxiliary model request. Not a Network tool dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NativeSearchBudgetKind {
    ModelAuxiliaryRequest,
}

/// Budget claim that must later increment the same model-turn ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSearchBudgetClaim {
    pub kind: NativeSearchBudgetKind,
    pub parent_call_id: String,
}

impl NativeSearchBudgetClaim {
    #[cfg(test)]
    pub(crate) fn is_network_tool_dispatch(&self) -> bool {
        false
    }
}

/// Constructed native search subrequest. Outbound payload contains only approved fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSearchSubrequest {
    pub query: String,
    pub public_scope: NativeSearchPublicScope,
    pub identity: NativeSearchRequestIdentity,
    pub endpoint: NativeSearchEndpointRef,
    pub budget_claim: NativeSearchBudgetClaim,
}

/// Mechanical fixture family. Shape samples, not V05 endpoint acceptance.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeSearchPayloadFamily {
    OpenAiShaped,
    GeminiShaped,
    AnthropicShaped,
}

/// Parser output mapped onto K11 `RouteAttemptOutcome` fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeSearchParse {
    pub candidates: Vec<SearchHit>,
    pub has_retrieval_credentials: bool,
    pub generated_text_only: bool,
    pub failure: Option<RouteFailureClass>,
}

/// Stable C10 reason when native search is unsupported. Not "能力降级".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NativeSearchUnsupportedReason {
    AdapterAbsent,
    CapabilityAbsent,
}

/// Construct a native search subrequest that never forwards private notes.
pub(crate) fn construct_native_search_subrequest(
    draft: NativeSearchSubrequestDraft,
) -> Result<NativeSearchSubrequest, NativeSearchConstructError> {
    let query = draft.query.trim();
    if query.is_empty() {
        return Err(NativeSearchConstructError::EmptyQuery);
    }
    let _private_material = draft.private_material;
    Ok(NativeSearchSubrequest {
        query: query.to_string(),
        public_scope: draft.public_scope,
        identity: draft.identity.clone(),
        endpoint: draft.endpoint,
        budget_claim: NativeSearchBudgetClaim {
            kind: NativeSearchBudgetKind::ModelAuxiliaryRequest,
            parent_call_id: draft.identity.parent_call_id,
        },
    })
}

/// Parse a mechanical native-search fixture. Not a live V05 verdict.
pub(crate) fn parse_native_search_payload(
    family: NativeSearchPayloadFamily,
    value: &Value,
) -> NativeSearchParse {
    let (structured, candidates) = match family {
        NativeSearchPayloadFamily::OpenAiShaped => openai_search_hits(value),
        NativeSearchPayloadFamily::GeminiShaped => (
            has_gemini_structured_search(value),
            collect_structured_https_hits(value),
        ),
        NativeSearchPayloadFamily::AnthropicShaped => anthropic_search_hits(value),
    };
    if !structured {
        return NativeSearchParse {
            candidates: Vec::new(),
            has_retrieval_credentials: false,
            generated_text_only: true,
            failure: Some(RouteFailureClass::ProtocolOrResultInsufficient),
        };
    }
    if candidates.is_empty() {
        return NativeSearchParse {
            candidates,
            has_retrieval_credentials: false,
            generated_text_only: false,
            failure: None,
        };
    }
    NativeSearchParse {
        candidates,
        has_retrieval_credentials: true,
        generated_text_only: false,
        failure: None,
    }
}

/// C10 per-endpoint native search probe. Never infers Available from brand or tools.
pub(crate) fn native_search_support_for(
    endpoint: Option<&NativeSearchEndpointRef>,
) -> NativeSearchSupport {
    if native_search_unsupported_reason_for(endpoint).is_some() {
        NativeSearchSupport::Unsupported
    } else {
        NativeSearchSupport::Available
    }
}

/// Stable unsupported reason for dualPath JSON.
pub(crate) fn native_search_unsupported_reason_for(
    endpoint: Option<&NativeSearchEndpointRef>,
) -> Option<NativeSearchUnsupportedReason> {
    let Some(endpoint) = endpoint else {
        return Some(NativeSearchUnsupportedReason::AdapterAbsent);
    };
    if catalog_capability_absent(&endpoint.model_id) {
        return Some(NativeSearchUnsupportedReason::CapabilityAbsent);
    }
    if test_adapter_registered(&endpoint.model_id)
        || crate::ai_runtime::native_search_adapter::lookup_production_adapter(&endpoint.model_id)
            .is_some()
    {
        return None;
    }
    Some(NativeSearchUnsupportedReason::AdapterAbsent)
}

/// Production adapter registry size. Counted from per-model adapter files.
#[cfg(test)]
pub(crate) fn production_native_search_adapter_count() -> usize {
    crate::ai_runtime::native_search_adapter::production_adapter_count()
}

fn catalog_capability_absent(model_id: &str) -> bool {
    crate::llm::model_catalog::find_model(model_id).is_some_and(|entry| {
        entry.endpoint_family == EndpointFamily::ResponsesReserved && !entry.supports_tools
    })
}

fn test_adapter_registered(model_id: &str) -> bool {
    #[cfg(test)]
    {
        test_adapter_registry()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains(model_id)
    }
    #[cfg(not(test))]
    {
        let _ = model_id;
        false
    }
}

#[cfg(test)]
fn test_adapter_registry() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    static REGISTRY: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

fn openai_search_hits(value: &Value) -> (bool, Vec<SearchHit>) {
    if value
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| status != "completed")
    {
        return (false, Vec::new());
    }
    let output = value["output"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let completed = output
        .iter()
        .any(|item| item["type"] == "web_search_call" && item["status"] == "completed");
    let mut hits = Vec::new();
    if completed {
        for message in output.iter().filter(|item| item["type"] == "message") {
            for part in message["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|part| part["type"] == "output_text")
            {
                for citation in part["annotations"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|citation| citation["type"] == "url_citation")
                {
                    if let Some(url) = citation["url"].as_str() {
                        push_https_hit(
                            &mut hits,
                            url,
                            citation["title"].as_str().unwrap_or_default(),
                        );
                    }
                }
            }
        }
    }
    (completed && !hits.is_empty(), hits)
}

fn anthropic_search_hits(value: &Value) -> (bool, Vec<SearchHit>) {
    let blocks = value["content"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let calls = blocks
        .iter()
        .filter(|item| item["type"] == "server_tool_use" && item["name"] == "web_search")
        .filter_map(|item| item["id"].as_str())
        .filter(|id| !id.is_empty())
        .collect::<Vec<_>>();
    let mut valid_result = false;
    let mut hits = Vec::new();
    for result in blocks
        .iter()
        .filter(|item| item["type"] == "web_search_tool_result")
    {
        if !result["tool_use_id"]
            .as_str()
            .is_some_and(|id| calls.contains(&id))
        {
            continue;
        }
        let Some(results) = result["content"].as_array() else {
            continue;
        };
        if results
            .iter()
            .any(|item| item["type"] != "web_search_result")
        {
            continue;
        }
        valid_result = true;
        for hit in results {
            if let Some(url) = hit["url"].as_str() {
                push_https_hit(&mut hits, url, hit["title"].as_str().unwrap_or_default());
            }
        }
    }
    (valid_result, hits)
}

fn has_gemini_structured_search(value: &Value) -> bool {
    let mut found = false;
    walk_json(value, &mut |node| {
        if node.get("groundingChunks").is_some()
            || node.get("grounding_chunks").is_some()
            || node.get("groundingMetadata").is_some()
            || node.get("grounding_metadata").is_some()
        {
            found = true;
        }
    });
    found
}

fn collect_structured_https_hits(value: &Value) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    walk_json(value, &mut |node| {
        let Some(object) = node.as_object() else {
            return;
        };
        if object.get("type").and_then(Value::as_str) == Some("url_citation")
            || object.get("type").and_then(Value::as_str) == Some("web_search_result")
        {
            if let Some(url) = object.get("url").and_then(Value::as_str) {
                let title = object
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                push_https_hit(&mut hits, url, title);
            }
        }
        if let Some(web) = object.get("web").and_then(Value::as_object) {
            if let Some(url) = web
                .get("uri")
                .or_else(|| web.get("url"))
                .and_then(Value::as_str)
            {
                let title = web.get("title").and_then(Value::as_str).unwrap_or_default();
                push_https_hit(&mut hits, url, title);
            }
        }
    });
    hits
}

fn push_https_hit(hits: &mut Vec<SearchHit>, url: &str, title: &str) {
    if !url.starts_with("https://")
        || !reqwest::Url::parse(url)
            .is_ok_and(|parsed| parsed.scheme() == "https" && parsed.host_str().is_some())
    {
        return;
    }
    if hits.iter().any(|hit| hit.url == url) {
        return;
    }
    let title = title.trim();
    let snippet = if title.is_empty() {
        url.to_string()
    } else {
        title.to_string()
    };
    hits.push(SearchHit {
        url: url.to_string(),
        title: title.to_string(),
        snippet,
    });
}

fn walk_json(value: &Value, visit: &mut impl FnMut(&Value)) {
    visit(value);
    match value {
        Value::Array(items) => {
            for item in items {
                walk_json(item, visit);
            }
        }
        Value::Object(object) => {
            for child in object.values() {
                walk_json(child, visit);
            }
        }
        _ => {}
    }
}

impl NativeSearchSubrequest {
    /// Outbound JSON contains only the approved query, public scope, identity, and endpoint.
    #[cfg(test)]
    pub(crate) fn outbound_payload(&self) -> Value {
        json!({
            "query": self.query,
            "publicScope": {
                "date": self.public_scope.date,
                "region": self.public_scope.region,
                "language": self.public_scope.language,
            },
            "identity": {
                "runId": self.identity.run_id,
                "inputRevision": self.identity.input_revision,
                "parentCallId": self.identity.parent_call_id,
                "attempt": self.identity.attempt,
            },
            "endpoint": {
                "modelId": self.endpoint.model_id,
                "endpointFamily": self.endpoint.endpoint_family,
            },
        })
    }
}

impl NativeSearchParse {
    pub(crate) fn into_route_outcome(self) -> RouteAttemptOutcome {
        RouteAttemptOutcome {
            candidates: self.candidates,
            has_retrieval_credentials: self.has_retrieval_credentials,
            generated_text_only: self.generated_text_only,
            failure: self.failure,
            internal_provider_attempts: 0,
            ..Default::default()
        }
    }
}

const SEARCH_EVENT_KINDS: &[&str] = &[
    "web_search_call",
    "url_citation",
    "server_tool_use",
    "web_search_tool_result",
    "web_search_result",
];

/// Collect supplier-defined search event type names from a JSON payload.
pub(crate) fn collect_search_event_kinds(value: &Value) -> Vec<String> {
    let mut kinds = Vec::new();
    walk_json(value, &mut |node| {
        if let Some(kind) = node.get("type").and_then(Value::as_str) {
            if SEARCH_EVENT_KINDS.contains(&kind) && !kinds.iter().any(|existing| existing == kind)
            {
                kinds.push(kind.to_string());
            }
        }
        if (node.get("groundingChunks").is_some()
            || node.get("grounding_chunks").is_some()
            || node.get("groundingMetadata").is_some()
            || node.get("grounding_metadata").is_some())
            && !kinds.iter().any(|existing| existing == "groundingMetadata")
        {
            kinds.push("groundingMetadata".to_string());
        }
    });
    kinds
}

/// Map a main-dialogue SSE/JSON fragment onto a leak observation.
///
/// Structured search events count as retrieval credentials even before HTTPS
/// citations arrive. This observation never drives K11 native `succeeded`.
pub(crate) fn observation_from_main_stream_json(value: &Value) -> Option<RetrievalObservation> {
    let event_kinds = collect_search_event_kinds(value);
    let openai = parse_native_search_payload(NativeSearchPayloadFamily::OpenAiShaped, value);
    let gemini = parse_native_search_payload(NativeSearchPayloadFamily::GeminiShaped, value);
    let anthropic = parse_native_search_payload(NativeSearchPayloadFamily::AnthropicShaped, value);
    let parse = [openai, gemini, anthropic]
        .into_iter()
        .max_by_key(|item| (item.has_retrieval_credentials, item.candidates.len()))
        .expect("three parse families");
    let mut candidates = parse.candidates;
    if candidates.is_empty() {
        candidates = collect_structured_https_hits(value);
    }
    if event_kinds.is_empty() && candidates.is_empty() && !parse.has_retrieval_credentials {
        return None;
    }
    Some(RetrievalObservation {
        origin: RetrievalOrigin::MainStreamLeak,
        identity: SearchActionIdentity::default(),
        has_retrieval_credentials: parse.has_retrieval_credentials || !event_kinds.is_empty(),
        candidates,
        event_kinds,
        prompt_tokens: extract_reported_prompt_tokens(value),
        completion_tokens: extract_reported_completion_tokens(value),
    })
}

/// Fold another stream fragment into an accumulated observation.
pub(crate) fn merge_retrieval_observation(
    slot: &mut Option<RetrievalObservation>,
    next: RetrievalObservation,
) {
    let Some(existing) = slot.as_mut() else {
        *slot = Some(next);
        return;
    };
    existing.origin = next.origin;
    existing.has_retrieval_credentials |= next.has_retrieval_credentials;
    for candidate in next.candidates {
        if !existing
            .candidates
            .iter()
            .any(|existing_hit| existing_hit.url == candidate.url)
        {
            existing.candidates.push(candidate);
        }
    }
    for kind in next.event_kinds {
        if !existing
            .event_kinds
            .iter()
            .any(|existing_kind| existing_kind == &kind)
        {
            existing.event_kinds.push(kind);
        }
    }
    if existing.prompt_tokens.is_none() {
        existing.prompt_tokens = next.prompt_tokens;
    }
    if existing.completion_tokens.is_none() {
        existing.completion_tokens = next.completion_tokens;
    }
}

pub(crate) fn extract_reported_prompt_tokens(value: &Value) -> Option<u32> {
    value
        .pointer("/usage/input_tokens")
        .or_else(|| value.pointer("/usage/prompt_tokens"))
        .or_else(|| value.pointer("/message/usage/input_tokens"))
        .and_then(Value::as_u64)
        .and_then(|tokens| u32::try_from(tokens).ok())
}

pub(crate) fn extract_reported_completion_tokens(value: &Value) -> Option<u32> {
    value
        .pointer("/usage/output_tokens")
        .or_else(|| value.pointer("/usage/completion_tokens"))
        .and_then(Value::as_u64)
        .and_then(|tokens| u32::try_from(tokens).ok())
}

impl RetrievalObservation {
    #[cfg(test)]
    pub(crate) fn from_isolated_parse(
        identity: SearchActionIdentity,
        parse: &NativeSearchParse,
        event_kinds: Vec<String>,
        prompt_tokens: Option<u32>,
        completion_tokens: Option<u32>,
    ) -> Self {
        Self {
            origin: RetrievalOrigin::IsolatedSubrequest,
            identity,
            has_retrieval_credentials: parse.has_retrieval_credentials,
            candidates: parse.candidates.clone(),
            event_kinds,
            prompt_tokens,
            completion_tokens,
        }
    }
}

#[cfg(test)]
pub(crate) fn register_native_search_adapter_for_tests(model_id: &str) {
    test_adapter_registry()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(model_id.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::dual_path_search::{
        coordinate_dual_path_search, dual_path_status_json, DualPathSearchRequest,
        NativeSearchSupportProbe, ProductionNativeSearchRoute, ProductionNativeSearchSupport,
        SearchActionIdentity, SearchRoute,
    };
    use serde_json::json;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn identity() -> NativeSearchRequestIdentity {
        NativeSearchRequestIdentity {
            run_id: "run-k12".into(),
            input_revision: "3".into(),
            parent_call_id: "call-parent".into(),
            attempt: 1,
            ..Default::default()
        }
    }

    fn endpoint(model_id: &str, family: EndpointFamily) -> NativeSearchEndpointRef {
        NativeSearchEndpointRef::new(model_id, family)
    }

    fn draft(query: &str, private_material: Option<&str>) -> NativeSearchSubrequestDraft {
        NativeSearchSubrequestDraft {
            query: query.into(),
            public_scope: NativeSearchPublicScope {
                date: Some("2026-09".into()),
                region: Some("CN".into()),
                language: Some("zh".into()),
            },
            identity: identity(),
            endpoint: endpoint(
                "qwen2.5:7b",
                EndpointFamily::OpenAiCompatibleChatCompletions,
            ),
            private_material: private_material.map(str::to_string),
        }
    }

    fn openai_fixture() -> Value {
        json!({
            "output": [
                {
                    "type": "web_search_call",
                    "status": "completed",
                    "action": { "query": "approved query" }
                },
                {
                    "type": "message",
                    "content": [{
                        "type": "output_text",
                        "text": "summary",
                        "annotations": [
                            {
                                "type": "url_citation",
                                "url": "https://openai-shaped.example/a",
                                "title": "OpenAI hit"
                            },
                            {
                                "type": "url_citation",
                                "url": "http://insecure.example/drop",
                                "title": "drop me"
                            }
                        ]
                    }]
                }
            ]
        })
    }

    fn gemini_fixture() -> Value {
        json!({
            "candidates": [{
                "content": { "parts": [{ "text": "summary" }] },
                "groundingMetadata": {
                    "groundingChunks": [
                        { "web": { "uri": "https://gemini-shaped.example/g", "title": "Gemini hit" } },
                        { "web": { "uri": "http://insecure.example/drop", "title": "drop me" } }
                    ]
                }
            }]
        })
    }

    fn text_only_fixture() -> Value {
        json!({
            "choices": [{
                "message": {
                    "content": "see https://invented.example/from-text and notes/vault/secret.md"
                }
            }]
        })
    }

    fn structured_empty_openai_fixture() -> Value {
        json!({
            "output": [{
                "type": "web_search_call",
                "status": "completed"
            }]
        })
    }

    /// Shape taken from the 2026-09-20 MiniMax-M3 `/v1/responses` live probe.
    /// Hosts are examples; this is not a V05 verdict.
    fn minimax_responses_live_shape_fixture() -> Value {
        json!({
            "id": "resp-fixture",
            "object": "response",
            "status": "completed",
            "model": "MiniMax-M3",
            "output": [
                {
                    "id": "call_fixture",
                    "type": "web_search_call",
                    "status": "completed",
                    "action": { "type": "search", "query": "Shanghai weather today" }
                },
                {
                    "id": "resp-fixture_msg",
                    "type": "message",
                    "status": "completed",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": "summary",
                        "annotations": [{
                            "type": "url_citation",
                            "title": "example weather",
                            "url": "https://weather.example/shanghai"
                        }]
                    }]
                }
            ],
            "tools": [{ "type": "web_search" }]
        })
    }

    /// Shape taken from the 2026-09-20 DeepSeek Anthropic Messages live probe:
    /// `web_search_20250305` produced `server_tool_use` + `web_search_result`.
    fn deepseek_anthropic_live_shape_fixture() -> Value {
        json!({
            "id": "msg-fixture",
            "type": "message",
            "role": "assistant",
            "model": "deepseek-flash",
            "stop_reason": "end_turn",
            "content": [
                {
                    "type": "server_tool_use",
                    "id": "fixture-search-call",
                    "name": "web_search",
                    "input": { "query": "Shanghai weather today" }
                },
                {
                    "type": "web_search_tool_result",
                    "tool_use_id": "fixture-search-call",
                    "content": [{
                        "type": "web_search_result",
                        "title": "example weather",
                        "url": "https://weather.example/shanghai"
                    }]
                },
                {
                    "type": "text",
                    "text": "summary"
                }
            ]
        })
    }

    /// Shape taken from the 2026-09-20 MiniMax Anthropic Messages live probe:
    /// HTTP 200, `stop_reason=end_turn`, only a `text` block. Not native search.
    fn minimax_anthropic_messages_text_only_fixture() -> Value {
        json!({
            "id": "msg-fixture",
            "type": "message",
            "role": "assistant",
            "model": "MiniMax-M3",
            "content": [{
                "type": "text",
                "text": "I don't have access to real-time weather data."
            }],
            "stop_reason": "end_turn"
        })
    }

    struct CountingMcpRoute {
        calls: AtomicU32,
    }

    impl SearchRoute for CountingMcpRoute {
        async fn execute(&self, _query: &str) -> RouteAttemptOutcome {
            self.calls.fetch_add(1, Ordering::SeqCst);
            RouteAttemptOutcome {
                candidates: vec![SearchHit {
                    url: "https://mcp.example/k12".into(),
                    title: "mcp".into(),
                    snippet: "ok".into(),
                }],
                has_retrieval_credentials: true,
                generated_text_only: false,
                failure: None,
                internal_provider_attempts: 1,
                dispatched_attempts: 1,
                ..Default::default()
            }
        }
    }

    struct ParsedNativeRoute {
        family: NativeSearchPayloadFamily,
        payload: Value,
        endpoint: NativeSearchEndpointRef,
    }

    impl SearchRoute for ParsedNativeRoute {
        async fn execute(&self, query: &str) -> RouteAttemptOutcome {
            let constructed = construct_native_search_subrequest(NativeSearchSubrequestDraft {
                query: query.into(),
                public_scope: NativeSearchPublicScope::default(),
                identity: identity(),
                endpoint: self.endpoint.clone(),
                private_material: Some("笔记正文 notes/vault/secret.md".into()),
            })
            .expect("constructor must accept the approved query");
            let blob = constructed.outbound_payload().to_string();
            assert!(
                !blob.contains("笔记正文") && !blob.contains("notes/vault/secret.md"),
                "native executor must not forward notes: {blob}"
            );
            let mut outcome =
                parse_native_search_payload(self.family, &self.payload).into_route_outcome();
            // This route represents one executed fixture request; credential
            // parsing alone deliberately cannot manufacture a dispatch fact.
            outcome.dispatched_attempts = 1;
            outcome.internal_provider_attempts = 1;
            outcome
        }
    }

    fn assert_no_degradation(value: &Value) {
        let blob = value.to_string();
        for needle in ["能力降级", "模型能力降级", "模型出错"] {
            assert!(
                !blob.contains(needle),
                "payload must not say {needle}: {blob}"
            );
        }
    }

    #[test]
    fn constructor_keeps_identity_and_strips_notes_and_vault_paths() {
        let sub = construct_native_search_subrequest(draft(
            "approved query",
            Some("这是笔记正文\npath: notes/vault/secret.md\n更多私有内容"),
        ))
        .expect("approved query must construct");
        assert_eq!(sub.query, "approved query");
        assert_eq!(sub.identity.run_id, "run-k12");
        assert_eq!(sub.identity.input_revision, "3");
        assert_eq!(sub.identity.parent_call_id, "call-parent");
        assert_eq!(sub.identity.attempt, 1);
        assert_eq!(sub.endpoint.model_id, "qwen2.5:7b");
        let payload = sub.outbound_payload();
        assert_eq!(payload["query"], "approved query");
        assert_eq!(payload["identity"]["runId"], "run-k12");
        assert_eq!(payload["identity"]["parentCallId"], "call-parent");
        let blob = payload.to_string();
        assert!(!blob.contains("笔记正文"), "{blob}");
        assert!(!blob.contains("notes/vault/secret.md"), "{blob}");
        assert!(!blob.contains("私有内容"), "{blob}");
        assert_no_degradation(&payload);
    }

    #[test]
    fn constructor_rejects_empty_query() {
        let error = construct_native_search_subrequest(draft("   ", None))
            .expect_err("empty query must be rejected");
        assert_eq!(error, NativeSearchConstructError::EmptyQuery);
    }

    #[test]
    fn review_regression_d_anthropic_credentials_require_successful_matching_result() {
        let adapter = crate::ai_runtime::native_search_adapter::lookup_production_adapter(
            "deepseek-v4-flash",
        )
        .unwrap();
        let mut response = json!({"content": [
            {"type":"server_tool_use", "name":"web_search", "id":"s1"},
            {"type":"web_search_tool_result", "tool_use_id":"s1", "content":{
                "type":"web_search_tool_result_error", "error_code":"unavailable"}},
            {"type":"text", "text":"unrelated", "citations":[{
                "type":"url_citation", "url":"https://unrelated.example/a"}]}
        ]});
        assert!(!adapter.parse_response(&response).has_retrieval_credentials);
        response["content"][1]["content"] =
            json!([{"type":"web_search_result", "url":"https://found.example/a"}]);
        response["content"][1]["tool_use_id"] = json!("other");
        assert!(!adapter.parse_response(&response).has_retrieval_credentials);
        response["content"][1]["tool_use_id"] = json!("s1");
        let parsed = adapter.parse_response(&response);
        assert!(parsed.has_retrieval_credentials);
        assert_eq!(parsed.candidates.len(), 1);
        assert_eq!(parsed.candidates[0].url, "https://found.example/a");
    }

    #[test]
    fn review_regression_d_invalid_https_spelling_is_not_retrieval_credentials() {
        for invalid in ["https://", "https://bad host/path"] {
            let mut responses = minimax_responses_live_shape_fixture();
            responses["output"][1]["content"][0]["annotations"][0]["url"] = json!(invalid);
            let mut anthropic = deepseek_anthropic_live_shape_fixture();
            anthropic["content"][1]["content"][0]["url"] = json!(invalid);
            for (model, body) in [("MiniMax-M3", responses), ("deepseek-v4-flash", anthropic)] {
                let adapter =
                    crate::ai_runtime::native_search_adapter::lookup_production_adapter(model)
                        .unwrap();
                let parsed = adapter.parse_response(&body);
                assert!(!parsed.has_retrieval_credentials, "{model}: {invalid}");
                assert!(parsed.candidates.is_empty());
            }
        }
    }

    #[test]
    fn review_regression_d_responses_require_completed_response_and_scoped_citations() {
        let adapter =
            crate::ai_runtime::native_search_adapter::lookup_production_adapter("MiniMax-M3")
                .unwrap();
        for status in ["failed", "incomplete", "in_progress"] {
            let mut response = minimax_responses_live_shape_fixture();
            response["status"] = json!(status);
            assert!(
                !adapter.parse_response(&response).has_retrieval_credentials,
                "{status}"
            );
        }
        let mut response = minimax_responses_live_shape_fixture();
        response["output"][1]["content"] = json!([]);
        response["metadata"] = json!({"type":"url_citation", "url":"https://unrelated.example/a"});
        assert!(!adapter.parse_response(&response).has_retrieval_credentials);
    }

    #[test]
    fn constructor_budget_claim_is_model_auxiliary_not_network_dispatch() {
        let sub = construct_native_search_subrequest(draft("approved query", None)).unwrap();
        assert_eq!(
            sub.budget_claim.kind,
            NativeSearchBudgetKind::ModelAuxiliaryRequest
        );
        assert_eq!(sub.budget_claim.parent_call_id, "call-parent");
        assert!(!sub.budget_claim.is_network_tool_dispatch());
    }

    #[test]
    fn anthropic_shaped_fixture_yields_https_credentials() {
        let parse = parse_native_search_payload(
            NativeSearchPayloadFamily::AnthropicShaped,
            &deepseek_anthropic_live_shape_fixture(),
        );
        assert!(parse.has_retrieval_credentials);
        assert!(!parse.generated_text_only);
        assert_eq!(parse.failure, None);
        assert_eq!(parse.candidates.len(), 1);
        assert_eq!(parse.candidates[0].url, "https://weather.example/shanghai");
    }

    #[test]
    fn deepseek_anthropic_shape_is_not_openai_web_search_call() {
        let openai = parse_native_search_payload(
            NativeSearchPayloadFamily::OpenAiShaped,
            &deepseek_anthropic_live_shape_fixture(),
        );
        assert!(
            !openai.has_retrieval_credentials,
            "DeepSeek hosted search is Anthropic blocks, not Responses web_search_call"
        );
        let anthropic = parse_native_search_payload(
            NativeSearchPayloadFamily::AnthropicShaped,
            &deepseek_anthropic_live_shape_fixture(),
        );
        assert!(anthropic.has_retrieval_credentials);
    }

    #[test]
    fn openai_shaped_fixture_yields_https_credentials() {
        let parse =
            parse_native_search_payload(NativeSearchPayloadFamily::OpenAiShaped, &openai_fixture());
        assert!(parse.has_retrieval_credentials);
        assert!(!parse.generated_text_only);
        assert_eq!(parse.failure, None);
        assert_eq!(parse.candidates.len(), 1);
        assert_eq!(parse.candidates[0].url, "https://openai-shaped.example/a");
        assert!(parse
            .candidates
            .iter()
            .all(|hit| hit.url.starts_with("https://")));
    }

    #[test]
    fn gemini_shaped_fixture_yields_https_credentials() {
        let parse =
            parse_native_search_payload(NativeSearchPayloadFamily::GeminiShaped, &gemini_fixture());
        assert!(parse.has_retrieval_credentials);
        assert!(!parse.generated_text_only);
        assert_eq!(parse.failure, None);
        assert_eq!(parse.candidates.len(), 1);
        assert_eq!(parse.candidates[0].url, "https://gemini-shaped.example/g");
    }

    #[test]
    fn minimax_responses_live_shape_yields_https_credentials() {
        let parse = parse_native_search_payload(
            NativeSearchPayloadFamily::OpenAiShaped,
            &minimax_responses_live_shape_fixture(),
        );
        assert!(parse.has_retrieval_credentials);
        assert!(!parse.generated_text_only);
        assert_eq!(parse.failure, None);
        assert_eq!(parse.candidates.len(), 1);
        assert_eq!(parse.candidates[0].url, "https://weather.example/shanghai");
    }

    #[test]
    fn minimax_anthropic_messages_text_only_is_protocol_insufficient() {
        let parse = parse_native_search_payload(
            NativeSearchPayloadFamily::OpenAiShaped,
            &minimax_anthropic_messages_text_only_fixture(),
        );
        assert!(!parse.has_retrieval_credentials);
        assert!(parse.generated_text_only);
        assert_eq!(
            parse.failure,
            Some(RouteFailureClass::ProtocolOrResultInsufficient)
        );
    }

    #[test]
    fn text_only_url_is_protocol_insufficient_not_succeeded_or_unsupported() {
        let parse = parse_native_search_payload(
            NativeSearchPayloadFamily::OpenAiShaped,
            &text_only_fixture(),
        );
        assert!(!parse.has_retrieval_credentials);
        assert!(parse.generated_text_only);
        assert_eq!(
            parse.failure,
            Some(RouteFailureClass::ProtocolOrResultInsufficient)
        );
        let outcome = parse.into_route_outcome();
        assert!(!outcome.has_retrieval_credentials);
        assert_ne!(
            outcome.failure,
            Some(RouteFailureClass::TransportOrProviderFailure)
        );
    }

    #[test]
    fn main_stream_leak_is_not_isolated_native_route_success() {
        let value = json!({
            "type": "web_search_call",
            "status": "completed",
            "action": { "query": "q" }
        });
        let leak = observation_from_main_stream_json(&value).expect("leak observation");
        assert_eq!(leak.origin, RetrievalOrigin::MainStreamLeak);
        assert!(leak.has_retrieval_credentials);
        assert!(
            leak.identity.run_id.is_empty(),
            "leaked SSE credentials must not carry a native search identity"
        );
        let isolated = RetrievalObservation::from_isolated_parse(
            SearchActionIdentity {
                run_id: "run-1".into(),
                input_revision: "1".into(),
                action_id: "web_search".into(),
                attempt: 1,
                ..Default::default()
            },
            &parse_native_search_payload(NativeSearchPayloadFamily::OpenAiShaped, &value),
            leak.event_kinds.clone(),
            None,
            None,
        );
        assert_eq!(isolated.origin, RetrievalOrigin::IsolatedSubrequest);
        assert_ne!(leak.origin, isolated.origin);
    }

    #[test]
    fn structured_empty_result_is_empty_not_transport_failure() {
        let parse = parse_native_search_payload(
            NativeSearchPayloadFamily::OpenAiShaped,
            &structured_empty_openai_fixture(),
        );
        assert!(parse.candidates.is_empty());
        assert!(parse.generated_text_only);
        assert_eq!(
            parse.failure,
            Some(RouteFailureClass::ProtocolOrResultInsufficient)
        );
    }

    #[test]
    fn openai_in_progress_call_with_citation_is_not_retrieval() {
        let parse = parse_native_search_payload(
            NativeSearchPayloadFamily::OpenAiShaped,
            &json!({
                "output": [
                    { "type": "web_search_call", "status": "in_progress" },
                    {
                        "type": "message",
                        "content": [{
                            "type": "output_text",
                            "annotations": [{
                                "type": "url_citation",
                                "url": "https://openai-shaped.example/a"
                            }]
                        }]
                    }
                ]
            }),
        );
        assert!(!parse.has_retrieval_credentials);
        assert!(parse.generated_text_only);
    }

    #[test]
    fn anthropic_server_tool_use_without_result_is_not_retrieval() {
        let parse = parse_native_search_payload(
            NativeSearchPayloadFamily::AnthropicShaped,
            &json!({
                "content": [{
                    "type": "server_tool_use",
                    "id": "fixture-search-call",
                    "name": "web_search",
                    "input": { "query": "q" }
                }]
            }),
        );
        assert!(!parse.has_retrieval_credentials);
        assert!(parse.generated_text_only);
    }

    #[test]
    fn anthropic_tool_result_without_web_search_use_is_not_retrieval() {
        let parse = parse_native_search_payload(
            NativeSearchPayloadFamily::AnthropicShaped,
            &json!({
                "content": [{
                    "type": "web_search_tool_result",
                    "tool_use_id": "fixture-search-call",
                    "content": [{
                        "type": "web_search_result",
                        "url": "https://weather.example/shanghai"
                    }]
                }]
            }),
        );
        assert!(!parse.has_retrieval_credentials);
        assert!(parse.generated_text_only);
    }

    #[test]
    fn responses_reserved_asr_is_capability_absent_and_does_not_block_mcp() {
        let asr = endpoint("MiMo-V2.5-ASR", EndpointFamily::ResponsesReserved);
        assert_eq!(
            native_search_support_for(Some(&asr)),
            NativeSearchSupport::Unsupported
        );
        assert_eq!(
            native_search_unsupported_reason_for(Some(&asr)),
            Some(NativeSearchUnsupportedReason::CapabilityAbsent)
        );
    }

    #[tokio::test]
    async fn capability_absent_probe_still_executes_mcp_route() {
        let asr = endpoint("MiMo-V2.5-TTS", EndpointFamily::ResponsesReserved);
        let probe = ProductionNativeSearchSupport {
            endpoint: Some(asr),
        };
        assert_eq!(
            probe.native_search_support(),
            NativeSearchSupport::Unsupported
        );
        assert_eq!(
            probe.native_search_unsupported_reason(),
            Some(NativeSearchUnsupportedReason::CapabilityAbsent)
        );
        let mcp = CountingMcpRoute {
            calls: AtomicU32::new(0),
        };
        let outcome = coordinate_dual_path_search(
            DualPathSearchRequest {
                identity: SearchActionIdentity {
                    run_id: "run-k12".into(),
                    input_revision: "1".into(),
                    action_id: "web_search".into(),
                    attempt: 1,
                    ..Default::default()
                },
                query: "approved query".into(),
                allow_second_route: true,
            },
            &probe,
            &ProductionNativeSearchRoute::default(),
            &mcp,
        )
        .await;
        assert_eq!(mcp.calls.load(Ordering::SeqCst), 1);
        assert!(!outcome.native.supported);
        assert!(!outcome.native.attempted);
        assert!(outcome.mcp.succeeded);
        let json = dual_path_status_json(&outcome);
        assert_eq!(json["native"]["unsupportedReason"], "capability_absent");
        assert_no_degradation(&json);
    }

    #[test]
    fn ordinary_chat_model_without_adapter_is_adapter_absent_not_available() {
        let chat = endpoint(
            "qwen2.5:7b",
            EndpointFamily::OpenAiCompatibleChatCompletions,
        );
        assert_eq!(
            native_search_support_for(Some(&chat)),
            NativeSearchSupport::Unsupported
        );
        assert_eq!(
            native_search_unsupported_reason_for(Some(&chat)),
            Some(NativeSearchUnsupportedReason::AdapterAbsent)
        );
        assert_ne!(
            native_search_support_for(Some(&chat)),
            NativeSearchSupport::Available
        );
    }

    #[test]
    fn minimax_m3_production_adapter_is_available() {
        let entry = crate::llm::model_catalog::find_model("MiniMax-M3").expect("catalog");
        assert!(entry.supports_tools);
        assert_eq!(
            entry.endpoint_family,
            EndpointFamily::OpenAiCompatibleChatCompletions
        );
        let endpoint = endpoint(entry.id, entry.endpoint_family);
        assert_eq!(
            native_search_support_for(Some(&endpoint)),
            NativeSearchSupport::Available
        );
        assert_eq!(native_search_unsupported_reason_for(Some(&endpoint)), None);
        assert_eq!(production_native_search_adapter_count(), 2);
    }

    #[test]
    fn supports_tools_true_does_not_infer_available() {
        let tools_true = crate::llm::model_catalog::find_model("qwen2.5:7b").expect("catalog");
        assert!(tools_true.supports_tools);
        let endpoint = endpoint(tools_true.id, tools_true.endpoint_family);
        assert_eq!(
            native_search_support_for(Some(&endpoint)),
            NativeSearchSupport::Unsupported
        );
        assert_eq!(
            native_search_unsupported_reason_for(Some(&endpoint)),
            Some(NativeSearchUnsupportedReason::AdapterAbsent)
        );
    }

    #[tokio::test]
    async fn registered_adapter_runs_constructor_and_parser_through_coordinator() {
        register_native_search_adapter_for_tests("k12-test-native-adapter");
        let native_endpoint = endpoint(
            "k12-test-native-adapter",
            EndpointFamily::OpenAiCompatibleChatCompletions,
        );
        assert_eq!(
            native_search_support_for(Some(&native_endpoint)),
            NativeSearchSupport::Available
        );
        let native = ParsedNativeRoute {
            family: NativeSearchPayloadFamily::OpenAiShaped,
            payload: openai_fixture(),
            endpoint: native_endpoint.clone(),
        };
        let mcp = CountingMcpRoute {
            calls: AtomicU32::new(0),
        };
        let probe = ProductionNativeSearchSupport {
            endpoint: Some(native_endpoint),
        };
        let outcome = coordinate_dual_path_search(
            DualPathSearchRequest {
                identity: SearchActionIdentity {
                    run_id: "run-k12-adapter".into(),
                    input_revision: "1".into(),
                    action_id: "web_search".into(),
                    attempt: 1,
                    ..Default::default()
                },
                query: "approved query".into(),
                allow_second_route: true,
            },
            &probe,
            &native,
            &mcp,
        )
        .await;
        assert!(outcome.native.supported);
        assert!(outcome.native.attempted);
        assert!(outcome.native.succeeded);
        assert!(outcome.mcp.succeeded);
        assert!(outcome
            .candidates
            .iter()
            .any(|candidate| candidate.url == "https://openai-shaped.example/a"));
        let json = dual_path_status_json(&outcome);
        assert!(json["native"]["unsupportedReason"].is_null());
        assert_no_degradation(&json);
    }

    #[tokio::test]
    async fn production_registry_adapts_deepseek_flash_and_still_calls_mcp() {
        assert_eq!(production_native_search_adapter_count(), 2);
        let chat = endpoint(
            "deepseek-v4-flash",
            EndpointFamily::OpenAiCompatibleChatCompletions,
        );
        let probe = ProductionNativeSearchSupport {
            endpoint: Some(chat.clone()),
        };
        assert_eq!(
            probe.native_search_support(),
            NativeSearchSupport::Available
        );
        assert_eq!(probe.native_search_unsupported_reason(), None);
        let mcp = CountingMcpRoute {
            calls: AtomicU32::new(0),
        };
        let native = ParsedNativeRoute {
            family: NativeSearchPayloadFamily::AnthropicShaped,
            payload: deepseek_anthropic_live_shape_fixture(),
            endpoint: chat,
        };
        let outcome = coordinate_dual_path_search(
            DualPathSearchRequest {
                identity: SearchActionIdentity::default(),
                query: "approved query".into(),
                allow_second_route: true,
            },
            &probe,
            &native,
            &mcp,
        )
        .await;
        assert_eq!(mcp.calls.load(Ordering::SeqCst), 1);
        assert!(outcome.native.supported);
        assert!(outcome.native.attempted);
        assert!(outcome.native.succeeded);
        assert!(outcome.mcp.succeeded);
        let json = dual_path_status_json(&outcome);
        assert!(json["native"]["unsupportedReason"].is_null());
        assert_eq!(json["native"]["supported"], true);
        assert_no_degradation(&json);
    }
}
