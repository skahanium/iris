//! DeepSeek / OpenAI tool-calling fallbacks: structured retry hints and ReAct text parsing.

use crate::ai_runtime::model_gateway::ToolCall;

/// Parse ReAct-style tool invocations from plain assistant text.
///
/// Supported patterns:
/// ```text
/// Action: search_hybrid
/// Action Input: {"query": "foo"}
/// ```
pub fn parse_react_tool_calls(content: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        if let Some(action_name) = line
            .strip_prefix("Action:")
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let mut args = "{}".to_string();
            if i + 1 < lines.len() {
                let next = lines[i + 1].trim();
                if let Some(input) = next.strip_prefix("Action Input:") {
                    args = input.trim().to_string();
                    if args.is_empty() {
                        args = "{}".to_string();
                    }
                    i += 1;
                }
            }
            calls.push(ToolCall::new(
                format!("react_{}", calls.len()),
                action_name,
                args,
            ));
        }
        i += 1;
    }
    calls
}

/// Whether tool call arguments JSON looks truncated / invalid.
pub fn arguments_look_invalid(arguments: &str) -> bool {
    let trimmed = arguments.trim();
    if trimmed.is_empty() {
        return true;
    }
    if trimmed.starts_with('{') && !trimmed.ends_with('}') {
        return true;
    }
    serde_json::from_str::<serde_json::Value>(trimmed).is_err()
}

/// Merge streamed tool-call deltas when the final JSON is invalid (retry signal).
pub fn should_retry_tool_parse(tool_calls: &[ToolCall]) -> bool {
    tool_calls
        .iter()
        .any(|tc| arguments_look_invalid(&tc.function.arguments))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_react_block() {
        let text = r#"Let me search.
Action: search_hybrid
Action Input: {"query": "党纪处分"}
"#;
        let calls = parse_react_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "search_hybrid");
        assert!(calls[0].function.arguments.contains("党纪处分"));
    }

    #[test]
    fn parse_truncated_arguments_flagged() {
        assert!(arguments_look_invalid(r#"{"query": "x""#));
        assert!(!arguments_look_invalid(r#"{"query": "x"}"#));
    }

    #[test]
    fn retry_when_arguments_invalid() {
        let calls = vec![ToolCall::new("1", "search_hybrid", "{\"q\":")];
        assert!(should_retry_tool_parse(&calls));
    }

    #[test]
    fn parse_empty_returns_none() {
        assert!(parse_react_tool_calls("no tools here").is_empty());
    }
}
