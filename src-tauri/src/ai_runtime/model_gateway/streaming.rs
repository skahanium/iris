use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::ai_types::{FunctionCall, TokenUsage, ToolCall};
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

/// Send a streaming request and emit events to frontend.
pub async fn send_streaming_request(
    app_handle: &AppHandle,
    client: &Client,
    request_id: &str,
    request: GatewayRequest,
) -> AppResult<GatewayResponse> {
    if is_abort_requested(request_id) {
        clear_abort(request_id);
        return Err(AppError::msg("request aborted"));
    }

    let url = crate::llm::providers::chat_completions_url(&request.provider.base_url);

    let mut body = build_llm_api_body(&request)?;
    body["stream"] = serde_json::json!(true);

    let mut req_builder = client.post(&url).header("Content-Type", "application/json");

    if let Some(api_key) = &request.provider.api_key {
        req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
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

    // Incremental tool call accumulator: index -> (id, name, args_buf).
    // OpenAI streams tool calls as deltas: id+name arrive first, then
    // argument fragments across multiple subsequent deltas.
    let mut tool_call_deltas: std::collections::HashMap<
        usize,
        (Option<String>, Option<String>, String),
    > = std::collections::HashMap::new();

    // Process SSE stream with carry buffer to handle chunks split across TCP boundaries
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    let mut carry = String::new();
    let mut carry_truncated = false;
    const MAX_CARRY_BYTES: usize = 1_048_576;

    while let Some(chunk_result) = stream.next().await {
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
                };
                emit_stream_event(app_handle, &event, token_index)?;
                continue;
            }

            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                // Process content delta
                if let Some(delta) = json["choices"][0]["delta"]["content"].as_str() {
                    full_content.push_str(delta);
                    let event = StreamEvent {
                        request_id: request_id.to_string(),
                        event_type: StreamEventType::Token,
                        data: StreamEventData::Token {
                            token: delta.to_string(),
                        },
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
                app_handle
                    .emit(
                        "llm:token",
                        serde_json::json!({
                            "request_id": event.request_id,
                            "token": token,
                            "index": token_index,
                        }),
                    )
                    .map_err(emit_err)?;
            }
        }
        StreamEventType::Done => {
            app_handle
                .emit(
                    "llm:done",
                    serde_json::json!({ "request_id": event.request_id }),
                )
                .map_err(emit_err)?;
        }
        StreamEventType::Error => {
            let message = if let StreamEventData::Error { message } = &event.data {
                message.clone()
            } else {
                "stream error".to_string()
            };
            app_handle
                .emit(
                    "llm:error",
                    serde_json::json!({
                        "request_id": event.request_id,
                        "error": message,
                    }),
                )
                .map_err(emit_err)?;
        }
        StreamEventType::ToolCall => {
            app_handle.emit("ai:tool_call", event).map_err(emit_err)?;
        }
    }
    Ok(())
}
