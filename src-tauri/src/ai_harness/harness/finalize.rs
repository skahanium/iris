//! Harness run completion and evidence export.

use crate::ai_runtime::deliberation::{
    append_verification_notice, save_deliberation_state, verification_notice, verify_completion,
    DeliberationInput, DeliberationState,
};
use crate::ai_runtime::evidence_ledger::EvidenceLedger;
use crate::ai_runtime::model_gateway::{TokenUsage, ToolCall};
use crate::ai_runtime::trace::{TraceRecorder, TraceStatus};
use crate::ai_runtime::ContextPacket;
use crate::app::AppState;
use crate::error::AppResult;

use super::token_estimator::UsageSource;
use super::tools::merge_tool_packets_into;
use super::types::{HarnessFinishReason, HarnessRunInput, HarnessRunResult};

pub(crate) struct FinishRunParams {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub tool_results: Vec<serde_json::Value>,
    pub usage: TokenUsage,
    pub harness_rounds: u32,
    pub pending_confirmation: bool,
    pub evidence_packets: Vec<ContextPacket>,
    pub usage_source: UsageSource,
    pub finish_reason: HarnessFinishReason,
}

pub(crate) async fn finish_run(
    state: &AppState,
    input: HarnessRunInput,
    params: FinishRunParams,
) -> AppResult<HarnessRunResult> {
    let FinishRunParams {
        mut content,
        tool_calls,
        tool_results,
        usage,
        harness_rounds,
        pending_confirmation,
        evidence_packets,
        usage_source,
        finish_reason,
    } = params;
    let citation_result =
        crate::ai_runtime::guardrails::verify_citations(&content, &evidence_packets);
    let citation_valid = matches!(
        citation_result,
        crate::ai_runtime::guardrails::GuardResult::Pass
    );
    let trace_status = if pending_confirmation {
        TraceStatus::AwaitingToolConfirmation
    } else {
        TraceStatus::Completed
    };
    let deliberation_state = DeliberationState::from_input(DeliberationInput {
        request_id: input.request_id.clone(),
        session_id: input.session_id,
        user_goal: input.user_message.clone(),
        evidence_packet_count: evidence_packets.len(),
        tool_result_count: tool_results.len(),
        max_rounds: input.task_policy.max_agentic_rounds,
        token_budget: input
            .token_budget
            .unwrap_or(input.task_policy.max_token_budget),
    });
    let verification_summary = verify_completion(
        deliberation_state.clone(),
        &content,
        &evidence_packets,
        finish_reason,
    );
    let notice = verification_notice(&verification_summary, finish_reason);
    content = append_verification_notice(&content, notice.as_ref());
    let _ = save_deliberation_state(&state.db, &deliberation_state, &verification_summary);
    TraceRecorder::update_status(&state.db, &input.request_id, trace_status)?;
    Ok(HarnessRunResult {
        request_id: input.request_id,
        session_id: input.session_id,
        content,
        tool_calls,
        tool_results,
        usage,
        citation_valid,
        harness_rounds,
        pending_confirmation,
        evidence_packets,
        usage_source,
        finish_reason,
        deliberation_state: Some(deliberation_state),
        verification_summary: Some(verification_summary),
    })
}

pub(crate) fn ingest_tool_packets(
    ledger: &mut EvidenceLedger,
    tool_name: &str,
    output: &serde_json::Value,
) {
    let mut batch = Vec::new();
    merge_tool_packets_into(tool_name, output, &mut batch);
    ledger.ingest_many(batch);
}

pub(crate) fn ledger_to_packets(ledger: &EvidenceLedger, token_budget: u32) -> Vec<ContextPacket> {
    let clone = ledger.clone();
    clone.into_packets(token_budget as usize)
}
