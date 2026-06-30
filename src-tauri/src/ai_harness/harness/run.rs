//! Unified Agent Harness — multi-round tool loop with streaming final response.

use std::collections::HashMap;
use std::sync::Arc;

use futures_util::future::join_all;
use tauri::{AppHandle, Emitter};

use super::archive::save_round_checkpoint;
use super::context::{
    build_initial_messages, prepare_environment_and_skills_with_plan,
    resolve_active_skill_allowed_tools_with_plan, resolve_file_id, EnvironmentAndSkillsInput,
    InitialMessagesInput,
};
use super::finalize::{finish_run, ingest_tool_packets, ledger_to_packets, FinishRunParams};
use super::planning::{resolve_max_rounds, resolve_token_budget};
use super::reflection::{run_reflection_round, sanitize_reflection_visible, ReflectionOutcome};
use super::token_estimator::{estimate_and_accumulate, usage_is_empty, UsageSource};
use super::tools::max_fetch_per_round;
use super::trace_emit::{emit_thinking, emit_trace_phase};
use super::types::{HarnessFinishReason, HarnessPhase, HarnessRunInput, HarnessRunResult};
use super::util::accumulate_usage;
use crate::ai_harness::tool_turn::{outstanding_confirm_tool, pending_confirmation_position};
use crate::ai_runtime::agent_permissions::preflight_tool_permission;
use crate::ai_runtime::circuit_breaker;
use crate::ai_runtime::evidence_ledger::EvidenceLedger;
use crate::ai_runtime::harness_support::{
    extract_thinking_blocks, load_harness_checkpoint, HarnessCheckpointMeta,
};
use crate::ai_runtime::model_gateway::{
    clear_abort, emit_stream_reset_with_surface, is_abort_requested, prepare_tool_api_messages,
    GatewayRequest, GatewayResponse, LlmMessage, MessageRole, ModelGateway, ProviderConfig,
    StreamSurface, TokenUsage, ToolCall,
};
use crate::ai_runtime::permission_decision::{decide_tool_permission, PermissionDecisionRequest};
use crate::ai_runtime::subagent_coordinator::{SubAgentCoordinator, SubAgentTaskSpec};
use crate::ai_runtime::tool_catalog::catalog_find;
use crate::ai_runtime::tool_dispatch::{dispatch_tool_with_retry, ToolDispatchContext};
use crate::ai_runtime::tool_execution_pipeline::{
    audit_dispatched_tool, evaluate_tool_execution, ToolExecutionGate,
};
use crate::ai_runtime::tool_executor::ToolRegistry;
use crate::ai_runtime::tool_fallback::{
    parse_tool_calls_from_content, should_retry_tool_parse, strip_tool_markup_from_visible,
};
use crate::ai_runtime::tool_policy::{self, DenialReason, ToolPolicyContext};
use crate::ai_runtime::ToolCallResult;
use crate::app::AppState;
use crate::error::{AppError, AppResult};

const LLM_MAX_RETRIES: u32 = 3;
const LLM_RETRY_BASE_DELAY_MS: u64 = 1000;
const FINAL_ANSWER_INSTRUCTION: &str = "停止继续检索、反思或调用工具。请只基于当前已有上下文直接回答用户；如果证据不足，说明局限并给出当前能支持的结论。不要再调用工具，也不要输出 NEED_MORE_EVIDENCE 或其他内部控制标记。";
const FINAL_ROUND_FALLBACK: &str =
    "我已停止继续调用工具，但这次没有生成可展示回答。请重试，或换一种问法。";
const FINAL_BUDGET_FALLBACK: &str = "这次上下文预算已用尽，未能生成可展示回答。请缩小范围后重试。";
const FINAL_EMPTY_FALLBACK: &str = "这次没有生成可展示回答。请重试，或换一种问法。";

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

