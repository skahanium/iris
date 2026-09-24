use crate::ai_runtime::native_search_subrequest::RetrievalObservation;
pub use crate::ai_types::{
    ContextPacket, EndpointFamily, FunctionCall, LlmMessage, MessageRole, ProviderConfig,
    TokenUsage, ToolCall, ToolSpec,
};
use crate::error::{AppError, AppResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fmt;
#[path = "model_gateway/abort.rs"]
mod abort_impl;
#[path = "model_gateway/anthropic_response.rs"]
mod anthropic_response_impl;
#[path = "model_gateway/body.rs"]
mod body_impl;
#[path = "model_gateway/http_backend.rs"]
mod http_backend_impl;
#[path = "model_gateway/messages.rs"]
mod messages_impl;
#[path = "model_gateway/minimax_tool_call.rs"]
mod minimax_tool_call_impl;
#[path = "model_gateway/responses.rs"]
mod responses_impl;
#[path = "model_gateway/streaming_anthropic.rs"]
mod streaming_anthropic;
#[path = "model_gateway/streaming_chat_completions.rs"]
mod streaming_chat_completions;
#[path = "model_gateway/streaming.rs"]
mod streaming_impl;
#[path = "model_gateway/streaming_reasoning.rs"]
mod streaming_reasoning;
#[path = "model_gateway/streaming_search_events.rs"]
mod streaming_search_events;
#[path = "model_gateway/streaming_witness.rs"]
mod streaming_witness;
#[path = "model_gateway/usage.rs"]
mod usage_impl;

pub use abort_impl::{clear_abort, is_abort_requested, request_abort};
pub(crate) use abort_impl::{
    notify_web_revoked, wait_for_abort, wait_for_web_revocation, web_revocation_epoch,
};
use anthropic_response_impl::parse_anthropic_response;
pub use body_impl::{build_chat_completions_body, GatewayRequest, LlmFunctionDef, LlmToolDef};
use body_impl::{build_llm_api_body, uses_openai_responses};
use http_backend_impl::format_llm_http_error;
pub use http_backend_impl::HttpLlmBackend;
pub(crate) use messages_impl::messages_for_api_with_reasoning_continuation;
pub use messages_impl::{
    insert_missing_tool_result_stubs, messages_for_api, prepare_tool_api_messages,
    remove_orphan_tool_messages, repair_tool_api_messages, tool_api_message_chain_valid,
};
pub use streaming_impl::{
    StreamEvent, StreamEventData, StreamEventObserver, StreamEventType, StreamSurface,
};
use usage_impl::parse_usage;

/// Opaque provider state needed to continue a multi-turn tool exchange.
///
/// It intentionally has no serializer and redacts its identifier in `Debug` so
/// response-chain metadata cannot leak through ordinary diagnostics.
#[derive(Clone, PartialEq, Eq)]
pub enum ProviderContinuation {
    OpenAiResponses { response_id: String },
}

impl fmt::Debug for ProviderContinuation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpenAiResponses { .. } => {
                formatter.write_str("OpenAiResponses{response_id:[redacted]}")
            }
        }
    }
}

/// Gateway response (non-streaming).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GatewayResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
    pub finish_reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip)]
    pub continuation: Option<ProviderContinuation>,
    #[serde(skip)]
    pub(crate) retrieval_observation: Option<RetrievalObservation>,
}

/// Model Gateway: handles LLM provider communication.
pub struct ModelGateway {
    client: Client,
    providers: Vec<ProviderConfig>,
}

fn extract_http_status_code(message: &str) -> Option<u16> {
    let bytes = message.as_bytes();
    if bytes.len() < 3 {
        return None;
    }
    for index in 0..=(bytes.len() - 3) {
        let code = &bytes[index..index + 3];
        if code.iter().all(u8::is_ascii_digit) {
            let value = (code[0] - b'0') as u16 * 100
                + (code[1] - b'0') as u16 * 10
                + (code[2] - b'0') as u16;
            if (400..=599).contains(&value) {
                return Some(value);
            }
        }
    }
    None
}

