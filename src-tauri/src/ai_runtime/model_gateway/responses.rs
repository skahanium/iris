use std::collections::BTreeMap;

use crate::ai_types::{FunctionCall, TokenUsage, ToolCall};
use crate::error::{AppError, AppResult};

use super::{GatewayResponse, ProviderContinuation};

/// A normalized, safe-to-classify fragment emitted by the OpenAI Responses SSE
/// stream. Reasoning entries are provider-authored *summaries*, never hidden
/// chain-of-thought tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ResponsesStreamDelta {
    Text(String),
    ReasoningSummary { summary_id: String, text: String },
}

pub(super) struct ResponsesStreamState {
    content: String,
    summaries: BTreeMap<String, String>,
    tool_calls: BTreeMap<String, ToolCall>,
    usage: TokenUsage,
    finish_reason: String,
    response_id: Option<String>,
}

impl Default for ResponsesStreamState {
    fn default() -> Self {
        Self {
            content: String::new(),
            summaries: BTreeMap::new(),
            tool_calls: BTreeMap::new(),
            usage: TokenUsage::default(),
            finish_reason: "stop".to_string(),
            response_id: None,
        }
    }
}

impl ResponsesStreamState {
    pub(super) fn apply_event_json(
        &mut self,
        json: &serde_json::Value,
    ) -> AppResult<Vec<ResponsesStreamDelta>> {
        self.capture_response_metadata(json);
        let mut deltas = Vec::new();
        match json["type"].as_str() {
            Some("response.output_text.delta") => {
                if let Some(text) = json["delta"].as_str() {
                    self.content.push_str(text);
                    deltas.push(ResponsesStreamDelta::Text(text.to_string()));
                }
            }
            Some("response.reasoning_summary_text.delta") => {
                let item_id = json["item_id"].as_str().unwrap_or("summary");
                let summary_index = json["summary_index"].as_u64().unwrap_or(0);
                let summary_id = format!("{item_id}:{summary_index}");
                if let Some(delta) = json["delta"].as_str() {
                    let text = self.summaries.entry(summary_id.clone()).or_default();
                    text.push_str(delta);
                    deltas.push(ResponsesStreamDelta::ReasoningSummary {
                        summary_id,
                        text: text.clone(),
                    });
                }
            }
            Some("response.reasoning_summary_text.done") => {
                if let Some(delta) = self.capture_completed_reasoning_summary(
                    json["item_id"].as_str(),
                    json["summary_index"].as_u64(),
                    json["text"].as_str(),
                ) {
                    deltas.push(delta);
                }
            }
            Some("response.reasoning_summary_part.done") => {
                if let Some(delta) = self.capture_completed_reasoning_summary(
                    json["item_id"].as_str(),
                    json["summary_index"].as_u64(),
                    json["part"]["text"].as_str(),
                ) {
                    deltas.push(delta);
                }
            }
            Some("response.function_call_arguments.done") => {
                self.capture_function_call(
                    json["call_id"].as_str(),
                    json["name"].as_str(),
                    json["arguments"].as_str(),
                );
            }
            Some("response.output_item.done") => {
                let item = &json["item"];
                if item["type"].as_str() == Some("function_call") {
                    self.capture_function_call(
                        item["call_id"].as_str().or_else(|| item["id"].as_str()),
                        item["name"].as_str(),
                        item["arguments"].as_str(),
                    );
                }
            }
            Some("error") | Some("response.failed") | Some("response.incomplete") => {
                let message = json["error"]["message"]
                    .as_str()
                    .or_else(|| json["response"]["error"]["message"].as_str())
                    .unwrap_or("OpenAI Responses stream error");
                return Err(AppError::msg(message.to_string()));
            }
            _ => {}
        }
        Ok(deltas)
    }

    pub(super) fn into_gateway_response(self) -> GatewayResponse {
        GatewayResponse {
            content: (!self.content.is_empty()).then_some(self.content),
            tool_calls: self.tool_calls.into_values().collect(),
            usage: self.usage,
            finish_reason: self.finish_reason,
            reasoning_content: None,
            continuation: self
                .response_id
                .map(|response_id| ProviderContinuation::OpenAiResponses { response_id }),
        }
    }

    fn capture_response_metadata(&mut self, json: &serde_json::Value) {
        let response = json.get("response").unwrap_or(json);
        if let Some(response_id) = response["id"].as_str() {
            self.response_id = Some(response_id.to_string());
        }
        if let Some(reason) = response["status"].as_str() {
            self.finish_reason = reason.to_string();
        }
        if let Some(usage) = response.get("usage") {
            self.usage.prompt_tokens = usage["input_tokens"].as_u64().unwrap_or(0) as u32;
            self.usage.completion_tokens = usage["output_tokens"].as_u64().unwrap_or(0) as u32;
            self.usage.total_tokens = usage["total_tokens"]
                .as_u64()
                .map(|value| value as u32)
                .unwrap_or(self.usage.prompt_tokens + self.usage.completion_tokens);
        }
    }

