//! Harness checkpoint persistence.

use crate::ai_runtime::harness_support::{
    save_harness_checkpoint, HarnessCheckpoint, HarnessCheckpointMeta,
};
use crate::ai_runtime::model_gateway::{LlmMessage, TokenUsage, ToolCall};
use crate::ai_runtime::ContextPacket;
use crate::app::AppState;
use crate::error::AppResult;

use super::types::HarnessRunInput;
use super::UsageSource;

#[allow(clippy::too_many_arguments)]
pub(crate) fn save_round_checkpoint(
    state: &AppState,
    input: &HarnessRunInput,
    meta: &HarnessCheckpointMeta,
    round: u32,
    bonus_round_used: bool,
    messages: &[LlmMessage],
    tool_calls: &[ToolCall],
    tool_results: &[serde_json::Value],
    evidence_packets: &[ContextPacket],
    usage: &TokenUsage,
    usage_source: UsageSource,
) -> AppResult<()> {
    let checkpoint = HarnessCheckpoint {
        meta: meta.clone(),
        round,
        messages: messages.to_vec(),
        tool_calls: tool_calls.to_vec(),
        tool_results: tool_results.to_vec(),
        evidence_packets: evidence_packets.to_vec(),
        usage: usage.clone(),
        usage_source,
        bonus_round_used,
    };
    save_harness_checkpoint(&state.db, &input.request_id, &checkpoint)
}
