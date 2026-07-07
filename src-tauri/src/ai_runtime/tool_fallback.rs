//! DeepSeek / OpenAI tool-calling fallbacks: structured retry hints, ReAct and DSML text parsing.

use std::sync::LazyLock;

use regex::Regex;

use crate::ai_runtime::model_gateway::ToolCall;

static DSML_INVOKE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"invoke\s+name\s*=\s*"([^"]+)""#).expect("dsml invoke regex"));

static DSML_PARAM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"parameter\s+name\s*=\s*"([^"]+)"([^>]*)>([^<]*)"#).expect("dsml param regex")
});

static DSML_TOOL_CALLS_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<[^>]*DSML[^>]*tool_calls>.*?</[^>]*DSML[^>]*tool_calls>")
        .expect("dsml tool_calls block regex")
});

static DSML_ORPHAN_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?s)<[^>]*DSML[^>]*(?:invoke|parameter)[^>]*>.*?</[^>]*DSML[^>]*(?:invoke|parameter)>",
    )
    .expect("dsml orphan tag regex")
});

static REACT_TOOL_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*Action:\s*[^\r\n]+(?:\r?\n\s*Action Input:\s*[^\r\n]*)?\r?\n?")
        .expect("react tool block regex")
});

/// Parse DeepSeek DSML-style tool invocations embedded in assistant text.
pub fn parse_dsml_tool_calls(content: &str) -> Vec<ToolCall> {
    if !content.contains("invoke") && !content.contains("DSML") {
        return Vec::new();
    }

    let mut calls = Vec::new();
    for (idx, cap) in DSML_INVOKE_RE.captures_iter(content).enumerate() {
        let name = cap.get(1).map(|m| m.as_str()).unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        let start = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let block_end = content[start..]
            .find("/invoke")
            .map(|i| start + i)
            .unwrap_or_else(|| content.len().min(start + 4096));
        let block = &content[start..block_end];
        let mut args_map = serde_json::Map::new();
        for pcap in DSML_PARAM_RE.captures_iter(block) {
            let key = pcap.get(1).map(|m| m.as_str()).unwrap_or_default();
            let attrs = pcap.get(2).map(|m| m.as_str()).unwrap_or_default();
            let raw_value = pcap.get(3).map(|m| m.as_str()).unwrap_or_default();
            if !key.is_empty() {
                args_map.insert(
                    key.to_string(),
                    parse_dsml_parameter_value(attrs, raw_value),
                );
            }
        }
        let args = if args_map.is_empty() {
            "{}".to_string()
        } else {
            serde_json::Value::Object(args_map).to_string()
        };
        calls.push(ToolCall::new(format!("dsml_{idx}"), name, args));
    }
    calls
}

fn parse_dsml_parameter_value(attrs: &str, raw_value: &str) -> serde_json::Value {
    let trimmed = raw_value.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(trimmed);
    if attrs.contains(r#"string="true""#) || attrs.contains("string='true'") {
        return serde_json::Value::String(unquoted.to_string());
    }
    serde_json::from_str::<serde_json::Value>(trimmed)
        .unwrap_or_else(|_| serde_json::Value::String(unquoted.to_string()))
}

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

/// Prefer API tool_calls; else DSML in content; else ReAct.
pub fn parse_tool_calls_from_content(content: &str) -> Vec<ToolCall> {
    let dsml = parse_dsml_tool_calls(content);
    if !dsml.is_empty() {
        return dsml;
    }
    parse_react_tool_calls(content)
}

/// Remove DSML / pseudo tool markup from user-visible assistant text.
pub fn strip_tool_markup_from_visible(content: &str) -> String {
    let mut out = DSML_TOOL_CALLS_BLOCK_RE
        .replace_all(content, "")
        .into_owned();
    out = DSML_ORPHAN_TAG_RE.replace_all(&out, "").into_owned();
    out = REACT_TOOL_BLOCK_RE.replace_all(&out, "").into_owned();
    if out.contains("DSML") || out.contains("invoke name=") {
        out = DSML_INVOKE_RE.replace_all(&out, "").into_owned();
    }
    let trimmed = out.trim();
    if trimmed.is_empty()
        && (content.contains("DSML")
            || content.contains("invoke name=")
            || content.contains("Action:"))
    {
        String::new()
    } else {
        trimmed.to_string()
    }
}

/// Detect short internal tool-call argument fragments that must not become final chat text.
pub fn is_internal_tool_artifact_text(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("action input:")
        || lower.starts_with("action:")
        || lower.contains("invoke name=")
        || lower.contains("parameter name=")
    {
        return true;
    }

    if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if map.contains_key("path")
            || map.contains_key("source_path")
            || map.contains_key("max_chars")
            || map.contains_key("maxChars")
        {
            return true;
        }
    }

    if lower.contains(".md") {
        if lower.contains("max_chars")
            || lower.contains("maxchars")
            || lower.contains("path=")
            || lower.contains("source_path=")
            || lower.contains("sourcepath=")
        {
            return true;
        }

        let mut parts = trimmed.split_whitespace();
        if let (Some(first), Some(second), None) = (parts.next(), parts.next(), parts.next()) {
            if first.chars().all(|c| c.is_ascii_digit())
                && second.to_ascii_lowercase().ends_with(".md")
            {
                return true;
            }
        }
    }

    false
}

