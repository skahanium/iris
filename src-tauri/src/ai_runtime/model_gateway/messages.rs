use crate::ai_types::{LlmMessage, MessageRole, ToolCall};

/// Ensure every `tool` message follows an assistant message listing its `tool_call_id`.
/// Repairs checkpoints produced before tool_calls were persisted on assistant turns.
pub fn repair_tool_api_messages(messages: &mut [LlmMessage]) {
    for i in 0..messages.len() {
        if !matches!(messages[i].role, MessageRole::Tool) {
            continue;
        }
        let Some(tool_id) = messages[i].tool_call_id.clone().filter(|s| !s.is_empty()) else {
            continue;
        };
        let mut j = i;
        while j > 0 && matches!(messages[j - 1].role, MessageRole::Tool) {
            j -= 1;
        }
        if j == 0 {
            continue;
        }
        let parent = j - 1;
        if !matches!(messages[parent].role, MessageRole::Assistant) {
            continue;
        }
        let has = messages[parent]
            .tool_calls
            .as_ref()
            .is_some_and(|calls| calls.iter().any(|tc| tc.id == tool_id));
        if !has {
            let placeholder = ToolCall::new(&tool_id, "tool", "{}");
            match &mut messages[parent].tool_calls {
                Some(calls) => {
                    if !calls.iter().any(|tc| tc.id == tool_id) {
                        calls.push(placeholder);
                    }
                }
                None => messages[parent].tool_calls = Some(vec![placeholder]),
            }
        }
    }
}

/// Drop tool-role messages that are not part of a valid assistant -> tool* chain.
pub fn remove_orphan_tool_messages(messages: &mut Vec<LlmMessage>) {
    let mut i = 0;
    while i < messages.len() {
        if !matches!(messages[i].role, MessageRole::Tool) {
            i += 1;
            continue;
        }
        if !tool_message_has_valid_chain(messages, i) {
            messages.remove(i);
            continue;
        }
        i += 1;
    }
}

fn tool_message_has_valid_chain(messages: &[LlmMessage], tool_idx: usize) -> bool {
    let tool_id = match messages[tool_idx].tool_call_id.as_deref() {
        Some(id) if !id.is_empty() => id,
        _ => return false,
    };
    let mut j = tool_idx;
    while j > 0 && matches!(messages[j - 1].role, MessageRole::Tool) {
        j -= 1;
    }
    if j == 0 {
        return false;
    }
    let parent = j - 1;
    matches!(messages[parent].role, MessageRole::Assistant)
        && messages[parent]
            .tool_calls
            .as_ref()
            .is_some_and(|calls| calls.iter().any(|tc| tc.id == tool_id))
}

/// Insert error stubs for tool_calls on the latest assistant turn that still lack tool results.
pub fn insert_missing_tool_result_stubs(messages: &mut Vec<LlmMessage>, skip_ids: &[String]) {
    let skip: std::collections::HashSet<&str> = skip_ids.iter().map(String::as_str).collect();
    let Some(assistant_idx) = messages
        .iter()
        .rposition(|m| m.tool_calls.as_ref().is_some_and(|c| !c.is_empty()))
    else {
        return;
    };
    let Some(calls) = messages[assistant_idx].tool_calls.clone() else {
        return;
    };
    let mut insert_at = assistant_idx + 1;
    while insert_at < messages.len() && matches!(messages[insert_at].role, MessageRole::Tool) {
        insert_at += 1;
    }
    let responded: std::collections::HashSet<String> = messages[assistant_idx + 1..insert_at]
        .iter()
        .filter_map(|m| m.tool_call_id.clone())
        .collect();
    for tc in calls {
        if responded.contains(&tc.id) || skip.contains(tc.id.as_str()) {
            continue;
        }
        messages.insert(
            insert_at,
            LlmMessage {
                role: MessageRole::Tool,
                content: r#"{"error":"tool execution incomplete"}"#.into(),
                tool_call_id: Some(tc.id.clone()),
                tool_calls: None,
                reasoning_content: None,
            },
        );
        insert_at += 1;
    }
}

/// Normalize message history before sending to tool-capable chat APIs.
pub fn prepare_tool_api_messages(messages: &mut Vec<LlmMessage>, skip_stub_ids: &[String]) {
    repair_tool_api_messages(messages);
    remove_orphan_tool_messages(messages);
    insert_missing_tool_result_stubs(messages, skip_stub_ids);
}

/// Returns true when `messages_for_api` would satisfy OpenAI tool-turn ordering.
pub fn tool_api_message_chain_valid(messages: &[LlmMessage]) -> bool {
    let api = messages_for_api(messages);
    let mut last_assistant_had_tool_calls = false;
    for msg in api {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
        if role == "assistant" {
            last_assistant_had_tool_calls = msg
                .get("tool_calls")
                .and_then(|v| v.as_array())
                .is_some_and(|a| !a.is_empty());
        } else if role == "tool" {
            if !last_assistant_had_tool_calls {
                return false;
            }
        } else {
            last_assistant_had_tool_calls = false;
        }
    }
    true
}

/// Serialize messages for provider APIs (tool_calls need `type`, tool role needs `tool_call_id`).
pub fn messages_for_api(messages: &[LlmMessage]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|m| {
            let role = match m.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "tool",
            };
            if matches!(m.role, MessageRole::Tool) {
                return serde_json::json!({
                    "role": "tool",
                    "tool_call_id": m.tool_call_id,
                    "content": m.content,
                });
            }
            if matches!(m.role, MessageRole::Assistant)
                && m.tool_calls.as_ref().is_some_and(|tc| !tc.is_empty())
            {
                let content: serde_json::Value = if m.content.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(m.content.text_content())
                };
                let tool_calls = serde_json::to_value(m.tool_calls.as_ref().unwrap())
                    .unwrap_or_else(|_| serde_json::json!([]));
                let mut msg = serde_json::json!({
                    "role": "assistant",
                    "content": content,
                    "tool_calls": tool_calls,
                });
                if let Some(reasoning) = &m.reasoning_content {
                    msg["reasoning_content"] = serde_json::Value::String(reasoning.clone());
                }
                return msg;
            }
            if matches!(m.role, MessageRole::Assistant) {
                let mut msg = serde_json::json!({
                    "role": "assistant",
                    "content": m.content,
                });
                if let Some(reasoning) = &m.reasoning_content {
                    msg["reasoning_content"] = serde_json::Value::String(reasoning.clone());
                }
                return msg;
            }
            serde_json::json!({
                "role": role,
                "content": m.content,
            })
        })
        .collect()
}
