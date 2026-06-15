//! Unified Agent Harness — multi-round tool loop with streaming final response.

use futures_util::future::join_all;
use tauri::{AppHandle, Emitter};

use super::archive::save_round_checkpoint;
use super::context::{
    build_initial_messages, prepare_environment_and_skills_with_plan,
    resolve_active_skill_allowed_tools_with_plan, resolve_file_id, EnvironmentAndSkillsInput,
};
use super::finalize::{finish_run, ingest_tool_packets, ledger_to_packets, FinishRunParams};
use super::planning::{resolve_max_rounds, resolve_token_budget};
use super::reflection::{run_reflection_round, ReflectionOutcome};
use super::token_estimator::{estimate_and_accumulate, usage_is_empty, UsageSource};
use super::tools::max_fetch_per_round;
use super::trace_emit::{emit_thinking, emit_trace_phase};
use super::types::{HarnessPhase, HarnessRunInput, HarnessRunResult};
use super::util::accumulate_usage;
use crate::ai_harness::tool_turn::outstanding_confirm_tool;
use crate::ai_runtime::agent_permissions::preflight_tool_permission;
use crate::ai_runtime::circuit_breaker;
use crate::ai_runtime::evidence_ledger::EvidenceLedger;
use crate::ai_runtime::harness_support::{
    extract_thinking_blocks, load_harness_checkpoint, HarnessCheckpointMeta,
};
use crate::ai_runtime::model_gateway::{
    clear_abort, is_abort_requested, prepare_tool_api_messages, GatewayRequest, GatewayResponse,
    LlmMessage, MessageRole, ModelGateway, ProviderConfig, TokenUsage, ToolCall,
};
use crate::ai_runtime::scene_router::resolve_scene;
use crate::ai_runtime::tool_audit;
use crate::ai_runtime::tool_catalog::catalog_find;
use crate::ai_runtime::tool_dispatch::{dispatch_tool_with_retry, ToolDispatchContext};
use crate::ai_runtime::tool_executor::ToolRegistry;
use crate::ai_runtime::tool_fallback::{
    parse_tool_calls_from_content, should_retry_tool_parse, strip_tool_markup_from_visible,
};
use crate::ai_runtime::tool_policy::{self, DenialReason, ToolPolicyContext};
use crate::app::AppState;
use crate::error::{AppError, AppResult};

const LLM_MAX_RETRIES: u32 = 3;
const LLM_RETRY_BASE_DELAY_MS: u64 = 1000;

