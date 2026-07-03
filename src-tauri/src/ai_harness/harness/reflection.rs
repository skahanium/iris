//! Harness reflection round and bonus evidence retrieval.

use tauri::AppHandle;

use super::token_estimator::{estimate_and_accumulate, usage_is_empty, UsageSource};
use super::trace_emit::{emit_thinking, emit_trace_phase};
use super::types::{HarnessPhase, HarnessRunInput};
use super::util::accumulate_usage;
use crate::ai_runtime::evidence_ledger::EvidenceLedger;
use crate::ai_runtime::harness_support::extract_thinking_blocks;
use crate::ai_runtime::model_gateway::{
    emit_stream_reset_with_surface, GatewayRequest, LlmMessage, MessageRole, ModelGateway,
    StreamSurface, TokenUsage, ToolCall,
};
use crate::ai_runtime::tool_fallback::strip_tool_markup_from_visible;
use crate::app::AppState;
use crate::error::AppResult;

/// Outcome of a reflection LLM call.
pub(crate) enum ReflectionOutcome {
    /// Continue agent loop with an extra evidence round.
    BonusRound,
    /// Reflection did not produce a final answer; caller should fall through.
    NoAnswer,
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_reflection_round(
    _state: &AppState,
    app_handle: &AppHandle,
    input: &HarnessRunInput,
    gateway: &ModelGateway,
    provider_config: &crate::ai_runtime::model_gateway::ProviderConfig,
    max_tokens: Option<u32>,
    thinking: bool,
    messages: &mut Vec<LlmMessage>,
    _evidence_ledger: &EvidenceLedger,
    _all_tool_calls: &[ToolCall],
    _tool_results_json: &[serde_json::Value],
    total_usage: &mut TokenUsage,
    harness_rounds: u32,
    _pending_confirmation: bool,
    bonus_round_used: &mut bool,
    max_rounds: &mut u32,
    _token_budget: u32,
    usage_source: &mut UsageSource,
) -> AppResult<ReflectionOutcome> {
    emit_trace_phase(
        app_handle,
        &input.request_id,
        harness_rounds,
        HarnessPhase::Reflection,
        "reflection",
        "running",
        None,
        None,
        None,
    )?;
    messages.push(LlmMessage {
        role: MessageRole::User,
        content: "请审视当前证据是否足以准确回答用户。若不足，只回复 NEED_MORE_EVIDENCE；若足够，只回复 EVIDENCE_SUFFICIENT。不要调用工具，不要生成最终正文。"
            .into(),
        tool_call_id: None,
        tool_calls: None,
        ..Default::default()
    });
    let reflect_request = GatewayRequest {
        provider: provider_config.clone(),
        messages: messages.clone(),
        tools: vec![],
        max_tokens,
        input_token_budget: input.input_budget,
        temperature: Some(0.5),
        stream: true,
        thinking,
        skip_stub_ids: vec![],
    };
    if let Ok(reflect_resp) = gateway
        .send_streaming_request_with_surface(
            &input.request_id,
            reflect_request,
            StreamSurface::InternalCandidate,
            true,
        )
        .await
    {
        if usage_is_empty(&reflect_resp.usage) {
            let content = reflect_resp.content.as_deref().unwrap_or("");
            estimate_and_accumulate(total_usage, messages, content);
            *usage_source = UsageSource::Estimated;
        } else {
            accumulate_usage(total_usage, &reflect_resp.usage);
        }
        if let Some(text) = reflect_resp.content {
            if text.contains("NEED_MORE_EVIDENCE")
                && !*bonus_round_used
                && harness_rounds < input.task_policy.max_agentic_rounds
            {
                *bonus_round_used = true;
                messages.push(LlmMessage {
                    role: MessageRole::Assistant,
                    content: text.into(),
                    tool_call_id: None,
                    tool_calls: None,
                    ..Default::default()
                });
                messages.push(LlmMessage {
                    role: MessageRole::User,
                    content: "证据仍不足，请继续使用检索类工具补充证据后再作答。".into(),
                    tool_call_id: None,
                    tool_calls: None,
                    ..Default::default()
                });
                *max_rounds = (harness_rounds + 1).min(input.task_policy.max_agentic_rounds);
                // Non-terminal: reflection returned the NEED_MORE_EVIDENCE
                // sentinel. The streamed sentinel must not reach the answer
                // surface; clear it before the bonus round re-streams.
                tracing::debug!(
                    request_id = %input.request_id,
                    event = "reflection_reset",
                    candidate_kind = "internal_candidate",
                    reason_kind = "need_more_evidence",
                    round = harness_rounds,
                    "AI lifecycle reflection reset"
                );
                emit_stream_reset_with_surface(
                    app_handle,
                    &input.request_id,
                    "need_more_evidence",
                    StreamSurface::InternalCandidate,
                    Some(harness_rounds),
                )?;
                return Ok(ReflectionOutcome::BonusRound);
            }
            let stripped = strip_tool_markup_from_visible(&text);
            let (visible, thinking) = extract_thinking_blocks(&stripped);
            if let Some(t) = thinking {
                emit_thinking(app_handle, &input.request_id, harness_rounds, &t)?;
            }
            if sanitize_reflection_visible(&visible).is_some() {
                tracing::debug!(
                    request_id = %input.request_id,
                    event = "reflection_sufficient",
                    candidate_kind = "internal_candidate",
                    round = harness_rounds,
                    "AI lifecycle reflection found sufficient evidence"
                );
                return Ok(ReflectionOutcome::NoAnswer);
            }
        }
    }
    // Non-terminal: reflection produced no usable answer. Any streamed content
    // was inconclusive; clear it before the caller falls through to FinalStream.
    tracing::debug!(
        request_id = %input.request_id,
        event = "reflection_reset",
        candidate_kind = "internal_candidate",
        reason_kind = "reflection_no_answer",
        round = harness_rounds,
        "AI lifecycle reflection reset"
    );
    emit_stream_reset_with_surface(
        app_handle,
        &input.request_id,
        "reflection_no_answer",
        StreamSurface::InternalCandidate,
        Some(harness_rounds),
    )?;
    Ok(ReflectionOutcome::NoAnswer)
}

pub(crate) fn sanitize_reflection_visible(text: &str) -> Option<String> {
    let cleaned = text.replace("NEED_MORE_EVIDENCE", "");
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_need_more_evidence_is_not_visible_answer() {
        assert!(sanitize_reflection_visible("NEED_MORE_EVIDENCE").is_none());
        assert!(sanitize_reflection_visible("  NEED_MORE_EVIDENCE\n").is_none());
    }

    #[test]
    fn normal_reflection_answer_is_preserved() {
        assert_eq!(
            sanitize_reflection_visible("今天是星期二。").as_deref(),
            Some("今天是星期二。")
        );
    }
}
