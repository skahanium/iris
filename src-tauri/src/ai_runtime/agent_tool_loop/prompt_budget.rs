//! Bounded projection of old tool observations; execution facts stay intact.

use super::*;

/// Compact only tool observations preceding the newest complete tool batch.
/// Return affected call IDs so the loop can restore their bounded observations
/// on an explicit replay. The original execution status and metadata survive.
pub(crate) fn compact_tool_observations(
    messages: &mut [LlmMessage],
    tools: &[ToolSpec],
    budget: AgentModelTurnBudget,
) -> Vec<String> {
    let Some(limit) = budget.max_prompt_tokens else {
        return Vec::new();
    };
    let protected = messages
        .iter()
        .rposition(|message| {
            matches!(message.role, MessageRole::Assistant)
                && message
                    .tool_calls
                    .as_ref()
                    .is_some_and(|calls| !calls.is_empty())
        })
        .or_else(|| {
            messages
                .iter()
                .rposition(|message| matches!(message.role, MessageRole::Tool))
        })
        .unwrap_or(0);
    let mut compacted = Vec::new();
    for index in 0..protected {
        if estimate_prompt_tokens(messages, tools) <= limit {
            break;
        }
        let call = messages[index].tool_call_id.as_ref().and_then(|id| {
            messages
                .iter()
                .filter_map(|message| message.tool_calls.as_ref())
                .flatten()
                .find(|call| &call.id == id)
        });
        if call.is_some_and(|call| {
            tools
                .iter()
                .any(|tool| tool.name == call.function.name && tool.requires_confirmation)
                || crate::ai_runtime::tool_catalog::catalog_find(&call.function.name)
                    .is_some_and(|entry| entry.requires_confirmation)
        }) {
            // A write receipt is already bounded; preserve the entire receipt.
            continue;
        }
        let message = &mut messages[index];
        if !matches!(message.role, MessageRole::Tool) {
            continue;
        }
        let original = message.content.text_content();
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&original) else {
            // Unknown historical payloads cannot truthfully acquire a status.
            continue;
        };
        if value["compacted"] == true {
            continue;
        }
        let Some(object) = value.as_object_mut() else {
            continue;
        };
        if let Some(output) = object.get_mut("output") {
            omit_prose(output);
        }
        object.insert("compacted".into(), serde_json::json!(true));
        object.insert(
            "recovery".into(),
            serde_json::json!("repeat_original_call_for_recorded_observation"),
        );
        let Ok(reduced) = serde_json::to_string(&value) else {
            continue;
        };
        if reduced.len() >= original.len() {
            continue;
        }
        message.content = reduced.into();
        if let Some(id) = &message.tool_call_id {
            compacted.push(id.clone());
        }
    }
    compacted
}

/// Keep identities, ranges, stable errors and receipts. Only known prose slots
/// in read-result shapes are omitted; arbitrary receipt objects remain intact.
fn omit_prose(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(_) => *value = serde_json::Value::Null,
        serde_json::Value::Array(items) => {
            for item in items {
                omit_prose(item);
            }
        }
        serde_json::Value::Object(object) => {
            for key in ["content", "excerpt", "text", "body", "snippet"] {
                if object.get(key).is_some_and(serde_json::Value::is_string) {
                    object.insert(key.into(), serde_json::Value::Null);
                }
            }
            for key in ["results", "packets"] {
                if let Some(items) = object
                    .get_mut(key)
                    .and_then(serde_json::Value::as_array_mut)
                {
                    for item in items {
                        omit_prose(item);
                    }
                }
            }
        }
        _ => {}
    }
}
