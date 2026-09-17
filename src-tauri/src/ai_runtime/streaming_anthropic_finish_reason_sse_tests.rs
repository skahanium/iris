//! Byte-level Anthropic Messages SSE stop_reason contract.
//!
//! Kept out of `streaming.rs` so the protocol double does not grow that file.
//! Chat Completions coverage stays in `streaming_finish_reason_sse_tests.rs`.

use super::agent_capacity_eval::{spawn_llm_protocol_double, HttpResponseScript};
use super::model_gateway::{
    GatewayRequest, LlmFunctionDef, LlmMessage, LlmToolDef, MessageRole, ModelGateway, StreamEvent,
    StreamEventObserver,
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
        endpoint_family: EndpointFamily::AnthropicMessages,
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

async fn stream_anthropic(run_id: &str, body: &str) -> super::model_gateway::GatewayResponse {
    let double = spawn_llm_protocol_double(vec![HttpResponseScript::sse(body)])
        .await
        .expect("protocol double");
    let gateway = ModelGateway::new(reqwest::Client::new(), Vec::new());
    let mut observer = NoopObserver;
    let response = gateway
        .send_streaming_request_to_observer(
            run_id,
            request(provider(&double.base_url)),
            &mut observer,
        )
        .await
        .expect("streamed anthropic messages response");
    let _ = double.finish().await;
    response
}

#[tokio::test]
async fn anthropic_sse_end_turn_after_message_stop_stays_end_turn() {
    let response = stream_anthropic(
        "run-c11-anthropic-sse-end-turn",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"完整回答。\"}}\n\n\
         data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":4}}\n\n\
         data: {\"type\":\"message_stop\"}\n\n",
    )
    .await;

    assert_eq!(response.finish_reason, "end_turn");
    assert_eq!(response.content.as_deref(), Some("完整回答。"));
    assert!(response.tool_calls.is_empty());
}

#[tokio::test]
async fn anthropic_sse_preserves_max_tokens_and_clears_truncated_tools() {
    let response = stream_anthropic(
        "run-c11-anthropic-sse-max-tokens",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_truncated\",\"name\":\"web_search\",\"input\":{}}}\n\n\
         data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"query\\\":\\\"status\\\"}\"}}\n\n\
         data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"max_tokens\"},\"usage\":{\"output_tokens\":20}}\n\n\
         data: {\"type\":\"message_stop\"}\n\n",
    )
    .await;

    assert_eq!(response.finish_reason, "max_tokens");
    assert_ne!(response.finish_reason, "stop");
    assert!(
        response.tool_calls.is_empty(),
        "max_tokens must not yield executable tool_use: {:?}",
        response.tool_calls
    );
}

#[tokio::test]
async fn anthropic_sse_tool_use_keeps_complete_calls() {
    let response = stream_anthropic(
        "run-c11-anthropic-sse-tool-use",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_search\",\"name\":\"web_search\",\"input\":{}}}\n\n\
         data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"query\\\":\\\"status\\\"}\"}}\n\n\
         data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":11}}\n\n\
         data: {\"type\":\"message_stop\"}\n\n",
    )
    .await;

    assert_eq!(response.finish_reason, "tool_use");
    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].id, "toolu_search");
    assert_eq!(response.tool_calls[0].function.name, "web_search");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&response.tool_calls[0].function.arguments)
            .unwrap(),
        serde_json::json!({ "query": "status" })
    );
}

#[tokio::test]
async fn anthropic_sse_message_stop_without_stop_reason_is_unknown_not_stop() {
    let response = stream_anthropic(
        "run-c11-anthropic-sse-unknown",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"没有终止原因\"}}\n\n\
         data: {\"type\":\"message_stop\"}\n\n",
    )
    .await;

    assert_eq!(response.finish_reason, "unknown");
    assert_ne!(response.finish_reason, "stop");
    assert_eq!(response.content.as_deref(), Some("没有终止原因"));
}

#[tokio::test]
async fn anthropic_sse_missing_stop_reason_does_not_yield_executable_calls() {
    let response = stream_anthropic(
        "run-c11-anthropic-sse-unknown-tools",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_search\",\"name\":\"web_search\",\"input\":{}}}\n\n\
         data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"query\\\":\\\"status\\\"}\"}}\n\n\
         data: {\"type\":\"message_stop\"}\n\n",
    )
    .await;

    assert_eq!(response.finish_reason, "unknown");
    assert_ne!(response.finish_reason, "stop");
    assert!(
        response.tool_calls.is_empty(),
        "missing stop_reason must not dispatch assembled tool_use: {:?}",
        response.tool_calls
    );
}

#[tokio::test]
async fn anthropic_sse_connection_close_without_stop_reason_is_unknown_not_stop() {
    let response = stream_anthropic(
        "run-c11-anthropic-sse-close-unknown",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"连接结束前没有终止块\"}}\n\n",
    )
    .await;

    assert_eq!(response.finish_reason, "unknown");
    assert_ne!(response.finish_reason, "stop");
    assert_eq!(response.content.as_deref(), Some("连接结束前没有终止块"));
}
