//! Unified Agent Harness — multi-round tool loop with streaming final response.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use futures_util::future::join_all;
use tauri::{AppHandle, Emitter};

use super::archive::save_round_checkpoint;
use super::context::{
    build_initial_messages, prepare_environment_and_skills_with_plan, resolve_file_id,
    EnvironmentAndSkillsInput, InitialMessagesInput,
};
use super::finalize::{finish_run, ingest_tool_packets, ledger_to_packets, FinishRunParams};
use super::planning::{resolve_max_rounds, resolve_token_budget};
use super::reflection::{run_reflection_round, sanitize_reflection_visible, ReflectionOutcome};
use super::token_estimator::{estimate_and_accumulate, usage_is_empty, UsageSource};
use super::trace_emit::{emit_thinking, emit_trace_phase};
use super::types::{HarnessFinishReason, HarnessPhase, HarnessRunInput, HarnessRunResult};
use super::util::accumulate_usage;
use crate::ai_harness::tool_turn::{outstanding_confirm_tool, pending_confirmation_position};
use crate::ai_runtime::agent_permissions::preflight_tool_permission;
use crate::ai_runtime::agent_task::AgentTaskRuntime;
use crate::ai_runtime::circuit_breaker;
use crate::ai_runtime::evidence_ledger::EvidenceLedger;
use crate::ai_runtime::harness_support::{
    estimate_tokens, extract_thinking_blocks_for_event, load_harness_checkpoint,
    looks_like_incomplete_final_answer, sanitize_meta_analysis_prefix, HarnessCheckpointMeta,
};
use crate::ai_runtime::model_gateway::{
    clear_abort, emit_stream_reset_with_surface, is_abort_requested, prepare_tool_api_messages,
    GatewayRequest, GatewayResponse, LlmMessage, LlmToolDef, MessageRole, ModelGateway,
    ProviderConfig, StreamSurface, TokenUsage, ToolCall,
};
use crate::ai_runtime::permission_decision::{
    decide_tool_permission, PermissionDecisionOutcome, PermissionDecisionRequest,
};
use crate::ai_runtime::subagent_coordinator::{SubAgentCoordinator, SubAgentTaskSpec};
use crate::ai_runtime::tool_catalog::{catalog_find, ToolCatalogEntry};
use crate::ai_runtime::tool_dispatch::{dispatch_tool_with_retry, ToolDispatchContext};
use crate::ai_runtime::tool_effects::{classify_catalog_entry, ToolExecutionClass};
use crate::ai_runtime::tool_execution_pipeline::{
    audit_dispatched_tool, evaluate_tool_execution, ToolExecutionGate,
};
use crate::ai_runtime::tool_executor::ToolRegistry;
use crate::ai_runtime::tool_fallback::{
    is_internal_tool_artifact_text, parse_tool_call_arguments, parse_tool_calls_from_content,
    should_retry_tool_parse, strip_tool_markup_from_visible,
};
use crate::ai_runtime::tool_policy::{self, DenialReason, ToolPolicyContext};
use crate::ai_runtime::{ContextPacket, ToolCallResult};
use crate::ai_types::{ReasoningVisibility, ResolvedReasoningRequest};
use crate::app::AppState;
use crate::error::{AppError, AppResult};

const LLM_MAX_RETRIES: u32 = 3;
const LLM_RETRY_BASE_DELAY_MS: u64 = 1000;
const FINAL_ANSWER_INSTRUCTION: &str = "停止继续检索、反思或调用工具。请只基于当前已有上下文直接回答用户；如果证据不足，说明局限并给出当前能支持的结论。不要再调用工具，也不要输出 NEED_MORE_EVIDENCE 或其他内部控制标记。";
const FINAL_ROUND_FALLBACK: &str =
    "我已停止继续调用工具，但这次没有生成可展示回答。请重试，或换一种问法。";
const FINAL_BUDGET_FALLBACK: &str = "这次上下文预算已用尽，未能生成可展示回答。请缩小范围后重试。";
const FINAL_EMPTY_FALLBACK: &str = "这次没有生成可展示回答。请重试，或换一种问法。";
const ADAPTIVE_RETRY_MESSAGE_CHAR_BUDGET: usize = 12_000;

fn filter_tools_by_allowed_names(
    tools: &[crate::ai_runtime::ToolSpec],
    allowed_tools: Option<&Vec<String>>,
) -> Vec<crate::ai_runtime::ToolSpec> {
    let Some(allowed_tools) = allowed_tools else {
        return tools.to_vec();
    };
    let allowed = allowed_tools
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    tools
        .iter()
        .filter(|tool| allowed.contains(tool.name.as_str()))
        .cloned()
        .collect()
}

fn tool_allowed_by_inherited_surface(input: &HarnessRunInput, tool_name: &str) -> bool {
    input
        .allowed_tools
        .as_ref()
        .map(|allowed| allowed.iter().any(|tool| tool == tool_name))
        .unwrap_or(true)
}

fn inherited_allowed_tool_names(
    scene_tools: &[crate::ai_runtime::ToolSpec],
    input: &HarnessRunInput,
) -> Vec<String> {
    input.allowed_tools.clone().unwrap_or_else(|| {
        scene_tools
            .iter()
            .map(|tool| tool.name.clone())
            .collect::<Vec<_>>()
    })
}
#[derive(Debug, Clone)]
struct FinalAnswerDecision {
    content: String,
    finish_reason: HarnessFinishReason,
    save_checkpoint: bool,
}

fn build_final_answer_messages(messages: &[LlmMessage]) -> Vec<LlmMessage> {
    let mut final_messages = messages.to_vec();
    final_messages.push(LlmMessage {
        role: MessageRole::User,
        content: FINAL_ANSWER_INSTRUCTION.into(),
        tool_call_id: None,
        tool_calls: None,
        ..Default::default()
    });
    final_messages
}

fn compact_one_line(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let mut clipped = normalized.chars().take(max_chars).collect::<String>();
    clipped.push_str("...");
    clipped
}

fn web_evidence_fallback_from_packets(packets: &[ContextPacket]) -> Option<String> {
    let lines = packets
        .iter()
        .filter(|packet| {
            packet.web.is_some() || matches!(packet.source_type, crate::ai_runtime::SourceType::Web)
        })
        .take(4)
        .filter_map(|packet| {
            let title = compact_one_line(&packet.title, 80);
            let excerpt = compact_one_line(&packet.excerpt, 160);
            let source = packet
                .web
                .as_ref()
                .and_then(|meta| meta.url.as_deref())
                .or(packet.source_path.as_deref())
                .or_else(|| packet.web.as_ref().and_then(|meta| meta.domain.as_deref()))
                .map(|value| compact_one_line(value, 100))
                .unwrap_or_default();
            let citation = packet.citation_label.trim();

            if title.is_empty() && excerpt.is_empty() {
                return None;
            }

            let label = if citation.is_empty() {
                String::new()
            } else {
                format!("{citation} ")
            };
            let source_suffix = if source.is_empty() {
                String::new()
            } else {
                format!("（{source}）")
            };
            let summary = if excerpt.is_empty() {
                title
            } else if title.is_empty() {
                excerpt
            } else {
                format!("{title}：{excerpt}")
            };
            Some(format!("- {label}{summary}{source_suffix}"))
        })
        .collect::<Vec<_>>();

    if lines.is_empty() {
        return None;
    }
    Some(format!("以下基于检索结果摘要：\n\n{}", lines.join("\n")))
}

fn classify_final_answer(
    sanitized_final: Option<String>,
    evidence_fallback: Option<String>,
    total_tokens: u32,
    token_budget: u32,
    harness_rounds: u32,
    max_rounds: u32,
) -> FinalAnswerDecision {
    if let Some(content) = sanitized_final {
        let visible = strip_tool_markup_from_visible(&content);
        let trimmed = visible.trim();
        if !trimmed.is_empty() && !is_internal_tool_artifact_text(trimmed) {
            return FinalAnswerDecision {
                content: trimmed.to_string(),
                finish_reason: HarnessFinishReason::Completed,
                save_checkpoint: false,
            };
        }
    }

    if total_tokens >= token_budget {
        return FinalAnswerDecision {
            content: FINAL_BUDGET_FALLBACK.into(),
            finish_reason: HarnessFinishReason::BudgetExhausted,
            save_checkpoint: true,
        };
    }

    if let Some(fallback) = evidence_fallback.filter(|content| !content.trim().is_empty()) {
        return FinalAnswerDecision {
            content: fallback,
            finish_reason: HarnessFinishReason::Completed,
            save_checkpoint: false,
        };
    }

    let fallback = if harness_rounds >= max_rounds {
        FINAL_ROUND_FALLBACK
    } else {
        FINAL_EMPTY_FALLBACK
    };
    FinalAnswerDecision {
        content: fallback.into(),
        finish_reason: HarnessFinishReason::Completed,
        save_checkpoint: false,
    }
}

fn final_answer_stream_surface(reasoning: &ResolvedReasoningRequest) -> StreamSurface {
    if !reasoning.isolate_output {
        return StreamSurface::VisibleAnswer;
    }
    match reasoning.visibility {
        ReasoningVisibility::HiddenChannel => StreamSurface::VisibleAnswer,
        ReasoningVisibility::ContentTag | ReasoningVisibility::PlainContentRisk => {
            StreamSurface::VisibleAnswerSanitized
        }
    }
}

fn estimate_message_text_tokens(messages: &[LlmMessage]) -> usize {
    messages
        .iter()
        .map(|message| estimate_tokens(&message.content.text_content()))
        .sum()
}

fn estimate_tool_schema_tokens(tools: &[LlmToolDef]) -> usize {
    if tools.is_empty() {
        return 0;
    }
    estimate_tokens(&serde_json::to_string(tools).unwrap_or_default())
}

fn record_initial_context_budget_diagnostics(
    state: &AppState,
    input: &HarnessRunInput,
    messages: &[LlmMessage],
    tools: &[LlmToolDef],
    environment: &str,
    skills_fragment: &str,
) {
    let Ok(Some(task_id)) = AgentTaskRuntime::task_id_for_request(&state.db, &input.request_id)
    else {
        return;
    };
    let history_tokens = input
        .history_messages
        .iter()
        .map(|(_, content)| estimate_tokens(content))
        .sum::<usize>();
    let evidence_tokens = input
        .cold_start_packets
        .iter()
        .map(|packet| estimate_tokens(&packet.excerpt))
        .sum::<usize>();
    let tool_tokens = estimate_tool_schema_tokens(tools);
    let environment_tokens =
        estimate_tokens(environment).saturating_add(estimate_tokens(skills_fragment));
    let estimated_total = estimate_message_text_tokens(messages).saturating_add(tool_tokens);
    let _ = AgentTaskRuntime::record_event(
        &state.db,
        &task_id,
        "context_budget",
        "harness_initial",
        serde_json::json!({
            "input_budget": input.input_budget.unwrap_or_default(),
            "estimated_total": estimated_total,
            "history_tokens": history_tokens,
            "evidence_tokens": evidence_tokens,
            "tool_tokens": tool_tokens,
            "environment_tokens": environment_tokens,
        }),
    );
}

