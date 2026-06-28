use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::ai_types::{EndpointFamily, FunctionCall, TokenUsage, ToolCall};
use crate::error::{AppError, AppResult};

use super::{
    abort_impl::{clear_abort, is_abort_requested},
    body_impl::{build_llm_api_body, GatewayRequest},
    http_backend_impl::format_llm_http_error,
    usage_impl::parse_usage,
    GatewayResponse,
};

/// Streaming event emitted to frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    pub request_id: String,
    pub event_type: StreamEventType,
    pub data: StreamEventData,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub classified: bool,
}

/// Stream event types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamEventType {
    Token,
    ToolCall,
    Done,
    Error,
}

/// Stream event data payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StreamEventData {
    Token { token: String },
    ToolCall { tool_call: ToolCall },
    Done { usage: Option<TokenUsage> },
    Error { message: String },
}

fn streaming_endpoint_url(base_url: &str, endpoint_family: EndpointFamily) -> String {
    let base = base_url.trim_end_matches('/');
    match endpoint_family {
        EndpointFamily::AnthropicMessages => {
            if base.ends_with("/v1") {
                format!("{base}/messages")
            } else {
                format!("{base}/v1/messages")
            }
        }
        EndpointFamily::OpenAiCompatibleChatCompletions | EndpointFamily::ResponsesReserved => {
            crate::llm::providers::chat_completions_url(base_url)
        }
    }
}

fn apply_streaming_auth_headers(
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
            builder.header("Authorization", format!("Bearer {api_key}"))
        }
    }
}

#[derive(Default)]
struct AnthropicToolUseBlock {
    id: Option<String>,
    name: Option<String>,
    input_json: String,
}

#[derive(Default)]
struct AnthropicStreamState {
    content: String,
    tool_blocks: std::collections::BTreeMap<usize, AnthropicToolUseBlock>,
    usage: TokenUsage,
    finish_reason: Option<String>,
}

