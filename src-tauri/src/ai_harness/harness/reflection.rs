//! Harness reflection round and bonus evidence retrieval.

use tauri::AppHandle;

use super::finalize::{finish_run, ledger_to_packets, FinishRunParams};
use super::token_estimator::{estimate_and_accumulate, usage_is_empty, UsageSource};
use super::trace_emit::{emit_thinking, emit_trace_phase};
use super::types::{HarnessFinishReason, HarnessPhase, HarnessRunInput, HarnessRunResult};
use super::util::accumulate_usage;
use crate::ai_runtime::evidence_ledger::EvidenceLedger;
use crate::ai_runtime::harness_support::extract_thinking_blocks;
use crate::ai_runtime::model_gateway::{
    GatewayRequest, LlmMessage, MessageRole, ModelGateway, TokenUsage, ToolCall,
};
use crate::ai_runtime::tool_fallback::strip_tool_markup_from_visible;
use crate::app::AppState;
use crate::error::AppResult;

/// Outcome of a reflection LLM call.
pub(crate) enum ReflectionOutcome {
    /// Continue agent loop with an extra evidence round.
    BonusRound,
    /// Final answer ready.
    Done(Box<HarnessRunResult>),
    /// Reflection did not produce a final answer; caller should fall through.
    NoAnswer,
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_reflection_round(
    state: &AppState,
    app_handle: &AppHandle,
    input: &HarnessRunInput,
    gateway: &ModelGateway,
    provider_config: &crate::ai_runtime::model_gateway::ProviderConfig,
    max_tokens: Option<u32>,
    thinking: bool,
    messages: &mut Vec<LlmMessage>,
    evidence_ledger: &EvidenceLedger,
    all_tool_calls: &[ToolCall],
    tool_results_json: &[serde_json::Value],
    total_usage: &mut TokenUsage,
    harness_rounds: u32,
    pending_confirmation: bool,
    bonus_round_used: &mut bool,
    max_rounds: &mut u32,
    token_budget: u32,
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
    )?;
    messages.push(LlmMessage {
        role: MessageRole::User,
        content: "请审视当前证据是否足以准确回答用户。若不足，回复 NEED_MORE_EVIDENCE；否则直接给出完整回答（勿再调用工具）。"
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
        temperature: Some(0.5),
        stream: false,
        thinking,
        skip_stub_ids: vec![],
    };
    if let Ok(reflect_resp) = gateway.send_request(reflect_request).await {
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
                return Ok(ReflectionOutcome::BonusRound);
            }
            let stripped = strip_tool_markup_from_visible(&text);
            let (visible, thinking) = extract_thinking_blocks(&stripped);
            if let Some(t) = thinking {
                emit_thinking(app_handle, &input.request_id, harness_rounds, &t)?;
            }
            if let Some(content) = sanitize_reflection_visible(&visible) {
                return Ok(ReflectionOutcome::Done(Box::new(
                    finish_run(
                        state,
                        input.clone(),
                        FinishRunParams {
                            content,
                            tool_calls: all_tool_calls.to_vec(),
                            tool_results: tool_results_json.to_vec(),
                            usage: total_usage.clone(),
                            harness_rounds,
                            pending_confirmation,
                            evidence_packets: ledger_to_packets(evidence_ledger, token_budget),
                            usage_source: *usage_source,
                            finish_reason: if pending_confirmation {
                                HarnessFinishReason::AwaitingConfirmation
                            } else {
                                HarnessFinishReason::Completed
                            },
                        },
                    )
                    .await?,
                )));
            }
        }
    }
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