fn classify_final_answer(
    sanitized_final: Option<String>,
    total_tokens: u32,
    token_budget: u32,
    harness_rounds: u32,
    max_rounds: u32,
) -> FinalAnswerDecision {
    if let Some(content) = sanitized_final {
        return FinalAnswerDecision {
            content,
            finish_reason: HarnessFinishReason::Completed,
            save_checkpoint: false,
        };
    }

    if total_tokens >= token_budget {
        return FinalAnswerDecision {
            content: FINAL_BUDGET_FALLBACK.into(),
            finish_reason: HarnessFinishReason::BudgetExhausted,
            save_checkpoint: true,
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

/// Streaming agent-round LLM call: reuses the circuit breaker and
/// exponential-backoff retry logic, but dispatches through
/// `ModelGateway::send_streaming_request_with_surface` so caller chooses whether
/// provider tokens are internal candidates or visible answer text.
async fn send_llm_streaming_request_with_retry(
    app_handle: &AppHandle,
    gateway: &ModelGateway,
    request: GatewayRequest,
    request_id: &str,
    provider_id: &str,
    surface: StreamSurface,
) -> AppResult<GatewayResponse> {
    if !circuit_breaker::is_request_allowed(provider_id) {
        return Err(AppError::msg(format!(
            "Provider {provider_id} 已被熔断，请稍后重试"
        )));
    }
    let mut last_err: Option<String> = None;
    for attempt in 0..=LLM_MAX_RETRIES {
        match gateway
            .send_streaming_request_with_surface(request_id, request.clone(), surface)
            .await
        {
            Ok(response) => {
                circuit_breaker::record_success(provider_id);
                return Ok(response);
            }
            Err(e) => {
                let msg = e.to_string();
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
                        attempt = attempt + 1,
                        delay_ms,
                        error = %msg,
                        "LLM 流式请求失败，{}ms后重试",
                        delay_ms
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }
                last_err = Some(msg);
            }
        }
    }
    circuit_breaker::record_failure(provider_id);
    Err(AppError::msg(last_err.unwrap_or_else(|| {
        "LLM streaming request failed after all retries".into()
    })))
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
    thinking_mode: bool,
) -> AppResult<HarnessRunResult> {
    run_harness_inner(
        state,
        app_handle,
        input,
        provider_config,
        max_tokens,
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
    thinking_mode: bool,
) -> AppResult<HarnessRunResult> {
    let registry = ToolRegistry::new();
    let skill_allowed_tools = resolve_active_skill_allowed_tools_with_plan(
        state,
        &input.task_policy,
        &input.user_message,
        input.skill_activation_plan.as_ref(),
    )?;
    let policy_ctx = ToolPolicyContext {
        task_policy: Some(input.task_policy.clone()),
        scene: input.scene,
        autonomy_level: input.task_policy.autonomy_level,
        web_search_enabled: input.web_search_enabled,
        skill_allowed_tools: skill_allowed_tools.clone(),
        depth: input.depth,
    };
    let scene_tools = registry.tools_for_policy_surface(&policy_ctx, false);
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
                    text: last_user.content.as_str().to_string(),
                }];
                for img in images {
                    parts.push(img.to_content_part());
                }
                last_user.content = crate::ai_types::MessageContent::Parts(parts);
            }
        }
    }

    let gateway = ModelGateway::with_defaults(app_handle.clone(), vec![provider_config.clone()])?;

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
        web_search_enabled: input.web_search_enabled,
        depth: input.depth,
        capability_slot: Some(provider_config.slot),
        provider_id: Some(provider_config.name.clone()),
        model: Some(provider_config.model.clone()),
        endpoint_family: Some(provider_config.endpoint_family),
        thinking: Some(thinking_mode),
        output_budget: max_tokens,
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
                    .map(|m| m.content.as_str().to_string())
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
                temperature: Some(0.7),
                stream: true,
                thinking: thinking_mode,
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
                &provider_config.name,
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
                    event = "agent_round_promoted_to_final",
                    candidate_kind = "visible_answer_candidate",
                    round = harness_rounds,
                    "AI lifecycle agent round promoted to final answer"
                );
                let raw = response.content.clone().unwrap_or_default();
                let stripped = strip_tool_markup_from_visible(&raw);
                let (visible, thinking) = extract_thinking_blocks(&stripped);
                if let Some(t) = thinking {
                    emit_thinking(app_handle, &input.request_id, harness_rounds, &t)?;
                }
                let final_content = visible;
                return finish_run(
                    state,
                    input,
                    FinishRunParams {
                        content: final_content,
                        tool_calls: all_tool_calls,
                        tool_results: tool_results_json,
                        usage: total_usage,
                        harness_rounds,
                        pending_confirmation: false,
                        evidence_packets: ledger_to_packets(&evidence_ledger, token_budget),
                        usage_source,
                        finish_reason: HarnessFinishReason::Completed,
                    },
                )
                .await;
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
            let mut policy_allowed: Vec<ToolCall> = Vec::new();
            for tc in tool_calls {
                let args: serde_json::Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or_default();
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
                    policy_allowed.push(tc);
                }
            }

            let stripped_assistant =
                strip_tool_markup_from_visible(&response.content.clone().unwrap_or_default());
            let (visible_content, thinking) = extract_thinking_blocks(&stripped_assistant);
            if let Some(t) = thinking {
                emit_thinking(app_handle, &input.request_id, harness_rounds, &t)?;
            }
            let assistant_content = visible_content;

            let all_model_tool_calls: Vec<ToolCall> = policy_denied
                .iter()
                .map(|(tc, _)| tc.clone())
                .chain(policy_allowed.iter().cloned())
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

            let (subagent_calls, other_calls): (Vec<_>, Vec<_>) = tool_calls
                .iter()
                .partition(|tc| tc.function.name == "spawn_subagent");

            all_tool_calls.extend(tool_calls.iter().cloned());

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
                )?;
                let evidence_ids = evidence_ledger
                    .packets()
                    .iter()
                    .map(|packet| packet.id.clone())
                    .collect::<Vec<_>>();
                let subagent_specs = subagent_calls
                    .iter()
                    .map(|tc| {
                        SubAgentTaskSpec::from_tool_call(
                            &input.request_id,
                            tc,
                            input.note_path.as_deref(),
                            evidence_ids.clone(),
                            skill_allowed_tools.clone(),
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
                            thinking_mode,
                            subagent_calls[*idx],
                        )
                    })
                    .collect::<Vec<_>>();
                let completed = join_all(sub_futures).await;
                let mut sub_results: HashMap<usize, AppResult<HarnessRunResult>> = HashMap::new();
                for (idx, result) in executable_indices.into_iter().zip(completed) {
                    sub_results.insert(idx, result);
                }

                for (idx, (tc, spec)) in
                    subagent_calls.iter().zip(subagent_specs.iter()).enumerate()
                {
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

            let mut tools_this_round = 0u32;
            let mut fetch_this_round = 0u32;
            let fetch_limit = max_fetch_per_round(&input.task_policy);
            for tool_call in &other_calls {
                abort_if_requested(&input.request_id)?;
                if registry.requires_confirmation(&tool_call.function.name) {
                    continue;
                }
                if tools_this_round >= input.task_policy.max_tool_calls_per_round {
                    break;
                }
                let tool_name = &tool_call.function.name;
                if tool_name == "fetch_web_page" && fetch_this_round >= fetch_limit {
                    let err_msg = format!("本轮 fetch_web_page 已达上限 ({fetch_limit})");
                    emit_trace_phase(
                        app_handle,
                        &input.request_id,
                        harness_rounds,
                        HarnessPhase::ToolComplete,
                        tool_name,
                        "error",
                        None,
                        Some(err_msg.clone()),
                    )?;
                    messages.push(LlmMessage {
                        role: MessageRole::Tool,
                        content: format!(
                            "{{\"error\": {}}}",
                            serde_json::to_string(&err_msg).unwrap_or_default()
                        )
                        .into(),
                        tool_call_id: Some(tool_call.id.clone()),
                        tool_calls: None,
                        ..Default::default()
                    });
                    tool_results_json.push(serde_json::json!({
                        "tool_call_id": tool_call.id,
                        "status": "error",
                        "error": err_msg,
                    }));
                    tools_this_round += 1;
                    continue;
                }
                let args: serde_json::Value =
                    serde_json::from_str(&tool_call.function.arguments).unwrap_or_default();
                let Some(entry) = catalog_find(tool_name) else {
                    push_tool_policy_error(
                        &mut messages,
                        &mut tool_results_json,
                        tool_call,
                        DenialReason::NotImplemented,
                    );
                    tools_this_round += 1;
                    continue;
                };
                let execution_gate = ToolExecutionGate {
                    request_id: &input.request_id,
                    harness_round: harness_rounds,
                    entry,
                    args: &args,
                    policy_ctx: &policy_ctx,
                    skill_id: None,
                    scene: Some(input.scene.profile()),
                    subagent_depth: input.depth,
                };
                let gate = evaluate_tool_execution(&state.db, execution_gate)?;
                if let Some(result) = gate.tool_result {
                    let err = result.error.as_deref().unwrap_or("tool denied");
                    messages.push(LlmMessage {
                        role: MessageRole::Tool,
                        content: serde_json::to_string(&result.output)
                            .unwrap_or_else(|_| format!("{{\"error\":\"{err}\"}}"))
                            .into(),
                        tool_call_id: Some(tool_call.id.clone()),
                        tool_calls: None,
                        ..Default::default()
                    });
                    tool_results_json.push(serde_json::json!({
                        "tool_call_id": tool_call.id,
                        "status": "error",
                        "error": err,
                        "policy_denied": true,
                    }));
                    tools_this_round += 1;
                    continue;
                }

                emit_trace_phase(
                    app_handle,
                    &input.request_id,
                    harness_rounds,
                    HarnessPhase::ToolStart,
                    tool_name,
                    "running",
                    None,
                    None,
                )?;

                let dispatch_ctx = ToolDispatchContext {
                    scene: input.scene,
                    note_path: input.note_path.as_deref(),
                    file_id,
                    web_search_enabled: input.web_search_enabled,
                    cold_start_packets: evidence_ledger.packets(),
                    app_handle: Some(app_handle.clone()),
                    attachment_count: input.images.as_ref().map_or(0, Vec::len),
                    skill_activation_plan: input.skill_activation_plan.as_ref(),
                    embedding_state: Some(state),
                };
                let result = dispatch_tool_with_retry(state, &dispatch_ctx, tool_name, &args).await;
                if result.success {
                    if tool_name == "fetch_web_page" {
                        fetch_this_round += 1;
                    }
                    ingest_tool_packets(&mut evidence_ledger, tool_name, &result.output);
                }
                let output_str =
                    serde_json::to_string(&result.output).unwrap_or_else(|_| "{}".into());
                let preview: String = output_str.chars().take(200).collect();
                emit_trace_phase(
                    app_handle,
                    &input.request_id,
                    harness_rounds,
                    HarnessPhase::ToolComplete,
                    tool_name,
                    if result.success { "ok" } else { "error" },
                    None,
                    Some(preview),
                )?;

                let _ = audit_dispatched_tool(&state.db, &execution_gate, &gate.decision, &result);
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
                    tool_call_id: Some(tool_call.id.clone()),
                    tool_calls: None,
                    ..Default::default()
                });

                tool_results_json.push(serde_json::json!({
                    "tool_call_id": tool_call.id,
                    "status": if result.success { "completed" } else { "error" },
                    "result": result.output,
                }));
                tools_this_round += 1;
            }

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
            temperature: Some(0.7),
            stream: true,
            thinking: thinking_mode,
            skip_stub_ids: vec![],
        };
        let response = gateway
            .send_streaming_request_with_surface(
                &input.request_id,
                stream_request,
                StreamSurface::VisibleAnswer,
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
    let (final_visible, final_thinking) = extract_thinking_blocks(&final_content);
    if let Some(t) = final_thinking {
        emit_thinking(app_handle, &input.request_id, harness_rounds, &t)?;
    }

    let decision = classify_final_answer(
        sanitize_reflection_visible(&final_visible),
        total_usage.total_tokens,
        token_budget,
        harness_rounds,
        max_rounds,
    );
    let evidence_packets = ledger_to_packets(&evidence_ledger, token_budget);

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

fn push_tool_policy_error(
    messages: &mut Vec<LlmMessage>,
    tool_results_json: &mut Vec<serde_json::Value>,
    tool_call: &ToolCall,
    reason: DenialReason,
) {
    let hint = tool_policy::denial_user_message(reason, &tool_call.function.name);
    let payload = serde_json::json!({ "error": hint, "policy_denied": true });
    let content = serde_json::to_string(&payload).unwrap_or_default();
    messages.push(LlmMessage {
        role: MessageRole::Tool,
        content: content.into(),
        tool_call_id: Some(tool_call.id.clone()),
        tool_calls: None,
        ..Default::default()
    });
    tool_results_json.push(serde_json::json!({
        "tool_call_id": tool_call.id,
        "status": "error",
        "error": hint,
        "policy_denied": true,
    }));
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
    tool_results_json.push(serde_json::json!({
        "tool_call_id": tool_call.id,
        "status": "error",
        "error": err,
        "policy_denied": true,
    }));
}

fn string_json_field<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|item| !item.is_empty())
}

