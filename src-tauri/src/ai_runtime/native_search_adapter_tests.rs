//! TDD coverage for per-model native-search adapters (MiniMax-M3 Responses and
//! DeepSeek-Flash Anthropic Messages).

use super::dual_path_search::{
    coordinate_dual_path_search, DualPathSearchRequest, NativeSearchSupport,
    NativeSearchSupportProbe, ProductionNativeSearchSupport, RouteFailureClass,
    SearchActionIdentity, SearchHit, SearchRoute,
};
use super::native_search_adapter::{
    bind_production_route, execute_native_search as execute_bound_native_search,
    https_api_base_without_suffixes, lookup_production_adapter, production_adapters,
    NativeSearchHttpResult, NativeSearchModelAdapter, NativeSearchTransport,
};
use super::native_search_subrequest::{
    construct_native_search_subrequest, production_native_search_adapter_count,
    NativeSearchBudgetClaim, NativeSearchBudgetKind, NativeSearchEndpointRef, NativeSearchParse,
    NativeSearchPublicScope, NativeSearchRequestIdentity, NativeSearchSubrequest,
    NativeSearchSubrequestDraft, NativeSearchUnsupportedReason,
};
use crate::ai_types::EndpointFamily;
use serde_json::{json, Value};
use std::sync::Mutex;

async fn execute_native_search<T: NativeSearchTransport>(
    draft: NativeSearchSubrequestDraft,
    transport: &T,
    bearer: Option<&str>,
    api_base: &str,
) -> super::dual_path_search::RouteAttemptOutcome {
    let _budget = crate::ai_runtime::model_turn_ledger::BindGuard::new(&draft.identity.run_id, 8);
    execute_bound_native_search(draft, transport, bearer, api_base).await
}

// Opt-in live probes are adapter probes, not production Run authorization tests.
async fn execute_configured_adapter_probe(
    endpoint: Option<&NativeSearchEndpointRef>,
    identity: &SearchActionIdentity,
    query: &str,
) -> super::dual_path_search::RouteAttemptOutcome {
    let endpoint = endpoint.unwrap().clone();
    let adapter = lookup_production_adapter(&endpoint.model_id).unwrap();
    let binding = bind_production_route(adapter, &endpoint).unwrap();
    let secret = crate::credentials::get_runtime_secret(&binding.credential_service).unwrap();
    let draft = NativeSearchSubrequestDraft {
        query: query.into(),
        public_scope: NativeSearchPublicScope::default(),
        identity: NativeSearchRequestIdentity {
            run_id: identity.run_id.clone(),
            input_revision: identity.input_revision.clone(),
            parent_call_id: identity.action_id.clone(),
            attempt: identity.attempt,
            ..Default::default()
        },
        endpoint,
        private_material: None,
    };
    execute_native_search(
        draft,
        &super::native_search_adapter::LiveNativeSearchTransport,
        Some(secret.as_str()),
        &binding.api_base,
    )
    .await
}

fn minimax_endpoint() -> NativeSearchEndpointRef {
    NativeSearchEndpointRef::new(
        "MiniMax-M3",
        EndpointFamily::OpenAiCompatibleChatCompletions,
    )
}

#[test]
fn isolated_parse_and_witness_carry_search_event_kinds_content_free() {
    let body = responses_live_shape();
    let parse = crate::ai_runtime::native_search_subrequest::parse_native_search_payload(
        crate::ai_runtime::native_search_subrequest::NativeSearchPayloadFamily::OpenAiShaped,
        &body,
    );
    assert!(
        parse
            .event_kinds
            .iter()
            .any(|kind| kind == "web_search_call"),
        "parse must carry supplier-defined event kinds: {:?}",
        parse.event_kinds
    );
    assert!(parse.event_kinds.iter().any(|kind| kind == "url_citation"));
    let outcome = parse.into_route_outcome();
    let payload = crate::ai_runtime::native_search_adapter::native_attempt_witness_payload(
        "2xx",
        &outcome,
        (Some(11), Some(7)),
        true,
    );
    assert_eq!(payload["origin"], "isolated_subrequest");
    let kinds = payload["eventKinds"].as_array().expect("event kinds");
    assert!(kinds.iter().any(|kind| kind == "web_search_call"));
    assert!(kinds.iter().any(|kind| kind == "url_citation"));
    let encoded = payload.to_string();
    assert!(!encoded.contains("https://"));
    assert!(!encoded.contains("sk-"));
}

#[test]
fn run_context_endpoint_fallback_carries_api_base_and_credential_service() {
    let model = crate::ai_runtime::run_contract::ModelOverride {
        provider_id: "minimax".into(),
        model_id: "MiniMax-M3".into(),
    };
    let endpoint =
        NativeSearchEndpointRef::for_model_override(&model).expect("catalog model resolves");
    assert_eq!(endpoint.model_id, "MiniMax-M3");
    assert_eq!(
        endpoint.api_base.as_deref(),
        Some("https://api.minimaxi.com/v1")
    );
    assert_eq!(
        endpoint.credential_service.as_deref(),
        Some(crate::credentials::llm_credential_service("minimax").as_str())
    );
    assert!(
        NativeSearchEndpointRef::for_model_override(
            &crate::ai_runtime::run_contract::ModelOverride {
                provider_id: "custom".into(),
                model_id: "not-a-catalog-model".into(),
            }
        )
        .is_none(),
        "unknown models keep the honest no-endpoint answer"
    );
}

fn draft(query: &str, private_material: Option<&str>) -> NativeSearchSubrequestDraft {
    NativeSearchSubrequestDraft {
        query: query.into(),
        public_scope: NativeSearchPublicScope {
            date: Some("2026-09".into()),
            region: Some("CN".into()),
            language: Some("zh".into()),
        },
        identity: NativeSearchRequestIdentity {
            run_id: uuid::Uuid::new_v4().to_string(),
            input_revision: "4".into(),
            parent_call_id: "web_search".into(),
            attempt: 1,
            ..Default::default()
        },
        endpoint: minimax_endpoint(),
        private_material: private_material.map(str::to_string),
    }
}

