//! Unified Agent Harness — multi-round tool loop with streaming final response.

use futures_util::future::join_all;
use tauri::{AppHandle, Emitter};

use super::archive::save_round_checkpoint;
use super::context::{build_initial_messages, prepare_environment_and_skills, resolve_file_id};
use super::finalize::{finish_run, FinishRunParams, ingest_tool_packets, ledger_to_packets};
use super::planning::{resolve_max_rounds, resolve_token_budget};
use super::reflection::{run_reflection_round, ReflectionOutcome};
use super::tools::max_fetch_per_round;
use super::trace_emit::{emit_thinking, emit_trace_phase};
use super::types::{HarnessPhase, HarnessRunInput, HarnessRunResult};
use super::util::accumulate_usage;
use crate::ai_runtime::evidence_ledger::EvidenceLedger;
use crate::ai_runtime::harness_support::{
    extract_thinking_blocks, load_harness_checkpoint, HarnessCheckpointMeta,
};
use crate::ai_runtime::model_gateway::{
    clear_abort, is_abort_requested, GatewayRequest, LlmMessage, MessageRole, ModelGateway,
    ProviderConfig, TokenUsage, ToolCall,
};
use crate::ai_runtime::scene_router::resolve_scene;
use crate::ai_runtime::tool_dispatch::{dispatch_tool_with_retry, ToolDispatchContext};
use crate::ai_runtime::tool_executor::{check_tool_permission, ToolRegistry, ToolSurfaceFilter};
use crate::ai_runtime::tool_fallback::{
    parse_tool_calls_from_content, should_retry_tool_parse, strip_tool_markup_from_visible,
};
use crate::app::AppState;
use crate::error::{AppError, AppResult};