impl AnthropicStreamState {
    fn apply_event_json(&mut self, json: &serde_json::Value) -> AppResult<Option<String>> {
        match json["type"].as_str() {
            Some("content_block_start") => {
                let index = json["index"].as_u64().unwrap_or(0) as usize;
                let block = &json["content_block"];
                match block["type"].as_str() {
                    Some("text") => {
                        if let Some(text) = block["text"].as_str() {
                            self.content.push_str(text);
                            return Ok(Some(text.to_string()));
                        }
                    }
                    Some("tool_use") => {
                        let entry = self.tool_blocks.entry(index).or_default();
                        entry.id = block["id"].as_str().map(str::to_string);
                        entry.name = block["name"].as_str().map(str::to_string);
                        if let Some(input) = block.get("input") {
                            if input != &serde_json::json!({}) {
                                entry.input_json = serde_json::to_string(input)
                                    .unwrap_or_else(|_| "{}".to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
            Some("content_block_delta") => {
                let index = json["index"].as_u64().unwrap_or(0) as usize;
                let delta = &json["delta"];
                match delta["type"].as_str() {
                    Some("text_delta") => {
                        if let Some(text) = delta["text"].as_str() {
                            self.content.push_str(text);
                            return Ok(Some(text.to_string()));
                        }
                    }
                    Some("input_json_delta") => {
                        if let Some(partial) = delta["partial_json"].as_str() {
                            self.tool_blocks
                                .entry(index)
                                .or_default()
                                .input_json
                                .push_str(partial);
                        }
                    }
                    _ => {}
                }
            }
            Some("message_start") | Some("message_delta") => {
                if let Some(stop_reason) = json["delta"]["stop_reason"].as_str() {
                    self.finish_reason = Some(stop_reason.to_string());
                }
                if let Some(input_tokens) = json["message"]["usage"]["input_tokens"].as_u64() {
                    self.usage.prompt_tokens = input_tokens as u32;
                }
                if let Some(input_tokens) = json["usage"]["input_tokens"].as_u64() {
                    self.usage.prompt_tokens = input_tokens as u32;
                }
                if let Some(output_tokens) = json["usage"]["output_tokens"].as_u64() {
                    self.usage.completion_tokens = output_tokens as u32;
                }
                self.usage.total_tokens = self.usage.prompt_tokens + self.usage.completion_tokens;
            }
            Some("error") => {
                let message = json["error"]["message"]
                    .as_str()
                    .or_else(|| json["message"].as_str())
                    .unwrap_or("Anthropic stream error");
                return Err(AppError::msg(message.to_string()));
            }
            _ => {}
        }
        Ok(None)
    }

    fn into_gateway_response(self) -> GatewayResponse {
        let tool_calls = self
            .tool_blocks
            .into_values()
            .filter_map(|block| {
                let id = block.id?;
                let name = block.name?;
                let arguments = normalize_tool_arguments(block.input_json);
                Some(ToolCall {
                    id,
                    call_type: "function".to_string(),
                    function: FunctionCall { name, arguments },
                })
            })
            .collect();

        GatewayResponse {
            content: if self.content.is_empty() {
                None
            } else {
                Some(self.content)
            },
            tool_calls,
            usage: self.usage,
            finish_reason: self.finish_reason.unwrap_or_else(|| "stop".to_string()),
            reasoning_content: None,
        }
    }
}

fn normalize_tool_arguments(input_json: String) -> String {
    let trimmed = input_json.trim();
    if trimmed.is_empty() {
        return "{}".to_string();
    }
    serde_json::from_str::<serde_json::Value>(trimmed)
        .and_then(|value| serde_json::to_string(&value))
        .unwrap_or(input_json)
}

/// Send a streaming request and emit events to frontend.
pub async fn send_streaming_request(
    app_handle: &AppHandle,
    _client: &Client,
    request_id: &str,
    request: GatewayRequest,
) -> AppResult<GatewayResponse> {
    send_streaming_request_with_meta(app_handle, _client, request_id, request, false).await
}

/// Send a streaming request and attach domain metadata to emitted events.
pub async fn send_streaming_request_with_meta(
    app_handle: &AppHandle,
    _client: &Client,
    request_id: &str,
    request: GatewayRequest,
    classified: bool,
) -> AppResult<GatewayResponse> {
    if is_abort_requested(request_id) {
        clear_abort(request_id);
        return Err(AppError::msg("request aborted"));
    }

    let endpoint_family = request.provider.endpoint_family;
    let url = streaming_endpoint_url(request.provider.base_url.as_str(), endpoint_family);

    let mut body = build_llm_api_body(&request)?;
    body["stream"] = serde_json::json!(true);

    let streaming_client = crate::network::cert_pinning::create_streaming_https_client()?;
    let mut req_builder = streaming_client
        .post(&url)
        .header("Content-Type", "application/json");

    if let Some(api_key) = &request.provider.api_key {
        req_builder = apply_streaming_auth_headers(req_builder, endpoint_family, api_key);
    }

    let response = req_builder
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::msg(format!("LLM streaming request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(AppError::msg(format_llm_http_error(status, &text)));
    }

    let mut full_content = String::new();
    let mut full_reasoning = String::new();
    let mut usage = TokenUsage::default();
    let mut token_index: u32 = 0;
    let mut anthropic_state = AnthropicStreamState::default();

    // Incremental tool call accumulator: index -> (id, name, args_buf).
    // OpenAI streams tool calls as deltas: id+name arrive first, then
    // argument fragments across multiple subsequent deltas.
    let mut tool_call_deltas: std::collections::HashMap<
        usize,
        (Option<String>, Option<String>, String),
    > = std::collections::HashMap::new();

    // Process SSE stream with carry buffer to handle chunks split across TCP boundaries.
    // The outer loop is labeled so the [DONE] / message_stop terminators can break out
    // of the stream entirely instead of looping back to `stream.next().await` (which
    // would block on keep-alive sockets until the read_timeout, surfacing as a
    // spurious "重试中" cycle to the user).
    //
    // The loop races `stream.next()` against a periodic abort poll via
    // `tokio::time::timeout`. Without this, a stalled/half-open socket (no chunks
    // arriving) would block `stream.next().await` and the per-chunk abort check
    // would never run — so clicking "中止" could not interrupt a hung stream until
    // reqwest's read_timeout killed it. The timeout race lets abort fire within
    // ~500ms regardless of whether chunks are flowing.
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    let mut carry = String::new();
    let mut carry_truncated = false;
    const MAX_CARRY_BYTES: usize = 1_048_576;
    const ABORT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

    'stream: loop {
        // Race the next stream chunk against a periodic abort poll so a stalled
        // socket (no incoming chunks) can still be interrupted by the user
        // within ~500ms, rather than waiting for reqwest's read_timeout.
        let chunk_opt = match tokio::time::timeout(ABORT_POLL_INTERVAL, stream.next()).await {
            Ok(chunk) => chunk,
            Err(_) => {
                if is_abort_requested(request_id) {
                    clear_abort(request_id);
                    return Err(AppError::msg("request aborted"));
                }
                continue 'stream;
            }
        };

        let chunk_result = match chunk_opt {
            Some(r) => r,
            None => break 'stream,
        };

        if is_abort_requested(request_id) {
            clear_abort(request_id);
            return Err(AppError::msg("request aborted"));
        }

        let chunk = chunk_result.map_err(|e| AppError::msg(format!("Stream read error: {}", e)))?;

        let chunk_text = String::from_utf8_lossy(&chunk);
        if carry.len() + chunk_text.len() > MAX_CARRY_BYTES {
            if !carry_truncated {
                tracing::warn!(
                    request_id = %request_id,
                    carry_len = carry.len(),
                    "SSE 缓冲区超过 1 MiB 上限，截断缓冲数据"
                );
                carry_truncated = true;
            }
        } else {
            carry.push_str(&chunk_text);
        }

        while let Some(pos) = carry.find('\n') {
            let line: String = carry.drain(..=pos).collect();
            let line = line.trim_end_matches('\n').trim_end_matches('\r');

            if !line.starts_with("data: ") {
                continue;
            }

            let data = &line[6..];
            if data == "[DONE]" {
                let event = StreamEvent {
                    request_id: request_id.to_string(),
                    event_type: StreamEventType::Done,
                    data: StreamEventData::Done {
                        usage: Some(usage.clone()),
                    },
                    classified,
                };
                emit_stream_event(app_handle, &event, token_index)?;
                // The stream is finished; stop reading from the socket. Some
                // providers/proxies keep the connection open after [DONE];
                // `continue` here would wait for the server to close (or the
                // 60s timeout), surfacing as a hang.
                break 'stream;
            }

            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                if endpoint_family == EndpointFamily::AnthropicMessages {
                    if let Some(delta) = anthropic_state.apply_event_json(&json)? {
                        let event = StreamEvent {
                            request_id: request_id.to_string(),
                            event_type: StreamEventType::Token,
                            data: StreamEventData::Token { token: delta },
                            classified,
                        };
                        emit_stream_event(app_handle, &event, token_index)?;
                        token_index += 1;
                    }
                    if json["type"].as_str() == Some("message_stop") {
                        let event = StreamEvent {
                            request_id: request_id.to_string(),
                            event_type: StreamEventType::Done,
                            data: StreamEventData::Done {
                                usage: Some(anthropic_state.usage.clone()),
                            },
                            classified,
                        };
                        emit_stream_event(app_handle, &event, token_index)?;
                        // Anthropic's terminal event; stop reading the socket
                        // so a half-open connection cannot hang the loop.
                        break 'stream;
                    }
                    continue;
                }

                // Process content delta
                if let Some(delta) = json["choices"][0]["delta"]["content"].as_str() {
                    full_content.push_str(delta);
                    let event = StreamEvent {
                        request_id: request_id.to_string(),
                        event_type: StreamEventType::Token,
                        data: StreamEventData::Token {
                            token: delta.to_string(),
                        },
                        classified,
                    };
                    emit_stream_event(app_handle, &event, token_index)?;
                    token_index += 1;
                }

                if let Some(reasoning) = json["choices"][0]["delta"]["reasoning_content"].as_str() {
                    full_reasoning.push_str(reasoning);
                }

                // Accumulate tool call deltas by index
                if let Some(tc_deltas) = json["choices"][0]["delta"]["tool_calls"].as_array() {
                    for tc_delta in tc_deltas {
                        let idx = tc_delta["index"].as_u64().unwrap_or(0) as usize;
                        let entry =
                            tool_call_deltas
                                .entry(idx)
                                .or_insert((None, None, String::new()));

                        if let Some(id) = tc_delta["id"].as_str() {
                            entry.0 = Some(id.to_string());
                        }
                        if let Some(name) = tc_delta["function"]["name"].as_str() {
                            entry.1 = Some(name.to_string());
                        }
                        if let Some(args) = tc_delta["function"]["arguments"].as_str() {
                            entry.2.push_str(args);
                        }
                    }
                }

                if json.get("usage").is_some() && json["usage"].get("prompt_tokens").is_some() {
                    usage = parse_usage(&json);
                }
            }
        }
    }

    // Flush remaining carry buffer
    if !carry.trim().is_empty() {
        if let Some(pos) = carry.find("data: ") {
            let remainder = &carry[pos..];
            if let Some(data) = remainder.strip_prefix("data: ") {
                let data = data.trim();
                if data != "[DONE]" {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                        if endpoint_family == EndpointFamily::AnthropicMessages {
                            if let Some(delta) = anthropic_state.apply_event_json(&json)? {
                                full_content.push_str(delta.as_str());
                            }
                            if json["type"].as_str() == Some("message_stop") {
                                let event = StreamEvent {
                                    request_id: request_id.to_string(),
                                    event_type: StreamEventType::Done,
                                    data: StreamEventData::Done {
                                        usage: Some(anthropic_state.usage.clone()),
                                    },
                                    classified,
                                };
                                emit_stream_event(app_handle, &event, token_index)?;
                            }
                            return Ok(anthropic_state.into_gateway_response());
                        }

                        if let Some(delta) = json["choices"][0]["delta"]["content"].as_str() {
                            full_content.push_str(delta);
                        }
                        if let Some(tc_deltas) =
                            json["choices"][0]["delta"]["tool_calls"].as_array()
                        {
                            for tc_delta in tc_deltas {
                                let idx = tc_delta["index"].as_u64().unwrap_or(0) as usize;
                                let entry = tool_call_deltas.entry(idx).or_insert((
                                    None,
                                    None,
                                    String::new(),
                                ));
                                if let Some(id) = tc_delta["id"].as_str() {
                                    entry.0 = Some(id.to_string());
                                }
                                if let Some(name) = tc_delta["function"]["name"].as_str() {
                                    entry.1 = Some(name.to_string());
                                }
                                if let Some(args) = tc_delta["function"]["arguments"].as_str() {
                                    entry.2.push_str(args);
                                }
                            }
                        }
                        if json.get("usage").is_some()
                            && json["usage"].get("prompt_tokens").is_some()
                        {
                            usage = parse_usage(&json);
                        }
                    }
                }
            }
        }
    }

    if endpoint_family == EndpointFamily::AnthropicMessages {
        let response = anthropic_state.into_gateway_response();
        for tc in &response.tool_calls {
            let event = StreamEvent {
                request_id: request_id.to_string(),
                event_type: StreamEventType::ToolCall,
                data: StreamEventData::ToolCall {
                    tool_call: tc.clone(),
                },
                classified,
            };
            emit_stream_event(app_handle, &event, token_index)?;
        }
        return Ok(response);
    }

    // Assemble tool calls from accumulated deltas (deduplicated by index)
    let tool_calls: Vec<ToolCall> = tool_call_deltas
        .into_iter()
        .filter_map(|(_, (id, name, args))| {
            Some(ToolCall {
                id: id?,
                call_type: "function".into(),
                function: FunctionCall {
                    name: name?,
                    arguments: args,
                },
            })
        })
        .collect();

    // Emit tool call events for each assembled call
    for tc in &tool_calls {
        let event = StreamEvent {
            request_id: request_id.to_string(),
            event_type: StreamEventType::ToolCall,
            data: StreamEventData::ToolCall {
                tool_call: tc.clone(),
            },
            classified,
        };
        emit_stream_event(app_handle, &event, token_index)?;
    }

    Ok(GatewayResponse {
        content: if full_content.is_empty() {
            None
        } else {
            Some(full_content)
        },
        tool_calls,
        usage,
        finish_reason: "stop".to_string(),
        reasoning_content: if full_reasoning.is_empty() {
            None
        } else {
            Some(full_reasoning)
        },
    })
}

/// Emit a stream event to the frontend (`llm:*` 与 `engine.rs` / 侧栏监听一致).
pub(super) fn emit_stream_event(
    app_handle: &AppHandle,
    event: &StreamEvent,
    token_index: u32,
) -> AppResult<()> {
    let emit_err = |e: tauri::Error| AppError::msg(format!("Failed to emit stream event: {e}"));
    match event.event_type {
        StreamEventType::Token => {
            if let StreamEventData::Token { token } = &event.data {
                let mut payload = serde_json::json!({
                    "request_id": event.request_id,
                    "token": token,
                    "index": token_index,
                });
                if event.classified {
                    payload["classified"] = serde_json::json!(true);
                }
                app_handle.emit("llm:token", payload).map_err(emit_err)?;
            }
        }
        StreamEventType::Done => {
            let mut payload = serde_json::json!({ "request_id": event.request_id });
            if event.classified {
                payload["classified"] = serde_json::json!(true);
            }
            app_handle.emit("llm:done", payload).map_err(emit_err)?;
        }
        StreamEventType::Error => {
            let message = if let StreamEventData::Error { message } = &event.data {
                message.clone()
            } else {
                "stream error".to_string()
            };
            let mut payload = serde_json::json!({
                "request_id": event.request_id,
                "error": message,
            });
            if event.classified {
                payload["classified"] = serde_json::json!(true);
            }
            app_handle.emit("llm:error", payload).map_err(emit_err)?;
        }
        StreamEventType::ToolCall => {
            app_handle.emit("ai:tool_call", event).map_err(emit_err)?;
        }
    }
    Ok(())
}

/// Emit a `llm:reset` event so the frontend drops buffered tokens from a
/// non-terminal round (tool-call round or inconclusive reflection) before the
/// next round begins streaming. This prevents intermediate preamble or
/// `NEED_MORE_EVIDENCE` sentinels from sticking to the final answer surface.
pub fn emit_stream_reset(app_handle: &AppHandle, request_id: &str) -> AppResult<()> {
    app_handle
        .emit("llm:reset", serde_json::json!({ "request_id": request_id }))
        .map_err(|e| AppError::msg(format!("Failed to emit llm:reset: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_stream_state_accumulates_text_and_tool_use_blocks() {
        let mut state = AnthropicStreamState::default();

        state
            .apply_event_json(&serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {
                    "type": "text_delta",
                    "text": "先查一下。"
                }
            }))
            .unwrap();
        state
            .apply_event_json(&serde_json::json!({
                "type": "content_block_start",
                "index": 1,
                "content_block": {
                    "type": "tool_use",
                    "id": "toolu_stream_1",
                    "name": "search_hybrid",
                    "input": {}
                }
            }))
            .unwrap();
        state
            .apply_event_json(&serde_json::json!({
                "type": "content_block_delta",
                "index": 1,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": "{\"query\":\"阶段 1\""
                }
            }))
            .unwrap();
        state
            .apply_event_json(&serde_json::json!({
                "type": "content_block_delta",
                "index": 1,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": ",\"limit\":5}"
                }
            }))
            .unwrap();
        state
            .apply_event_json(&serde_json::json!({
                "type": "message_delta",
                "delta": { "stop_reason": "tool_use" },
                "usage": { "output_tokens": 11 }
            }))
            .unwrap();

        let response = state.into_gateway_response();
        assert_eq!(response.content.as_deref(), Some("先查一下。"));
        assert_eq!(response.finish_reason, "tool_use");
        assert_eq!(response.usage.completion_tokens, 11);
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "toolu_stream_1");
        assert_eq!(response.tool_calls[0].function.name, "search_hybrid");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&response.tool_calls[0].function.arguments)
                .unwrap(),
            serde_json::json!({ "query": "阶段 1", "limit": 5 })
        );
    }
}
