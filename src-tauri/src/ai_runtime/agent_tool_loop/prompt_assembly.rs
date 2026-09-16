//! Prompt assembly and token-budget helpers for the tool loop.
//!
//! Split out of `agent_tool_loop.rs` to keep that file inside the module size
//! budget. The parent re-exports these items, so callers are unaffected.

use super::*;

/// Open the loop with the behavior this bounded exchange expects.
///
/// The exact Host allowances are deliberately absent: the Host enforces them,
/// the model only needs to know that the loop is bounded and that each
/// observation reports whether another action is still available. Publishing
/// the counters here made the model repeat internal budget to the user.
pub(crate) fn initial_loop_budget_instruction() -> LlmMessage {
    LlmMessage {
        role: MessageRole::System,
        content: "This Run uses one bounded observation-action loop. Its allowances are Host-enforced maxima, not targets. Each observation reports whether you may continue and what to do next. Choose only actions that can add information, inspect each returned observation before dependent actions, and finish as soon as the user goal is adequately supported. Do not reveal private reasoning or describe the run's internal allowances to the user.".into(),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    }
}

pub(crate) fn tool_surface_closed_instruction() -> LlmMessage {
    LlmMessage {
        role: MessageRole::System,
        content: "Tool work is now closed because it has reached a bounded limit or produced no new safe resources in two complete rounds. Synthesize the best answer from the current transcript, state material uncertainty plainly, and do not request another business tool.".into(),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    }
}

pub(crate) fn missing_evidence_repair_instruction() -> LlmMessage {
    LlmMessage {
        role: MessageRole::System,
        content: "The previous draft cannot be shown because this Run requires current evidence and no usable evidence has been registered. Use an available authorized read tool now to verify the answer. Do not invent facts or claim that you searched when no tool result is present.".into(),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    }
}

pub(crate) fn source_binding_repair_instruction() -> LlmMessage {
    LlmMessage {
        role: MessageRole::System,
        content: "The previous draft cannot be shown because it does not bind its factual claims to this Run's sources. Revise it using only the existing transcript and cite the supplied Run-local source labels precisely. Do not call further business tools or invent a source.".into(),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    }
}

/// Update the already-compiled system message after the bounded compaction
/// turn succeeds. The final answer must see the same durable summary that was
/// just persisted; deferring it until the next Run made the compression call
/// consume budget without improving the active answer.
pub(crate) fn replace_conversation_memory_in_current_messages(
    messages: &mut [LlmMessage],
    prior_fragment: &str,
    updated_fragment: &str,
) {
    let Some(system_message) = messages
        .iter_mut()
        .find(|message| matches!(message.role, MessageRole::System))
    else {
        return;
    };
    let Some(content) = system_message.content.as_mut_str() else {
        return;
    };
    let prior_block = format!("{prior_fragment}{CONVERSATION_HISTORY_COVERAGE_WARNING}");
    if content.contains(&prior_block) {
        *content = content.replacen(&prior_block, updated_fragment, 1);
    } else if content.contains(prior_fragment) {
        *content = content.replacen(prior_fragment, updated_fragment, 1);
    }
}

pub(crate) fn enforce_prompt_budget(
    messages: &[LlmMessage],
    tools: &[ToolSpec],
    budget: AgentModelTurnBudget,
) -> AppResult<()> {
    if budget
        .max_prompt_tokens
        .is_some_and(|limit| estimate_prompt_tokens(messages, tools) > limit)
    {
        return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit));
    }
    Ok(())
}

pub(crate) fn resolved_turn_usage(
    response: &GatewayResponse,
    messages: &[LlmMessage],
    tools: &[ToolSpec],
) -> (u32, u32, u32) {
    let prompt_tokens = nonzero_or_estimate(
        response.usage.prompt_tokens,
        estimate_prompt_tokens(messages, tools),
    );
    let completion_tokens = nonzero_or_estimate(
        response.usage.completion_tokens,
        estimate_completion_tokens(response),
    );
    let total_tokens = nonzero_or_estimate(
        response.usage.total_tokens,
        prompt_tokens.saturating_add(completion_tokens),
    );
    (prompt_tokens, completion_tokens, total_tokens)
}

pub(crate) fn nonzero_or_estimate(reported: u32, estimate: u32) -> u32 {
    if reported == 0 {
        estimate
    } else {
        reported
    }
}

pub(crate) fn estimate_prompt_tokens(messages: &[LlmMessage], tools: &[ToolSpec]) -> u32 {
    let message_tokens = messages.iter().fold(0_u32, |total, message| {
        total.saturating_add(estimate_tokens(&message.content.text_content()))
    });
    let tool_tokens = serde_json::to_string(tools)
        .ok()
        .map(|serialized| estimate_tokens(&serialized))
        .unwrap_or_default();
    message_tokens.saturating_add(tool_tokens)
}

pub(crate) fn estimate_completion_tokens(response: &GatewayResponse) -> u32 {
    let content_tokens = response
        .content
        .as_deref()
        .map(estimate_tokens)
        .unwrap_or_default();
    let tool_tokens = if response.tool_calls.is_empty() {
        0
    } else {
        serde_json::to_string(&response.tool_calls)
            .ok()
            .map(|serialized| estimate_tokens(&serialized))
            .unwrap_or_default()
    };
    let reasoning_tokens = response
        .reasoning_content
        .as_deref()
        .map(estimate_tokens)
        .unwrap_or_default();
    content_tokens
        .saturating_add(tool_tokens)
        .saturating_add(reasoning_tokens)
}

pub(crate) fn estimate_tokens(value: &str) -> u32 {
    crate::ai_runtime::text_support::estimate_tokens(value)
        .try_into()
        .unwrap_or(u32::MAX)
}
