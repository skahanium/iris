//! Chat Completions SSE accumulator.
//!
//! Split out of `streaming.rs` to keep that file inside its pinned size
//! budget. This state machine is the only place Chat Completions streaming
//! may decide `finish_reason` and which tool calls are executable.

use std::collections::HashMap;

use super::{
    streaming_search_events::note_stream_search_json, usage_impl::parse_usage, GatewayResponse,
};
use crate::ai_runtime::final_answer_integrity::FinalAnswerIntegrity;
use crate::ai_runtime::native_search_subrequest::RetrievalObservation;
use crate::ai_types::{FunctionCall, TokenUsage, ToolCall};

/// Incremental Chat Completions stream: content, tool deltas, usage, and the
/// provider's last non-empty `choices[0].finish_reason`.
#[derive(Default)]
pub(crate) struct ChatCompletionsStreamState {
    content: String,
    reasoning: String,
    tool_call_deltas: HashMap<usize, (Option<String>, Option<String>, String)>,
    extra_tool_calls: Vec<ToolCall>,
    finish_reason: Option<String>,
    usage: TokenUsage,
    retrieval_observation: Option<RetrievalObservation>,
}

impl ChatCompletionsStreamState {
    pub(crate) fn apply_event_json(&mut self, json: &serde_json::Value) -> Option<String> {
        if let Some(reason) = json["choices"][0]["finish_reason"].as_str() {
            if !reason.is_empty() {
                self.finish_reason = Some(reason.to_string());
            }
        }
        if json.get("usage").is_some() && json["usage"].get("prompt_tokens").is_some() {
            self.usage = parse_usage(json);
        }

        let content_delta = json["choices"][0]["delta"]["content"]
            .as_str()
            .map(str::to_string)
            .filter(|delta| !delta.is_empty());
        if let Some(delta) = content_delta.as_deref() {
            self.content.push_str(delta);
        }

        if let Some(reasoning) = json["choices"][0]["delta"]["reasoning_content"].as_str() {
            self.reasoning.push_str(reasoning);
        }

        if let Some(tc_deltas) = json["choices"][0]["delta"]["tool_calls"].as_array() {
            for tc_delta in tc_deltas {
                let idx = tc_delta["index"].as_u64().unwrap_or(0) as usize;
                let entry = self
                    .tool_call_deltas
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

        note_stream_search_json(&mut self.retrieval_observation, json);

        content_delta
    }

    pub(crate) fn replace_visible_content(&mut self, content: String) {
        self.content = content;
    }

    pub(crate) fn push_completed_tool_calls(&mut self, calls: Vec<ToolCall>) {
        self.extra_tool_calls.extend(calls);
    }

    pub(crate) fn proposed_tool_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .tool_call_deltas
            .values()
            .filter_map(|(_, name, _)| name.clone())
            .collect();
        names.extend(
            self.extra_tool_calls
                .iter()
                .map(|call| call.function.name.clone()),
        );
        names
    }

    pub(crate) fn into_gateway_response(self) -> GatewayResponse {
        let finish_reason = self
            .finish_reason
            .filter(|reason| !reason.is_empty())
            .unwrap_or_else(|| "unknown".to_string());
        let mut assembled: Vec<(usize, ToolCall)> = self
            .tool_call_deltas
            .into_iter()
            .filter_map(|(idx, (id, name, args))| {
                Some((
                    idx,
                    ToolCall {
                        id: id?,
                        call_type: "function".into(),
                        function: FunctionCall {
                            name: name?,
                            arguments: args,
                        },
                    },
                ))
            })
            .collect();
        assembled.sort_by_key(|(idx, _)| *idx);
        let mut tool_calls: Vec<ToolCall> = assembled.into_iter().map(|(_, call)| call).collect();
        tool_calls.extend(self.extra_tool_calls);
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
            reasoning_content: (!self.reasoning.is_empty()).then_some(self.reasoning),
            continuation: None,
            retrieval_observation: self.retrieval_observation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::streaming_witness::finish_reason_class;
    use super::ChatCompletionsStreamState;

    fn apply_all(
        events: &[serde_json::Value],
    ) -> crate::ai_runtime::model_gateway::GatewayResponse {
        let mut state = ChatCompletionsStreamState::default();
        for event in events {
            state.apply_event_json(event);
        }
        state.into_gateway_response()
    }

    #[test]
    fn length_finish_reason_is_preserved_and_is_not_rewritten_to_stop() {
        let response = apply_all(&[
            serde_json::json!({
                "choices": [{"delta": {"content": "截断前的正文"}}]
            }),
            serde_json::json!({
                "choices": [{"delta": {}, "finish_reason": "length"}]
            }),
        ]);
        assert_eq!(response.finish_reason, "length");
        assert_eq!(response.content.as_deref(), Some("截断前的正文"));
        assert_eq!(finish_reason_class(&response.finish_reason), "length");
    }

    #[test]
    fn tool_calls_finish_reason_keeps_complete_calls() {
        let response = apply_all(&[
            serde_json::json!({
                "choices": [{
                    "delta": {
                        "tool_calls": [{
                            "index": 0,
                            "id": "call-search",
                            "function": {"name": "web_search", "arguments": ""}
                        }]
                    }
                }]
            }),
            serde_json::json!({
                "choices": [{
                    "delta": {
                        "tool_calls": [{
                            "index": 0,
                            "function": {"arguments": "{\"query\":\"status\"}"}
                        }]
                    },
                    "finish_reason": "tool_calls"
                }]
            }),
        ]);
        assert_eq!(response.finish_reason, "tool_calls");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "call-search");
        assert_eq!(response.tool_calls[0].function.name, "web_search");
        assert_eq!(
            response.tool_calls[0].function.arguments,
            "{\"query\":\"status\"}"
        );
    }

    #[test]
    fn missing_finish_reason_is_unknown_not_stop() {
        let response = apply_all(&[serde_json::json!({
            "choices": [{"delta": {"content": "没有终止块"}}]
        })]);
        assert_eq!(response.finish_reason, "unknown");
        assert_ne!(response.finish_reason, "stop");
    }

    #[test]
    fn missing_finish_reason_does_not_yield_executable_calls() {
        let response = apply_all(&[serde_json::json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call-search",
                        "function": {"name": "web_search", "arguments": "{\"query\":\"status\"}"}
                    }]
                }
            }]
        })]);
        assert_eq!(response.finish_reason, "unknown");
        assert!(
            response.tool_calls.is_empty(),
            "missing termination must not dispatch assembled calls: {:?}",
            response.tool_calls
        );
    }

    #[test]
    fn max_tokens_finish_reason_does_not_yield_executable_calls() {
        let response = apply_all(&[
            serde_json::json!({
                "choices": [{
                    "delta": {
                        "tool_calls": [{
                            "index": 0,
                            "id": "call-search",
                            "function": {"name": "web_search", "arguments": "{\"query\":\"status\"}"}
                        }]
                    }
                }]
            }),
            serde_json::json!({
                "choices": [{"delta": {}, "finish_reason": "max_tokens"}]
            }),
        ]);
        assert_eq!(response.finish_reason, "max_tokens");
        assert!(response.tool_calls.is_empty());
    }

    #[test]
    fn tool_call_deltas_are_ordered_by_index() {
        let response = apply_all(&[
            serde_json::json!({
                "choices": [{
                    "delta": {
                        "tool_calls": [{
                            "index": 1,
                            "id": "call-second",
                            "function": {"name": "system_time_now", "arguments": "{}"}
                        }]
                    }
                }]
            }),
            serde_json::json!({
                "choices": [{
                    "delta": {
                        "tool_calls": [{
                            "index": 0,
                            "id": "call-first",
                            "function": {"name": "web_search", "arguments": "{\"query\":\"status\"}"}
                        }]
                    },
                    "finish_reason": "tool_calls"
                }]
            }),
        ]);
        assert_eq!(response.tool_calls.len(), 2);
        assert_eq!(response.tool_calls[0].id, "call-first");
        assert_eq!(response.tool_calls[1].id, "call-second");
    }

    #[test]
    fn trailing_finish_reason_only_chunk_is_kept() {
        let response = apply_all(&[
            serde_json::json!({
                "choices": [{"delta": {"content": "正文"}}]
            }),
            serde_json::json!({
                "choices": [{"delta": {}, "finish_reason": "stop"}]
            }),
        ]);
        assert_eq!(response.finish_reason, "stop");
        assert_eq!(response.content.as_deref(), Some("正文"));
    }

    #[test]
    fn length_with_partial_tool_delta_does_not_yield_executable_calls() {
        let response = apply_all(&[
            serde_json::json!({
                "choices": [{
                    "delta": {
                        "tool_calls": [{
                            "index": 0,
                            "id": "call-truncated",
                            "function": {"name": "web_search", "arguments": "{\"query\":"}
                        }]
                    }
                }]
            }),
            serde_json::json!({
                "choices": [{"delta": {}, "finish_reason": "length"}]
            }),
        ]);
        assert_eq!(response.finish_reason, "length");
        assert!(
            response.tool_calls.is_empty(),
            "truncated tool deltas must not become executable calls: {:?}",
            response.tool_calls
        );
    }

    #[test]
    fn normal_stop_with_prose_stays_stop() {
        let response = apply_all(&[
            serde_json::json!({
                "choices": [{"delta": {"content": "完整回答。"}}]
            }),
            serde_json::json!({
                "choices": [{"delta": {}, "finish_reason": "stop"}]
            }),
        ]);
        assert_eq!(response.finish_reason, "stop");
        assert_eq!(response.content.as_deref(), Some("完整回答。"));
        assert!(response.tool_calls.is_empty());
    }

    #[test]
    fn web_search_call_json_mints_main_stream_leak_without_tool_calls() {
        let response = apply_all(&[serde_json::json!({
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
        })]);
        assert!(response.tool_calls.is_empty());
        let observation = response
            .retrieval_observation
            .expect("Chat Completions search events must become credentials");
        assert_eq!(
            observation.origin,
            crate::ai_runtime::native_search_subrequest::RetrievalOrigin::MainStreamLeak
        );
        assert!(observation.has_retrieval_credentials);
        assert!(observation
            .candidates
            .iter()
            .any(|hit| hit.url == "https://example.com/from-sse"));
    }
}