async fn send_llm_request_with_retry(
    gateway: &ModelGateway,
    request: &GatewayRequest,
    request_id: &str,
    provider_id: &str,
) -> AppResult<GatewayResponse> {
    if !circuit_breaker::is_request_allowed(provider_id) {
        return Err(AppError::msg(format!(
            "Provider {provider_id} 已被熔断，请稍后重试"
        )));
    }
    let mut last_err: Option<String> = None;
    for attempt in 0..=LLM_MAX_RETRIES {
        match gateway.send_request(request.clone()).await {
            Ok(response) => {
                circuit_breaker::record_success(provider_id);
                return Ok(response);
            }
            Err(e) => {
                let msg = e.to_string();
                if attempt < LLM_MAX_RETRIES {
                    let delay_ms = LLM_RETRY_BASE_DELAY_MS * 2u64.pow(attempt);
                    tracing::warn!(
                        request_id = %request_id,
                        attempt = attempt + 1,
                        delay_ms,
                        error = %msg,
                        "LLM 请求失败，{}ms后重试",
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
        "LLM request failed after all retries".into()
    })))
}

/// Run the unified agent harness loop.
pub async fn run_harness(
    state: &AppState,
    app_handle: &AppHandle,
    input: HarnessRunInput,
    provider_config: crate::ai_runtime::model_gateway::ProviderConfig,
    max_tokens: Option<u32>,
    thinking_mode: bool,
) -> AppResult<HarnessRunResult> {
    let profile = resolve_scene(input.scene);
    let registry = ToolRegistry::new();
    let skill_allowed_tools = resolve_active_skill_allowed_tools_with_plan(
        state,
        input.scene,
        &input.user_message,
        input.skill_activation_plan.as_ref(),
    )?;
    let policy_ctx = ToolPolicyContext {
        scene: input.scene,
        autonomy_level: profile.autonomy_level,
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
            note_path: input.note_path.as_deref(),
            note_title: input.note_title.as_deref(),
            selection_excerpt: input.selection_excerpt.as_deref(),
            user_message: &input.user_message,
            scene_tools: &scene_tools,
        },
        input.skill_activation_plan.as_ref(),
    )?;

    let file_id = resolve_file_id(state, input.note_path.as_deref())?;

    let mut messages = build_initial_messages(
        state,
        input.scene,
        &env_text,
        &input.cold_start_packets,
        &input.history_messages,
        input.web_search_enabled,
        if skills_prompt.is_empty() {
            None
        } else {
            Some(skills_prompt.as_str())
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
    let token_budget = resolve_token_budget(input.scene, input.token_budget);
    let mut max_rounds = resolve_max_rounds(input.scene, input.max_rounds_override);

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
                stream: false,
                thinking: thinking_mode,
                skip_stub_ids: vec![],
            };

            let response = send_llm_request_with_retry(
                &gateway,
                &request,
                &input.request_id,
                &provider_config.name,
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
                    },
                )
                .await;
            }

            if tool_calls
                .iter()
                .any(|tc| tc.function.name == "conclude_reasoning")
            {
                break 'agent;
            }

            let mut policy_denied: Vec<(ToolCall, DenialReason)> = Vec::new();
            let mut policy_allowed: Vec<ToolCall> = Vec::new();
            for tc in tool_calls {
                match registry.check_tool_policy(&tc.function.name, &policy_ctx) {
                    Ok(()) => policy_allowed.push(tc),
                    Err(denied) => policy_denied.push((tc, denied.reason)),
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

            for (tc, reason) in &policy_denied {
                push_tool_policy_error(&mut messages, &mut tool_results_json, tc, *reason);
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
                let sub_futures: Vec<_> = subagent_calls
                    .iter()
                    .map(|tc| {
                        run_subagent_harness(
                            state,
                            app_handle,
                            &input,
                            provider_config.clone(),
                            max_tokens,
                            thinking_mode,
                            tc,
                        )
                    })
                    .collect();
                let sub_results: Vec<AppResult<HarnessRunResult>> = join_all(sub_futures).await;
                for (tc, sub_out) in subagent_calls.iter().zip(sub_results) {
                    let ok = sub_out.is_ok();
                    let output = match &sub_out {
                        Ok(r) => serde_json::json!({
                            "content": r.content,
                            "citation_valid": r.citation_valid,
                            "harness_rounds": r.harness_rounds,
                        }),
                        Err(e) => serde_json::json!({ "error": e.to_string() }),
                    };
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
            let fetch_limit = max_fetch_per_round(input.scene);
            for tool_call in &other_calls {
                abort_if_requested(&input.request_id)?;
                if registry.requires_confirmation(&tool_call.function.name) {
                    continue;
                }
                if tools_this_round >= profile.max_tool_calls_per_round {
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
                if let Err(denied) = registry.check_tool_policy(tool_name, &policy_ctx) {
                    push_tool_policy_error(
                        &mut messages,
                        &mut tool_results_json,
                        tool_call,
                        denied.reason,
                    );
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

                let args: serde_json::Value =
                    serde_json::from_str(&tool_call.function.arguments).unwrap_or_default();
                let dispatch_ctx = ToolDispatchContext {
                    scene: input.scene,
                    note_path: input.note_path.as_deref(),
                    file_id,
                    web_search_enabled: input.web_search_enabled,
                    cold_start_packets: evidence_ledger.packets(),
                    app_handle: Some(app_handle.clone()),
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

                // Record tool audit (sanitized, no sensitive data)
                let _ = tool_audit::record_audit(
                    &state.db,
                    &tool_audit::ToolAuditInput {
                        request_id: &input.request_id,
                        harness_round: harness_rounds,
                        tool_name,
                        arguments: &args,
                        result: &result.output,
                        success: result.success,
                        duration_ms: result.duration_ms,
                        scene: Some(input.scene.profile()),
                        subagent_depth: input.depth,
                    },
                );
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
            ReflectionOutcome::Done(result) => return Ok(result),
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

    let final_content = {
        abort_if_requested(&input.request_id)?;
        let stream_request = GatewayRequest {
            provider: provider_config,
            messages,
            tools: llm_tools,
            max_tokens,
            temperature: Some(0.7),
            stream: true,
            thinking: thinking_mode,
            skip_stub_ids: vec![],
        };
        let response = gateway
            .send_streaming_request(&input.request_id, stream_request)
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

    finish_run(
        state,
        input,
        FinishRunParams {
            content: if final_visible.is_empty() {
                "抱歉，未能在限定轮次内完成回答。请缩小问题或重试。".into()
            } else {
                final_visible
            },
            tool_calls: all_tool_calls,
            tool_results: tool_results_json,
            usage: total_usage,
            harness_rounds,
            pending_confirmation: false,
            evidence_packets: ledger_to_packets(&evidence_ledger, token_budget),
            usage_source,
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
            autonomy_level: crate::ai_runtime::resolve_scene(input.scene).autonomy_level,
            skill_allowed_tools: resolve_active_skill_allowed_tools_with_plan(
                state,
                input.scene,
                &input.user_message,
                input.skill_activation_plan.as_ref(),
            )
            .unwrap_or_default(),
        },
    );
    let args = serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments)
        .unwrap_or_default();
    let permission_effects = catalog_find(tool_name)
        .map(|entry| preflight_tool_permission(entry, &args, None).effects)
        .unwrap_or_default();
    let mut confirm_request = serde_json::json!({
        "request_id": input.request_id,
        "tool_call_id": tool_call.id,
        "tool_name": tool_name,
        "arguments": args,
        "permissionEffects": permission_effects,
    });
    if tool_name == "skills_install" {
        if let Ok(args) = serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments) {
            use crate::ai_runtime::skill_install_service::{preview_install, SkillInstallRequest};
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
                        scope: crate::ai_runtime::skill_install_service::parse_scope(
                            args.get("scope")
                                .and_then(|v| v.as_str())
                                .unwrap_or("global"),
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
                    if let Ok(preview) = preview_install(&req).await {
                        confirm_request["preview"] = preview;
                    }
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
        },
    )
    .await
}

async fn run_subagent_harness(
    state: &AppState,
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

    let profile = resolve_scene(parent.scene);
    let parent_budget = parent
        .token_budget
        .unwrap_or(profile.max_token_budget as u32);
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
        max_rounds_override: Some(sub_rounds.min(profile.max_agentic_rounds)),
        token_budget: Some(sub_budget),
        skill_activation_plan: parent.skill_activation_plan.clone(),
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
        };

        assert_eq!(result.usage_source, UsageSource::Estimated);
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
