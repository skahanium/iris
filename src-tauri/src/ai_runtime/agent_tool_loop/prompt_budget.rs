//! Prompt-budget management for the Agent tool loop.
//!
//! Split out of `agent_tool_loop.rs` to stay inside the repository file-size
//! budget (`npm run size:check`).

use super::*;

/// Smallest tool observation worth keeping verbatim before the trace is compacted.
const MIN_TOOL_OBSERVATION_CHARS: usize = 400;

/// Marker left in place of a compacted tool result.
const COMPACTED_TOOL_OBSERVATION: &str =
    "{\"success\":true,\"output\":null,\"compacted\":true,\"note\":\"earlier tool result compacted to fit this turn's prompt budget\"}";

/// Compact the current Run's accumulated tool observations when they alone push
/// the prompt over budget.
///
/// The tool trace grows with every dispatched call and previously had no
/// self-management: once history compaction had done its part, an over-budget
/// prompt failed the turn outright. Replacing the **oldest** large tool results
/// with a bounded marker keeps the Run alive, keeps every `tool_call_id`
/// answerable, and leaves the newest observations — the ones the current
/// reasoning depends on — intact. Compaction never touches user, assistant or
/// system messages, and the caller still fails closed if the prompt remains over
/// budget (for example when a single non-tool message is oversized).
pub(crate) fn compact_tool_observations(
    messages: &mut [LlmMessage],
    tools: &[ToolSpec],
    budget: AgentModelTurnBudget,
) {
    let Some(limit) = budget.max_prompt_tokens else {
        return;
    };
    let mut compacted = 0_u32;
    while estimate_prompt_tokens(messages, tools) > limit {
        let Some(index) = messages.iter().enumerate().find_map(|(index, message)| {
            (matches!(message.role, MessageRole::Tool)
                && message.content.text_content().chars().count() > MIN_TOOL_OBSERVATION_CHARS)
                .then_some(index)
        }) else {
            return;
        };
        messages[index].content =
            crate::ai_types::MessageContent::Text(COMPACTED_TOOL_OBSERVATION.to_string());
        compacted += 1;
        if compacted > 64 {
            return;
        }
    }
}
