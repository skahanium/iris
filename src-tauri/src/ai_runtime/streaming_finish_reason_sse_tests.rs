//! Byte-level Chat Completions SSE finish_reason contract.
//!
//! Kept out of `streaming.rs` so the protocol double does not grow that file.

use super::agent_capacity_eval::{spawn_llm_protocol_double, HttpResponseScript};
use super::model_gateway::{
    GatewayRequest, LlmFunctionDef, LlmMessage, LlmToolDef, MessageRole, ModelGateway, StreamEvent,
    StreamEventObserver, StreamEventType,
};
use super::native_search_subrequest::{
    parse_native_search_payload, NativeSearchPayloadFamily, RetrievalOrigin,
};
use crate::ai_types::{EndpointFamily, ProviderConfig, ResolvedReasoningRequest};
use crate::error::AppResult;

struct NoopObserver;

impl StreamEventObserver for NoopObserver {
    fn observe(&mut self, _event: &StreamEvent, _token_index: u32) -> AppResult<()> {
        Ok(())
    }
}

fn provider(base_url: &str) -> ProviderConfig {
    ProviderConfig {
        name: "contract-provider".into(),
        base_url: base_url.into(),
        api_key: None,
        model: "contract-model".into(),
        endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
    }
}

fn request(provider: ProviderConfig) -> GatewayRequest {
    GatewayRequest {
        provider,
        messages: vec![LlmMessage {
            role: MessageRole::User,
            content: "protocol probe".into(),
            tool_call_id: None,
            tool_calls: None,
            ..Default::default()
        }],
        tools: vec![LlmToolDef {
            tool_type: "function".into(),
            function: LlmFunctionDef {
                name: "web_search".into(),
                description: "contract search".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {"query": {"type": "string"}},
                    "required": ["query"]
                }),
            },
        }],
        max_tokens: Some(128),
        input_token_budget: None,
        temperature: Some(0.0),
        stream: true,
        thinking: false,
        reasoning: ResolvedReasoningRequest::disabled(),
        continuation: None,
        skip_stub_ids: Vec::new(),
        boundary: None,
    }
}

#[tokio::test]
async fn chat_completions_sse_preserves_length_finish_reason() {
    let double = spawn_llm_protocol_double(vec![HttpResponseScript::sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"截断前的正文\"}}]}\n\n\
         data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\n\
         data: [DONE]\n\n",
    )])
    .await
    .expect("protocol double");
    let gateway = ModelGateway::new(reqwest::Client::new(), Vec::new());
    let mut observer = NoopObserver;

    let response = gateway
        .send_streaming_request_to_observer(
            "run-q04-sse-length",
            request(provider(&double.base_url)),
            &mut observer,
        )
        .await
        .expect("streamed chat completions response");
    let _ = double.finish().await;

    assert_eq!(response.finish_reason, "length");
    assert_eq!(response.content.as_deref(), Some("截断前的正文"));
    assert!(response.tool_calls.is_empty());
}

#[tokio::test]
async fn chat_completions_sse_missing_finish_reason_is_unknown_not_stop() {
    let double = spawn_llm_protocol_double(vec![HttpResponseScript::sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"没有终止块\"}}]}\n\n\
         data: [DONE]\n\n",
    )])
    .await
    .expect("protocol double");
    let gateway = ModelGateway::new(reqwest::Client::new(), Vec::new());
    let mut observer = NoopObserver;

    let response = gateway
        .send_streaming_request_to_observer(
            "run-q04-sse-unknown",
            request(provider(&double.base_url)),
            &mut observer,
        )
        .await
        .expect("streamed chat completions response");
    let _ = double.finish().await;

    assert_eq!(response.finish_reason, "unknown");
    assert_ne!(response.finish_reason, "stop");
    assert_eq!(response.content.as_deref(), Some("没有终止块"));
}

struct RecordingObserver {
    events: Vec<StreamEventType>,
}

impl StreamEventObserver for RecordingObserver {
    fn observe(&mut self, event: &StreamEvent, _token_index: u32) -> AppResult<()> {
        self.events.push(event.event_type.clone());
        Ok(())
    }
}

#[tokio::test]
async fn chat_completions_sse_web_search_call_does_not_yield_executable_tool_calls() {
    let payload = search_shaped_chat_completions_payload();
    let sse = format!("data: {payload}\n\ndata: [DONE]\n\n");
    let double = spawn_llm_protocol_double(vec![HttpResponseScript::sse(&sse)])
        .await
        .expect("protocol double");
    let gateway = ModelGateway::new(reqwest::Client::new(), Vec::new());
    let mut observer = RecordingObserver { events: Vec::new() };

    let response = gateway
        .send_streaming_request_to_observer(
            "run-g03-sse-search-not-tool-call",
            request(provider(&double.base_url)),
            &mut observer,
        )
        .await
        .expect("search-shaped SSE must not fail the Chat Completions parser");
    let _ = double.finish().await;

    assert!(
        !observer
            .events
            .iter()
            .any(|event_type| matches!(event_type, StreamEventType::Error)),
        "search-shaped SSE must not emit Error: {:?}",
        observer.events
    );
    assert!(response.tool_calls.is_empty());
    assert_eq!(response.content.as_deref(), Some("ok"));
    assert_eq!(response.finish_reason, "stop");
}

#[tokio::test]
async fn chat_completions_sse_web_search_call_mints_main_stream_leak_credentials() {
    let payload = search_shaped_chat_completions_payload();
    let sse = format!("data: {payload}\n\ndata: [DONE]\n\n");
    let double = spawn_llm_protocol_double(vec![HttpResponseScript::sse(&sse)])
        .await
        .expect("protocol double");
    let gateway = ModelGateway::new(reqwest::Client::new(), Vec::new());
    let mut observer = RecordingObserver { events: Vec::new() };

    let response = gateway
        .send_streaming_request_to_observer(
            "run-g03-sse-search-credentials",
            request(provider(&double.base_url)),
            &mut observer,
        )
        .await
        .expect("search-shaped SSE must not fail the Chat Completions parser");
    let _ = double.finish().await;

    let parsed = parse_native_search_payload(NativeSearchPayloadFamily::GeminiShaped, &payload);
    assert!(
        parsed.has_retrieval_credentials,
        "the fixture parser still recognizes grounding"
    );
    let observation = response
        .retrieval_observation
        .expect("streaming path must surface leaked search credentials");
    assert_eq!(observation.origin, RetrievalOrigin::MainStreamLeak);
    assert!(observation.identity.run_id.is_empty());
    assert_ne!(observation.origin, RetrievalOrigin::IsolatedSubrequest);
    assert!(observation.has_retrieval_credentials);
    assert!(
        observation
            .candidates
            .iter()
            .any(|hit| hit.url == "https://example.com/from-sse"),
        "HTTPS grounding URI must be retained: {:?}",
        observation.candidates
    );
    assert!(response.tool_calls.is_empty());
}

fn search_shaped_chat_completions_payload() -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "delta": { "content": "ok" },
            "finish_reason": "stop"
        }],
        "output": [{
            "type": "web_search_call",
            "status": "completed"
        }],
        "groundingMetadata": {
            "groundingChunks": [{
                "web": { "uri": "https://example.com/from-sse", "title": "SSE hit" }
            }]
        }
    })
}