fn is_provider_level_failover_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    if lower.contains("request aborted")
        || lower.contains("partial_visible_stream_error")
        || lower.contains("invalid_api_key")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("api key")
        || lower.contains("auth")
        || lower.contains("context length")
        || lower.contains("maximum context")
        || lower.contains("too many tokens")
        || lower.contains("unprocessable entity")
        || lower.contains("policy")
    {
        return false;
    }

    match extract_http_status_code(message) {
        Some(429) => return true,
        Some(status) if (500..=599).contains(&status) => return true,
        Some(_) => return false,
        None => {}
    }

    lower.contains("llm streaming request failed")
        || lower.contains("llm request failed")
        || lower.contains("request failed")
        || lower.contains("error sending request")
        || lower.contains("connection")
        || lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("deadline")
        || lower.contains("service unavailable")
        || lower.contains("too busy")
        || lower.contains("overloaded")
        || lower.contains("stream_invalid_json")
        // Chinese providers report busy/rate limits in business payloads that
        // may arrive with HTTP 200; without these keywords the failure is
        // treated as permanent and failover never moves to the next candidate.
        || lower.contains("模型服务繁忙")
        || lower.contains("服务繁忙")
        || lower.contains("系统繁忙")
        || lower.contains("服务器繁忙")
        || lower.contains("请求过于频繁")
        || lower.contains("限流")
}

fn select_failover_provider(
    candidates: &[ProviderConfig],
    failed_provider: &ProviderConfig,
    error_message: &str,
) -> Option<ProviderConfig> {
    if !is_provider_level_failover_error(error_message) {
        return None;
    }
    let failed_index = candidates.iter().position(|candidate| {
        candidate.name == failed_provider.name
            && candidate.model == failed_provider.model
            && candidate.base_url == failed_provider.base_url
    })?;
    candidates
        .iter()
        .skip(failed_index + 1)
        .find(|candidate| candidate.name != failed_provider.name)
        .cloned()
}

impl ModelGateway {
    /// Create a new gateway with injected HTTP client and provider configurations.
    pub fn new(client: Client, providers: Vec<ProviderConfig>) -> Self {
        Self { client, providers }
    }

    /// Create a gateway with default pinned HTTP client.
    ///
    /// When any configured provider points at a loopback endpoint (Ollama on
    /// localhost), the gateway uses a client that permits plain HTTP to the
    /// loopback interface; otherwise the strict HTTPS-only client is used.
    pub fn with_defaults(providers: Vec<ProviderConfig>) -> AppResult<Self> {
        let requires_loopback = providers
            .iter()
            .any(|provider| crate::security::ipc_policy::is_loopback_url(&provider.base_url));
        let client = if requires_loopback {
            crate::network::cert_pinning::loopback_http_client_builder()
                .build()
                .map_err(|error| AppError::msg(format!("HTTP client: {error}")))?
        } else {
            crate::network::cert_pinning::create_https_client()?
        };
        Ok(Self::new(client, providers))
    }

    /// Select the next configured model only for provider-level failures.
    pub fn failover_provider_after(
        &self,
        failed_provider: &ProviderConfig,
        error_message: &str,
    ) -> Option<ProviderConfig> {
        select_failover_provider(&self.providers, failed_provider, error_message)
    }
    /// Format context packets as markdown evidence block.
    pub fn format_evidence_packets(packets: &[ContextPacket]) -> String {
        let mut evidence = String::from("## 本地证据包\n\n");
        evidence.push_str("以下是从你的笔记中检索到的材料，请在回答中引用（使用 [标签] 格式），并结合网络搜索结果交叉验证：\n\n");
        for packet in packets {
            evidence.push_str(&format!(
                "### {} ({})\n",
                packet.citation_label, packet.title
            ));
            if let Some(path) = &packet.source_path {
                evidence.push_str(&format!("来源: {path}\n"));
            }
            if let Some(corpus) = &packet.corpus {
                evidence.push_str(&format!(
                    "语料角色: {}（{}）\n使用边界: {}\n",
                    corpus.label, corpus.name, corpus.instruction
                ));
            }
            if let Some(heading) = &packet.heading_path {
                evidence.push_str(&format!("章节: {heading}\n"));
            }
            evidence.push_str(&format!("相关度: {:.0}%\n", packet.score * 100.0));
            evidence.push_str(&format!("{}\n\n", packet.excerpt));
        }
        evidence
    }