/// Run the unified agent harness loop.
pub async fn run_harness(
    state: &AppState,
    app_handle: &AppHandle,
    input: HarnessRunInput,
    provider_config: crate::ai_runtime::model_gateway::ProviderConfig,
    max_tokens: Option<u32>,
) -> AppResult<HarnessRunResult> {
    let profile = resolve_scene(input.scene);
    let registry = ToolRegistry::new();
    let scene_tools = registry.tools_for_surface(
        input.scene,
        ToolSurfaceFilter {
            web_search_enabled: input.web_search_enabled,
            depth: input.depth,
            only_auto: false,
        },
    );
    let llm_tools = ModelGateway::tools_to_llm_format(&scene_tools);

    let (env_text, skills_prompt) =
        prepare_environment_and_skills(
            state,
            input.scene,
            input.note_path.as_deref(),
            input.note_title.as_deref(),
            input.selection_excerpt.as_deref(),
            &scene_tools,
        )?;

    let file_id = resolve_file_id(state, input.note_path.as_deref())?;

    let mut messages = build_initial_messages(
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

    let gateway = ModelGateway::with_defaults(app_handle.clone(), vec![provider_config.clone()])?;

    let mut total_usage = TokenUsage::default();
    let mut all_tool_calls: Vec<ToolCall> = Vec::new();
    let mut tool_results_json: Vec<serde_json::Value> = Vec::new();
    let mut evidence_ledger = EvidenceLedger::new(input.cold_start_packets.clone());
    let mut harness_rounds: u32 = 0;
    let pending_confirmation = false;
    let mut reflection_done = false;
    let mut bonus_round_used = false;
    let token_budget = resolve_token_budget(input.scene, input.token_budget);
    let mut max_rounds = resolve_max_rounds(input.scene, input.max_rounds_override);

    if input.resume_from_checkpoint {
        if let Some(cp) = load_harness_checkpoint(&state.db, &input.request_id)? {
            messages = cp.messages;
            harness_rounds = cp.round;
            all_tool_calls = cp.tool_calls;
            tool_results_json = cp.tool_results;
            evidence_ledger = EvidenceLedger::new(cp.evidence_packets);
            total_usage = cp.usage;
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
    };

    'agent: loop {
        while harness_rounds < max_rounds {
            abort_if_requested(&input.request_id)?;
            if total_usage.total_tokens >= token_budget {
                break 'agent;
            }
            harness_rounds += 1;

            let request = GatewayRequest {
                provider: provider_config.clone(),
                messages: messages.clone(),
                tools: llm_tools.clone(),
                max_tokens,
                temperature: Some(0.7),
                stream: false,
            };

            let response = gateway.send_request(request).await?;
            accumulate_usage(&mut total_usage, &response.usage);

            let mut tool_calls = response.tool_calls.clone();
            if tool_calls.is_empty() {
                if let Some(content) = &response.content {
                    tool_calls = parse_tool_calls_from_content(content);
                }
            }

            if should_retry_tool_parse(&tool_calls) {
                messages.push(LlmMessage {
                    role: MessageRole::User,
                    content: "工具参数 JSON 不完整，请重新输出合法的 tool_calls。".into(),
                    tool_call_id: None,
                    tool_calls: None,
                });
                continue;
            }

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
                        pending_confirmation,
                        evidence_packets: ledger_to_packets(&evidence_ledger, token_budget),
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

            let (subagent_calls, other_calls): (Vec<_>, Vec<_>) = tool_calls
                .iter()
                .partition(|tc| tc.function.name == "spawn_subagent");

            let pending_tool_call = first_pending_confirmation_call(&registry, &tool_calls).cloned();
            let model_tool_calls = pending_tool_call
                .as_ref()
                .map(|tc| vec![tc.clone()])
                .unwrap_or_else(|| tool_calls.clone());

            all_tool_calls.extend(model_tool_calls.clone());
            let stripped_assistant =
                strip_tool_markup_from_visible(&response.content.clone().unwrap_or_default());
            let (visible_content, thinking) = extract_thinking_blocks(&stripped_assistant);
            if let Some(t) = thinking {
                emit_thinking(app_handle, &input.request_id, harness_rounds, &t)?;
            }
            let assistant_content = visible_content;
            messages.push(LlmMessage {
                role: MessageRole::Assistant,
                content: assistant_content.clone(),
                tool_call_id: None,
                tool_calls: Some(model_tool_calls),
            });

            if let Some(tool_call) = pending_tool_call {
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
                    assistant_content,
                    file_id,
                    &tool_call,
                )
                .await;
            }

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
                        content: output_str.clone(),
                        tool_call_id: Some(tc.id.clone()),
                        tool_calls: None,
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
            ) {
                tracing::warn!("checkpoint save failed for {}: {e}", input.request_id);
            }

            let mut tools_this_round = 0u32;
            let mut fetch_this_round = 0u32;
            let fetch_limit = max_fetch_per_round(input.scene);
            for tool_call in &other_calls {
                abort_if_requested(&input.request_id)?;
                if tools_this_round >= profile.max_tool_calls_per_round {
                    break;
                }
                let tool_name = &tool_call.function.name;
                if tool_name == "fetch_web_page" && fetch_this_round >= fetch_limit {
                    let err_msg =
                        format!("本轮 fetch_web_page 已达上限 ({fetch_limit})");
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
                        ),
                        tool_call_id: Some(tool_call.id.clone()),
                        tool_calls: None,
                    });
                    tool_results_json.push(serde_json::json!({
                        "tool_call_id": tool_call.id,
                        "status": "error",
                        "error": err_msg,
                    }));
                    tools_this_round += 1;
                    continue;
                }
                if let Some(spec) = registry.find(tool_name) {
                    if check_tool_permission(spec, input.scene, profile.autonomy_level).is_err() {
                        continue;
                    }
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
                };
                let result =
                    dispatch_tool_with_retry(state, &dispatch_ctx, tool_name, &args).await;
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
                    },
                    tool_call_id: Some(tool_call.id.clone()),
                    tool_calls: None,
                });

                tool_results_json.push(serde_json::json!({
                    "tool_call_id": tool_call.id,
                    "status": if result.success { "completed" } else { "error" },
                    "result": result.output,
                }));
                tools_this_round += 1;
            }

            if pending_confirmation {
                return finish_run(
                    state,
                    input,
                    FinishRunParams {
                        content: assistant_content,
                        tool_calls: all_tool_calls,
                        tool_results: tool_results_json,
                        usage: total_usage,
                        harness_rounds,
                        pending_confirmation: true,
                        evidence_packets: ledger_to_packets(&evidence_ledger, token_budget),
                    },
                )
                .await;
            }
        }

        if reflection_done || input.depth != 0 {
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
            &mut messages,
            &evidence_ledger,
            &all_tool_calls,
            &tool_results_json,
            &mut total_usage,
            harness_rounds,
            pending_confirmation,
            &mut bonus_round_used,
            &mut max_rounds,
            token_budget,
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
        };
        let response = gateway
            .send_streaming_request(&input.request_id, stream_request)
            .await?;
        accumulate_usage(&mut total_usage, &response.usage);
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
            pending_confirmation,
            evidence_packets: ledger_to_packets(&evidence_ledger, token_budget),
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

fn first_pending_confirmation_call<'a>(
    registry: &ToolRegistry,
    tool_calls: &'a [ToolCall],
) -> Option<&'a ToolCall> {
    tool_calls
        .iter()
        .find(|tc| registry.requires_confirmation(&tc.function.name))
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
    assistant_content: String,
    file_id: Option<i64>,
    tool_call: &ToolCall,
) -> AppResult<HarnessRunResult> {
    let tool_name = &tool_call.function.name;
    crate::llm::safe_lock(&state.pending_tool_calls).insert(
        tool_call.id.clone(),
        crate::app::PendingToolCall {
            tool_name: tool_name.clone(),
            arguments: tool_call.function.arguments.clone(),
            request_id: input.request_id.clone(),
            scene: input.scene,
            note_path: input.note_path.clone(),
            file_id,
            web_search_enabled: input.web_search_enabled,
        },
    );
    let confirm_request = serde_json::json!({
        "request_id": input.request_id,
        "tool_call_id": tool_call.id,
        "tool_name": tool_name,
        "arguments": serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments).unwrap_or_default(),
    });
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
        history_messages: vec![("user".to_string(), task)],
        depth: parent.depth + 1,
        resume_from_checkpoint: false,
        max_rounds_override: Some(sub_rounds.min(profile.max_agentic_rounds)),
        token_budget: Some(sub_budget),
    };

    run_harness(state, app_handle, sub_input, provider_config, max_tokens).await
}