fn responses_live_shape() -> Value {
    json!({
        "id": "resp-scripted",
        "object": "response",
        "status": "completed",
        "model": "MiniMax-M3",
        "output": [
            {
                "type": "web_search_call",
                "status": "completed",
                "action": { "type": "search", "query": "approved query" }
            },
            {
                "type": "message",
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
        ]
    })
}

#[tokio::test]
async fn review_regression_d_adapter_requires_explicit_completed_response() {
    let mut body = responses_live_shape();
    body.as_object_mut().unwrap().remove("status");
    let transport = ScriptedTransport::ok(body);
    let result = execute_native_search(
        draft("approved query", None),
        &transport,
        Some("fixture"),
        "https://example.test/v1",
    )
    .await;
    assert!(!result.has_retrieval_credentials);
    assert!(result.candidates.is_empty());
    assert_eq!(
        result.failure,
        Some(RouteFailureClass::ProtocolOrResultInsufficient)
    );
}

#[tokio::test]
async fn review_regression_d_adapter_preserves_partial_usage_dimensions() {
    for (usage, expected_prompt, expected_output) in [
        (json!({"input_tokens":11}), Some(11), None),
        (json!({"output_tokens":7}), None, Some(7)),
        (json!({"input_tokens":0,"output_tokens":7}), None, Some(7)),
    ] {
        let mut body = responses_live_shape();
        body["usage"] = usage;
        let result = execute_native_search(
            draft("approved query", None),
            &ScriptedTransport::ok(body),
            Some("fixture"),
            "https://example.test/v1",
        )
        .await;
        assert_eq!(result.prompt_tokens, expected_prompt);
        assert_eq!(result.completion_tokens, expected_output);
        assert!(result.usage_unknown);
    }
}

#[tokio::test]
async fn review_regression_d_adapter_rejects_over_limit_response_after_settlement() {
    for usage in [
        json!({"input_tokens":11,"output_tokens":2049}),
        json!({"input_tokens":128001,"output_tokens":7}),
    ] {
        let mut body = responses_live_shape();
        body["usage"] = usage.clone();
        let result = execute_native_search(
            draft("approved query", None),
            &ScriptedTransport::ok(body),
            Some("fixture"),
            "https://example.test/v1",
        )
        .await;
        assert_eq!(
            result.prompt_tokens,
            usage["input_tokens"].as_u64().map(|v| v as u32)
        );
        assert_eq!(
            result.completion_tokens,
            usage["output_tokens"].as_u64().map(|v| v as u32)
        );
        assert_eq!(result.dispatched_attempts, 1);
        assert!(!result.has_retrieval_credentials);
        assert_eq!(
            result.failure,
            Some(RouteFailureClass::ProtocolOrResultInsufficient)
        );
    }
}

#[tokio::test]
async fn review_regression_d_adapter_invalid_header_is_not_dispatched() {
    let drafted = draft("approved query", None);
    let _budget = crate::ai_runtime::model_turn_ledger::BindGuard::new(&drafted.identity.run_id, 8);
    let result = execute_bound_native_search(
        drafted,
        &super::native_search_adapter::LiveNativeSearchTransport,
        Some("invalid\nheader"),
        "https://example.test/v1",
    )
    .await;
    assert_eq!(result.dispatched_attempts, 0);
}

struct ScriptedTransport {
    status: u16,
    body: Value,
    transport_failed: bool,
    recorded_url: Mutex<Option<String>>,
    recorded_body: Mutex<Option<Value>>,
    recorded_auth: Mutex<Option<bool>>,
    recorded_headers: Mutex<Option<Vec<(String, String)>>>,
}

impl ScriptedTransport {
    fn ok(body: Value) -> Self {
        Self {
            status: 200,
            body,
            transport_failed: false,
            recorded_url: Mutex::new(None),
            recorded_body: Mutex::new(None),
            recorded_auth: Mutex::new(None),
            recorded_headers: Mutex::new(None),
        }
    }
}

impl NativeSearchTransport for ScriptedTransport {
    async fn post_json(
        &self,
        url: &str,
        body: &Value,
        headers: &[(String, String)],
        before_dispatch: &(dyn Fn() -> bool + Sync),
    ) -> NativeSearchHttpResult {
        if !before_dispatch() {
            return NativeSearchHttpResult {
                status: 0,
                body: Value::Null,
                transport_failed: true,
            };
        }
        *self.recorded_url.lock().expect("url") = Some(url.to_string());
        *self.recorded_body.lock().expect("body") = Some(body.clone());
        *self.recorded_headers.lock().expect("headers") = Some(headers.to_vec());
        *self.recorded_auth.lock().expect("auth") = Some(headers.iter().any(|(name, value)| {
            !value.is_empty()
                && (name.eq_ignore_ascii_case("authorization")
                    || name.eq_ignore_ascii_case("x-api-key"))
        }));
        NativeSearchHttpResult {
            status: self.status,
            body: self.body.clone(),
            transport_failed: self.transport_failed,
        }
    }
}

struct SequentialTransport {
    responses: Mutex<std::collections::VecDeque<NativeSearchHttpResult>>,
    recorded_count: Mutex<u32>,
}

impl SequentialTransport {
    fn new(responses: Vec<(u16, Value)>) -> Self {
        Self {
            responses: Mutex::new(
                responses
                    .into_iter()
                    .map(|(status, body)| NativeSearchHttpResult {
                        status,
                        body,
                        transport_failed: false,
                    })
                    .collect(),
            ),
            recorded_count: Mutex::new(0),
        }
    }
}

impl NativeSearchTransport for SequentialTransport {
    async fn post_json(
        &self,
        _url: &str,
        _body: &Value,
        _headers: &[(String, String)],
        before_dispatch: &(dyn Fn() -> bool + Sync),
    ) -> NativeSearchHttpResult {
        if !before_dispatch() {
            return NativeSearchHttpResult {
                status: 0,
                body: Value::Null,
                transport_failed: true,
            };
        }
        *self.recorded_count.lock().expect("count") += 1;
        self.responses
            .lock()
            .expect("responses")
            .pop_front()
            .unwrap_or(NativeSearchHttpResult {
                status: 0,
                body: Value::Null,
                transport_failed: true,
            })
    }
}

fn minimax_completed_text_only_with_tool_echo() -> Value {
    json!({
        "id": "resp-no-search",
        "object": "response",
        "status": "completed",
        "error": null,
        "tools": [{ "type": "web_search" }],
        "output": [{
            "type": "message",
            "role": "assistant",
            "content": [{
                "type": "output_text",
                "text": "I don't have access to real-time weather data."
            }]
        }]
    })
}

#[tokio::test]
async fn review_regression_d_failed_retry_preserves_first_attempt_usage() {
    let mut first = minimax_completed_text_only_with_tool_echo();
    first["usage"] = json!({"input_tokens":11,"output_tokens":7});
    let transport = SequentialTransport::new(vec![(200, first), (503, Value::Null)]);
    let outcome = execute_native_search(
        draft("approved query", None),
        &transport,
        Some("fixture"),
        "https://search.example/v1",
    )
    .await;
    assert_eq!(*transport.recorded_count.lock().unwrap(), 2);
    assert_eq!(outcome.prompt_tokens, Some(11));
    assert_eq!(outcome.completion_tokens, Some(7));
    assert!(!outcome.has_retrieval_credentials);
}

struct CountingMcpRoute;

impl SearchRoute for CountingMcpRoute {
    async fn execute(&self, _query: &str) -> super::dual_path_search::RouteAttemptOutcome {
        super::dual_path_search::RouteAttemptOutcome {
            candidates: vec![SearchHit {
                url: "https://mcp.example/adapter".into(),
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

#[test]
fn minimax_responses_outbound_declares_web_search_and_drops_notes() {
    let sub = construct_native_search_subrequest(draft(
        "approved query",
        Some("这是笔记正文\npath: notes/vault/secret.md"),
    ))
    .expect("approved query must construct");
    let adapter = lookup_production_adapter("MiniMax-M3").expect("MiniMax-M3 adapter");
    assert_eq!(adapter.id(), "MiniMax-M3");
    let body = adapter.outbound_body(&sub);
    assert_eq!(body["model"], "MiniMax-M3");
    assert_eq!(body["input"], "approved query");
    assert_eq!(body["tools"][0]["type"], "web_search");
    assert_eq!(body["stream"], false);
    assert_eq!(body["store"], false);
    assert_eq!(body["tool_choice"], "auto");
    assert_eq!(body["temperature"], 0.1);
    let blob = body.to_string();
    assert!(
        body["instructions"]
            .as_str()
            .is_some_and(|text| text.contains("web_search")),
        "{blob}"
    );
    assert!(!blob.contains("web_search_20250305"), "{blob}");
    assert!(!blob.contains("笔记正文"), "{blob}");
    assert!(!blob.contains("notes/vault/secret.md"), "{blob}");
    assert!(!blob.contains("run-adapter"), "{blob}");
}

#[tokio::test]
async fn minimax_responses_scripted_http_yields_https_hits() {
    let transport = ScriptedTransport::ok(responses_live_shape());
    let outcome = execute_native_search(
        draft("approved query", Some("笔记正文")),
        &transport,
        Some("secret-must-not-be-recorded"),
        "https://api.minimaxi.com/v1",
    )
    .await;
    assert_eq!(
        transport.recorded_url.lock().expect("url").as_deref(),
        Some("https://api.minimaxi.com/v1/responses")
    );
    assert_eq!(*transport.recorded_auth.lock().expect("auth"), Some(true));
    let sent = transport
        .recorded_body
        .lock()
        .expect("body")
        .clone()
        .expect("recorded");
    let sent_blob = sent.to_string();
    assert!(
        !sent_blob.contains("secret-must-not-be-recorded"),
        "{sent_blob}"
    );
    assert!(!sent_blob.contains("笔记正文"), "{sent_blob}");
    assert_eq!(sent["tools"][0]["type"], "web_search");
    assert!(outcome.has_retrieval_credentials);
    assert_eq!(outcome.failure, None);
    assert_eq!(outcome.candidates.len(), 1);
    assert_eq!(
        outcome.candidates[0].url,
        "https://weather.example/shanghai"
    );
    assert_eq!(outcome.candidates[0].title, "example weather");
    assert_eq!(
        outcome.candidates[0].snippet, "example weather",
        "title must fill snippet so discovery-only web_search treats native hits as usable"
    );
}

#[tokio::test]
async fn native_search_attaches_reported_tokens_without_counting_as_network_tool() {
    let mut body = responses_live_shape();
    body["usage"] = json!({ "input_tokens": 11, "output_tokens": 7 });
    let transport = ScriptedTransport::ok(body);
    let drafted = draft("approved query", None);
    let outcome = execute_native_search(
        drafted.clone(),
        &transport,
        Some("secret-must-not-be-recorded"),
        "https://api.minimaxi.com/v1",
    )
    .await;
    assert_eq!(outcome.prompt_tokens, Some(11));
    assert_eq!(outcome.completion_tokens, Some(7));
    let claim = NativeSearchBudgetClaim {
        kind: NativeSearchBudgetKind::ModelAuxiliaryRequest,
        parent_call_id: drafted.identity.parent_call_id,
    };
    assert!(!claim.is_network_tool_dispatch());
}

#[tokio::test]
async fn minimax_text_only_200_retries_once_and_keeps_later_citations() {
    let transport = SequentialTransport::new(vec![
        (200, minimax_completed_text_only_with_tool_echo()),
        (200, responses_live_shape()),
    ]);
    let outcome = execute_native_search(
        draft("approved query", None),
        &transport,
        Some("secret"),
        "https://api.minimaxi.com/v1",
    )
    .await;
    assert_eq!(*transport.recorded_count.lock().expect("count"), 2);
    assert!(
        outcome.has_retrieval_credentials,
        "MiniMax sometimes omits web_search_call on the first 200; retry must keep citations"
    );
    assert_eq!(
        outcome.candidates[0].url,
        "https://weather.example/shanghai"
    );
}

#[tokio::test]
async fn minimax_tools_echo_without_web_search_call_is_not_retrieval() {
    let transport = ScriptedTransport::ok(minimax_completed_text_only_with_tool_echo());
    let outcome = execute_native_search(
        draft("approved query", None),
        &transport,
        Some("secret"),
        "https://api.minimaxi.com/v1",
    )
    .await;
    assert!(!outcome.has_retrieval_credentials);
    assert!(outcome.generated_text_only);
    assert_eq!(
        outcome.failure,
        Some(RouteFailureClass::ProtocolOrResultInsufficient)
    );
}

#[tokio::test]
async fn minimax_anthropic_text_only_http_200_is_protocol_insufficient() {
    let transport = ScriptedTransport::ok(json!({
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": "I don't have access to real-time weather data."}],
        "stop_reason": "end_turn"
    }));
    let outcome = execute_native_search(
        draft("approved query", None),
        &transport,
        Some("secret"),
        "https://api.minimaxi.com/v1",
    )
    .await;
    assert!(!outcome.has_retrieval_credentials);
    assert!(
        !outcome.generated_text_only,
        "an invalid Responses envelope is not retryable text-only output"
    );
    assert_eq!(
        outcome.failure,
        Some(RouteFailureClass::ProtocolOrResultInsufficient)
    );
}

#[tokio::test]
async fn missing_bearer_is_temporary_failure_and_does_not_post() {
    let transport = ScriptedTransport::ok(responses_live_shape());
    let outcome = execute_native_search(
        draft("approved query", None),
        &transport,
        None,
        "https://api.minimaxi.com/v1",
    )
    .await;
    assert!(transport.recorded_url.lock().expect("url").is_none());
    assert_eq!(outcome.failure, Some(RouteFailureClass::TemporaryFailure));
    assert!(!outcome.has_retrieval_credentials);
}

fn draft_for_model(model_id: &str, query: &str) -> NativeSearchSubrequestDraft {
    let mut value = draft(query, None);
    value.endpoint.model_id = model_id.into();
    value
}

#[tokio::test]
async fn minimax_request_uses_canonical_model_id_when_casing_differs() {
    let transport = ScriptedTransport::ok(responses_live_shape());
    let outcome = execute_native_search(
        draft_for_model("minimax-m3", "approved query"),
        &transport,
        Some("secret"),
        "https://api.minimaxi.com/v1",
    )
    .await;
    let sent = transport
        .recorded_body
        .lock()
        .expect("body")
        .clone()
        .expect("recorded");
    assert_eq!(sent["model"], "MiniMax-M3");
    assert!(outcome.has_retrieval_credentials);
}

#[tokio::test]
async fn native_search_does_not_post_when_c13_ledger_is_exhausted() {
    let run_id = "run-native-c13-exhausted";
    let _guard = crate::ai_runtime::model_turn_ledger::BindGuard::new(run_id, 0);
    let transport = ScriptedTransport::ok(responses_live_shape());
    let mut subrequest = draft("approved query", None);
    subrequest.identity.run_id = run_id.into();
    let outcome = execute_native_search(
        subrequest,
        &transport,
        Some("secret"),
        "https://api.minimaxi.com/v1",
    )
    .await;
    assert!(transport.recorded_url.lock().expect("url").is_none());
    assert_eq!(
        outcome.failure,
        Some(RouteFailureClass::ProtocolOrResultInsufficient)
    );
}

#[tokio::test]
async fn native_search_posts_claim_the_c13_ledger() {
    let run_id = "run-native-c13-claim";
    let _guard = crate::ai_runtime::model_turn_ledger::BindGuard::new(run_id, 8);
    let transport = ScriptedTransport::ok(responses_live_shape());
    let mut subrequest = draft("approved query", None);
    subrequest.identity.run_id = run_id.into();
    let outcome = execute_native_search(
        subrequest,
        &transport,
        Some("secret"),
        "https://api.minimaxi.com/v1",
    )
    .await;
    assert!(transport.recorded_url.lock().expect("url").is_some());
    assert!(outcome.has_retrieval_credentials);
    assert_eq!(crate::ai_runtime::model_turn_ledger::used(run_id), 1);
}

#[tokio::test]
async fn http_401_is_temporary_failure_not_unsupported() {
    let transport = ScriptedTransport {
        status: 401,
        body: json!({"error": {"type": "authentication_error"}}),
        transport_failed: false,
        recorded_url: Mutex::new(None),
        recorded_body: Mutex::new(None),
        recorded_auth: Mutex::new(None),
        recorded_headers: Mutex::new(None),
    };
    let outcome = execute_native_search(
        draft("approved query", None),
        &transport,
        Some("secret"),
        "https://api.minimaxi.com/v1",
    )
    .await;
    assert_eq!(outcome.failure, Some(RouteFailureClass::TemporaryFailure));
    assert!(!outcome.has_retrieval_credentials);
}

#[tokio::test]
async fn minimax_responses_error_object_is_protocol_insufficient() {
    let transport = ScriptedTransport::ok(json!({
        "id": "resp-error",
        "object": "response",
        "status": "completed",
        "error": { "type": "server_error", "message": "search backend failed" },
        "output": [{
            "type": "web_search_call",
            "status": "completed"
        }]
    }));
    let outcome = execute_native_search(
        draft("approved query", None),
        &transport,
        Some("secret"),
        "https://api.minimaxi.com/v1",
    )
    .await;
    assert!(!outcome.has_retrieval_credentials);
    assert_eq!(
        outcome.failure,
        Some(RouteFailureClass::ProtocolOrResultInsufficient)
    );
}

#[tokio::test]
async fn minimax_responses_incomplete_without_citations_is_protocol_insufficient() {
    let transport = ScriptedTransport::ok(json!({
        "id": "resp-incomplete",
        "object": "response",
        "status": "incomplete",
        "incomplete_details": { "reason": "max_output_tokens" },
        "output": [{
            "type": "web_search_call",
            "status": "completed"
        }]
    }));
    let outcome = execute_native_search(
        draft("approved query", None),
        &transport,
        Some("secret"),
        "https://api.minimaxi.com/v1",
    )
    .await;
    assert!(!outcome.has_retrieval_credentials);
    assert_eq!(
        outcome.failure,
        Some(RouteFailureClass::ProtocolOrResultInsufficient)
    );
}

#[tokio::test]
async fn minimax_responses_incomplete_does_not_claim_retrieval_success() {
    let mut body = responses_live_shape();
    body["status"] = json!("incomplete");
    body["incomplete_details"] = json!({ "reason": "max_output_tokens" });
    let transport = ScriptedTransport::ok(body);
    let outcome = execute_native_search(
        draft("approved query", None),
        &transport,
        Some("secret"),
        "https://api.minimaxi.com/v1",
    )
    .await;
    assert!(!outcome.has_retrieval_credentials);
    assert_eq!(
        outcome.failure,
        Some(RouteFailureClass::ProtocolOrResultInsufficient)
    );
    assert!(outcome.candidates.is_empty());
}

#[tokio::test]
async fn chat_completions_api_base_still_posts_to_responses() {
    let transport = ScriptedTransport::ok(responses_live_shape());
    let outcome = execute_native_search(
        draft("approved query", None),
        &transport,
        Some("secret"),
        "https://api.minimaxi.com/v1/chat/completions",
    )
    .await;
    assert_eq!(
        transport.recorded_url.lock().expect("url").as_deref(),
        Some("https://api.minimaxi.com/v1/responses")
    );
    assert!(outcome.has_retrieval_credentials);
}

#[tokio::test]
async fn http_429_is_temporary_failure_not_unsupported() {
    let transport = ScriptedTransport {
        status: 429,
        body: json!({"error": {"type": "rate_limit_error"}}),
        transport_failed: false,
        recorded_url: Mutex::new(None),
        recorded_body: Mutex::new(None),
        recorded_auth: Mutex::new(None),
        recorded_headers: Mutex::new(None),
    };
    let outcome = execute_native_search(
        draft("approved query", None),
        &transport,
        Some("secret"),
        "https://api.minimaxi.com/v1",
    )
    .await;
    assert_eq!(outcome.failure, Some(RouteFailureClass::TemporaryFailure));
    assert!(!outcome.has_retrieval_credentials);
}

#[tokio::test]
async fn transport_failed_flag_is_transport_failure() {
    let transport = ScriptedTransport {
        status: 0,
        body: Value::Null,
        transport_failed: true,
        recorded_url: Mutex::new(None),
        recorded_body: Mutex::new(None),
        recorded_auth: Mutex::new(None),
        recorded_headers: Mutex::new(None),
    };
    let outcome = execute_native_search(
        draft("approved query", None),
        &transport,
        Some("secret"),
        "https://api.minimaxi.com/v1",
    )
    .await;
    assert_eq!(
        outcome.failure,
        Some(RouteFailureClass::TransportOrProviderFailure)
    );
}

#[test]
fn web_search_call_deadline_extends_only_when_native_adapter_is_available() {
    use std::time::Duration;

    use super::native_search_adapter::web_search_call_deadline;

    assert_eq!(
        web_search_call_deadline(None),
        Duration::from_secs(20),
        "MCP-only path keeps the existing 20s tool deadline"
    );
    assert_eq!(
        web_search_call_deadline(Some(&minimax_endpoint())),
        Duration::from_secs(90),
        "MiniMax Available must reserve MCP 20s + native 60s + buffer"
    );
    assert_eq!(
        web_search_call_deadline(Some(&NativeSearchEndpointRef::new(
            "deepseek-v4-flash",
            EndpointFamily::OpenAiCompatibleChatCompletions,
        ))),
        Duration::from_secs(90),
        "DeepSeek Available must reserve MCP 20s + native 60s + buffer"
    );
}

#[tokio::test]
async fn http_500_is_transport_failure_not_unsupported() {
    let transport = ScriptedTransport {
        status: 500,
        body: json!({"error": {"type": "server_error"}}),
        transport_failed: false,
        recorded_url: Mutex::new(None),
        recorded_body: Mutex::new(None),
        recorded_auth: Mutex::new(None),
        recorded_headers: Mutex::new(None),
    };
    let outcome = execute_native_search(
        draft("approved query", None),
        &transport,
        Some("secret"),
        "https://api.minimaxi.com/v1",
    )
    .await;
    assert_eq!(
        outcome.failure,
        Some(RouteFailureClass::TransportOrProviderFailure)
    );
    assert!(!outcome.has_retrieval_credentials);
}

#[tokio::test]
async fn non_https_api_base_does_not_post() {
    let transport = ScriptedTransport::ok(responses_live_shape());
    let outcome = execute_native_search(
        draft("approved query", None),
        &transport,
        Some("secret"),
        "http://api.minimaxi.com/v1",
    )
    .await;
    assert!(transport.recorded_url.lock().expect("url").is_none());
    assert_eq!(
        outcome.failure,
        Some(RouteFailureClass::TransportOrProviderFailure)
    );
}

#[test]
fn production_registry_matches_model_id_not_provider_brand() {
    assert_eq!(
        lookup_production_adapter("MiniMax-M3").map(NativeSearchModelAdapter::id),
        Some("MiniMax-M3")
    );
    assert!(lookup_production_adapter("minimax-m3").is_some());
    assert!(lookup_production_adapter("minimax").is_none());
    assert!(lookup_production_adapter("MiniMax-M2").is_none());
    assert!(lookup_production_adapter("deepseek-v4-flash").is_some());
    assert!(lookup_production_adapter("deepseek-flash").is_some());
    assert!(lookup_production_adapter("deepseek").is_none());
    assert!(lookup_production_adapter("deepseek-v4-pro").is_none());
    assert!(lookup_production_adapter("qwen2.5:7b").is_none());
}

#[test]
fn every_production_adapter_id_exists_in_catalog_and_matches_aliases() {
    let mut saw_deepseek = false;
    let mut saw_minimax = false;
    for adapter in production_adapters() {
        let id = adapter.id();
        assert!(
            crate::llm::model_catalog::find_model(id).is_some(),
            "adapter id must exist in catalog: {id}"
        );
        assert!(
            adapter.matches(id),
            "adapter must match its catalog id: {id}"
        );
        assert!(
            adapter.matches(adapter.outbound_model()),
            "adapter must match its outbound model: {} / {}",
            id,
            adapter.outbound_model()
        );
        if id == "deepseek-v4-flash" {
            saw_deepseek = true;
            assert_eq!(adapter.outbound_model(), "deepseek-flash");
        }
        if id == "MiniMax-M3" {
            saw_minimax = true;
            assert_eq!(adapter.outbound_model(), "MiniMax-M3");
        }
    }
    assert!(saw_deepseek, "DeepSeek-Flash adapter must stay registered");
    assert!(saw_minimax, "MiniMax-M3 adapter must stay registered");
    assert!(lookup_production_adapter("deepseek").is_none());
    assert!(lookup_production_adapter("minimax").is_none());
}

struct CatalogMissAdapter;

impl NativeSearchModelAdapter for CatalogMissAdapter {
    fn id(&self) -> &'static str {
        "not-in-catalog"
    }

    fn matches(&self, model_id: &str) -> bool {
        model_id == "not-in-catalog"
    }

    fn request_url(&self, _api_base: &str) -> Result<String, RouteFailureClass> {
        Ok("https://example.invalid/v1".into())
    }

    fn outbound_body(&self, _subrequest: &NativeSearchSubrequest) -> Value {
        json!({})
    }

    fn constrain_output(&self, _body: &mut Value, _max_tokens: u32) {}

    fn parse_response(&self, _body: &Value) -> NativeSearchParse {
        NativeSearchParse {
            candidates: Vec::new(),
            has_retrieval_credentials: false,
            generated_text_only: true,
            failure: None,
            event_kinds: Vec::new(),
        }
    }
}

#[test]
fn catalog_miss_is_protocol_insufficient_not_temporary() {
    let mut endpoint = NativeSearchEndpointRef::new(
        "not-in-catalog",
        EndpointFamily::OpenAiCompatibleChatCompletions,
    );
    endpoint.api_base = Some("https://example.invalid/v1".into());
    endpoint.credential_service = Some("iris.llm.example".into());
    assert_eq!(
        bind_production_route(&CatalogMissAdapter, &endpoint),
        Err(RouteFailureClass::ProtocolOrResultInsufficient)
    );
}

#[test]
fn https_api_base_strips_known_suffixes_once() {
    assert_eq!(
        https_api_base_without_suffixes(
            "https://api.example.com/v1/chat/completions",
            &["/chat/completions", "/responses", "/messages"],
        ),
        Ok("https://api.example.com/v1".into())
    );
    assert_eq!(
        https_api_base_without_suffixes("https://api.example.com/v1/", &["/chat/completions"]),
        Ok("https://api.example.com/v1".into())
    );
    assert_eq!(
        https_api_base_without_suffixes(
            "https://api.example.com/v1/responses",
            &["/chat/completions", "/responses", "/messages"],
        ),
        Ok("https://api.example.com/v1".into())
    );
    assert_eq!(
        https_api_base_without_suffixes("http://api.example.com/v1", &["/responses"]),
        Err(RouteFailureClass::TransportOrProviderFailure)
    );
    assert_eq!(
        https_api_base_without_suffixes(
            "https://api.example.com/anthropic/v1/messages",
            &["/messages", "/v1/messages"],
        ),
        Ok("https://api.example.com/anthropic".into())
    );
}

#[test]
fn bind_production_route_fills_catalog_base_when_endpoint_omits_it() {
    let minimax = lookup_production_adapter("MiniMax-M3").expect("registered");
    let binding = bind_production_route(minimax, &minimax_endpoint()).expect("catalog hit");
    assert_eq!(binding.api_base, "https://api.minimaxi.com/v1");
    assert_eq!(binding.credential_service, "iris.llm.minimax");

    let deepseek = lookup_production_adapter("deepseek-flash").expect("registered");
    let binding = bind_production_route(deepseek, &deepseek_endpoint()).expect("catalog hit");
    assert_eq!(binding.api_base, "https://api.deepseek.com");
    assert_eq!(binding.credential_service, "iris.llm.deepseek");

    let mut overridden = minimax_endpoint();
    overridden.api_base = Some("https://override.example.com/v1".into());
    overridden.credential_service = Some("iris.llm.override".into());
    let binding = bind_production_route(minimax, &overridden).expect("endpoint wins");
    assert_eq!(binding.api_base, "https://override.example.com/v1");
    assert_eq!(binding.credential_service, "iris.llm.override");
}

#[test]
fn minimax_m3_production_adapter_is_available_without_inferring_other_models() {
    assert_eq!(production_native_search_adapter_count(), 2);
    let probe = ProductionNativeSearchSupport {
        endpoint: Some(minimax_endpoint()),
    };
    assert_eq!(
        probe.native_search_support(),
        NativeSearchSupport::Available
    );
    assert_eq!(probe.native_search_unsupported_reason(), None);

    let qwen = ProductionNativeSearchSupport {
        endpoint: Some(NativeSearchEndpointRef::new(
            "qwen2.5:7b",
            EndpointFamily::OpenAiCompatibleChatCompletions,
        )),
    };
    assert_eq!(
        qwen.native_search_support(),
        NativeSearchSupport::Unsupported
    );
    assert_eq!(
        qwen.native_search_unsupported_reason(),
        Some(NativeSearchUnsupportedReason::AdapterAbsent)
    );
}

#[tokio::test]
async fn minimax_m3_production_probe_attempts_native_and_mcp() {
    let native = CountingNativeRoute;
    let outcome = coordinate_dual_path_search(
        DualPathSearchRequest {
            identity: SearchActionIdentity {
                run_id: "run-minimax".into(),
                input_revision: "1".into(),
                action_id: "web_search".into(),
                attempt: 1,
                ..Default::default()
            },
            query: "approved query".into(),
            allow_second_route: true,
        },
        &ProductionNativeSearchSupport {
            endpoint: Some(minimax_endpoint()),
        },
        &native,
        &CountingMcpRoute,
    )
    .await;
    assert!(outcome.native.supported);
    assert!(outcome.native.attempted);
    assert!(outcome.native.succeeded);
    assert!(outcome.mcp.succeeded);
    assert!(outcome.both_available_routes_attempted());
}

fn deepseek_endpoint() -> NativeSearchEndpointRef {
    NativeSearchEndpointRef::new(
        "deepseek-v4-flash",
        EndpointFamily::OpenAiCompatibleChatCompletions,
    )
}

fn deepseek_draft(query: &str, private_material: Option<&str>) -> NativeSearchSubrequestDraft {
    NativeSearchSubrequestDraft {
        query: query.into(),
        public_scope: NativeSearchPublicScope {
            date: Some("2026-09".into()),
            region: Some("CN".into()),
            language: Some("zh".into()),
        },
        identity: NativeSearchRequestIdentity {
            run_id: uuid::Uuid::new_v4().to_string(),
            input_revision: "4".into(),
            parent_call_id: "web_search".into(),
            attempt: 1,
            ..Default::default()
        },
        endpoint: deepseek_endpoint(),
        private_material: private_material.map(str::to_string),
    }
}

fn deepseek_anthropic_live_shape() -> Value {
    json!({
        "id": "msg-scripted",
        "type": "message",
        "role": "assistant",
        "model": "deepseek-flash",
        "stop_reason": "end_turn",
        "content": [
            {
                "type": "server_tool_use",
                    "id": "fixture-search-call",
                "name": "web_search",
                "input": { "query": "approved query" }
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
            { "type": "text", "text": "summary" }
        ]
    })
}

fn deepseek_anthropic_text_only() -> Value {
    json!({
        "id": "msg-no-search",
        "type": "message",
        "role": "assistant",
        "model": "deepseek-flash",
        "stop_reason": "end_turn",
        "content": [{ "type": "text", "text": "I don't have access to real-time weather data." }]
    })
}

struct CountingNativeRoute;

impl SearchRoute for CountingNativeRoute {
    async fn execute(&self, _query: &str) -> super::dual_path_search::RouteAttemptOutcome {
        super::dual_path_search::RouteAttemptOutcome {
            candidates: vec![SearchHit {
                url: "https://weather.example/shanghai".into(),
                title: "example weather".into(),
                snippet: String::new(),
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

#[test]
fn deepseek_flash_outbound_uses_anthropic_hosted_search_not_minimax_responses() {
    let sub = construct_native_search_subrequest(deepseek_draft(
        "approved query",
        Some("这是笔记正文\npath: notes/vault/secret.md"),
    ))
    .expect("approved query must construct");
    let adapter = lookup_production_adapter("deepseek-v4-flash").expect("DeepSeek adapter");
    assert_eq!(adapter.id(), "deepseek-v4-flash");
    let body = adapter.outbound_body(&sub);
    assert_eq!(body["model"], "deepseek-flash");
    assert_eq!(body["messages"][0]["content"], "approved query");
    assert_eq!(body["tools"][0]["type"], "web_search_20250305");
    assert_eq!(body["tools"][0]["name"], "web_search");
    assert_eq!(body["tool_choice"]["type"], "tool");
    assert_eq!(body["thinking"]["type"], "disabled");
    assert_eq!(body["max_tokens"], 2048);
    assert!(body.get("input").is_none(), "{body}");
    assert!(body.get("store").is_none(), "{body}");
    assert!(body.get("max_output_tokens").is_none(), "{body}");
    assert_ne!(body["tools"][0]["type"], "web_search");
    let blob = body.to_string();
    assert!(!blob.contains("笔记正文"), "{blob}");
    assert!(!blob.contains("notes/vault/secret.md"), "{blob}");
    assert!(!blob.contains("run-adapter"), "{blob}");
    let headers = adapter.http_headers("secret-must-not-appear-in-body");
    assert!(headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("x-api-key")));
    assert!(!headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("authorization")));
}

#[tokio::test]
async fn deepseek_flash_scripted_http_posts_anthropic_messages() {
    let transport = ScriptedTransport::ok(deepseek_anthropic_live_shape());
    let outcome = execute_native_search(
        deepseek_draft("approved query", Some("笔记正文")),
        &transport,
        Some("secret-must-not-be-recorded"),
        "https://api.deepseek.com",
    )
    .await;
    assert_eq!(
        transport.recorded_url.lock().expect("url").as_deref(),
        Some("https://api.deepseek.com/anthropic/v1/messages")
    );
    let headers = transport
        .recorded_headers
        .lock()
        .expect("headers")
        .clone()
        .expect("recorded");
    assert!(headers
        .iter()
        .any(|(name, value)| name.eq_ignore_ascii_case("x-api-key") && !value.is_empty()));
    assert!(!headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("authorization")));
    let sent = transport
        .recorded_body
        .lock()
        .expect("body")
        .clone()
        .expect("recorded");
    let sent_blob = sent.to_string();
    assert!(
        !sent_blob.contains("secret-must-not-be-recorded"),
        "{sent_blob}"
    );
    assert!(!sent_blob.contains("笔记正文"), "{sent_blob}");
    assert_eq!(sent["tools"][0]["type"], "web_search_20250305");
    assert!(outcome.has_retrieval_credentials);
    assert_eq!(outcome.failure, None);
    assert_eq!(outcome.candidates.len(), 1);
    assert_eq!(
        outcome.candidates[0].url,
        "https://weather.example/shanghai"
    );
}

#[tokio::test]
async fn deepseek_chat_completions_api_base_still_posts_anthropic_messages() {
    let transport = ScriptedTransport::ok(deepseek_anthropic_live_shape());
    let outcome = execute_native_search(
        deepseek_draft("approved query", None),
        &transport,
        Some("secret"),
        "https://api.deepseek.com/v1/chat/completions",
    )
    .await;
    assert_eq!(
        transport.recorded_url.lock().expect("url").as_deref(),
        Some("https://api.deepseek.com/anthropic/v1/messages")
    );
    assert!(outcome.has_retrieval_credentials);
}

#[tokio::test]
async fn deepseek_text_only_200_retries_once_and_keeps_later_results() {
    let transport = SequentialTransport::new(vec![
        (200, deepseek_anthropic_text_only()),
        (200, deepseek_anthropic_live_shape()),
    ]);
    let outcome = execute_native_search(
        deepseek_draft("approved query", None),
        &transport,
        Some("secret"),
        "https://api.deepseek.com",
    )
    .await;
    assert_eq!(*transport.recorded_count.lock().expect("count"), 2);
    assert!(outcome.has_retrieval_credentials);
    assert_eq!(
        outcome.candidates[0].url,
        "https://weather.example/shanghai"
    );
}

#[tokio::test]
async fn deepseek_responses_tools_echo_is_not_used_as_native_search() {
    let transport = ScriptedTransport::ok(json!({
        "status": "completed",
        "error": null,
        "tools": [{ "type": "web_search" }],
        "output": [{
            "type": "message",
            "content": [{ "type": "output_text", "text": "memory answer" }]
        }]
    }));
    let outcome = execute_native_search(
        deepseek_draft("approved query", None),
        &transport,
        Some("secret"),
        "https://api.deepseek.com",
    )
    .await;
    assert_eq!(
        transport.recorded_url.lock().expect("url").as_deref(),
        Some("https://api.deepseek.com/anthropic/v1/messages")
    );
    assert!(!outcome.has_retrieval_credentials);
}

#[tokio::test]
async fn deepseek_flash_production_probe_attempts_native_and_mcp() {
    let native = CountingNativeRoute;
    let outcome = coordinate_dual_path_search(
        DualPathSearchRequest {
            identity: SearchActionIdentity {
                run_id: "run-deepseek".into(),
                input_revision: "1".into(),
                action_id: "web_search".into(),
                attempt: 1,
                ..Default::default()
            },
            query: "approved query".into(),
            allow_second_route: true,
        },
        &ProductionNativeSearchSupport {
            endpoint: Some(deepseek_endpoint()),
        },
        &native,
        &CountingMcpRoute,
    )
    .await;
    assert!(outcome.native.supported);
    assert!(outcome.native.attempted);
    assert!(outcome.native.succeeded);
    assert!(outcome.mcp.succeeded);
    assert!(outcome.both_available_routes_attempted());
}

#[tokio::test]
#[ignore = "live MiniMax native search; requires local iris.llm.minimax"]
async fn live_minimax_m3_adapter_returns_https_citations() {
    let home = std::env::var("HOME").expect("HOME");
    std::env::set_var(
        "IRIS_CONFIG_DIR",
        format!("{home}/Library/Application Support/Iris/config"),
    );
    std::env::set_var(
        "IRIS_DATA_DIR",
        format!("{home}/Library/Application Support/com.iris.notes/app-data"),
    );
    let mut endpoint = minimax_endpoint();
    endpoint.api_base = Some("https://api.minimaxi.com/v1".into());
    endpoint.credential_service = Some("iris.llm.minimax".into());
    let outcome = execute_configured_adapter_probe(
        Some(&endpoint),
        &SearchActionIdentity {
            run_id: "live-minimax-adapter".into(),
            input_revision: "1".into(),
            action_id: "web_search".into(),
            attempt: 1,
            ..Default::default()
        },
        "What is the weather in Shanghai?",
    )
    .await;
    assert!(
        outcome.has_retrieval_credentials,
        "native MiniMax search must return retrieval credentials: {outcome:?}; {}",
        super::native_search_adapter::last_live_native_search_trace()
    );
    assert!(outcome
        .candidates
        .iter()
        .all(|hit| hit.url.starts_with("https://")));
    assert!(!outcome.candidates.is_empty());
    let mut hosts: Vec<String> = outcome
        .candidates
        .iter()
        .filter_map(|hit| {
            hit.url
                .strip_prefix("https://")
                .map(|rest| rest.split('/').next().unwrap_or(rest).to_string())
        })
        .collect();
    hosts.sort();
    hosts.dedup();
    eprintln!(
        "live_minimax_trace {} has_retrieval_credentials={} candidate_count={} prompt_tokens={:?} completion_tokens={:?} hosts={hosts:?}",
        super::native_search_adapter::last_live_native_search_trace(),
        outcome.has_retrieval_credentials,
        outcome.candidates.len(),
        outcome.prompt_tokens,
        outcome.completion_tokens,
    );
}

fn install_deepseek_live_dirs() {
    let home = std::env::var("HOME").expect("HOME");
    std::env::set_var(
        "IRIS_CONFIG_DIR",
        format!("{home}/Library/Application Support/Iris/config"),
    );
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root");
    let candidates = [
        repo_root.join(".iris-dev/app-data"),
        std::path::PathBuf::from(format!(
            "{home}/Library/Application Support/com.iris.notes/app-data"
        )),
    ];
    for data_dir in candidates {
        std::env::set_var("IRIS_DATA_DIR", &data_dir);
        if crate::credentials::credential_available("iris.llm.deepseek").unwrap_or(false) {
            return;
        }
    }
}

fn summarize_deepseek_probe(
    name: &str,
    url: &str,
    status: u16,
    elapsed_ms: u128,
    raw_len: usize,
    body: &Value,
) -> String {
    let mut types = Vec::new();
    let mut https_citations = 0_u32;
    let mut flags = (false, false, false);
    fn walk(
        value: &Value,
        types: &mut Vec<String>,
        https_citations: &mut u32,
        flags: &mut (bool, bool, bool),
    ) {
        if let Some(kind) = value.get("type").and_then(Value::as_str) {
            match kind {
                "web_search_call" => flags.0 = true,
                "server_tool_use" => flags.1 = true,
                "web_search_tool_result" => flags.2 = true,
                _ => {}
            }
            if !types.iter().any(|existing| existing == kind) {
                types.push(kind.to_string());
            }
            if (kind == "url_citation" || kind == "web_search_result")
                && value
                    .get("url")
                    .and_then(Value::as_str)
                    .is_some_and(|url| url.starts_with("https://"))
            {
                *https_citations += 1;
            }
        }
        match value {
            Value::Array(items) => {
                for item in items {
                    walk(item, types, https_citations, flags);
                }
            }
            Value::Object(object) => {
                for child in object.values() {
                    walk(child, types, https_citations, flags);
                }
            }
            _ => {}
        }
    }
    walk(body, &mut types, &mut https_citations, &mut flags);
    let keys = body
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>().join(","))
        .unwrap_or_else(|| "-".into());
    let error_code = body
        .get("error")
        .and_then(|error| {
            error
                .get("code")
                .or_else(|| error.get("type"))
                .or_else(|| error.get("param"))
        })
        .and_then(Value::as_str)
        .unwrap_or("-");
    format!(
        "{name} host_path={} status={status} ms={elapsed_ms} raw_len={raw_len} keys={keys} output_status={} error={} error_code={error_code} types={} web_search_call={} server_tool_use={} web_search_tool_result={} https_citations={https_citations} usage_in={} usage_out={}",
        url.trim_start_matches("https://api.deepseek.com"),
        body.get("status")
            .or_else(|| body.get("stop_reason"))
            .or_else(|| body.get("finish_reason"))
            .and_then(Value::as_str)
            .unwrap_or("-"),
        match body.get("error") {
            None => "absent",
            Some(Value::Null) => "null",
            Some(_) => "present",
        },
        types.join(","),
        flags.0,
        flags.1,
        flags.2,
        body.get("usage")
            .and_then(|usage| usage
                .get("input_tokens")
                .or_else(|| usage.get("prompt_tokens")))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        body.get("usage")
            .and_then(|usage| usage
                .get("output_tokens")
                .or_else(|| usage.get("completion_tokens")))
            .and_then(Value::as_u64)
            .unwrap_or(0),
    )
}

#[tokio::test]
#[ignore = "live DeepSeek protocol probe; requires local iris.llm.deepseek"]
async fn live_deepseek_flash_native_search_protocol_probe() {
    install_deepseek_live_dirs();
    let secret = crate::credentials::get_runtime_secret("iris.llm.deepseek")
        .expect("iris.llm.deepseek must decrypt");
    let client = crate::network::cert_pinning::https_client_builder()
        .timeout(std::time::Duration::from_secs(90))
        .read_timeout(std::time::Duration::from_secs(90))
        .build()
        .expect("https client");
    let query = "What is the weather in Shanghai?";
    let responses_auto = json!({
        "model": "deepseek-flash",
        "input": query,
        "instructions": "This is an isolated web search subrequest. Use web_search and return URL citations. Do not answer from memory.",
        "stream": false,
        "max_output_tokens": 2048,
        "reasoning": { "effort": "none" },
        "tools": [{ "type": "web_search" }],
        "tool_choice": "auto",
    });
    let cases = [
        (
            "responses_root_auto",
            "https://api.deepseek.com/responses",
            responses_auto.clone(),
            false,
        ),
        (
            "responses_root_forced",
            "https://api.deepseek.com/responses",
            json!({
                "model": "deepseek-flash",
                "input": query,
                "stream": false,
                "max_output_tokens": 2048,
                "reasoning": { "effort": "none" },
                "tools": [{ "type": "web_search" }],
                "tool_choice": { "type": "web_search" },
            }),
            false,
        ),
        (
            "responses_v1_auto",
            "https://api.deepseek.com/v1/responses",
            responses_auto,
            false,
        ),
        (
            "chat_v1_web_search_tool",
            "https://api.deepseek.com/v1/chat/completions",
            json!({
                "model": "deepseek-flash",
                "messages": [{ "role": "user", "content": query }],
                "stream": false,
                "thinking": { "type": "disabled" },
                "tools": [{ "type": "web_search" }],
            }),
            false,
        ),
        (
            "anthropic_web_search_20250305",
            "https://api.deepseek.com/anthropic/v1/messages",
            json!({
                "model": "deepseek-flash",
                "max_tokens": 2048,
                "thinking": { "type": "disabled" },
                "messages": [{ "role": "user", "content": query }],
                "tools": [{ "type": "web_search_20250305", "name": "web_search" }],
            }),
            true,
        ),
        (
            "responses_root_required",
            "https://api.deepseek.com/responses",
            json!({
                "model": "deepseek-flash",
                "input": query,
                "stream": false,
                "max_output_tokens": 2048,
                "reasoning": { "effort": "none" },
                "tools": [{ "type": "web_search" }],
                "tool_choice": "required",
            }),
            false,
        ),
        (
            "responses_root_web_search_2025_08_26",
            "https://api.deepseek.com/responses",
            json!({
                "model": "deepseek-flash",
                "input": query,
                "stream": false,
                "max_output_tokens": 2048,
                "reasoning": { "effort": "none" },
                "tools": [{ "type": "web_search_2025_08_26" }],
                "tool_choice": { "type": "web_search_2025_08_26" },
            }),
            false,
        ),
        (
            "anthropic_type_web_search",
            "https://api.deepseek.com/anthropic/v1/messages",
            json!({
                "model": "deepseek-flash",
                "max_tokens": 2048,
                "thinking": { "type": "disabled" },
                "messages": [{ "role": "user", "content": query }],
                "tools": [{ "type": "web_search", "name": "web_search" }],
            }),
            true,
        ),
    ];
    let mut lines = Vec::new();
    for (name, url, body, anthropic) in cases {
        let started = std::time::Instant::now();
        let mut request = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body);
        request = if anthropic {
            request
                .header("x-api-key", secret.as_str())
                .header("anthropic-version", "2023-06-01")
        } else {
            request.header("Authorization", format!("Bearer {}", secret.as_str()))
        };
        let response = request.send().await;
        let elapsed_ms = started.elapsed().as_millis();
        let line = match response {
            Err(_) => format!("{name} transport_failed ms={elapsed_ms}"),
            Ok(response) => {
                let status = response.status().as_u16();
                let raw = response.text().await.unwrap_or_default();
                let parsed = serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null);
                summarize_deepseek_probe(name, url, status, elapsed_ms, raw.len(), &parsed)
            }
        };
        eprintln!("{line}");
        lines.push(line);
    }
    assert!(
        lines.iter().any(|line| line.contains("status=")),
        "DeepSeek probe produced no HTTP statuses: {lines:?}"
    );
}

#[tokio::test]
#[ignore = "live DeepSeek native search; requires local iris.llm.deepseek"]
async fn live_deepseek_flash_adapter_returns_https_citations() {
    install_deepseek_live_dirs();
    let mut endpoint = deepseek_endpoint();
    endpoint.api_base = Some("https://api.deepseek.com".into());
    endpoint.credential_service = Some("iris.llm.deepseek".into());
    let outcome = execute_configured_adapter_probe(
        Some(&endpoint),
        &SearchActionIdentity {
            run_id: "live-deepseek-adapter".into(),
            input_revision: "1".into(),
            action_id: "web_search".into(),
            attempt: 1,
            ..Default::default()
        },
        "What is the weather in Shanghai?",
    )
    .await;
    assert!(
        outcome.has_retrieval_credentials,
        "native DeepSeek search must return retrieval credentials: {outcome:?}; {}",
        super::native_search_adapter::last_live_native_search_trace()
    );
    assert!(outcome
        .candidates
        .iter()
        .all(|hit| hit.url.starts_with("https://")));
    assert!(!outcome.candidates.is_empty());
    let mut hosts: Vec<String> = outcome
        .candidates
        .iter()
        .filter_map(|hit| {
            hit.url
                .strip_prefix("https://")
                .map(|rest| rest.split('/').next().unwrap_or(rest).to_string())
        })
        .collect();
    hosts.sort();
    hosts.dedup();
    eprintln!(
        "live_deepseek_trace {} has_retrieval_credentials={} candidate_count={} prompt_tokens={:?} completion_tokens={:?} hosts={hosts:?}",
        super::native_search_adapter::last_live_native_search_trace(),
        outcome.has_retrieval_credentials,
        outcome.candidates.len(),
        outcome.prompt_tokens,
        outcome.completion_tokens,
    );
}