    /// Convert ToolSpec to LLM tool definition format.
    pub fn tools_to_llm_format(tools: &[ToolSpec]) -> Vec<LlmToolDef> {
        tools
            .iter()
            .map(|t| LlmToolDef {
                tool_type: "function".into(),
                function: LlmFunctionDef {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.input_schema.clone(),
                },
            })
            .collect()
    }

    /// Send a request to the LLM provider (non-streaming).
    pub async fn send_request(&self, request: GatewayRequest) -> AppResult<GatewayResponse> {
        let url = llm_endpoint_url(&request);

        let body = build_llm_api_body(&request)?;
        if let Some(slot) = &request.boundary {
            slot.note_generated(
                &request.messages,
                u32::try_from(request.tools.len()).unwrap_or(u32::MAX),
                !request.provider.model.is_empty(),
            );
            let family = if uses_openai_responses(&request) {
                crate::ai_runtime::boundary_events::ProtocolFamily::OpenAiResponses
            } else {
                match request.provider.endpoint_family {
                    EndpointFamily::AnthropicMessages => {
                        crate::ai_runtime::boundary_events::ProtocolFamily::AnthropicMessages
                    }
                    _ => crate::ai_runtime::boundary_events::ProtocolFamily::OpenAiChatCompletions,
                }
            };
            slot.note_serialized(family, &body, &request.messages);
        }

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        if let Some(api_key) = &request.provider.api_key {
            req_builder =
                apply_auth_headers(req_builder, request.provider.endpoint_family, api_key);
        }

        let response = req_builder.json(&body).send().await.map_err(|error| {
            if let Some(slot) = &request.boundary {
                slot.note_known_failure("transport");
            }
            AppError::from_reqwest_transport(error)
        })?;
        if let Some(slot) = &request.boundary {
            slot.note_request_sent();
        }

        if !response.status().is_success() {
            let status = response.status();
            if let Some(slot) = &request.boundary {
                slot.note_provider_returned(
                    crate::ai_runtime::boundary_events::ProviderReturnStructure {
                        http_status_class: Some(if status.as_u16() < 500 { "4xx" } else { "5xx" }),
                        has_content: false,
                        tool_call_count: 0,
                        finish_reason_class: Some("error"),
                    },
                );
                slot.note_handshake_end(
                    crate::ai_runtime::boundary_events::RecordCompleteness::Complete,
                );
            }
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::from_llm_http_status(
                status,
                format_llm_http_error(status, &text),
            ));
        }

        let response_text = response.text().await.map_err(|e| {
            if let Some(slot) = &request.boundary {
                slot.note_known_failure("transport");
            }
            AppError::msg(format!("Failed to read LLM response body: {}", e))
        })?;

        let json = match parse_gateway_json(&response_text) {
            Ok(json) => json,
            Err(error) => {
                if let Some(slot) = &request.boundary {
                    slot.note_known_failure("protocol");
                }
                return Err(error);
            }
        };

