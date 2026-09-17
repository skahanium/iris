//! Anthropic Messages SSE accumulator.
//!
//! Split out of `streaming.rs` to keep that file inside its pinned size
//! budget. This state machine is the only place Anthropic streaming may
//! decide `finish_reason` and which tool_use blocks are executable.

use std::collections::BTreeMap;

use super::GatewayResponse;
use crate::ai_runtime::final_answer_integrity::FinalAnswerIntegrity;
use crate::ai_types::{FunctionCall, TokenUsage, ToolCall};
use crate::error::{AppError, AppResult};

#[derive(Default)]
struct AnthropicToolUseBlock {
    id: Option<String>,
    name: Option<String>,
    input_json: String,
}

/// Incremental Anthropic Messages stream: text, tool_use blocks, usage, and
/// the provider's last non-empty `delta.stop_reason`.
#[derive(Default)]
pub(crate) struct AnthropicStreamState {
    content: String,
    tool_blocks: BTreeMap<usize, AnthropicToolUseBlock>,
    pub(crate) usage: TokenUsage,
    finish_reason: Option<String>,
}

impl AnthropicStreamState {
    pub(crate) fn apply_event_json(
        &mut self,
        json: &serde_json::Value,
    ) -> AppResult<Option<String>> {
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
                    if !stop_reason.is_empty() {
                        self.finish_reason = Some(stop_reason.to_string());
                    }
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

    pub(crate) fn into_gateway_response(self) -> GatewayResponse {
        let finish_reason = self
            .finish_reason
            .filter(|reason| !reason.is_empty())
            .unwrap_or_else(|| "unknown".to_string());
        let mut tool_calls: Vec<ToolCall> = self
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
        if !FinalAnswerIntegrity::may_execute_tool_calls(&finish_reason) {
            tool_calls.clear();
        }

        GatewayResponse {
            content: if self.content.is_empty() {
                None
            } else {
                Some(self.content)
            },
            tool_calls,
            usage: self.usage,
            finish_reason,
            reasoning_content: None,
            continuation: None,
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

#[cfg(test)]
mod tests {
    use super::super::streaming_witness::finish_reason_class;
    use super::AnthropicStreamState;

    fn apply_all(
        events: &[serde_json::Value],
    ) -> crate::ai_runtime::model_gateway::GatewayResponse {
        let mut state = AnthropicStreamState::default();
        for event in events {
            state.apply_event_json(event).expect("anthropic event");
        }
        state.into_gateway_response()
    }

    fn tool_use_start() -> serde_json::Value {
        serde_json::json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": {
                "type": "tool_use",
                "id": "toolu_search",
                "name": "web_search",
                "input": {}
            }
        })
    }

    #[test]
    fn end_turn_stop_reason_is_preserved_and_is_not_rewritten_to_stop() {
        let response = apply_all(&[
            serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "text_delta", "text": "完整回答。" }
            }),
            serde_json::json!({
                "type": "message_delta",
                "delta": { "stop_reason": "end_turn" }
            }),
            serde_json::json!({ "type": "message_stop" }),
        ]);
        assert_eq!(response.finish_reason, "end_turn");
        assert_eq!(response.content.as_deref(), Some("完整回答。"));
        assert_eq!(finish_reason_class(&response.finish_reason), "stop");
        assert!(response.tool_calls.is_empty());
    }

    #[test]
    fn tool_use_stop_reason_keeps_complete_calls() {
        let response = apply_all(&[
            tool_use_start(),
            serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": "{\"query\":\"status\"}"
                }
            }),
            serde_json::json!({
                "type": "message_delta",
                "delta": { "stop_reason": "tool_use" },
                "usage": { "output_tokens": 11 }
            }),
        ]);
        assert_eq!(response.finish_reason, "tool_use");
        assert_eq!(finish_reason_class(&response.finish_reason), "tool_calls");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "toolu_search");
        assert_eq!(response.tool_calls[0].function.name, "web_search");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&response.tool_calls[0].function.arguments)
                .unwrap(),
            serde_json::json!({ "query": "status" })
        );
    }

    #[test]
    fn missing_stop_reason_is_unknown_not_stop() {
        let response = apply_all(&[serde_json::json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": { "type": "text_delta", "text": "没有终止原因" }
        })]);
        assert_eq!(response.finish_reason, "unknown");
        assert_ne!(response.finish_reason, "stop");
    }

    #[test]
    fn message_stop_without_stop_reason_is_unknown_not_stop() {
        let response = apply_all(&[
            serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "text_delta", "text": "只有尾事件" }
            }),
            serde_json::json!({ "type": "message_stop" }),
        ]);
        assert_eq!(response.finish_reason, "unknown");
        assert_ne!(response.finish_reason, "stop");
        assert_eq!(response.content.as_deref(), Some("只有尾事件"));
    }

    #[test]
    fn missing_stop_reason_does_not_yield_executable_calls() {
        let response = apply_all(&[
            tool_use_start(),
            serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": "{\"query\":\"status\"}"
                }
            }),
        ]);
        assert_eq!(response.finish_reason, "unknown");
        assert!(
            response.tool_calls.is_empty(),
            "missing termination must not dispatch assembled tool_use: {:?}",
            response.tool_calls
        );
    }

    #[test]
    fn max_tokens_stop_reason_does_not_yield_executable_calls() {
        let response = apply_all(&[
            tool_use_start(),
            serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": "{\"query\":\"status\"}"
                }
            }),
            serde_json::json!({
                "type": "message_delta",
                "delta": { "stop_reason": "max_tokens" }
            }),
        ]);
        assert_eq!(response.finish_reason, "max_tokens");
        assert_eq!(finish_reason_class(&response.finish_reason), "length");
        assert!(response.tool_calls.is_empty());
    }

    #[test]
    fn max_tokens_with_partial_tool_json_does_not_yield_executable_calls() {
        let response = apply_all(&[
            tool_use_start(),
            serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": "{\"query\":"
                }
            }),
            serde_json::json!({
                "type": "message_delta",
                "delta": { "stop_reason": "max_tokens" }
            }),
        ]);
        assert_eq!(response.finish_reason, "max_tokens");
        assert!(
            response.tool_calls.is_empty(),
            "truncated tool_use must not become executable calls: {:?}",
            response.tool_calls
        );
    }

    #[test]
    fn end_turn_with_tools_keeps_complete_calls() {
        let response = apply_all(&[
            tool_use_start(),
            serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": "{\"query\":\"status\"}"
                }
            }),
            serde_json::json!({
                "type": "message_delta",
                "delta": { "stop_reason": "end_turn" }
            }),
        ]);
        assert_eq!(response.finish_reason, "end_turn");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "toolu_search");
    }

    #[test]
    fn anthropic_stream_state_accumulates_text_and_tool_use_blocks() {
        let response = apply_all(&[
            serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {
                    "type": "text_delta",
                    "text": "先查一下。"
                }
            }),
            serde_json::json!({
                "type": "content_block_start",
                "index": 1,
                "content_block": {
                    "type": "tool_use",
                    "id": "toolu_stream_1",
                    "name": "search_hybrid",
                    "input": {}
                }
            }),
            serde_json::json!({
                "type": "content_block_delta",
                "index": 1,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": "{\"query\":\"阶段 1\""
                }
            }),
            serde_json::json!({
                "type": "content_block_delta",
                "index": 1,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": ",\"limit\":5}"
                }
            }),
            serde_json::json!({
                "type": "message_delta",
                "delta": { "stop_reason": "tool_use" },
                "usage": { "output_tokens": 11 }
            }),
        ]);
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
