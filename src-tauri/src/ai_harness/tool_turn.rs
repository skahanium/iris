//! Tool-turn protocol helpers: confirm detection, skip-stub IDs, message validation.

use crate::ai_runtime::harness_support::HarnessCheckpoint;
use crate::ai_runtime::model_gateway::{LlmMessage, MessageRole, ToolCall};
use crate::ai_runtime::tool_executor::ToolRegistry;
use crate::ai_runtime::tool_policy::ToolPolicyContext;

/// Position of the current confirmation within the latest assistant tool batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingConfirmationPosition {
    pub index: usize,
    pub count: usize,
}

/// First confirm-required tool on the latest assistant turn that lacks a tool result.
pub fn outstanding_confirm_tool<'a>(
    registry: &ToolRegistry,
    messages: &'a [LlmMessage],
    policy_ctx: &ToolPolicyContext,
) -> Option<&'a ToolCall> {
    let (assistant_idx, calls) = latest_assistant_tool_calls(messages)?;
    let responded = responded_tool_ids(messages, assistant_idx);
    for tc in calls {
        if responded.contains(&tc.id) {
            continue;
        }
        if registry.requires_confirmation(&tc.function.name)
            && registry
                .check_tool_policy(&tc.function.name, policy_ctx)
                .is_ok()
        {
            return Some(tc);
        }
    }
    None
}

/// All confirm-required tool call IDs still awaiting user approval on the latest turn.
pub fn outstanding_confirm_ids(
    registry: &ToolRegistry,
    messages: &[LlmMessage],
    policy_ctx: &ToolPolicyContext,
) -> Vec<String> {
    let Some((assistant_idx, calls)) = latest_assistant_tool_calls(messages) else {
        return vec![];
    };
    let responded = responded_tool_ids(messages, assistant_idx);
    calls
        .iter()
        .filter(|tc| {
            !responded.contains(&tc.id)
                && registry.requires_confirmation(&tc.function.name)
                && registry
                    .check_tool_policy(&tc.function.name, policy_ctx)
                    .is_ok()
        })
        .map(|tc| tc.id.clone())
        .collect()
}

/// Confirmation progress for a specific pending tool call on the latest assistant turn.
pub fn pending_confirmation_position(
    registry: &ToolRegistry,
    messages: &[LlmMessage],
    policy_ctx: &ToolPolicyContext,
    tool_call_id: &str,
) -> Option<PendingConfirmationPosition> {
    let (_, calls) = latest_assistant_tool_calls(messages)?;
    let confirm_ids = calls
        .iter()
        .filter(|tc| {
            registry.requires_confirmation(&tc.function.name)
                && registry
                    .check_tool_policy(&tc.function.name, policy_ctx)
                    .is_ok()
        })
        .map(|tc| tc.id.as_str())
        .collect::<Vec<_>>();
    let index = confirm_ids
        .iter()
        .position(|id| *id == tool_call_id)
        .map(|idx| idx + 1)?;
    Some(PendingConfirmationPosition {
        index,
        count: confirm_ids.len(),
    })
}

/// Skip-stub IDs for checkpoint persistence after appending one tool result.
pub fn skip_stub_ids_for_checkpoint(
    cp: &HarnessCheckpoint,
    registry: &ToolRegistry,
    policy_ctx: &ToolPolicyContext,
) -> Vec<String> {
    outstanding_confirm_ids(registry, &cp.messages, policy_ctx)
}

fn latest_assistant_tool_calls(messages: &[LlmMessage]) -> Option<(usize, &[ToolCall])> {
    let assistant_idx = messages
        .iter()
        .rposition(|m| m.tool_calls.as_ref().is_some_and(|calls| !calls.is_empty()))?;
    let calls = messages[assistant_idx].tool_calls.as_ref()?;
    Some((assistant_idx, calls))
}

fn responded_tool_ids(
    messages: &[LlmMessage],
    assistant_idx: usize,
) -> std::collections::HashSet<String> {
    let mut insert_at = assistant_idx + 1;
    while insert_at < messages.len() && matches!(messages[insert_at].role, MessageRole::Tool) {
        insert_at += 1;
    }
    messages[assistant_idx + 1..insert_at]
        .iter()
        .filter_map(|m| m.tool_call_id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::{AiScene, AutonomyLevel};

    fn make_tool_call(name: &str) -> ToolCall {
        ToolCall::new(format!("call-{name}"), name, "{}")
    }

    fn ctx(web: bool) -> ToolPolicyContext {
        ToolPolicyContext {
            task_policy: None,
            scene: AiScene::KnowledgeLookup,
            autonomy_level: AutonomyLevel::L2,
            web_search_enabled: web,
            skill_allowed_tools: vec![],
            depth: 0,
        }
    }

    fn assistant_with(calls: Vec<ToolCall>) -> Vec<LlmMessage> {
        vec![LlmMessage {
            role: MessageRole::Assistant,
            content: String::new().into(),
            tool_call_id: None,
            tool_calls: Some(calls),
            ..Default::default()
        }]
    }

    #[test]
    fn outstanding_confirm_ids_lists_unresponded_confirm_tools() {
        let registry = ToolRegistry::new();
        let web = make_tool_call("web_search");
        let fetch = make_tool_call("fetch_web_page");
        let mut messages = assistant_with(vec![web.clone(), fetch.clone()]);
        messages.push(LlmMessage {
            role: MessageRole::Tool,
            content: r#"{"results":[]}"#.into(),
            tool_call_id: Some(web.id.clone()),
            tool_calls: None,
            ..Default::default()
        });
        let ids = outstanding_confirm_ids(&registry, &messages, &ctx(true));
        assert_eq!(ids, vec![fetch.id]);
    }

    #[test]
    fn fetch_skipped_when_web_disabled() {
        let registry = ToolRegistry::new();
        let messages = assistant_with(vec![make_tool_call("fetch_web_page")]);
        assert!(outstanding_confirm_tool(&registry, &messages, &ctx(false)).is_none());
    }
}