        let parsed = parse_gateway_response(&request, &json);
        if let Some(slot) = &request.boundary {
            slot.note_provider_returned(
                crate::ai_runtime::boundary_events::ProviderReturnStructure {
                    http_status_class: Some("2xx"),
                    has_content: parsed
                        .content
                        .as_deref()
                        .is_some_and(|content| !content.is_empty()),
                    tool_call_count: u32::try_from(parsed.tool_calls.len()).unwrap_or(u32::MAX),
                    finish_reason_class: Some(match parsed.finish_reason.as_str() {
                        "stop" | "end_turn" => "stop",
                        "tool_calls" | "tool_use" => "tool_calls",
                        "length" | "max_tokens" => "length",
                        _ => "other",
                    }),
                },
            );
            if !slot.has_name_origin() {
                slot.note_name_origin(
                    crate::ai_runtime::tool_name_origin::handshake_payload_from_calls(
                        slot.correlation().protocol_adapter.as_str(),
                        request.tools.iter().map(|tool| tool.function.name.as_str()),
                        parsed
                            .tool_calls
                            .iter()
                            .map(|call| call.function.name.as_str()),
                        std::iter::empty::<&str>(),
                    ),
                );
            }
            slot.note_handshake_end(
                crate::ai_runtime::boundary_events::RecordCompleteness::Complete,
            );
        }
        Ok(parsed)
    }

    /// Send a streaming request to a caller-owned observer without Tauri event emission.
    ///
    /// This is the Run-owned path: its observer persists user-visible answer deltas, so the
    /// gateway must normalize provider content before forwarding any token.
    pub async fn send_streaming_request_to_observer(
        &self,
        request_id: &str,
        request: GatewayRequest,
        observer: &mut dyn StreamEventObserver,
    ) -> AppResult<GatewayResponse> {
        streaming_impl::send_streaming_request_to_observer(
            &self.client,
            request_id,
            request,
            observer,
            false,
            run_observer_stream_surface(),
            true,
            None,
        )
        .await
    }

    /// Run-owned dispatch hook executes only when the validated HTTP future is polled.
    pub(crate) async fn send_streaming_request_with_dispatch(
        &self,
        request_id: &str,
        request: GatewayRequest,
        observer: &mut dyn StreamEventObserver,
        before_dispatch: &(dyn Fn() -> AppResult<()> + Send + Sync),
    ) -> AppResult<GatewayResponse> {
        streaming_impl::send_streaming_request_to_observer(
            &self.client,
            request_id,
            request,
            observer,
            false,
            run_observer_stream_surface(),
            true,
            Some(before_dispatch),
        )
        .await
    }
}

fn run_observer_stream_surface() -> StreamSurface {
    StreamSurface::VisibleAnswerSanitized
}

fn parse_gateway_json(response_text: &str) -> AppResult<serde_json::Value> {
    serde_json::from_str(response_text).map_err(|_| AppError::msg("llm_response_invalid_json"))
}

fn llm_endpoint_url(request: &GatewayRequest) -> String {
    if uses_openai_responses(request) {
        let base = request.provider.base_url.trim_end_matches('/');
        return if base.ends_with("/v1") {
            format!("{base}/responses")
        } else {
            format!("{base}/v1/responses")
        };
    }
    let base = request.provider.base_url.trim_end_matches('/');
    match request.provider.endpoint_family {
        EndpointFamily::OpenAiCompatibleChatCompletions | EndpointFamily::ResponsesReserved => {
            crate::llm::providers::chat_completions_url(&request.provider.base_url)
        }
        EndpointFamily::AnthropicMessages => {
            if base.ends_with("/v1") {
                format!("{base}/messages")
            } else {
                format!("{base}/v1/messages")
            }
        }
    }
}

fn apply_auth_headers(
    builder: reqwest::RequestBuilder,
    endpoint_family: EndpointFamily,
    api_key: &str,
) -> reqwest::RequestBuilder {
    match endpoint_family {
        EndpointFamily::AnthropicMessages => builder.header("x-api-key", api_key).header(
            "anthropic-version",
            crate::llm::providers::ANTHROPIC_API_VERSION,
        ),
        EndpointFamily::OpenAiCompatibleChatCompletions | EndpointFamily::ResponsesReserved => {
            builder.header("Authorization", format!("Bearer {}", api_key))
        }
    }
}

fn parse_gateway_response(request: &GatewayRequest, json: &serde_json::Value) -> GatewayResponse {
    if uses_openai_responses(request) {
        return parse_openai_responses_response(json);
    }
    match request.provider.endpoint_family {
        EndpointFamily::AnthropicMessages => parse_anthropic_response(json),
        EndpointFamily::OpenAiCompatibleChatCompletions | EndpointFamily::ResponsesReserved => {
            parse_openai_compatible_response(request, json)
        }
    }
}