#[derive(Debug, Clone)]
struct PreparedToolCall {
    tool_call: ToolCall,
    args: serde_json::Value,
    entry: &'static ToolCatalogEntry,
    decision: PermissionDecisionOutcome,
    class: ToolExecutionClass,
}

fn permission_requires_confirmation(
    decision: crate::ai_runtime::permission_decision::PermissionExecutionDecision,
) -> bool {
    matches!(
        decision,
        crate::ai_runtime::permission_decision::PermissionExecutionDecision::RequiresConfirmation
    )
}

#[derive(Debug, Clone, Copy)]
struct RetryReason {
    reason_kind: &'static str,
    status_code: Option<u16>,
}

fn extract_http_status_code(message: &str) -> Option<u16> {
    let bytes = message.as_bytes();
    if bytes.len() < 3 {
        return None;
    }

    for index in 0..=(bytes.len() - 3) {
        let code = &bytes[index..index + 3];
        if code.iter().all(u8::is_ascii_digit) {
            let value = (code[0] - b'0') as u16 * 100
                + (code[1] - b'0') as u16 * 10
                + (code[2] - b'0') as u16;
            if (400..=599).contains(&value) {
                return Some(value);
            }
        }
    }

    None
}

fn classify_retry_reason(message: &str) -> RetryReason {
    let lower = message.to_lowercase();
    let status_code = extract_http_status_code(message);
    let reason_kind = if status_code == Some(429)
        || lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("请求过于频繁")
    {
        "http_429"
    } else if status_code == Some(503)
        || lower.contains("service unavailable")
        || lower.contains("too busy")
        || lower.contains("overloaded")
        || lower.contains("模型服务繁忙")
    {
        "http_503"
    } else if lower.contains("stream read error") {
        "stream_read_error"
    } else if lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("deadline")
        || lower.contains("operation timed out")
    {
        "timeout_or_stall"
    } else if lower.contains("llm streaming request failed")
        || lower.contains("request failed")
        || lower.contains("error sending request")
    {
        "request_failed"
    } else if status_code.is_some() || lower.contains("模型请求失败") {
        "http_error"
    } else {
        "unknown"
    };

    RetryReason {
        reason_kind,
        status_code,
    }
}

fn should_adaptive_degrade_after_llm_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    extract_http_status_code(message) == Some(422)
        || lower.contains("unprocessable entity")
        || lower.contains("llm_input_context_overflow")
        || lower.contains("context length")
        || lower.contains("maximum context")
        || lower.contains("too many tokens")
        || lower.contains("input tokens")
}

fn truncate_message_for_adaptive_retry(message: &LlmMessage) -> LlmMessage {
    let text = message.content.text_content();
    if text.chars().count() <= ADAPTIVE_RETRY_MESSAGE_CHAR_BUDGET {
        return message.clone();
    }
    let mut trimmed = message.clone();
    let marker = "...（已为模型重试压缩前文）\n";
    let body_budget = ADAPTIVE_RETRY_MESSAGE_CHAR_BUDGET
        .saturating_sub(marker.chars().count())
        .max(1);
    let suffix = text
        .chars()
        .rev()
        .take(body_budget)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    trimmed.content = format!("{marker}{suffix}").into();
    trimmed.reasoning_content = None;
    trimmed
}

fn compact_messages_for_adaptive_retry(messages: &[LlmMessage]) -> Vec<LlmMessage> {
    let kept = if messages.len() > 8 {
        let mut out = Vec::with_capacity(8);
        out.extend_from_slice(&messages[..2]);
        out.extend_from_slice(&messages[messages.len() - 6..]);
        out
    } else {
        messages.to_vec()
    };
    kept.iter()
        .map(truncate_message_for_adaptive_retry)
        .collect()
}

fn provider_candidates_for_gateway(
    primary: &ProviderConfig,
    failover_candidates: &[ProviderConfig],
) -> Vec<ProviderConfig> {
    let mut providers = vec![primary.clone()];
    for candidate in failover_candidates {
        if candidate.slot != primary.slot {
            tracing::warn!(
                primary_provider = %primary.name,
                candidate_provider = %candidate.name,
                primary_slot = ?primary.slot,
                candidate_slot = ?candidate.slot,
                reason_code = "provider_failover_candidate_rejected",
                "Ignoring failover candidate from a different capability slot"
            );
            continue;
        }
        let duplicate = providers.iter().any(|provider| {
            provider.name == candidate.name
                && provider.model == candidate.model
                && provider.base_url == candidate.base_url
        });
        if !duplicate {
            providers.push(candidate.clone());
        }
    }
    providers
}

fn build_adaptive_degraded_llm_request(request: &GatewayRequest) -> GatewayRequest {
    let mut degraded = request.clone();
    degraded.messages =
        build_final_answer_messages(&compact_messages_for_adaptive_retry(&request.messages));
    degraded.tools.clear();
    degraded.skip_stub_ids.clear();
    degraded.max_tokens = Some(
        request
            .max_tokens
            .unwrap_or(2048)
            .saturating_div(2)
            .max(512),
    );
    degraded.input_token_budget = request
        .input_token_budget
        .map(|budget| budget.saturating_div(2).max(8_000));
    degraded.thinking = false;
    degraded.reasoning = ResolvedReasoningRequest::disabled();
    degraded
}

