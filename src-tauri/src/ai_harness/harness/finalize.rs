//! Harness run completion and evidence export.

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
        content,
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