    fn capture_function_call(
        &mut self,
        call_id: Option<&str>,
        name: Option<&str>,
        arguments: Option<&str>,
    ) {
        let (Some(call_id), Some(name)) = (call_id, name) else {
            return;
        };
        self.tool_calls.insert(
            call_id.to_string(),
            ToolCall {
                id: call_id.to_string(),
                call_type: "function".to_string(),
                function: FunctionCall {
                    name: name.to_string(),
                    arguments: arguments.unwrap_or("{}").to_string(),
                },
            },
        );
    }

    fn capture_completed_reasoning_summary(
        &mut self,
        item_id: Option<&str>,
        summary_index: Option<u64>,
        text: Option<&str>,
    ) -> Option<ResponsesStreamDelta> {
        let text = text?;
        let summary_id = format!(
            "{}:{}",
            item_id.unwrap_or("summary"),
            summary_index.unwrap_or(0)
        );
        if self
            .summaries
            .get(&summary_id)
            .is_some_and(|current| current == text)
        {
            return None;
        }
        self.summaries.insert(summary_id.clone(), text.to_string());
        Some(ResponsesStreamDelta::ReasoningSummary {
            summary_id,
            text: text.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_stream_keeps_summary_separate_from_final_text_and_tool_call() {
        let mut state = ResponsesStreamState::default();
        state
            .apply_event_json(&serde_json::json!({
                "type": "response.created",
                "response": {"id": "resp_chain_1", "status": "in_progress"}
            }))
            .unwrap();
        let deltas = state
            .apply_event_json(&serde_json::json!({
                "type": "response.reasoning_summary_text.delta",
                "item_id": "rs_1",
                "summary_index": 0,
                "delta": "正在确定需要的资料。"
            }))
            .unwrap();
        assert_eq!(
            deltas,
            vec![ResponsesStreamDelta::ReasoningSummary {
                summary_id: "rs_1:0".into(),
                text: "正在确定需要的资料。".into(),
            }]
        );
        state
            .apply_event_json(&serde_json::json!({
                "type": "response.function_call_arguments.done",
                "call_id": "call_1",
                "name": "web_search",
                "arguments": "{\"query\":\"Iris\"}"
            }))
            .unwrap();
        state
            .apply_event_json(&serde_json::json!({
                "type": "response.completed",
                "response": {
                    "id": "resp_chain_1",
                    "status": "completed",
                    "usage": {"input_tokens": 3, "output_tokens": 5, "total_tokens": 8}
                }
            }))
            .unwrap();

        let response = state.into_gateway_response();
        assert!(response.content.is_none());
        assert!(response.reasoning_content.is_none());
        assert_eq!(response.tool_calls[0].id, "call_1");
        assert_eq!(response.tool_calls[0].function.name, "web_search");
        assert_eq!(response.usage.total_tokens, 8);
        assert_eq!(
            response.continuation,
            Some(ProviderContinuation::OpenAiResponses {
                response_id: "resp_chain_1".into()
            })
        );
    }

    #[test]
    fn responses_stream_rejects_explicit_incomplete_terminal_state() {
        let mut state = ResponsesStreamState::default();

        let error = state
            .apply_event_json(&serde_json::json!({
                "type": "response.incomplete",
                "response": {"error": {"message": "upstream stopped"}}
            }))
            .expect_err("incomplete Responses streams cannot finalize a Run");

        assert_eq!(error.to_string(), "upstream stopped");
    }

    #[test]
    fn responses_stream_uses_completed_summary_events_when_no_delta_arrives() {
        let mut state = ResponsesStreamState::default();

        let deltas = state
            .apply_event_json(&serde_json::json!({
                "type": "response.reasoning_summary_text.done",
                "item_id": "rs_1",
                "summary_index": 2,
                "text": "已完成资料核对。"
            }))
            .expect("completed summary event");

        assert_eq!(
            deltas,
            vec![ResponsesStreamDelta::ReasoningSummary {
                summary_id: "rs_1:2".into(),
                text: "已完成资料核对。".into(),
            }]
        );
        let duplicate = state
            .apply_event_json(&serde_json::json!({
                "type": "response.reasoning_summary_part.done",
                "item_id": "rs_1",
                "summary_index": 2,
                "part": {"type": "summary_text", "text": "已完成资料核对。"}
            }))
            .expect("duplicate completed summary event");
        assert!(duplicate.is_empty());
    }
}