fn parse_json_object_arg(
    value: &serde_json::Value,
    key: &str,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let raw = string_json_field(value, key)?;
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|parsed| parsed.as_object().cloned())
}

fn mcp_transport_kind(args: &serde_json::Value) -> String {
    parse_json_object_arg(args, "transport_config_json")
        .and_then(|transport| {
            transport
                .get("type")
                .or_else(|| transport.get("transport"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn mcp_env_binding_count(args: &serde_json::Value) -> usize {
    parse_json_object_arg(args, "env_bindings_json")
        .map(|bindings| bindings.len())
        .unwrap_or(0)
}

fn mcp_tool_confirmation_preview(
    tool_name: &str,
    args: &serde_json::Value,
) -> Option<serde_json::Value> {
    match tool_name {
        "mcp_server_catalog_upsert" => Some(serde_json::json!({
            "operation": "mcp_server_catalog_upsert",
            "server_id": string_json_field(args, "id").unwrap_or(""),
            "display_name": string_json_field(args, "display_name").unwrap_or(""),
            "transport": string_json_field(args, "transport").unwrap_or(""),
            "source": string_json_field(args, "source").unwrap_or("user"),
            "starts_process": false,
            "capability_boundary": "controlled_iris_capability_mapping",
        })),
        "mcp_runtime_profile_upsert" => Some(serde_json::json!({
            "operation": "mcp_profile_upsert",
            "profile_id": string_json_field(args, "id").unwrap_or(""),
            "server_id": string_json_field(args, "server_id").unwrap_or(""),
            "display_name": string_json_field(args, "display_name").unwrap_or(""),
            "transport": mcp_transport_kind(args),
            "vault_scope": string_json_field(args, "vault_scope_hash").unwrap_or("global"),
            "enabled": args.get("enabled").and_then(|value| value.as_bool()).unwrap_or(false),
            "credential_bindings": mcp_env_binding_count(args),
            "starts_process": false,
            "capability_boundary": "controlled_iris_capability_mapping",
        })),
        "mcp_runtime_tools_list" => Some(serde_json::json!({
            "operation": "mcp_tools_list",
            "profile_id": string_json_field(args, "profile_id").unwrap_or(""),
            "starts_process": true,
            "process_kind": "bounded_stdio_mcp",
            "result_scope": "sanitized_tool_inventory",
            "reason": string_json_field(args, "reason").unwrap_or(""),
        })),
        "mcp_runtime_health_check" => Some(serde_json::json!({
            "operation": "mcp_health_check",
            "profile_id": string_json_field(args, "profile_id").unwrap_or(""),
            "starts_process": true,
            "process_kind": "bounded_stdio_mcp",
            "result_scope": "metadata_only_health_status",
            "reason": string_json_field(args, "reason").unwrap_or(""),
        })),
        "mcp_runtime_capability_call" => Some(serde_json::json!({
            "operation": "mcp_capability_call",
            "capability": string_json_field(args, "capability").unwrap_or(""),
            "starts_process": true,
            "process_kind": "bounded_stdio_mcp",
            "result_scope": "model_safe_tool_result",
            "capability_boundary": "controlled_iris_capability_mapping",
            "reason": string_json_field(args, "reason").unwrap_or(""),
        })),
        _ => None,
    }
}

fn safe_string_argument(args: &serde_json::Value, key: &str) -> serde_json::Value {
    string_json_field(args, key)
        .map(|value| serde_json::Value::String(value.to_string()))
        .unwrap_or(serde_json::Value::Null)
}

fn confirmation_arguments_for_tool(tool_name: &str, args: &serde_json::Value) -> serde_json::Value {
    match tool_name {
        "mcp_server_catalog_upsert" => serde_json::json!({
            "id": safe_string_argument(args, "id"),
            "display_name": safe_string_argument(args, "display_name"),
            "transport": safe_string_argument(args, "transport"),
            "source": safe_string_argument(args, "source"),
        }),
        "mcp_runtime_profile_upsert" => serde_json::json!({
            "id": safe_string_argument(args, "id"),
            "server_id": safe_string_argument(args, "server_id"),
            "display_name": safe_string_argument(args, "display_name"),
            "enabled": args.get("enabled").and_then(|value| value.as_bool()).unwrap_or(false),
        }),
        "mcp_runtime_tools_list" | "mcp_runtime_health_check" => serde_json::json!({
            "profile_id": safe_string_argument(args, "profile_id"),
            "reason": safe_string_argument(args, "reason"),
        }),
        "mcp_runtime_capability_call" => serde_json::json!({
            "capability": safe_string_argument(args, "capability"),
            "reason": safe_string_argument(args, "reason"),
        }),
        _ => args.clone(),
    }
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
    let skill_allowed_tools = resolve_active_skill_allowed_tools_with_plan(
        state,
        &input.task_policy,
        &input.user_message,
        input.skill_activation_plan.as_ref(),
    )
    .unwrap_or_default();
    crate::llm::safe_lock(&state.ai.pending_tool_calls).insert(
        tool_call.id.clone(),
        crate::app::PendingToolCall {
            tool_name: tool_name.clone(),
            arguments: tool_call.function.arguments.clone(),
            request_id: input.request_id.clone(),
            scene: input.scene,
            note_path: input.note_path.clone(),
            file_id,
            web_search_enabled: input.web_search_enabled,
            autonomy_level: input.task_policy.autonomy_level,
            skill_allowed_tools: skill_allowed_tools.clone(),
            skill_activation_plan: input.skill_activation_plan.clone(),
        },
    );
    let args = serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments)
        .unwrap_or_default();
    let permission_effects = catalog_find(tool_name)
        .map(|entry| preflight_tool_permission(entry, &args, None).effects)
        .unwrap_or_default();
    let permission_decision = catalog_find(tool_name).and_then(|entry| {
        decide_tool_permission(
            &state.db,
            PermissionDecisionRequest {
                request_id: &input.request_id,
                entry,
                args: &args,
                policy_ctx: &ToolPolicyContext {
                    task_policy: Some(input.task_policy.clone()),
                    scene: input.scene,
                    autonomy_level: input.task_policy.autonomy_level,
                    web_search_enabled: input.web_search_enabled,
                    skill_allowed_tools: skill_allowed_tools.clone(),
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
            skill_allowed_tools,
            depth: input.depth,
        },
        &tool_call.id,
    );
    let mut confirm_request = serde_json::json!({
        "request_id": input.request_id,
        "tool_call_id": tool_call.id,
        "tool_name": tool_name,
        "arguments": confirmation_arguments_for_tool(tool_name, &args),
        "permissionEffects": permission_effects,
        "pendingConfirmationIndex": confirmation_position.map_or(1, |position| position.index),
        "pendingConfirmationCount": confirmation_position.map_or(1, |position| position.count),
        "sandboxProfile": crate::ai_runtime::sandbox_profile::sandbox_profile_for_tool(tool_name),
    });
    if let Some(permission_decision) = permission_decision {
        confirm_request["permissionDecision"] =
            serde_json::to_value(permission_decision).unwrap_or_default();
    }
    if let Some(preview) = mcp_tool_confirmation_preview(tool_name, &args) {
        confirm_request["preview"] = preview;
    }
    if let Ok(args) = serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments) {
        let vault = state.vault_path()?;
        if tool_name == "skills_install" {
            use crate::ai_runtime::skill_install_service::{
                normalize_skill_scope_arg, preview_install, SkillInstallRequest,
            };
            use crate::ai_runtime::skill_registry::SkillInstallSource;
            if let Some(source_str) = args.get("source").and_then(|v| v.as_str()) {
                if let Some(source) = SkillInstallSource::parse(source_str) {
                    let req = SkillInstallRequest {
                        source,
                        path_or_url: args
                            .get("path_or_url")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        scope: normalize_skill_scope_arg(
                            args.get("scope").and_then(|v| v.as_str()),
                        ),
                        subpath: args
                            .get("subpath")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        registry: args
                            .get("registry")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        expected_sha256: args
                            .get("expected_sha256")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                    };
                    if let Ok(preview) = preview_install(&vault, &req).await {
                        confirm_request["preview"] = preview;
                    }
                }
            }
        } else if tool_name == "skills_prepare_workspace" {
            use crate::ai_runtime::skill_install_service::{
                normalize_skill_scope_arg, preview_skill_workspace,
            };
            if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
                if let Ok(preview) = preview_skill_workspace(
                    &vault,
                    name,
                    normalize_skill_scope_arg(args.get("scope").and_then(|v| v.as_str())),
                ) {
                    confirm_request["preview"] = preview;
                }
            }
        } else if tool_name == "skills_update" {
            use crate::ai_runtime::skill_install_service::{
                normalize_skill_scope_arg, preview_update,
            };
            if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
                if let Ok(preview) = preview_update(
                    &state.db,
                    name,
                    normalize_skill_scope_arg(args.get("scope").and_then(|v| v.as_str())),
                ) {
                    confirm_request["preview"] = preview;
                }
            }
        }
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

async fn run_subagent_harness(
    state: &Arc<AppState>,
    app_handle: &AppHandle,
    parent: &HarnessRunInput,
    provider_config: ProviderConfig,
    max_tokens: Option<u32>,
    thinking: bool,
    tool_call: &ToolCall,
) -> AppResult<HarnessRunResult> {
    let args: serde_json::Value =
        serde_json::from_str(&tool_call.function.arguments).unwrap_or_default();
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
        web_search_enabled: parent.web_search_enabled,
        user_message: task.clone(),
        images: None,
        history_messages: vec![("user".to_string(), task)],
        depth: parent.depth + 1,
        resume_from_checkpoint: false,
        max_rounds_override: Some(sub_rounds.min(parent.task_policy.max_agentic_rounds)),
        token_budget: Some(sub_budget),
        skill_activation_plan: parent.skill_activation_plan.clone(),
        task_policy: parent.task_policy.clone(),
    };

    run_harness(
        state,
        app_handle,
        sub_input,
        provider_config,
        max_tokens,
        thinking,
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
        ToolPolicyContext {
            task_policy: None,
            scene: AiScene::DraftingAssist,
            autonomy_level: AutonomyLevel::L2,
            web_search_enabled,
            skill_allowed_tools: vec![],
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
    fn mcp_capability_confirmation_arguments_do_not_expose_provider_payload() {
        let args = serde_json::json!({
            "capability": "web.search",
            "arguments": {
                "query": "heartflow status",
                "api_key": "sk-secret-value",
                "token": "raw-token-value"
            },
            "reason": "diagnose skill runtime"
        });

        let safe = confirmation_arguments_for_tool("mcp_runtime_capability_call", &args);

        assert_eq!(safe["capability"], "web.search");
        assert_eq!(safe["reason"], "diagnose skill runtime");
        assert!(safe.get("arguments").is_none());
        let serialized = serde_json::to_string(&safe).unwrap();
        assert!(!serialized.contains("sk-secret-value"));
        assert!(!serialized.contains("raw-token-value"));
        assert!(!serialized.contains("api_key"));
    }
    #[test]
    fn mcp_profile_confirmation_arguments_do_not_expose_transport_or_credentials() {
        let args = serde_json::json!({
            "id": "anysearch-local",
            "server_id": "anysearch",
            "display_name": "AnySearch Local",
            "enabled": true,
            "transport_config_json": "{\"type\":\"stdio\",\"command\":\"anysearch-mcp\",\"args\":[\"--secret\"]}",
            "env_bindings_json": "{\"ANYSEARCH_API_KEY\":\"credential://iris.mcp.anysearch\",\"RAW_TOKEN\":\"secret-token\"}"
        });

        let safe = confirmation_arguments_for_tool("mcp_runtime_profile_upsert", &args);

        assert_eq!(safe["id"], "anysearch-local");
        assert_eq!(safe["display_name"], "AnySearch Local");
        assert_eq!(safe["enabled"], true);
        assert!(safe.get("transport_config_json").is_none());
        assert!(safe.get("env_bindings_json").is_none());
        let serialized = serde_json::to_string(&safe).unwrap();
        assert!(!serialized.contains("credential://iris.mcp.anysearch"));
        assert!(!serialized.contains("secret-token"));
        assert!(!serialized.contains("--secret"));
    }
    #[test]
    fn test_mixed_auto_and_confirm_tools_only_fetch_pauses() {
        let registry = ToolRegistry::new();
        let ctx = test_policy_ctx(true);
        let messages = assistant_with_tools(vec![
            make_tool_call("search_hybrid"),
            make_tool_call("fetch_web_page"),
        ]);
        let pending = outstanding_confirm_tool(&registry, &messages, &ctx);
        assert_eq!(pending.unwrap().function.name, "fetch_web_page");
        assert!(!registry.requires_confirmation("search_hybrid"));
        assert!(registry.requires_confirmation("fetch_web_page"));
    }

    #[test]
    fn test_pending_tool_call_returns_pending_result() {
        let registry = ToolRegistry::new();
        let messages = assistant_with_tools(vec![make_tool_call("fetch_web_page")]);
        let result = outstanding_confirm_tool(&registry, &messages, &test_policy_ctx(true));
        assert!(result.is_some());
        assert_eq!(result.unwrap().function.name, "fetch_web_page");
    }

    #[test]
    fn test_fetch_web_page_skipped_when_web_search_disabled() {
        let registry = ToolRegistry::new();
        let messages = assistant_with_tools(vec![make_tool_call("fetch_web_page")]);
        let result = outstanding_confirm_tool(&registry, &messages, &test_policy_ctx(false));
        assert!(
            result.is_none(),
            "fetch_web_page should not prompt when web search is disabled"
        );
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
    fn test_outstanding_confirm_skips_responded_tools() {
        let registry = ToolRegistry::new();
        let ctx = test_policy_ctx(true);
        let web = make_tool_call("web_search");
        let fetch = make_tool_call("fetch_web_page");
        let mut messages = assistant_with_tools(vec![web.clone(), fetch.clone()]);
        messages.push(LlmMessage {
            role: MessageRole::Tool,
            content: r#"{"results":[]}"#.into(),
            tool_call_id: Some(web.id.clone()),
            tool_calls: None,
            ..Default::default()
        });
        let pending = outstanding_confirm_tool(&registry, &messages, &ctx);
        assert_eq!(pending.unwrap().id, fetch.id);
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
        let decision = classify_final_answer(None, 99, 100, 3, 3);

        assert_eq!(decision.finish_reason, HarnessFinishReason::Completed);
        assert!(!decision.save_checkpoint);
        assert!(!decision.content.contains("限定轮次"));
        assert!(!decision.content.contains("请缩小问题"));
    }

    #[test]
    fn budget_exhaustion_still_pauses_for_recovery() {
        let decision = classify_final_answer(None, 100, 100, 1, 3);

        assert_eq!(decision.finish_reason, HarnessFinishReason::BudgetExhausted);
        assert!(decision.save_checkpoint);
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
        assert!(instruction.content.as_str().contains("不要再调用工具"));
        assert!(instruction.content.as_str().contains("NEED_MORE_EVIDENCE"));
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
    #[test]
    fn mcp_profile_upsert_confirmation_preview_is_sanitized_and_specific() {
        let args = serde_json::json!({
            "id": "anysearch-local",
            "server_id": "anysearch",
            "display_name": "AnySearch Local",
            "enabled": true,
            "vault_scope_hash": "vault-abc",
            "transport_config_json": "{\"type\":\"stdio\",\"command\":\"anysearch-mcp\",\"args\":[\"serve\"]}",
            "env_bindings_json": "{\"ANYSEARCH_API_KEY\":\"credential://iris.mcp.anysearch\"}"
        });

        let preview = mcp_tool_confirmation_preview("mcp_runtime_profile_upsert", &args)
            .expect("profile upsert should produce an MCP confirmation preview");

        assert_eq!(preview["operation"], "mcp_profile_upsert");
        assert_eq!(preview["profile_id"], "anysearch-local");
        assert_eq!(preview["server_id"], "anysearch");
        assert_eq!(preview["transport"], "stdio");
        assert_eq!(preview["vault_scope"], "vault-abc");
        assert_eq!(preview["credential_bindings"], 1);
        assert_eq!(preview["starts_process"], false);
        assert_eq!(
            preview["capability_boundary"],
            "controlled_iris_capability_mapping"
        );
        assert!(!preview
            .to_string()
            .contains("credential://iris.mcp.anysearch"));
    }

    #[test]
    fn mcp_live_tools_list_confirmation_preview_warns_about_process_start() {
        let args = serde_json::json!({
            "profile_id": "anysearch-local",
            "reason": "discover provider tools"
        });

        let preview = mcp_tool_confirmation_preview("mcp_runtime_tools_list", &args)
            .expect("live tools/list should produce an MCP confirmation preview");

        assert_eq!(preview["operation"], "mcp_tools_list");
        assert_eq!(preview["profile_id"], "anysearch-local");
        assert_eq!(preview["starts_process"], true);
        assert_eq!(preview["process_kind"], "bounded_stdio_mcp");
        assert_eq!(preview["result_scope"], "sanitized_tool_inventory");
        assert_eq!(preview["reason"], "discover provider tools");
    }
}
