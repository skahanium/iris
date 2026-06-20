use crate::ai_types::{FunctionCall, TokenUsage, ToolCall};

use super::GatewayResponse;

pub(super) fn parse_anthropic_response(json: &serde_json::Value) -> GatewayResponse {
    let mut text = String::new();
    let mut tool_calls = Vec::new();

    if let Some(parts) = json["content"].as_array() {
        for part in parts {
            match part["type"].as_str() {
                Some("text") => {
                    if let Some(part_text) = part["text"].as_str() {
                        text.push_str(part_text);
                    }
                }
                Some("tool_use") => {
                    if let (Some(id), Some(name)) = (part["id"].as_str(), part["name"].as_str()) {
                        let arguments = match part.get("input") {
                            Some(input) => {
                                serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string())
                            }
                            None => "{}".to_string(),
                        };
                        tool_calls.push(ToolCall {
                            id: id.to_string(),
                            call_type: "function".to_string(),
                            function: FunctionCall {
                                name: name.to_string(),
                                arguments,
                            },
                        });
                    }
                }
                _ => {}
            }
        }
    }

    let usage = TokenUsage {
        prompt_tokens: json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32,
        completion_tokens: json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
        total_tokens: json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32
            + json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
        ..Default::default()
    };

    GatewayResponse {
        content: if text.is_empty() { None } else { Some(text) },
        tool_calls,
        usage,
        finish_reason: json["stop_reason"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        reasoning_content: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_anthropic_response_extracts_tool_use_blocks() {
        let response = serde_json::json!({
            "content": [
                {"type": "text", "text": "我需要查询。"},
                {
                    "type": "tool_use",
                    "id": "toolu_1",
                    "name": "search_hybrid",
                    "input": {"query": "阶段 1", "limit": 5}
                }
            ],
            "usage": {"input_tokens": 11, "output_tokens": 7},
            "stop_reason": "tool_use"
        });

        let parsed = parse_anthropic_response(&response);

        assert_eq!(parsed.content.as_deref(), Some("我需要查询。"));
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].id, "toolu_1");
        assert_eq!(parsed.tool_calls[0].function.name, "search_hybrid");
        assert_eq!(
            parsed.tool_calls[0].function.arguments,
            r#"{"limit":5,"query":"阶段 1"}"#
        );
        assert_eq!(parsed.finish_reason, "tool_use");
    }
}
