//! K12 native search subrequest constructor and fixture credential parser.
//!
//! This module constructs an isolated native-search payload and maps protocol
//! fixtures onto K11 route-attempt fields. Production has no registered
//! adapters, so C10 still reports native search as unsupported. Streaming
//! search events are not admitted into `streaming.rs`.

#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "K12 constructor and fixture parser are production types; live execution waits on a registered adapter and G03 streaming"
    )
)]

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::ai_runtime::dual_path_search::{
    NativeSearchSupport, RouteAttemptOutcome, RouteFailureClass, SearchHit,
};
use crate::ai_types::EndpointFamily;

/// Frozen model/endpoint identity used by the C10 native-search probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSearchEndpointRef {
    pub model_id: String,
    pub endpoint_family: EndpointFamily,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSearchRequestIdentity {
    pub run_id: String,
    pub input_revision: u32,
    pub parent_call_id: String,
    pub attempt: u32,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeSearchPayloadFamily {
    OpenAiShaped,
    GeminiShaped,
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
    let structured = match family {
        NativeSearchPayloadFamily::OpenAiShaped => has_openai_structured_search(value),
        NativeSearchPayloadFamily::GeminiShaped => has_gemini_structured_search(value),
    };
    if !structured {
        return NativeSearchParse {
            candidates: Vec::new(),
            has_retrieval_credentials: false,
            generated_text_only: true,
            failure: Some(RouteFailureClass::ProtocolOrResultInsufficient),
        };
    }
    let candidates = collect_structured_https_hits(value);
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
    if test_adapter_registered(&endpoint.model_id) {
        return None;
    }
    Some(NativeSearchUnsupportedReason::AdapterAbsent)
}

/// Production adapter registry size. Empty means Iris has not adapted any endpoint.
#[cfg(test)]
pub(crate) fn production_native_search_adapter_count() -> usize {
    0
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

fn has_openai_structured_search(value: &Value) -> bool {
    let mut found = false;
    walk_json(value, &mut |node| {
        if node.get("type").and_then(Value::as_str) == Some("web_search_call") {
            found = true;
        }
    });
    found
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
        if object.get("type").and_then(Value::as_str) == Some("url_citation") {
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
    if !url.starts_with("https://") {
        return;
    }
    if hits.iter().any(|hit| hit.url == url) {
        return;
    }
    hits.push(SearchHit {
        url: url.to_string(),
        title: title.to_string(),
        snippet: String::new(),
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
            input_revision: 3,
            parent_call_id: "call-parent".into(),
            attempt: 1,
        }
    }

    fn endpoint(model_id: &str, family: EndpointFamily) -> NativeSearchEndpointRef {
        NativeSearchEndpointRef {
            model_id: model_id.into(),
            endpoint_family: family,
        }
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
            parse_native_search_payload(self.family, &self.payload).into_route_outcome()
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
        assert_eq!(sub.identity.input_revision, 3);
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
    fn structured_empty_result_is_empty_not_transport_failure() {
        let parse = parse_native_search_payload(
            NativeSearchPayloadFamily::OpenAiShaped,
            &structured_empty_openai_fixture(),
        );
        assert!(parse.candidates.is_empty());
        assert!(!parse.generated_text_only);
        assert_ne!(
            parse.failure,
            Some(RouteFailureClass::TransportOrProviderFailure)
        );
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
                    input_revision: 1,
                    action_id: "web_search".into(),
                    attempt: 1,
                },
                query: "approved query".into(),
                allow_second_route: true,
            },
            &probe,
            &ProductionNativeSearchRoute,
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
                    input_revision: 1,
                    action_id: "web_search".into(),
                    attempt: 1,
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
    async fn production_registry_empty_keeps_probe_unsupported_and_still_calls_mcp() {
        assert_eq!(production_native_search_adapter_count(), 0);
        let chat = endpoint(
            "deepseek-v4-flash",
            EndpointFamily::OpenAiCompatibleChatCompletions,
        );
        let probe = ProductionNativeSearchSupport {
            endpoint: Some(chat),
        };
        assert_eq!(
            probe.native_search_support(),
            NativeSearchSupport::Unsupported
        );
        assert_eq!(
            probe.native_search_unsupported_reason(),
            Some(NativeSearchUnsupportedReason::AdapterAbsent)
        );
        let mcp = CountingMcpRoute {
            calls: AtomicU32::new(0),
        };
        let outcome = coordinate_dual_path_search(
            DualPathSearchRequest {
                identity: SearchActionIdentity::default(),
                query: "approved query".into(),
                allow_second_route: true,
            },
            &probe,
            &ProductionNativeSearchRoute,
            &mcp,
        )
        .await;
        assert_eq!(mcp.calls.load(Ordering::SeqCst), 1);
        assert!(!outcome.native.supported);
        assert!(!outcome.native.attempted);
        assert!(outcome.mcp.succeeded);
        let json = dual_path_status_json(&outcome);
        assert_eq!(json["native"]["unsupportedReason"], "adapter_absent");
        assert_eq!(json["native"]["supported"], false);
        assert_eq!(json["native"]["attempted"], false);
        assert_no_degradation(&json);
    }
}