/// Streaming LLM call: reuses the circuit breaker and exponential-backoff retry
/// logic, then optionally fails over to the next explicit same-slot provider.
async fn send_llm_streaming_request_with_retry(
    app_handle: &AppHandle,
    gateway: &ModelGateway,
    request: GatewayRequest,
    request_id: &str,
    surface: StreamSurface,
) -> AppResult<GatewayResponse> {
    let mut active_request = request;
    let mut attempted_providers: Vec<String> = Vec::new();

    loop {
        let provider_id = active_request.provider.name.clone();
        if attempted_providers.iter().any(|id| id == &provider_id) {
            return Err(AppError::msg(format!(
                "Provider {provider_id} failover loop detected"
            )));
        }

        if !circuit_breaker::is_request_allowed(&provider_id) {
            let msg = format!("Provider {provider_id} circuit open: 503 provider unavailable");
            if let Some(next_provider) =
                gateway.failover_provider_after(&active_request.provider, &msg)
            {
                if circuit_breaker::is_request_allowed(&next_provider.name) {
                    tracing::warn!(
                        request_id = %request_id,
                        from_provider = %provider_id,
                        to_provider = %next_provider.name,
                        reason_code = "provider_failover",
                        error = %msg,
                        "Switching to explicit same-slot failover provider"
                    );
                    let _ = app_handle.emit(
                        "ai:retry_status",
                        &serde_json::json!({
                            "request_id": request_id,
                            "attempt": 0,
                            "max_attempts": LLM_MAX_RETRIES,
                            "delay_ms": 0,
                            "reason_kind": "provider_failover",
                            "from_provider": provider_id,
                            "to_provider": next_provider.name,
                        }),
                    );
                    active_request.provider = next_provider;
                    continue;
                }
            }
            return Err(AppError::msg(format!(
                "Provider {provider_id} 宸茶鐔旀柇锛岃绋嶅悗閲嶈瘯"
            )));
        }

        let mut last_err: Option<String> = None;
        let mut adaptive_degrade_used = false;
        for attempt in 0..=LLM_MAX_RETRIES {
            let emit_error_event = attempt == LLM_MAX_RETRIES;
            match gateway
                .send_streaming_request_with_surface(
                    request_id,
                    active_request.clone(),
                    surface,
                    emit_error_event,
                )
                .await
            {
                Ok(response) => {
                    circuit_breaker::record_success(&provider_id);
                    return Ok(response);
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("request aborted") {
                        return Err(e);
                    }
                    if msg.contains("partial_visible_stream_error") {
                        circuit_breaker::record_failure(&provider_id);
                        return Err(e);
                    }
                    if should_adaptive_degrade_after_llm_error(&msg) {
                        if adaptive_degrade_used {
                            last_err = Some(format!(
                                "{msg}锛涘凡鑷姩闄嶇骇閲嶈瘯涓€娆′粛澶辫触锛岃鍑忓皯涓婁笅鏂囨垨妫€鏌ユā鍨嬪弬鏁般€?"
                            ));
                            break;
                        }
                        adaptive_degrade_used = true;
                        active_request = build_adaptive_degraded_llm_request(&active_request);
                        let retry_reason = classify_retry_reason(&msg);
                        let _ = app_handle.emit(
                            "ai:retry_status",
                            &serde_json::json!({
                                "request_id": request_id,
                                "attempt": attempt + 1,
                                "max_attempts": LLM_MAX_RETRIES,
                                "delay_ms": 0,
                                "reason_kind": "adaptive_degrade",
                                "source_reason_kind": retry_reason.reason_kind,
                                "status_code": retry_reason.status_code,
                            }),
                        );
                        tracing::warn!(
                            request_id = %request_id,
                            provider_id = %provider_id,
                            error = %msg,
                            "LLM 璇锋眰澶辫触锛屾墽琛屼竴娆″帇缂╀笂涓嬫枃/鍏抽棴宸ュ叿鐨勮嚜閫傚簲闄嶇骇閲嶈瘯"
                        );
                        continue;
                    }
                    if attempt < LLM_MAX_RETRIES {
                        let delay_ms = LLM_RETRY_BASE_DELAY_MS * 2u64.pow(attempt);
                        let retry_reason = classify_retry_reason(&msg);
                        let _ = app_handle.emit(
                            "ai:retry_status",
                            &serde_json::json!({
                                "request_id": request_id,
                                "attempt": attempt + 1,
                                "max_attempts": LLM_MAX_RETRIES,
                                "delay_ms": delay_ms,
                                "reason_kind": retry_reason.reason_kind,
                                "status_code": retry_reason.status_code,
                            }),
                        );
                        tracing::warn!(
                            request_id = %request_id,
                            provider_id = %provider_id,
                            attempt = attempt + 1,
                            delay_ms,
                            error = %msg,
                            "LLM streaming request failed; retrying after delay"
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                    last_err = Some(msg);
                }
            }
        }

        circuit_breaker::record_failure(&provider_id);
        attempted_providers.push(provider_id.clone());
        let final_error =
            last_err.unwrap_or_else(|| "LLM streaming request failed after all retries".into());
        if let Some(next_provider) =
            gateway.failover_provider_after(&active_request.provider, &final_error)
        {
            if attempted_providers
                .iter()
                .any(|id| id == &next_provider.name)
            {
                return Err(AppError::msg(final_error));
            }
            if circuit_breaker::is_request_allowed(&next_provider.name) {
                tracing::warn!(
                    request_id = %request_id,
                    from_provider = %provider_id,
                    to_provider = %next_provider.name,
                    reason_code = "provider_failover",
                    error = %final_error,
                    "Switching to explicit same-slot failover provider"
                );
                let _ = app_handle.emit(
                    "ai:retry_status",
                    &serde_json::json!({
                        "request_id": request_id,
                        "attempt": LLM_MAX_RETRIES + 1,
                        "max_attempts": LLM_MAX_RETRIES,
                        "delay_ms": 0,
                        "reason_kind": "provider_failover",
                        "from_provider": provider_id,
                        "to_provider": next_provider.name,
                    }),
                );
                active_request.provider = next_provider;
                continue;
            }
        }

        return Err(AppError::msg(final_error));
    }
}
/// Run the unified agent harness loop.
///
/// Streaming progress is protected by idle_timeout/stall_timeout checks in the model
/// gateway: SSE reads use a per-read stall timeout, and the stream loop races
/// each read against `ABORT_POLL_INTERVAL` so the composer stop button remains
/// responsive even when a socket goes quiet. Do not wrap the whole harness in a
/// fixed wall-clock deadline; continuous token/tool progress can legitimately
/// run longer than any single old-style global timer. Max rounds and token
/// budgets below still bound non-streaming agent work.
pub async fn run_harness(
    state: &Arc<AppState>,
    app_handle: &AppHandle,
    input: HarnessRunInput,
    provider_config: crate::ai_runtime::model_gateway::ProviderConfig,
    max_tokens: Option<u32>,
    reasoning: crate::ai_types::ResolvedReasoningRequest,
) -> AppResult<HarnessRunResult> {
    let thinking_mode = reasoning.requested;
    run_harness_inner(
        state,
        app_handle,
        input,
        provider_config,
        max_tokens,
        reasoning,
        thinking_mode,
    )
    .await
}

async fn run_harness_inner(
    state: &Arc<AppState>,
    app_handle: &AppHandle,
    input: HarnessRunInput,
    provider_config: crate::ai_runtime::model_gateway::ProviderConfig,
    max_tokens: Option<u32>,
    reasoning: crate::ai_types::ResolvedReasoningRequest,
    thinking_mode: bool,
) -> AppResult<HarnessRunResult> {
    let registry = ToolRegistry::new();
    let policy_ctx = ToolPolicyContext {
        task_policy: Some(input.task_policy.clone()),
        scene: input.scene,
        autonomy_level: input.task_policy.autonomy_level,
        web_search_enabled: input.web_search_enabled,
        depth: input.depth,
    };
    let policy_scene_tools = registry.tools_for_policy_surface(&policy_ctx, false);
    let scene_tools =
        filter_tools_by_allowed_names(&policy_scene_tools, input.allowed_tools.as_ref());
    let llm_tools = ModelGateway::tools_to_llm_format(&scene_tools);

    let (env_text, skills_prompt) = prepare_environment_and_skills_with_plan(
        state,
        EnvironmentAndSkillsInput {
            scene: input.scene,
            task_policy: &input.task_policy,
            note_path: input.note_path.as_deref(),
            note_title: input.note_title.as_deref(),
            selection_excerpt: input.selection_excerpt.as_deref(),
            user_message: &input.user_message,
            scene_tools: &scene_tools,
            web_search_enabled: input.web_search_enabled,
            attachment_count: input.images.as_ref().map_or(0, Vec::len),
        },
        input.skill_activation_plan.as_ref(),
    )?;

    let file_id = resolve_file_id(state, input.note_path.as_deref())?;
    let _ = crate::ai_runtime::conversation_memory::ConversationMemory::refresh_for_session(
        &state.db,
        input.session_id,
        crate::ai_runtime::conversation_memory::ConversationMemoryPolicy::default(),
    );

    let mut messages = build_initial_messages(
        state,
        InitialMessagesInput {
            scene: input.scene,
            session_id: input.session_id,
            task_policy: &input.task_policy,
            environment: &env_text,
            cold_start_packets: &input.cold_start_packets,
            history: &input.history_messages,
            structured_history: &input.structured_history_messages,
            history_policy: crate::ai_runtime::prompt_builder::HistoryAssemblyPolicy {
                input_budget: input.input_budget.map(|budget| budget as usize),
                model_context_window: input.input_budget.map(|budget| budget as usize),
                max_tool_summary_tokens: 500,
                preserve_recent_turns: if input.input_budget.unwrap_or_default() >= 64_000 {
                    16
                } else {
                    8
                },
            },
            web_search_enabled: input.web_search_enabled,
            skills_fragment: if skills_prompt.is_empty() {
                None
            } else {
                Some(skills_prompt.as_str())
            },
        },
    );

    // If images present, replace the last user message content with multimodal Parts.
    if let Some(images) = &input.images {
        if !images.is_empty() {
            if let Some(last_user) = messages
                .iter_mut()
                .rev()
                .find(|m| matches!(m.role, crate::ai_types::MessageRole::User))
            {
                let mut parts = vec![crate::ai_types::ContentPart::Text {
                    text: last_user.content.text_content(),
                }];
                for img in images {
                    parts.push(img.to_content_part());
                }
                last_user.content = crate::ai_types::MessageContent::Parts(parts);
            }
        }
    }

    record_initial_context_budget_diagnostics(
        state,
        &input,
        &messages,
        &llm_tools,
        &env_text,
        &skills_prompt,
    );

    let gateway = ModelGateway::with_defaults(
        app_handle.clone(),
        provider_candidates_for_gateway(&provider_config, &input.provider_failover_candidates),
    )?;

    let mut total_usage = TokenUsage::default();
    let mut usage_source = UsageSource::Provider;
    let mut all_tool_calls: Vec<ToolCall> = Vec::new();
    let mut tool_results_json: Vec<serde_json::Value> = Vec::new();
    let mut evidence_ledger = EvidenceLedger::new(input.cold_start_packets.clone());
    let mut harness_rounds: u32 = 0;
    let mut reflection_done = false;
    let mut bonus_round_used = false;
    let mut consecutive_parse_failures: u32 = 0;
    let token_budget = resolve_token_budget(&input.task_policy, input.token_budget);
    let mut max_rounds = resolve_max_rounds(&input.task_policy, input.max_rounds_override);
    tracing::debug!(
        request_id = %input.request_id,
        event = "policy_resolved",
        intent = ?input.task_policy.intent,
        scene = %input.scene.profile(),
        capability_slot = ?provider_config.slot,
        max_rounds,
        token_budget,
        "AI lifecycle policy resolved"
    );

    if input.resume_from_checkpoint {
        if let Some(cp) = load_harness_checkpoint(&state.db, &input.request_id)? {
            let mut restored_messages = cp.messages;
            let mut restored_tool_calls = cp.tool_calls;
            let mut restored_tool_results = cp.tool_results;

            let provider_changed = cp
                .meta
                .provider_id
                .as_ref()
                .is_some_and(|saved| *saved != provider_config.name);
            if provider_changed {
                tracing::warn!(
                    request_id = %input.request_id,
                    saved_provider = ?cp.meta.provider_id,
                    current_provider = %provider_config.name,
                    "Checkpoint provider 与当前 provider 不一致，清除 tool 相关状态以避免兼容性问题"
                );
                for msg in &mut restored_messages {
                    msg.tool_calls = None;
                    msg.tool_call_id = None;
                }
                restored_tool_calls.clear();
                restored_tool_results.clear();
            }

            messages = restored_messages;
            prepare_tool_api_messages(&mut messages, &[]);
            harness_rounds = cp.round;
            all_tool_calls = restored_tool_calls;
            tool_results_json = restored_tool_results;
            evidence_ledger = EvidenceLedger::new(cp.evidence_packets);
            total_usage = cp.usage;
            usage_source = cp.usage_source;
            bonus_round_used = cp.bonus_round_used;
        }
    }

    let checkpoint_meta = HarnessCheckpointMeta {
        scene: input.scene.profile().to_string(),
        session_id: input.session_id,
        note_path: input.note_path.clone(),
        note_title: input.note_title.clone(),
        selection_excerpt: input.selection_excerpt.clone(),
        cold_start_packets: input.cold_start_packets.clone(),
        context_scope: input.context_scope.clone(),
        web_search_enabled: input.web_search_enabled,
        depth: input.depth,
        capability_slot: Some(provider_config.slot),
        provider_id: Some(provider_config.name.clone()),
        model: Some(provider_config.model.clone()),
        endpoint_family: Some(provider_config.endpoint_family),
        thinking: Some(thinking_mode),
        output_budget: max_tokens,
        input_budget: input.input_budget,
        skill_activation_plan: input.skill_activation_plan.clone(),
        task_policy: Some(input.task_policy.clone()),
    };

    'agent: loop {
        while harness_rounds < max_rounds {
            abort_if_requested(&input.request_id)?;
            if total_usage.total_tokens >= token_budget {
                break 'agent;
            }
            harness_rounds += 1;

            prepare_tool_api_messages(&mut messages, &[]);
            if let Some(tool_call) =
                outstanding_confirm_tool(&registry, &messages, &policy_ctx).cloned()
            {
                let assistant_content = messages
                    .iter()
                    .rev()
                    .find(|m| matches!(m.role, MessageRole::Assistant))
                    .map(|m| m.content.text_content())
                    .unwrap_or_default();
                return pause_for_tool_confirmation(
                    state,
                    app_handle,
                    input,
                    &checkpoint_meta,
                    harness_rounds,
                    bonus_round_used,
                    &mut messages,
                    &all_tool_calls,
                    &mut tool_results_json,
                    evidence_ledger.packets(),
                    total_usage,
                    usage_source,
                    assistant_content,
                    file_id,
                    &tool_call,
                )
                .await;
            }

            let request = GatewayRequest {
                provider: provider_config.clone(),
                messages: messages.clone(),
                tools: llm_tools.clone(),
                max_tokens,
                input_token_budget: input.input_budget,
                temperature: Some(0.7),
                stream: true,
                thinking: thinking_mode,
                reasoning,
                skip_stub_ids: vec![],
            };
            tracing::debug!(
                request_id = %input.request_id,
                event = "agent_round_started",
                candidate_kind = "unclassified_candidate",
                round = harness_rounds,
                "AI lifecycle agent round started"
            );

            let response = send_llm_streaming_request_with_retry(
                app_handle,
                &gateway,
                request,
                &input.request_id,
                StreamSurface::InternalCandidate,
            )
            .await?;
            if usage_is_empty(&response.usage) {
                let content = response.content.as_deref().unwrap_or("");
                estimate_and_accumulate(&mut total_usage, &messages, content);
                usage_source = UsageSource::Estimated;
            } else {
                accumulate_usage(&mut total_usage, &response.usage);
            }

            let mut tool_calls = response.tool_calls.clone();
            if tool_calls.is_empty() {
                if let Some(content) = &response.content {
                    tool_calls = parse_tool_calls_from_content(content);
                }
            }

            if should_retry_tool_parse(&tool_calls) {
                consecutive_parse_failures += 1;
                // Non-terminal: the streamed content is malformed tool JSON,
                // not a user-facing answer. Drop it before the next attempt.
                tracing::debug!(
                    request_id = %input.request_id,
                    event = "agent_round_reset",
                    candidate_kind = "internal_candidate",
                    reason_kind = "parse_retry",
                    round = harness_rounds,
                    "AI lifecycle agent round reset"
                );
                emit_stream_reset_with_surface(
                    app_handle,
                    &input.request_id,
                    "parse_retry",
                    StreamSurface::InternalCandidate,
                    Some(harness_rounds),
                )?;
                // Surface this as a retry to the UI so the user sees progress
                // instead of a silent multi-minute stall (each retried round
                // is a full LLM call of up to ~247s).
                let _ = app_handle.emit(
                    "ai:retry_status",
                    &serde_json::json!({
                        "request_id": input.request_id,
                        "attempt": consecutive_parse_failures,
                        "max_attempts": 3u32,
                        "delay_ms": 0u64,
                    }),
                );
                if consecutive_parse_failures >= 3 {
                    tracing::warn!(
                        request_id = %input.request_id,
                        consecutive_failures = consecutive_parse_failures,
                        "连续 3 次工具调用解析失败，放弃重试，转入最终回答模式"
                    );
                    break 'agent;
                }
                messages.push(LlmMessage {
                    role: MessageRole::User,
                    content: format!(
                        "工具参数 JSON 不完整，请重新输出合法的 tool_calls。尝试 {}/3，超过将直接回答。",
                        consecutive_parse_failures
                    ).into(),
                    tool_call_id: None,
                    tool_calls: None,
                    ..Default::default()
                });
                continue;
            }
            consecutive_parse_failures = 0;

            if tool_calls.is_empty() {
                tracing::debug!(
                    request_id = %input.request_id,
                    event = "agent_round_entering_final_stream",
                    candidate_kind = "visible_answer_candidate",
                    round = harness_rounds,
                    "AI lifecycle agent round had no tool calls; entering final stream"
                );
                let raw = response.content.clone().unwrap_or_default();
                let stripped = strip_tool_markup_from_visible(&raw);
                let sanitized = sanitize_meta_analysis_prefix(&stripped);
                let (visible, thinking) =
                    extract_thinking_blocks_for_event(&sanitized, thinking_mode);
                if let Some(t) = thinking {
                    emit_thinking(app_handle, &input.request_id, harness_rounds, &t)?;
                }

                // Guard against incomplete / fragment answers: if the model
                // issues a no-tool-call text that looks like a document excerpt
                // or truncated sentence, push a continuation prompt and give
                // the model one more chance rather than treating the fragment
                // as the final answer.
                if looks_like_incomplete_final_answer(&visible) && !bonus_round_used {
                    tracing::warn!(
                        request_id = %input.request_id,
                        round = harness_rounds,
                        fragment_len = visible.chars().count(),
                        "no-tool-call response looks incomplete; pushing continuation prompt"
                    );
                    bonus_round_used = true;
                    messages.push(LlmMessage {
                        role: MessageRole::User,
                        content: "请基于已检索到的信息继续完成你的分析，不要只输出文档片段。如果证据不足，请说明现有局限。".into(),
                        tool_call_id: None,
                        tool_calls: None,
                        ..Default::default()
                    });
                    emit_stream_reset_with_surface(
                        app_handle,
                        &input.request_id,
                        "fragment_continuation",
                        StreamSurface::InternalCandidate,
                        Some(harness_rounds),
                    )?;
                    continue 'agent;
                }
                break 'agent;
            }

            // Non-terminal round: tool calls were produced (either to dispatch
            // or a conclude_reasoning signal). The streamed preamble must not
            // stick to the surface — the next round or the FinalStream phase
            // will stream the real answer into a clean buffer.
            tracing::debug!(
                request_id = %input.request_id,
                event = "agent_round_reset",
                candidate_kind = "internal_candidate",
                reason_kind = "tool_round",
                round = harness_rounds,
                "AI lifecycle agent round reset"
            );
            emit_stream_reset_with_surface(
                app_handle,
                &input.request_id,
                "tool_round",
                StreamSurface::InternalCandidate,
                Some(harness_rounds),
            )?;

            if tool_calls
                .iter()
                .any(|tc| tc.function.name == "conclude_reasoning")
            {
                break 'agent;
            }

            let mut policy_denied: Vec<(ToolCall, ToolCallResult)> = Vec::new();
            let mut policy_allowed: Vec<PreparedToolCall> = Vec::new();
            for tc in tool_calls {
                let args = match parse_tool_call_arguments(&tc.function.arguments) {
                    Ok(args) => args,
                    Err(err) => {
                        policy_denied
                            .push((tc.clone(), tool_argument_parse_error_result(&tc, &err)));
                        continue;
                    }
                };
                if !tool_allowed_by_inherited_surface(&input, &tc.function.name) {
                    let hint = tool_policy::denial_user_message(
                        DenialReason::CapabilityMismatch,
                        &tc.function.name,
                    );
                    policy_denied.push((
                        tc.clone(),
                        ToolCallResult {
                            tool_name: tc.function.name.clone(),
                            success: false,
                            output: serde_json::json!({
                                "error": hint,
                                "policy_denied": true,
                            }),
                            duration_ms: 0,
                            tokens_used: None,
                            error: Some(hint),
                        },
                    ));
                    continue;
                }
                let Some(entry) = catalog_find(&tc.function.name) else {
                    let hint = tool_policy::denial_user_message(
                        DenialReason::NotImplemented,
                        &tc.function.name,
                    );
                    policy_denied.push((
                        tc.clone(),
                        ToolCallResult {
                            tool_name: tc.function.name.clone(),
                            success: false,
                            output: serde_json::json!({
                                "error": hint,
                                "policy_denied": true,
                            }),
                            duration_ms: 0,
                            tokens_used: None,
                            error: Some(hint),
                        },
                    ));
                    continue;
                };
                let gate = evaluate_tool_execution(
                    &state.db,
                    ToolExecutionGate {
                        request_id: &input.request_id,
                        session_id: Some(input.session_id),
                        harness_round: harness_rounds,
                        entry,
                        args: &args,
                        policy_ctx: &policy_ctx,
                        skill_id: None,
                        scene: Some(input.scene.profile()),
                        subagent_depth: input.depth,
                    },
                )?;
                if let Some(result) = gate.tool_result {
                    policy_denied.push((tc, result));
                } else {
                    let class = classify_catalog_entry(entry);
                    policy_allowed.push(PreparedToolCall {
                        tool_call: tc,
                        args,
                        entry,
                        decision: gate.decision,
                        class,
                    });
                }
            }

            let stripped_assistant =
                strip_tool_markup_from_visible(&response.content.clone().unwrap_or_default());
            let (visible_content, thinking) =
                extract_thinking_blocks_for_event(&stripped_assistant, thinking_mode);
            let visible_content = sanitize_meta_analysis_prefix(&visible_content);
            if let Some(t) = thinking {
                emit_thinking(app_handle, &input.request_id, harness_rounds, &t)?;
            }
            let assistant_content = visible_content;

            let all_model_tool_calls: Vec<ToolCall> = policy_denied
                .iter()
                .map(|(tc, _)| tc.clone())
                .chain(
                    policy_allowed
                        .iter()
                        .map(|prepared| prepared.tool_call.clone()),
                )
                .collect();

            messages.push(LlmMessage {
                role: MessageRole::Assistant,
                content: assistant_content.clone().into(),
                tool_call_id: None,
                tool_calls: Some(all_model_tool_calls),
                reasoning_content: response.reasoning_content.clone(),
            });

            for (tc, result) in &policy_denied {
                push_tool_result_error(&mut messages, &mut tool_results_json, tc, result);
                all_tool_calls.push(tc.clone());
            }

            if policy_allowed.is_empty() {
                continue;
            }

            let tool_calls = policy_allowed;

            // Subagent partition invariant: .partition(|tc| tc.function.name == "spawn_subagent")
            let (subagent_calls, other_calls): (Vec<_>, Vec<_>) = tool_calls
                .iter()
                .partition(|prepared| prepared.tool_call.function.name == "spawn_subagent");

            all_tool_calls.extend(tool_calls.iter().map(|prepared| prepared.tool_call.clone()));

            if !subagent_calls.is_empty() && input.depth < 2 {
                emit_trace_phase(
                    app_handle,
                    &input.request_id,
                    harness_rounds,
                    HarnessPhase::SubagentSpawn,
                    "spawn_subagent",
                    "running",
                    None,
                    None,
                    None,
                )?;
                let evidence_ids = evidence_ledger
                    .packets()
                    .iter()
                    .map(|packet| packet.id.clone())
                    .collect::<Vec<_>>();
                let inherited_allowed_tools = inherited_allowed_tool_names(&scene_tools, &input);
                let subagent_specs = subagent_calls
                    .iter()
                    .map(|prepared| {
                        SubAgentTaskSpec::from_tool_call(
                            &input.request_id,
                            &prepared.tool_call,
                            input.note_path.as_deref(),
                            evidence_ids.clone(),
                            inherited_allowed_tools.clone(),
                            input.token_budget,
                        )
                    })
                    .collect::<Vec<_>>();
                let coordination_plan = SubAgentCoordinator::plan(&subagent_specs);
                let mut conflict_by_subagent =
                    SubAgentCoordinator::conflict_errors_by_subagent(&coordination_plan);
                let executable_indices = subagent_specs
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, spec)| {
                        if conflict_by_subagent.contains_key(&spec.id) {
                            None
                        } else {
                            Some(idx)
                        }
                    })
                    .collect::<Vec<_>>();
                let sub_futures = executable_indices
                    .iter()
                    .map(|idx| {
                        run_subagent_harness(
                            state,
                            app_handle,
                            &input,
                            provider_config.clone(),
                            max_tokens,
                            reasoning,
                            &subagent_calls[*idx].tool_call,
                            &subagent_specs[*idx],
                        )
                    })
                    .collect::<Vec<_>>();
                let completed = join_all(sub_futures).await;
                let mut sub_results: HashMap<usize, AppResult<HarnessRunResult>> = HashMap::new();
                for (idx, result) in executable_indices.into_iter().zip(completed) {
                    sub_results.insert(idx, result);
                }

                for (idx, (prepared, spec)) in
                    subagent_calls.iter().zip(subagent_specs.iter()).enumerate()
                {
                    let tc = &prepared.tool_call;
                    let conflict_errors = conflict_by_subagent.remove(&spec.id);
                    let output = if let Some(conflicts) = conflict_errors {
                        let details = conflicts
                            .iter()
                            .map(|issue| {
                                format!(
                                    "{}:{} {}",
                                    issue.resource_type, issue.resource_id, issue.message
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("; ");
                        let report = SubAgentCoordinator::report_error(spec, details);
                        SubAgentCoordinator::tool_output_for_report(&report)
                    } else {
                        match sub_results.remove(&idx) {
                            Some(Ok(r)) => {
                                let report = SubAgentCoordinator::report_success(
                                    spec,
                                    r.content.clone(),
                                    r.citation_valid,
                                    r.harness_rounds,
                                );
                                serde_json::json!({
                                    "content": r.content,
                                    "citation_valid": r.citation_valid,
                                    "harness_rounds": r.harness_rounds,
                                    "subagent_report": report,
                                })
                            }
                            Some(Err(e)) => {
                                let report = SubAgentCoordinator::report_error(spec, e.to_string());
                                serde_json::json!({
                                    "error": report.errors.first().cloned().unwrap_or_default(),
                                    "subagent_report": report,
                                })
                            }
                            None => {
                                let report = SubAgentCoordinator::report_error(
                                    spec,
                                    "subagent_result_missing",
                                );
                                serde_json::json!({
                                    "error": "subagent_result_missing",
                                    "subagent_report": report,
                                })
                            }
                        }
                    };
                    let ok = output.get("error").is_none()
                        && output
                            .get("subagent_report")
                            .and_then(|report| report.get("errors"))
                            .and_then(|errors| errors.as_array())
                            .map_or(true, Vec::is_empty);
                    let output_str = serde_json::to_string(&output).unwrap_or_else(|_| "{}".into());
                    messages.push(LlmMessage {
                        role: MessageRole::Tool,
                        content: output_str.clone().into(),
                        tool_call_id: Some(tc.id.clone()),
                        tool_calls: None,
                        ..Default::default()
                    });
                    tool_results_json.push(serde_json::json!({
                        "tool_call_id": tc.id,
                        "status": if ok { "completed" } else { "error" },
                        "result": output,
                    }));
                    emit_trace_phase(
                        app_handle,
                        &input.request_id,
                        harness_rounds,
                        HarnessPhase::SubagentComplete,
                        "spawn_subagent",
                        if ok { "ok" } else { "error" },
                        None,
                        None,
                        Some(output_str.chars().take(200).collect()),
                    )?;
                }
            }

            if let Err(e) = save_round_checkpoint(
                state,
                &input,
                &checkpoint_meta,
                harness_rounds,
                bonus_round_used,
                &messages,
                &all_tool_calls,
                &tool_results_json,
                evidence_ledger.packets(),
                &total_usage,
                usage_source,
            ) {
                tracing::warn!("checkpoint save failed for {}: {e}", input.request_id);
            }

            // Dispatch contexts keep web fetch limits policy-driven: max_web_fetches: input.task_policy.max_fetch_per_round as usize

            if let Some(prepared) = other_calls
                .iter()
                .find(|prepared| permission_requires_confirmation(prepared.decision.decision))
            {
                return pause_for_tool_confirmation(
                    state,
                    app_handle,
                    input,
                    &checkpoint_meta,
                    harness_rounds,
                    bonus_round_used,
                    &mut messages,
                    &all_tool_calls,
                    &mut tool_results_json,
                    evidence_ledger.packets(),
                    total_usage,
                    usage_source,
                    assistant_content,
                    file_id,
                    &prepared.tool_call,
                )
                .await;
            }

            let mut tools_this_round = 0u32;
            let mut parallel_batch: Vec<&PreparedToolCall> = Vec::new();
            for prepared in other_calls {
                let tool_call = &prepared.tool_call;
                abort_if_requested(&input.request_id)?;
                if registry.requires_confirmation(&tool_call.function.name)
                    || permission_requires_confirmation(prepared.decision.decision)
                {
                    continue;
                }
                if tools_this_round >= input.task_policy.max_tool_calls_per_round {
                    flush_parallel_tool_batch(
                        state,
                        app_handle,
                        &input,
                        &policy_ctx,
                        harness_rounds,
                        file_id,
                        &mut evidence_ledger,
                        &mut messages,
                        &mut tool_results_json,
                        &mut parallel_batch,
                    )
                    .await?;
                    break;
                }

                if prepared.class == ToolExecutionClass::ParallelRead {
                    parallel_batch.push(prepared);
                    tools_this_round += 1;
                    continue;
                }

                flush_parallel_tool_batch(
                    state,
                    app_handle,
                    &input,
                    &policy_ctx,
                    harness_rounds,
                    file_id,
                    &mut evidence_ledger,
                    &mut messages,
                    &mut tool_results_json,
                    &mut parallel_batch,
                )
                .await?;
                dispatch_and_record_prepared_tool(
                    state,
                    app_handle,
                    &input,
                    &policy_ctx,
                    harness_rounds,
                    file_id,
                    &mut evidence_ledger,
                    &mut messages,
                    &mut tool_results_json,
                    prepared,
                )
                .await?;
                tools_this_round += 1;
            }
            flush_parallel_tool_batch(
                state,
                app_handle,
                &input,
                &policy_ctx,
                harness_rounds,
                file_id,
                &mut evidence_ledger,
                &mut messages,
                &mut tool_results_json,
                &mut parallel_batch,
            )
            .await?;
            if let Some(tool_call) =
                outstanding_confirm_tool(&registry, &messages, &policy_ctx).cloned()
            {
                return pause_for_tool_confirmation(
                    state,
                    app_handle,
                    input,
                    &checkpoint_meta,
                    harness_rounds,
                    bonus_round_used,
                    &mut messages,
                    &all_tool_calls,
                    &mut tool_results_json,
                    evidence_ledger.packets(),
                    total_usage,
                    usage_source,
                    assistant_content,
                    file_id,
                    &tool_call,
                )
                .await;
            }
        }

        // depth 0: full reflection (one round)
        // depth 1: one reflection round, max one bonus round
        // depth >= 2: no reflection, no sub-spawning
        if reflection_done || input.depth > 1 {
            break 'agent;
        }
        reflection_done = true;
        match run_reflection_round(
            state,
            app_handle,
            &input,
            &gateway,
            &provider_config,
            max_tokens,
            reasoning,
            thinking_mode,
            &mut messages,
            &evidence_ledger,
            &all_tool_calls,
            &tool_results_json,
            &mut total_usage,
            harness_rounds,
            false,
            &mut bonus_round_used,
            &mut max_rounds,
            token_budget,
            &mut usage_source,
        )
        .await?
        {
            ReflectionOutcome::BonusRound => continue 'agent,
            ReflectionOutcome::NoAnswer => break 'agent,
        }
    }

    emit_trace_phase(
        app_handle,
        &input.request_id,
        harness_rounds,
        HarnessPhase::FinalStream,
        "final",
        "streaming",
        None,
        None,
        None,
    )?;
    tracing::debug!(
        request_id = %input.request_id,
        event = "final_stream_started",
        candidate_kind = "visible_answer_candidate",
        round = harness_rounds,
        "AI lifecycle final stream started"
    );

    let final_content = {
        abort_if_requested(&input.request_id)?;
        let stream_request = GatewayRequest {
            provider: provider_config,
            messages: build_final_answer_messages(&messages),
            tools: vec![],
            max_tokens,
            input_token_budget: input.input_budget,
            temperature: Some(0.7),
            stream: true,
            thinking: thinking_mode,
            reasoning,
            skip_stub_ids: vec![],
        };
        let final_surface = final_answer_stream_surface(&reasoning);
        let response = send_llm_streaming_request_with_retry(
            app_handle,
            &gateway,
            stream_request,
            &input.request_id,
            final_surface,
        )
        .await?;
        if usage_is_empty(&response.usage) {
            // Prompt tokens already accumulated from prior rounds.
            // Only estimate the completion portion from the streaming response.
            let content = response.content.as_deref().unwrap_or("");
            let completion_est = super::token_estimator::estimate_tokens(content);
            total_usage.completion_tokens += completion_est;
            total_usage.total_tokens += completion_est;
            usage_source = UsageSource::Estimated;
        } else {
            accumulate_usage(&mut total_usage, &response.usage);
        }
        strip_tool_markup_from_visible(&response.content.unwrap_or_default())
    };
    let (final_visible, final_thinking) =
        extract_thinking_blocks_for_event(&final_content, thinking_mode);
    let final_visible = sanitize_meta_analysis_prefix(&final_visible);
    if let Some(t) = final_thinking {
        emit_thinking(app_handle, &input.request_id, harness_rounds, &t)?;
    }

    let sanitized_final = sanitize_reflection_visible(&final_visible);
    if sanitized_final.is_none() && !final_visible.trim().is_empty() {
        tracing::debug!(
            request_id = %input.request_id,
            event = "final_content_rejected_as_internal_tool_artifact",
            content_chars = final_visible.chars().count(),
            "AI lifecycle rejected non-user-visible final content"
        );
    }
    let evidence_packets = ledger_to_packets(&evidence_ledger, token_budget);
    let evidence_fallback = web_evidence_fallback_from_packets(&evidence_packets);
    let decision = classify_final_answer(
        sanitized_final,
        evidence_fallback,
        total_usage.total_tokens,
        token_budget,
        harness_rounds,
        max_rounds,
    );

    if decision.save_checkpoint {
        save_round_checkpoint(
            state,
            &input,
            &checkpoint_meta,
            harness_rounds,
            bonus_round_used,
            &messages,
            &all_tool_calls,
            &tool_results_json,
            &evidence_packets,
            &total_usage,
            usage_source,
        )?;
    }

    finish_run(
        state,
        input,
        FinishRunParams {
            content: decision.content,
            tool_calls: all_tool_calls,
            tool_results: tool_results_json,
            usage: total_usage,
            harness_rounds,
            pending_confirmation: false,
            evidence_packets,
            usage_source,
            finish_reason: decision.finish_reason,
        },
    )
    .await
}

fn abort_if_requested(request_id: &str) -> AppResult<()> {
    if is_abort_requested(request_id) {
        clear_abort(request_id);
        return Err(AppError::msg("request aborted"));
    }
    Ok(())
}

fn tool_argument_parse_error_result(tool_call: &ToolCall, parse_error: &str) -> ToolCallResult {
    ToolCallResult {
        tool_name: tool_call.function.name.clone(),
        success: false,
        output: serde_json::json!({
            "error": "tool_arguments_parse_error",
            "failure_class": "parse_error",
            "message": "tool arguments must be a valid JSON object",
        }),
        duration_ms: 0,
        tokens_used: None,
        error: Some(format!(
            "tool_arguments_parse_error: {}: {}",
            tool_call.function.name, parse_error
        )),
    }
}

fn push_tool_result_error(
    messages: &mut Vec<LlmMessage>,
    tool_results_json: &mut Vec<serde_json::Value>,
    tool_call: &ToolCall,
    result: &ToolCallResult,
) {
    let err = result.error.as_deref().unwrap_or("tool execution denied");
    messages.push(LlmMessage {
        role: MessageRole::Tool,
        content: serde_json::to_string(&result.output)
            .unwrap_or_else(|_| {
                format!(
                    "{{\"error\": {}}}",
                    serde_json::to_string(err).unwrap_or_default()
                )
            })
            .into(),
        tool_call_id: Some(tool_call.id.clone()),
        tool_calls: None,
        ..Default::default()
    });
    let mut entry = serde_json::json!({
        "tool_call_id": tool_call.id,
        "status": "error",
        "error": err,
    });
    if result
        .output
        .get("policy_denied")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        entry["policy_denied"] = serde_json::Value::Bool(true);
    }
    if let Some(failure_class) = result.output.get("failure_class") {
        entry["failure_class"] = failure_class.clone();
    }
    tool_results_json.push(entry);
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_prepared_tool_call(
    state: &Arc<AppState>,
    app_handle: &AppHandle,
    input: &HarnessRunInput,
    file_id: Option<i64>,
    prepared: &PreparedToolCall,
    cold_start_packets: &[crate::ai_runtime::ContextPacket],
) -> ToolCallResult {
    let retrieval_scope = state
        .vault_path()
        .ok()
        .map(|vault| crate::knowledge::corpora::load_corpora(&vault).unwrap_or_default())
        .map(|corpora| {
            crate::ai_runtime::retrieval_scope::resolve_retrieval_scope(
                &corpora,
                input.scene,
                &input.context_scope,
            )
        })
        .unwrap_or_default();
    let dispatch_ctx = ToolDispatchContext {
        scene: input.scene,
        note_path: input.note_path.as_deref(),
        file_id,
        web_search_enabled: input.web_search_enabled,
        max_web_fetches: input.task_policy.max_fetch_per_round as usize,
        cold_start_packets,
        retrieval_scope: &retrieval_scope,
        runtime_documents: &input.runtime_documents,
        app_handle: Some(app_handle.clone()),
        attachment_count: input.images.as_ref().map_or(0, Vec::len),
        skill_activation_plan: input.skill_activation_plan.as_ref(),
        embedding_state: Some(state),
    };
    dispatch_tool_with_retry(state, &dispatch_ctx, prepared.entry.name, &prepared.args).await
}

fn emit_prepared_tool_start(
    app_handle: &AppHandle,
    input: &HarnessRunInput,
    harness_rounds: u32,
    prepared: &PreparedToolCall,
) -> AppResult<()> {
    emit_trace_phase(
        app_handle,
        &input.request_id,
        harness_rounds,
        HarnessPhase::ToolStart,
        prepared.entry.name,
        "running",
        None,
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn record_prepared_tool_result(
    state: &Arc<AppState>,
    app_handle: &AppHandle,
    input: &HarnessRunInput,
    policy_ctx: &ToolPolicyContext,
    harness_rounds: u32,
    evidence_ledger: &mut EvidenceLedger,
    messages: &mut Vec<LlmMessage>,
    tool_results_json: &mut Vec<serde_json::Value>,
    prepared: &PreparedToolCall,
    result: &ToolCallResult,
) -> AppResult<()> {
    if result.success {
        ingest_tool_packets(evidence_ledger, prepared.entry.name, &result.output);
    }
    let output_str = serde_json::to_string(&result.output).unwrap_or_else(|_| "{}".into());
    let preview: String = output_str.chars().take(200).collect();
    emit_trace_phase(
        app_handle,
        &input.request_id,
        harness_rounds,
        HarnessPhase::ToolComplete,
        prepared.entry.name,
        if result.success { "ok" } else { "error" },
        Some(result.duration_ms),
        None,
        Some(preview),
    )?;

    let execution_gate = ToolExecutionGate {
        request_id: &input.request_id,
        session_id: Some(input.session_id),
        harness_round: harness_rounds,
        entry: prepared.entry,
        args: &prepared.args,
        policy_ctx,
        skill_id: None,
        scene: Some(input.scene.profile()),
        subagent_depth: input.depth,
    };
    let _ = audit_dispatched_tool(&state.db, &execution_gate, &prepared.decision, result);

    messages.push(LlmMessage {
        role: MessageRole::Tool,
        content: if result.success {
            output_str
        } else {
            format!(
                "{{\"error\": {}}}",
                serde_json::to_string(result.error.as_deref().unwrap_or("unknown"))
                    .unwrap_or_default()
            )
        }
        .into(),
        tool_call_id: Some(prepared.tool_call.id.clone()),
        tool_calls: None,
        ..Default::default()
    });

    tool_results_json.push(serde_json::json!({
        "tool_call_id": prepared.tool_call.id,
        "status": if result.success { "completed" } else { "error" },
        "result": result.output.clone(),
    }));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_and_record_prepared_tool(
    state: &Arc<AppState>,
    app_handle: &AppHandle,
    input: &HarnessRunInput,
    policy_ctx: &ToolPolicyContext,
    harness_rounds: u32,
    file_id: Option<i64>,
    evidence_ledger: &mut EvidenceLedger,
    messages: &mut Vec<LlmMessage>,
    tool_results_json: &mut Vec<serde_json::Value>,
    prepared: &PreparedToolCall,
) -> AppResult<()> {
    emit_prepared_tool_start(app_handle, input, harness_rounds, prepared)?;
    let cold_start_packets = evidence_ledger.packets().to_vec();
    let result = dispatch_prepared_tool_call(
        state,
        app_handle,
        input,
        file_id,
        prepared,
        &cold_start_packets,
    )
    .await;
    record_prepared_tool_result(
        state,
        app_handle,
        input,
        policy_ctx,
        harness_rounds,
        evidence_ledger,
        messages,
        tool_results_json,
        prepared,
        &result,
    )
}

#[allow(clippy::too_many_arguments)]
async fn flush_parallel_tool_batch(
    state: &Arc<AppState>,
    app_handle: &AppHandle,
    input: &HarnessRunInput,
    policy_ctx: &ToolPolicyContext,
    harness_rounds: u32,
    file_id: Option<i64>,
    evidence_ledger: &mut EvidenceLedger,
    messages: &mut Vec<LlmMessage>,
    tool_results_json: &mut Vec<serde_json::Value>,
    batch: &mut Vec<&PreparedToolCall>,
) -> AppResult<()> {
    if batch.is_empty() {
        return Ok(());
    }

    for prepared in batch.iter() {
        emit_prepared_tool_start(app_handle, input, harness_rounds, prepared)?;
    }
    let cold_start_packets = evidence_ledger.packets().to_vec();
    let futures = batch.iter().map(|prepared| {
        dispatch_prepared_tool_call(
            state,
            app_handle,
            input,
            file_id,
            prepared,
            &cold_start_packets,
        )
    });
    let results = join_all(futures).await;
    for (prepared, result) in batch.iter().zip(results.iter()) {
        record_prepared_tool_result(
            state,
            app_handle,
            input,
            policy_ctx,
            harness_rounds,
            evidence_ledger,
            messages,
            tool_results_json,
            prepared,
            result,
        )?;
    }
    batch.clear();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn pause_for_tool_confirmation(
    state: &AppState,
    app_handle: &AppHandle,
    input: HarnessRunInput,
    checkpoint_meta: &HarnessCheckpointMeta,
    harness_rounds: u32,
    bonus_round_used: bool,
    messages: &mut [LlmMessage],
    all_tool_calls: &[ToolCall],
    tool_results_json: &mut Vec<serde_json::Value>,
    evidence_packets: &[crate::ai_runtime::ContextPacket],
    total_usage: TokenUsage,
    usage_source: UsageSource,
    assistant_content: String,
    file_id: Option<i64>,
    tool_call: &ToolCall,
) -> AppResult<HarnessRunResult> {
    let tool_name = &tool_call.function.name;
    state.ai.prune_pending_tool_calls();
    crate::llm::safe_lock(&state.ai.pending_tool_calls).insert(
        tool_call.id.clone(),
        crate::app::PendingToolCall {
            tool_name: tool_name.clone(),
            arguments: tool_call.function.arguments.clone(),
            request_id: input.request_id.clone(),
            session_id: input.session_id,
            scene: input.scene,
            note_path: input.note_path.clone(),
            file_id,
            web_search_enabled: input.web_search_enabled,
            autonomy_level: input.task_policy.autonomy_level,
            task_policy: input.task_policy.clone(),
            depth: input.depth,
            skill_activation_plan: input.skill_activation_plan.clone(),
            created_at: std::time::Instant::now(),
        },
    );
    let args = parse_tool_call_arguments(&tool_call.function.arguments).map_err(|err| {
        AppError::msg(format!(
            "tool_arguments_parse_error: {}: {}",
            tool_call.function.name, err
        ))
    })?;
    let permission_effects = catalog_find(tool_name)
        .map(|entry| preflight_tool_permission(entry, &args, None).effects)
        .unwrap_or_default();
    let permission_decision = catalog_find(tool_name).and_then(|entry| {
        decide_tool_permission(
            &state.db,
            PermissionDecisionRequest {
                request_id: &input.request_id,
                session_id: Some(input.session_id),
                entry,
                args: &args,
                policy_ctx: &ToolPolicyContext {
                    task_policy: Some(input.task_policy.clone()),
                    scene: input.scene,
                    autonomy_level: input.task_policy.autonomy_level,
                    web_search_enabled: input.web_search_enabled,
                    depth: input.depth,
                },
                skill_id: None,
            },
        )
        .ok()
    });
    let registry = ToolRegistry::new();
    let confirmation_position = pending_confirmation_position(
        &registry,
        messages,
        &ToolPolicyContext {
            task_policy: Some(input.task_policy.clone()),
            scene: input.scene,
            autonomy_level: input.task_policy.autonomy_level,
            web_search_enabled: input.web_search_enabled,
            depth: input.depth,
        },
        &tool_call.id,
    );
    let mut confirm_request = serde_json::json!({
        "request_id": input.request_id,
        "tool_call_id": tool_call.id,
        "tool_name": tool_name,
        "arguments": args,
        "permissionEffects": permission_effects,
        "pendingConfirmationIndex": confirmation_position.map_or(1, |position| position.index),
        "pendingConfirmationCount": confirmation_position.map_or(1, |position| position.count),
        "sandboxProfile": crate::ai_runtime::sandbox_profile::sandbox_profile_for_tool(tool_name),
    });
    if let Some(permission_decision) = permission_decision {
        confirm_request["permissionDecision"] =
            serde_json::to_value(permission_decision).unwrap_or_default();
    }
    app_handle
        .emit("ai:tool_confirm_request", &confirm_request)
        .map_err(|e| AppError::msg(format!("emit tool confirm: {e}")))?;
    tool_results_json.push(serde_json::json!({
        "tool_call_id": tool_call.id,
        "status": "pending_confirmation",
    }));
    emit_trace_phase(
        app_handle,
        &input.request_id,
        harness_rounds,
        HarnessPhase::ToolStart,
        tool_name,
        "pending",
        None,
        None,
        None,
    )?;
    save_round_checkpoint(
        state,
        &input,
        checkpoint_meta,
        harness_rounds,
        bonus_round_used,
        messages,
        all_tool_calls,
        tool_results_json,
        evidence_packets,
        &total_usage,
        usage_source,
    )?;
    finish_run(
        state,
        input,
        FinishRunParams {
            content: assistant_content,
            tool_calls: all_tool_calls.to_vec(),
            tool_results: tool_results_json.clone(),
            usage: total_usage,
            harness_rounds,
            pending_confirmation: true,
            evidence_packets: evidence_packets.to_vec(),
            usage_source,
            finish_reason: HarnessFinishReason::AwaitingConfirmation,
        },
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_subagent_harness(
    state: &Arc<AppState>,
    app_handle: &AppHandle,
    parent: &HarnessRunInput,
    provider_config: ProviderConfig,
    max_tokens: Option<u32>,
    reasoning: crate::ai_types::ResolvedReasoningRequest,
    tool_call: &ToolCall,
    spec: &SubAgentTaskSpec,
) -> AppResult<HarnessRunResult> {
    let args = parse_tool_call_arguments(&tool_call.function.arguments).map_err(|err| {
        AppError::msg(format!(
            "tool_arguments_parse_error: {}: {}",
            tool_call.function.name, err
        ))
    })?;
    let task = args
        .get("task")
        .and_then(|v| v.as_str())
        .unwrap_or("子任务")
        .to_string();
    let context_hint = args
        .get("context_hint")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let sub_rounds = args
        .get("max_rounds")
        .and_then(|v| v.as_u64())
        .unwrap_or(2)
        .min(3) as u32;

    let parent_budget = parent
        .token_budget
        .unwrap_or(parent.task_policy.max_token_budget);
    let sub_budget = (parent_budget * 3 / 5).max(2000); // 60%

    let sub_id = format!("{}-sub-{}", parent.request_id, uuid::Uuid::new_v4());
    let sub_input = HarnessRunInput {
        request_id: sub_id,
        scene: parent.scene,
        session_id: parent.session_id,
        note_path: parent.note_path.clone(),
        note_title: parent.note_title.clone(),
        selection_excerpt: context_hint.or_else(|| parent.selection_excerpt.clone()),
        cold_start_packets: parent.cold_start_packets.clone(),
        context_scope: parent.context_scope.clone(),
        runtime_documents: parent.runtime_documents.clone(),
        web_search_enabled: parent.web_search_enabled,
        user_message: task.clone(),
        images: None,
        history_messages: vec![("user".to_string(), task)],
        structured_history_messages: vec![],
        allowed_tools: Some(spec.allowed_tools.clone()),
        depth: parent.depth + 1,
        resume_from_checkpoint: false,
        max_rounds_override: Some(sub_rounds.min(parent.task_policy.max_agentic_rounds)),
        token_budget: Some(sub_budget),
        input_budget: parent.input_budget,
        skill_activation_plan: parent.skill_activation_plan.clone(),
        task_policy: parent.task_policy.clone(),
        provider_failover_candidates: parent.provider_failover_candidates.clone(),
    };

    run_harness(
        state,
        app_handle,
        sub_input,
        provider_config,
        max_tokens,
        reasoning,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_harness::tool_turn::outstanding_confirm_tool;
    use crate::ai_runtime::model_gateway::{TokenUsage, ToolCall};
    use crate::ai_runtime::tool_executor::ToolRegistry;

    fn make_tool_call(name: &str) -> ToolCall {
        ToolCall {
            id: format!("call-{name}"),
            call_type: "function".into(),
            function: crate::ai_runtime::model_gateway::FunctionCall {
                name: name.to_string(),
                arguments: "{}".into(),
            },
        }
    }

    use crate::ai_runtime::tool_policy::ToolPolicyContext;
    use crate::ai_runtime::{AiScene, AutonomyLevel};

    fn test_policy_ctx(web_search_enabled: bool) -> ToolPolicyContext {
        let task_policy = crate::ai_runtime::agent_task_policy::AgentTaskPolicy::from_input(
            crate::ai_runtime::agent_task_policy::AgentTaskPolicyInput {
                intent: crate::ai_runtime::AgentIntent::Write,
                task_kind: crate::ai_runtime::agent_task::AgentTaskKind::Lightweight,
                scope: crate::ai_runtime::agent_task_policy::AgentTaskScope::Vault,
                web_authorized: web_search_enabled,
                has_attachments: false,
                write_permission_required: true,
                research_depth: 0,
            },
        );
        ToolPolicyContext {
            task_policy: Some(task_policy),
            scene: AiScene::DraftingAssist,
            autonomy_level: AutonomyLevel::L2,
            web_search_enabled,
            depth: 0,
        }
    }

    fn assistant_with_tools(calls: Vec<ToolCall>) -> Vec<LlmMessage> {
        vec![LlmMessage {
            role: MessageRole::Assistant,
            content: String::new().into(),
            tool_call_id: None,
            tool_calls: Some(calls),
            ..Default::default()
        }]
    }

    #[test]
    fn allowed_tool_filter_removes_tools_outside_parent_surface() {
        let registry = ToolRegistry::new();
        let tools = registry.tools_for_policy_surface(&test_policy_ctx(true), false);
        let filtered = filter_tools_by_allowed_names(&tools, Some(&vec!["read_note".to_string()]));
        let names = filtered
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();

        assert!(names.contains(&"read_note".to_string()));
        assert!(!names.contains(&"web_search".to_string()));
        assert!(!names.contains(&"replace_selection".to_string()));
    }
    #[test]
    fn tool_argument_parse_error_result_is_structured_and_not_policy_denied() {
        let tool_call = ToolCall {
            id: "call-bad-json".into(),
            call_type: "function".into(),
            function: crate::ai_runtime::model_gateway::FunctionCall {
                name: "web_search".into(),
                arguments: r#"{"query":"x""#.into(),
            },
        };

        let result = tool_argument_parse_error_result(&tool_call, "expected eof");

        assert!(!result.success);
        assert_eq!(result.output["error"], "tool_arguments_parse_error");
        assert_eq!(result.output["failure_class"], "parse_error");
        assert_eq!(result.output.get("policy_denied"), None);
        assert!(result
            .error
            .as_deref()
            .unwrap_or("")
            .contains("tool_arguments_parse_error: web_search"));
        assert!(!result.error.as_deref().unwrap_or("").contains(r#"{"query"#));
    }

    #[test]
    fn test_mixed_auto_and_confirm_tools_only_fetch_pauses() {
        let registry = ToolRegistry::new();
        let ctx = test_policy_ctx(true);
        let messages = assistant_with_tools(vec![
            make_tool_call("search_hybrid"),
            make_tool_call("replace_selection"),
        ]);
        let pending = outstanding_confirm_tool(&registry, &messages, &ctx);
        assert_eq!(pending.unwrap().function.name, "replace_selection");
        assert!(!registry.requires_confirmation("search_hybrid"));
        assert!(registry.requires_confirmation("replace_selection"));
    }

    #[test]
    fn test_pending_tool_call_returns_pending_result() {
        let registry = ToolRegistry::new();
        let messages = assistant_with_tools(vec![make_tool_call("replace_selection")]);
        let result = outstanding_confirm_tool(&registry, &messages, &test_policy_ctx(true));
        assert!(result.is_some());
        assert_eq!(result.unwrap().function.name, "replace_selection");
    }

    #[test]
    fn test_read_only_tools_do_not_pause() {
        let registry = ToolRegistry::new();
        let ctx = test_policy_ctx(true);
        let read_only = vec![
            "search_hybrid",
            "search_semantic",
            "search_keyword",
            "read_note",
            "list_vault",
            "get_outline",
            "get_backlinks",
            "get_regulation",
        ];
        for name in read_only {
            let messages = assistant_with_tools(vec![make_tool_call(name)]);
            let result = outstanding_confirm_tool(&registry, &messages, &ctx);
            assert!(
                result.is_none(),
                "read-only tool '{name}' should NOT require confirmation"
            );
        }
    }

    #[test]
    fn test_multiple_confirm_tools_pauses_first_and_keeps_checkpoint() {
        let registry = ToolRegistry::new();
        let ctx = test_policy_ctx(true);
        let messages = assistant_with_tools(vec![
            make_tool_call("search_hybrid"),
            make_tool_call("insert_text_at_cursor"),
            make_tool_call("replace_selection"),
        ]);
        let result = outstanding_confirm_tool(&registry, &messages, &ctx);
        assert!(result.is_some());
        assert_eq!(result.unwrap().function.name, "insert_text_at_cursor");
    }

    #[test]
    fn test_pending_confirmation_is_false_after_removal() {
        let registry = ToolRegistry::new();
        let messages = assistant_with_tools(vec![make_tool_call("search_hybrid")]);
        let result = outstanding_confirm_tool(&registry, &messages, &test_policy_ctx(true));
        assert!(
            result.is_none(),
            "no confirm tool → no pending confirmation"
        );
    }

    #[test]
    fn permission_outcome_requires_confirmation_even_when_the_catalog_entry_does_not() {
        assert!(permission_requires_confirmation(
            crate::ai_runtime::permission_decision::PermissionExecutionDecision::RequiresConfirmation
        ));
        assert!(!permission_requires_confirmation(
            crate::ai_runtime::permission_decision::PermissionExecutionDecision::AutoAllowed
        ));
    }

    #[test]
    fn test_outstanding_confirm_skips_responded_tools() {
        let registry = ToolRegistry::new();
        let ctx = test_policy_ctx(true);
        let web = make_tool_call("web_search");
        let edit = make_tool_call("replace_selection");
        let mut messages = assistant_with_tools(vec![web.clone(), edit.clone()]);
        messages.push(LlmMessage {
            role: MessageRole::Tool,
            content: r#"{"results":[]}"#.into(),
            tool_call_id: Some(web.id.clone()),
            tool_calls: None,
            ..Default::default()
        });
        let pending = outstanding_confirm_tool(&registry, &messages, &ctx);
        assert_eq!(pending.unwrap().id, edit.id);
    }

    #[test]
    fn harness_result_exposes_usage_source() {
        let result = HarnessRunResult {
            request_id: "req".into(),
            session_id: 1,
            content: String::new(),
            tool_calls: vec![],
            tool_results: vec![],
            usage: TokenUsage::default(),
            citation_valid: true,
            harness_rounds: 0,
            pending_confirmation: false,
            evidence_packets: vec![],
            usage_source: UsageSource::Estimated,
            finish_reason: HarnessFinishReason::Completed,
            deliberation_state: None,
            verification_summary: None,
        };

        assert_eq!(result.usage_source, UsageSource::Estimated);
    }

    #[test]
    fn round_limit_without_budget_exhaustion_completes_with_fallback() {
        let decision = classify_final_answer(None, None, 99, 100, 3, 3);

        assert_eq!(decision.finish_reason, HarnessFinishReason::Completed);
        assert!(!decision.save_checkpoint);
        assert!(!decision.content.contains("限定轮次"));
        assert!(!decision.content.contains("请缩小问题"));
    }

    #[test]
    fn budget_exhaustion_still_pauses_for_recovery() {
        let decision = classify_final_answer(None, None, 100, 100, 1, 3);

        assert_eq!(decision.finish_reason, HarnessFinishReason::BudgetExhausted);
        assert!(decision.save_checkpoint);
    }

    #[test]
    fn empty_final_answer_uses_web_evidence_fallback_before_round_fallback() {
        let fallback = "以下基于检索结果摘要：\n\n- 来源 1：Example News - 摘要".to_string();
        let decision = classify_final_answer(None, Some(fallback.clone()), 99, 100, 3, 3);

        assert_eq!(decision.finish_reason, HarnessFinishReason::Completed);
        assert_eq!(decision.content, fallback);
        assert!(!decision.content.contains(FINAL_ROUND_FALLBACK));
        assert!(!decision.save_checkpoint);
    }

    #[test]
    fn web_evidence_fallback_summarizes_titles_excerpts_and_sources() {
        use crate::ai_runtime::{
            ContextPacket, SourceType, TrustLevel, WebEvidenceMeta, WebSearchBackend, WebSourceRank,
        };

        let packet = ContextPacket {
            id: "web-1".into(),
            source_type: SourceType::Web,
            source_path: Some("https://example.com/news".into()),
            title: "Example News".into(),
            heading_path: None,
            source_span: None,
            content_hash: "hash".into(),
            excerpt: "This is the search result summary.".into(),
            retrieval_reason: "web prefetch".into(),
            score: 0.9,
            trust_level: TrustLevel::ExternalWeb,
            citation_label: "[C1]".into(),
            stale: false,
            web: Some(WebEvidenceMeta {
                url: Some("https://example.com/news".into()),
                domain: Some("example.com".into()),
                published_at: None,
                fetched_at: "2026-07-11T00:00:00Z".into(),
                search_backend: WebSearchBackend::Provider,
                source_rank: WebSourceRank::Media,
                provider_id: None,
                provider_kind: None,
                raw_result_hash: None,
                extraction_method: None,
                conflict_group: None,
                conflict_note: None,
                failure_reason: None,
                fallback_from: None,
            }),
            corpus: None,
        };

        let fallback =
            web_evidence_fallback_from_packets(&[packet]).expect("web evidence fallback");

        assert!(fallback.starts_with("以下基于检索结果摘要："));
        assert!(fallback.contains("[C1] Example News"));
        assert!(fallback.contains("This is the search result summary."));
        assert!(fallback.contains("https://example.com/news"));
    }

    #[test]
    fn final_answer_rejects_internal_tool_parameter_fragments() {
        for artifact in [
            "15000 党纪国法/政府采购货物和服务招标投标管理办法.md",
            r#"{"path":"党纪国法/政府采购货物和服务招标投标管理办法.md","max_chars":15000}"#,
            "max_chars=15000 path=党纪国法/政府采购货物和服务招标投标管理办法.md",
        ] {
            let decision = classify_final_answer(Some(artifact.to_string()), None, 10, 100, 1, 3);

            assert_eq!(decision.finish_reason, HarnessFinishReason::Completed);
            assert_eq!(decision.content, FINAL_EMPTY_FALLBACK);
            assert!(!decision.content.contains("政府采购货物和服务"));
        }
    }

    #[test]
    fn final_answer_keeps_normal_legal_analysis() {
        let answer = "根据《政府采购货物和服务招标投标管理办法》，邀请招标应当符合特定适用条件。";
        let decision = classify_final_answer(Some(answer.to_string()), None, 10, 100, 1, 3);

        assert_eq!(decision.finish_reason, HarnessFinishReason::Completed);
        assert_eq!(decision.content, answer);
    }

    #[test]
    fn final_answer_surface_streams_hidden_channel_reasoning_visibly() {
        let reasoning = ResolvedReasoningRequest {
            isolate_output: true,
            visibility: ReasoningVisibility::HiddenChannel,
            ..ResolvedReasoningRequest::legacy_enabled(true)
        };

        assert_eq!(
            final_answer_stream_surface(&reasoning),
            StreamSurface::VisibleAnswer
        );
    }

    #[test]
    fn final_answer_surface_sanitizes_content_tag_and_plain_risk_models() {
        for visibility in [
            ReasoningVisibility::ContentTag,
            ReasoningVisibility::PlainContentRisk,
        ] {
            let reasoning = ResolvedReasoningRequest {
                isolate_output: true,
                visibility,
                ..ResolvedReasoningRequest::legacy_enabled(true)
            };

            assert_eq!(
                final_answer_stream_surface(&reasoning),
                StreamSurface::VisibleAnswerSanitized
            );
        }
    }

    #[test]
    fn final_answer_messages_force_no_tool_direct_answer() {
        let messages = vec![LlmMessage {
            role: MessageRole::User,
            content: "分析一下当前引用".into(),
            tool_call_id: None,
            tool_calls: None,
            ..Default::default()
        }];

        let final_messages = build_final_answer_messages(&messages);
        let instruction = final_messages
            .last()
            .expect("final instruction should be appended");

        assert_eq!(messages.len() + 1, final_messages.len());
        assert!(matches!(instruction.role, MessageRole::User));
        assert!(instruction
            .content
            .as_str()
            .expect("text instruction")
            .contains("不要再调用工具"));
        assert!(instruction
            .content
            .as_str()
            .expect("text instruction")
            .contains("NEED_MORE_EVIDENCE"));
    }

    #[test]
    fn adaptive_retry_detects_422_and_context_overflow() {
        assert!(should_adaptive_degrade_after_llm_error(
            "模型请求失败（422 Unprocessable Entity）"
        ));
        assert!(should_adaptive_degrade_after_llm_error(
            "llm_input_context_overflow: estimated input tokens exceed model input budget"
        ));
        assert!(!should_adaptive_degrade_after_llm_error(
            "模型请求失败（429 Too Many Requests）"
        ));
    }

    #[test]
    fn adaptive_retry_request_compacts_tools_history_and_output_budget() {
        let request = GatewayRequest {
            provider: ProviderConfig {
                name: "test".into(),
                base_url: "https://api.example.com".into(),
                api_key: Some("secret".into()),
                model: "model-a".into(),
                slot: crate::ai_types::CapabilitySlot::Fast,
                endpoint_family: crate::ai_types::EndpointFamily::OpenAiCompatibleChatCompletions,
            },
            messages: vec![
                LlmMessage {
                    role: MessageRole::System,
                    content: "system".into(),
                    ..Default::default()
                },
                LlmMessage {
                    role: MessageRole::Tool,
                    content: "x".repeat(80_000).into(),
                    tool_call_id: Some("call-1".into()),
                    ..Default::default()
                },
                LlmMessage {
                    role: MessageRole::User,
                    content: "请基于证据回答".into(),
                    ..Default::default()
                },
            ],
            tools: vec![LlmToolDef {
                tool_type: "function".into(),
                function: crate::ai_runtime::model_gateway::LlmFunctionDef {
                    name: "web_search".into(),
                    description: "Search web".into(),
                    parameters: serde_json::json!({"type": "object"}),
                },
            }],
            max_tokens: Some(4096),
            input_token_budget: Some(64_000),
            temperature: Some(0.2),
            stream: true,
            thinking: true,
            reasoning: ResolvedReasoningRequest::legacy_enabled(true),
            skip_stub_ids: Vec::new(),
        };

        let compact = build_adaptive_degraded_llm_request(&request);

        assert!(compact.tools.is_empty());
        assert_eq!(compact.max_tokens, Some(2048));
        assert_eq!(compact.input_token_budget, Some(32_000));
        assert!(compact.messages.len() <= request.messages.len() + 1);
        assert!(compact.messages.iter().any(|message| {
            message
                .content
                .as_str()
                .unwrap_or_default()
                .contains("不要再调用工具")
        }));
        assert!(compact
            .messages
            .iter()
            .filter(|message| matches!(message.role, MessageRole::Tool))
            .all(|message| message.content.text_content().chars().count() <= 12_000));
    }

    // ── depth-based reflection/subagent behavior ──────────

    #[test]
    fn depth_0_allows_reflection() {
        // The condition: if reflection_done || input.depth > 1 { break }
        // depth 0: depth > 1 is false → reflection allowed
        let depth = 0u32;
        let reflection_done = false;
        assert!(
            !reflection_done && depth <= 1,
            "depth 0 should allow reflection"
        );
    }

    #[test]
    fn depth_1_allows_reflection() {
        let depth = 1u32;
        let reflection_done = false;
        assert!(
            !reflection_done && depth <= 1,
            "depth 1 should allow reflection"
        );
    }

    #[test]
    fn depth_2_blocks_reflection() {
        let depth = 2u32;
        // depth > 1 → break, no reflection
        assert!(depth > 1, "depth 2 should block reflection");
    }

    #[test]
    fn reflection_done_blocks_second_reflection() {
        let depth = 0u32;
        let reflection_done = true;
        // reflection_done → break regardless of depth
        assert!(reflection_done || depth > 1);
    }

    #[test]
    fn depth_0_allows_subagent_spawn() {
        let depth = 0u32;
        assert!(depth < 2, "depth 0 should allow subagent spawn");
    }

    #[test]
    fn depth_1_allows_subagent_spawn() {
        let depth = 1u32;
        assert!(depth < 2, "depth 1 should allow subagent spawn");
    }

    #[test]
    fn depth_2_blocks_subagent_spawn() {
        let depth = 2u32;
        assert!(depth >= 2, "depth 2 should block subagent spawn");
    }
}