pub fn parse_tool_call_arguments(arguments: &str) -> Result<serde_json::Value, String> {
    let trimmed = arguments.trim();
    if trimmed.is_empty() {
        return Err("tool arguments must be a valid JSON object".into());
    }
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(value @ serde_json::Value::Object(_)) => Ok(value),
        Ok(_) => Err("tool arguments must be a valid JSON object".into()),
        Err(err) => Err(format!("tool arguments must be a valid JSON object: {err}")),
    }
}

/// Whether tool call arguments JSON looks truncated / invalid.
pub fn arguments_look_invalid(arguments: &str) -> bool {
    parse_tool_call_arguments(arguments).is_err()
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
    fn parse_dsml_web_search() {
        let text = r#"<｜｜DSML｜｜tool_calls>
<｜｜DSML｜｜invoke name="web_search">
<｜｜DSML｜｜parameter name="query" string="true">"test query"</｜｜DSML｜｜parameter>
</｜｜DSML｜｜invoke>
</｜｜DSML｜｜tool_calls>"#;
        let calls = parse_dsml_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "web_search");
        assert!(calls[0].function.arguments.contains("test query"));
    }

    #[test]
    fn parse_tool_calls_from_content_prefers_dsml() {
        let text = r#"Action: web_search
Action Input: {"query": "x"}
<｜｜DSML｜｜invoke name="web_search">
<｜｜DSML｜｜parameter name="query" string="true">"y"</｜｜DSML｜｜parameter>
</｜｜DSML｜｜invoke>"#;
        let calls = parse_tool_calls_from_content(text);
        assert!(!calls.is_empty());
        assert_eq!(calls[0].function.name, "web_search");
    }

    #[test]
    fn dsml_parameters_preserve_json_value_types() {
        let text = r#"<｜｜DSML｜｜invoke name="list_vault">
<｜｜DSML｜｜parameter name="limit">10</｜｜DSML｜｜parameter>
<｜｜DSML｜｜parameter name="include_archived">false</｜｜DSML｜｜parameter>
<｜｜DSML｜｜parameter name="query" string="true">"notes"</｜｜DSML｜｜parameter>
</｜｜DSML｜｜invoke>"#;

        let calls = parse_dsml_tool_calls(text);
        let args: serde_json::Value = serde_json::from_str(&calls[0].function.arguments).unwrap();

        assert_eq!(args["limit"].as_u64(), Some(10));
        assert_eq!(args["include_archived"].as_bool(), Some(false));
        assert_eq!(args["query"].as_str(), Some("notes"));
    }

    #[test]
    fn strip_dsml_markup() {
        let text = r#"好的，我来查一下。
<｜｜DSML｜｜tool_calls>
<｜｜DSML｜｜invoke name="web_search">
<｜｜DSML｜｜parameter name="query" string="true">"q"</｜｜DSML｜｜parameter>
</｜｜DSML｜｜invoke>
</｜｜DSML｜｜tool_calls>"#;
        let visible = strip_tool_markup_from_visible(text);
        assert!(visible.contains("好的"));
        assert!(!visible.contains("DSML"));
        assert!(!visible.contains("invoke name"));
    }

    #[test]
    fn strip_react_read_note_block_from_visible_text() {
        let text = r#"Action: read_note
Action Input: {"path":"党纪国法/政府采购货物和服务招标投标管理办法.md","max_chars":15000}
"#;

        let visible = strip_tool_markup_from_visible(text);

        assert!(visible.is_empty());
    }

    #[test]
    fn strip_react_tool_block_preserves_user_facing_preamble() {
        let text = r#"我先查一下本地材料。
Action: read_note
Action Input: {"path":"党纪国法/政府采购货物和服务招标投标管理办法.md","max_chars":15000}
"#;

        let visible = strip_tool_markup_from_visible(text);

        assert_eq!(visible, "我先查一下本地材料。");
        assert!(!visible.contains("max_chars"));
        assert!(!visible.contains(".md"));
    }
    #[test]
    fn parse_tool_call_arguments_requires_valid_json_object() {
        let parsed = parse_tool_call_arguments(r#"{"query":"x"}"#).unwrap();
        assert_eq!(parsed["query"], "x");
        assert!(parse_tool_call_arguments(r#"{"query":"x""#).is_err());
        assert!(parse_tool_call_arguments(r#"["not", "object"]"#).is_err());
        assert!(parse_tool_call_arguments("").is_err());
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
