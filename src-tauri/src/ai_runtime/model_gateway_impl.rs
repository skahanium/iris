pub use crate::ai_types::{
    ContextPacket, EndpointFamily, FunctionCall, LlmMessage, MessageRole, ProviderConfig,
    TokenUsage, ToolCall, ToolSpec,
};
use crate::error::{AppError, AppResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
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
#[path = "model_gateway/streaming.rs"]
mod streaming_impl;
#[path = "model_gateway/usage.rs"]
mod usage_impl;

pub use abort_impl::{clear_abort, is_abort_requested, request_abort};
use anthropic_response_impl::parse_anthropic_response;
use body_impl::build_llm_api_body;
pub use body_impl::{build_chat_completions_body, GatewayRequest, LlmFunctionDef, LlmToolDef};
use http_backend_impl::format_llm_http_error;
pub use http_backend_impl::HttpLlmBackend;
pub use messages_impl::{
    insert_missing_tool_result_stubs, messages_for_api, prepare_tool_api_messages,
    remove_orphan_tool_messages, repair_tool_api_messages, tool_api_message_chain_valid,
};
pub use streaming_impl::{
    emit_stream_reset, emit_stream_reset_with_reason, emit_stream_reset_with_surface,
    LegacyTauriStreamObserver, StreamEvent, StreamEventData, StreamEventObserver, StreamEventType,
    StreamSurface,
};
use usage_impl::parse_usage;

/// Gateway response (non-streaming).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
    pub finish_reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

/// Model Gateway: handles LLM provider communication.
pub struct ModelGateway {
    app_handle: AppHandle,
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
        || lower.contains("妯″瀷鏈嶅姟绻佸繖")
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
    pub fn new(app_handle: AppHandle, client: Client, providers: Vec<ProviderConfig>) -> Self {
        Self {
            app_handle,
            client,
            providers,
        }
    }

    /// Create a gateway with default pinned HTTP client.
    pub fn with_defaults(app_handle: AppHandle, providers: Vec<ProviderConfig>) -> AppResult<Self> {
        let client = crate::network::cert_pinning::create_https_client()?;
        Ok(Self::new(app_handle, client, providers))
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
        let url = llm_endpoint_url(&request.provider);

        let body = build_llm_api_body(&request)?;

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        if let Some(api_key) = &request.provider.api_key {
            req_builder =
                apply_auth_headers(req_builder, request.provider.endpoint_family, api_key);
        }

        let response = req_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::msg(format!("LLM request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::msg(format_llm_http_error(status, &text)));
        }

        let response_text = response
            .text()
            .await
            .map_err(|e| AppError::msg(format!("Failed to read LLM response body: {}", e)))?;

        let json = parse_gateway_json(&response_text)?;

        Ok(parse_gateway_response(
            request.provider.endpoint_family,
            &json,
        ))
    }

    pub async fn send_streaming_request(
        &self,
        request_id: &str,
        request: GatewayRequest,
    ) -> AppResult<GatewayResponse> {
        streaming_impl::send_streaming_request(&self.app_handle, &self.client, request_id, request)
            .await
    }

    /// Send a streaming request to a caller-owned observer without Tauri event emission.
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
            StreamSurface::VisibleAnswer,
            true,
        )
        .await
    }

    pub async fn send_streaming_request_with_surface(
        &self,
        request_id: &str,
        request: GatewayRequest,
        surface: StreamSurface,
        emit_error_event: bool,
    ) -> AppResult<GatewayResponse> {
        streaming_impl::send_streaming_request_with_surface(
            &self.app_handle,
            &self.client,
            request_id,
            request,
            surface,
            emit_error_event,
        )
        .await
    }

    pub async fn send_classified_streaming_request(
        &self,
        request_id: &str,
        request: GatewayRequest,
    ) -> AppResult<GatewayResponse> {
        streaming_impl::send_streaming_request_with_meta(
            &self.app_handle,
            &self.client,
            request_id,
            request,
            true,
            StreamSurface::VisibleAnswer,
        )
        .await
    }
}

fn parse_gateway_json(response_text: &str) -> AppResult<serde_json::Value> {
    serde_json::from_str(response_text).map_err(|_| AppError::msg("llm_response_invalid_json"))
}

fn llm_endpoint_url(provider: &ProviderConfig) -> String {
    let base = provider.base_url.trim_end_matches('/');
    match provider.endpoint_family {
        EndpointFamily::OpenAiCompatibleChatCompletions | EndpointFamily::ResponsesReserved => {
            crate::llm::providers::chat_completions_url(&provider.base_url)
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

fn parse_gateway_response(
    endpoint_family: EndpointFamily,
    json: &serde_json::Value,
) -> GatewayResponse {
    match endpoint_family {
        EndpointFamily::AnthropicMessages => parse_anthropic_response(json),
        EndpointFamily::OpenAiCompatibleChatCompletions | EndpointFamily::ResponsesReserved => {
            parse_openai_compatible_response(json)
        }
    }
}

fn parse_openai_compatible_response(json: &serde_json::Value) -> GatewayResponse {
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string());

    let reasoning_content = json["choices"][0]["message"]["reasoning_content"]
        .as_str()
        .map(|s| s.to_string());

    let tool_calls = json["choices"][0]["message"]["tool_calls"]
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

    GatewayResponse {
        content,
        tool_calls,
        usage: parse_usage(json),
        finish_reason: json["choices"][0]["finish_reason"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        reasoning_content,
    }
}

#[cfg(test)]
#[path = "model_gateway/tests.rs"]
mod tests;
