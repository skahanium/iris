//! TDD coverage for the MiniMax-M3 Responses native-search adapter.

use super::dual_path_search::{
    coordinate_dual_path_search, DualPathSearchRequest, NativeSearchSupport,
    NativeSearchSupportProbe, ProductionNativeSearchSupport, RouteFailureClass,
    SearchActionIdentity, SearchHit, SearchRoute,
};
use super::native_search_adapter::{
    execute_native_search, execute_production_route, lookup_production_adapter,
    NativeSearchHttpResult, NativeSearchModelAdapter, NativeSearchTransport,
};
use super::native_search_subrequest::{
    construct_native_search_subrequest, production_native_search_adapter_count,
    NativeSearchEndpointRef, NativeSearchPublicScope, NativeSearchRequestIdentity,
    NativeSearchSubrequestDraft, NativeSearchUnsupportedReason,
};
use crate::ai_types::EndpointFamily;
use serde_json::{json, Value};
use std::sync::Mutex;

fn minimax_endpoint() -> NativeSearchEndpointRef {
    NativeSearchEndpointRef::new(
        "MiniMax-M3",
        EndpointFamily::OpenAiCompatibleChatCompletions,
    )
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
            run_id: "run-adapter".into(),
            input_revision: 4,
            parent_call_id: "web_search".into(),
            attempt: 1,
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

struct ScriptedTransport {
    status: u16,
    body: Value,
    transport_failed: bool,
    recorded_url: Mutex<Option<String>>,
    recorded_body: Mutex<Option<Value>>,
    recorded_auth: Mutex<Option<bool>>,
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
        }
    }
}

impl NativeSearchTransport for ScriptedTransport {
    async fn post_json(
        &self,
        url: &str,
        body: &Value,
        bearer: Option<&str>,
    ) -> NativeSearchHttpResult {
        *self.recorded_url.lock().expect("url") = Some(url.to_string());
        *self.recorded_body.lock().expect("body") = Some(body.clone());
        *self.recorded_auth.lock().expect("auth") = Some(bearer.is_some());
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
        _bearer: Option<&str>,
    ) -> NativeSearchHttpResult {
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
    assert!(outcome.generated_text_only);
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
async fn http_401_is_temporary_failure_not_unsupported() {
    let transport = ScriptedTransport {
        status: 401,
        body: json!({"error": {"type": "authentication_error"}}),
        transport_failed: false,
        recorded_url: Mutex::new(None),
        recorded_body: Mutex::new(None),
        recorded_auth: Mutex::new(None),
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
async fn minimax_responses_incomplete_keeps_https_citations() {
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
    assert!(
        outcome.has_retrieval_credentials,
        "truncated summary must not drop already-returned HTTPS citations: {outcome:?}"
    );
    assert_eq!(outcome.failure, None);
    assert_eq!(
        outcome.candidates[0].url,
        "https://weather.example/shanghai"
    );
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
        Duration::from_secs(20)
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
    assert!(lookup_production_adapter("deepseek-v4-flash").is_none());
    assert!(lookup_production_adapter("qwen2.5:7b").is_none());
}

#[test]
fn minimax_m3_production_adapter_is_available_without_inferring_other_models() {
    assert_eq!(production_native_search_adapter_count(), 1);
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
                input_revision: 1,
                action_id: "web_search".into(),
                attempt: 1,
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
            internal_provider_attempts: 0,
        }
    }
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
    let outcome = execute_production_route(
        Some(&endpoint),
        &SearchActionIdentity {
            run_id: "live-minimax-adapter".into(),
            input_revision: 1,
            action_id: "web_search".into(),
            attempt: 1,
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
}