fn parse_openai_responses_response(json: &serde_json::Value) -> GatewayResponse {
    let mut content = String::new();
    let mut tool_calls = Vec::new();
    for item in json["output"].as_array().into_iter().flatten() {
        match item["type"].as_str() {
            Some("message") => {
                for part in item["content"].as_array().into_iter().flatten() {
                    if part["type"].as_str() == Some("output_text") {
                        if let Some(text) = part["text"].as_str() {
                            content.push_str(text);
                        }
                    }
                }
            }
            Some("function_call") => {
                if let (Some(call_id), Some(name)) =
                    (item["call_id"].as_str(), item["name"].as_str())
                {
                    tool_calls.push(ToolCall {
                        id: call_id.to_string(),
                        call_type: "function".to_string(),
                        function: FunctionCall {
                            name: name.to_string(),
                            arguments: item["arguments"].as_str().unwrap_or("{}").to_string(),
                        },
                    });
                }
            }
            _ => {}
        }
    }
    let usage = TokenUsage {
        prompt_tokens: json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32,
        completion_tokens: json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
        total_tokens: json["usage"]["total_tokens"].as_u64().unwrap_or(0) as u32,
        ..Default::default()
    };
    GatewayResponse {
        content: (!content.is_empty()).then_some(content),
        tool_calls,
        usage,
        finish_reason: json["status"].as_str().unwrap_or("completed").to_string(),
        reasoning_content: None,
        continuation: json["id"].as_str().map(|response_id| {
            ProviderContinuation::OpenAiResponses {
                response_id: response_id.to_string(),
            }
        }),
        retrieval_observation: None,
    }
}

fn parse_openai_compatible_response(
    request: &GatewayRequest,
    json: &serde_json::Value,
) -> GatewayResponse {
    let raw_content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("");
    let (visible_content, content_tool_calls) =
        if request.provider.name.eq_ignore_ascii_case("minimax") {
            let mut parser = minimax_tool_call_impl::MinimaxContentToolCallParser::new();
            let (visible, mut calls) = parser.push(raw_content);
            let (visible_tail, tail_calls) = parser.finish();
            calls.extend(tail_calls);
            (format!("{visible}{visible_tail}"), calls)
        } else {
            (raw_content.to_string(), Vec::new())
        };
    let content = {
        let sanitized = crate::ai_runtime::text_support::sanitize_provider_visible_content(
            &request.provider.name,
            &visible_content,
        );
        (!sanitized.is_empty()).then_some(sanitized)
    };

    let message = &json["choices"][0]["message"];
    let reasoning_content = if request.provider.name.eq_ignore_ascii_case("minimax") {
        message["reasoning_details"]
            .as_array()
            .and_then(|details| serde_json::to_string(details).ok())
    } else {
        message["reasoning_content"].as_str().map(str::to_string)
    };

    let mut tool_calls: Vec<ToolCall> = json["choices"][0]["message"]["tool_calls"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|tc| {
                    Some(ToolCall {
                        id: tc["id"].as_str()?.to_string(),
                        call_type: tc["type"].as_str().unwrap_or("function").to_string(),
                        function: FunctionCall {
                            name: tc["function"]["name"].as_str()?.to_string(),
                            arguments: tc["function"]["arguments"]
                                .as_str()
                                .unwrap_or("{}")
                                .to_string(),
                        },
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if let Some(slot) = &request.boundary {
        slot.note_name_origin(
            crate::ai_runtime::tool_name_origin::handshake_payload_from_calls(
                slot.correlation().protocol_adapter.as_str(),
                request.tools.iter().map(|tool| tool.function.name.as_str()),
                tool_calls.iter().map(|call| call.function.name.as_str()),
                content_tool_calls
                    .iter()
                    .map(|call| call.function.name.as_str()),
            ),
        );
    }
    tool_calls.extend(content_tool_calls);

    GatewayResponse {
        content,
        tool_calls,
        usage: parse_usage(json),
        finish_reason: json["choices"][0]["finish_reason"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        reasoning_content,
        continuation: None,
        retrieval_observation: None,
    }
}

#[cfg(test)]
#[path = "model_gateway/tests.rs"]
mod tests;
