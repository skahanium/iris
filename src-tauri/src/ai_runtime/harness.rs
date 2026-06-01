//! Unified Agent Harness — multi-round tool loop with streaming final response.

use futures_util::future::join_all;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::ai_runtime::environment::{build_environment_map, EnvironmentInput};
use crate::ai_runtime::harness_support::{
    compact_evidence, compress_history_messages, extract_thinking_blocks, load_harness_checkpoint,
    save_harness_checkpoint, HarnessCheckpoint, HarnessCheckpointMeta,
};
use crate::ai_runtime::model_gateway::{
    GatewayRequest, LlmMessage, MessageRole, ModelGateway, ProviderConfig, TokenUsage, ToolCall,
};
use crate::ai_runtime::scene_router::resolve_scene;
use crate::ai_runtime::skills::{inject_into_prompt, scan_all};
use crate::ai_runtime::tool_dispatch::{dispatch_tool_with_retry, ToolDispatchContext};
use crate::ai_runtime::tool_executor::{check_tool_permission, ToolRegistry};
use crate::ai_runtime::tool_fallback::{
    parse_tool_calls_from_content, should_retry_tool_parse, strip_tool_markup_from_visible,
};
use crate::ai_runtime::trace::{TraceRecorder, TraceStatus};
use crate::ai_runtime::{AiScene, ContextPacket};
use crate::app::AppState;
use crate::error::{AppError, AppResult};

/// Harness progress phase for structured UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessPhase {
    ToolStart,
    ToolComplete,
    SubagentSpawn,
    SubagentComplete,
    Reflection,
    FinalStream,
    Thinking,
}

/// Harness progress event for the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct HarnessTraceEvent {
    pub request_id: String,
    pub round: u32,
    pub phase: HarnessPhase,
    pub tool_name: String,
    pub status: String,
    pub message: Option<String>,
    pub output_preview: Option<String>,
}

/// Result of a harness run.
#[derive(Debug, Clone, Serialize)]
pub struct HarnessRunResult {
    pub request_id: String,
    pub session_id: i64,
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub tool_results: Vec<serde_json::Value>,
    pub usage: TokenUsage,
    pub citation_valid: bool,
    pub harness_rounds: u32,
    pub pending_confirmation: bool,
    /// 冷启动 + 工具检索合并后的证据包，供前端证据抽屉与引用跳转。
    pub evidence_packets: Vec<ContextPacket>,
}

/// Inputs for a harness run.
#[derive(Debug, Clone)]
pub struct HarnessRunInput {
    pub request_id: String,
    pub scene: AiScene,
    pub session_id: i64,
    pub note_path: Option<String>,
    pub note_title: Option<String>,
    pub selection_excerpt: Option<String>,
    pub cold_start_packets: Vec<ContextPacket>,
    pub web_search_enabled: bool,
    pub history_messages: Vec<(String, String)>,
    /// Sub-agent nesting depth (0 = root harness).
    pub depth: u32,
    /// Resume from persisted checkpoint (harness_resume).
    pub resume_from_checkpoint: bool,
}

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
    let mut scene_tools: Vec<_> = registry
        .for_scene(input.scene)
        .into_iter()
        .cloned()
        .collect();
    if !input.web_search_enabled {
        scene_tools.retain(|t| t.name != "web_search" && t.name != "fetch_web_page");
    }
    if input.depth >= 2 {
        scene_tools.retain(|t| t.name != "spawn_subagent");
    }
    let llm_tools = ModelGateway::tools_to_llm_format(&scene_tools);

    let vault = state.vault_path()?;
    let env_text = build_environment_map(
        &state.db,
        &vault,
        &EnvironmentInput {
            scene: input.scene,
            note_path: input.note_path.as_deref(),
            note_title: input.note_title.as_deref(),
            selection_excerpt: input.selection_excerpt.as_deref(),
            tools: &scene_tools,
        },
    )?;

    let file_id = resolve_file_id(state, input.note_path.as_deref())?;

    let all_skills = scan_all(&vault)?;
    let enabled_skills: Vec<_> = all_skills.into_iter().filter(|s| s.enabled).collect();
    let skills_prompt = inject_into_prompt(&enabled_skills, input.scene);

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

    let dispatch_ctx = ToolDispatchContext {
        scene: input.scene,
        note_path: input.note_path.as_deref(),
        file_id,
        web_search_enabled: input.web_search_enabled,
        cold_start_packets: &input.cold_start_packets,
    };

    let mut total_usage = TokenUsage::default();
    let mut all_tool_calls: Vec<ToolCall> = Vec::new();
    let mut tool_results_json: Vec<serde_json::Value> = Vec::new();
    let mut evidence_packets = input.cold_start_packets.clone();
    let mut harness_rounds: u32 = 0;
    let mut pending_confirmation = false;
    let mut reflection_done = false;
    let mut bonus_round_used = false;
    let token_budget = profile.max_token_budget as u32;
    let mut max_rounds = profile.max_agentic_rounds;

    if input.resume_from_checkpoint {
        if let Some(cp) = load_harness_checkpoint(&state.db, &input.request_id)? {
            messages = cp.messages;
            harness_rounds = cp.round;
            all_tool_calls = cp.tool_calls;
            tool_results_json = cp.tool_results;
            evidence_packets = cp.evidence_packets;
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
                        evidence_packets,
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

            all_tool_calls.extend(tool_calls.clone());
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
                tool_calls: Some(tool_calls.clone()),
            });

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
            )?;

            compact_evidence(&mut evidence_packets, profile.default_token_budget);

            let mut tools_this_round = 0u32;
            let mut fetch_this_round = 0u32;
            let fetch_limit = max_fetch_per_round(input.scene);
            for tool_call in &other_calls {
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

                if registry.requires_confirmation(tool_name) {
                    crate::llm::safe_lock(&state.pending_tool_calls).insert(
                        tool_call.id.clone(),
                        crate::app::PendingToolCall {
                            tool_name: tool_name.clone(),
                            arguments: tool_call.function.arguments.clone(),
                            request_id: input.request_id.clone(),
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
                    pending_confirmation = true;
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
                let result = dispatch_tool_with_retry(state, &dispatch_ctx, tool_name, &args).await;
                if result.success {
                    if tool_name == "fetch_web_page" {
                        fetch_this_round += 1;
                    }
                    merge_tool_packets_into(tool_name, &result.output, &mut evidence_packets);
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
                        evidence_packets,
                    },
                )
                .await;
            }
        }

        if reflection_done || input.depth != 0 {
            break 'agent;
        }
        reflection_done = true;
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
        });
        let reflect_request = GatewayRequest {
            provider: provider_config.clone(),
            messages: messages.clone(),
            tools: vec![],
            max_tokens,
            temperature: Some(0.5),
            stream: false,
        };
        if let Ok(reflect_resp) = gateway.send_request(reflect_request).await {
            accumulate_usage(&mut total_usage, &reflect_resp.usage);
            if let Some(text) = reflect_resp.content {
                if text.contains("NEED_MORE_EVIDENCE")
                    && !bonus_round_used
                    && harness_rounds < profile.max_agentic_rounds
                {
                    bonus_round_used = true;
                    messages.push(LlmMessage {
                        role: MessageRole::Assistant,
                        content: text,
                        tool_call_id: None,
                        tool_calls: None,
                    });
                    messages.push(LlmMessage {
                        role: MessageRole::User,
                        content: "证据仍不足，请继续使用检索类工具补充证据后再作答。".into(),
                        tool_call_id: None,
                        tool_calls: None,
                    });
                    max_rounds = harness_rounds
                        .saturating_add(1)
                        .min(profile.max_agentic_rounds);
                    continue 'agent;
                } else if !reflect_resp.tool_calls.is_empty() {
                    // reflection produced tool calls — ignore, proceed to stream
                } else if !text.trim().is_empty() && reflect_resp.tool_calls.is_empty() {
                    let stripped = strip_tool_markup_from_visible(&text);
                    let (visible, thinking) = extract_thinking_blocks(&stripped);
                    if let Some(t) = thinking {
                        emit_thinking(app_handle, &input.request_id, harness_rounds, &t)?;
                    }
                    return finish_run(
                        state,
                        input,
                        FinishRunParams {
                            content: visible,
                            tool_calls: all_tool_calls,
                            tool_results: tool_results_json,
                            usage: total_usage,
                            harness_rounds,
                            pending_confirmation,
                            evidence_packets,
                        },
                    )
                    .await;
                }
            }
        }
        break 'agent;
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
            evidence_packets,
        },
    )
    .await
}

struct FinishRunParams {
    content: String,
    tool_calls: Vec<ToolCall>,
    tool_results: Vec<serde_json::Value>,
    usage: TokenUsage,
    harness_rounds: u32,
    pending_confirmation: bool,
    evidence_packets: Vec<ContextPacket>,
}

async fn finish_run(
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
    } = params;
    let citation_result =
        crate::ai_runtime::guardrails::verify_citations(&content, &evidence_packets);
    let citation_valid = matches!(
        citation_result,
        crate::ai_runtime::guardrails::GuardResult::Pass
    );
    TraceRecorder::update_status(&state.db, &input.request_id, TraceStatus::Completed)?;
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
    })
}

/// 将检索类工具返回的 `results` 并入本轮证据包（按 id 去重）。
fn max_fetch_per_round(scene: AiScene) -> u32 {
    match scene {
        AiScene::ResearchSynthesis => 2,
        _ => 1,
    }
}

fn merge_tool_packets_into(
    tool_name: &str,
    output: &serde_json::Value,
    acc: &mut Vec<ContextPacket>,
) {
    if !matches!(
        tool_name,
        "search_hybrid"
            | "search_semantic"
            | "search_keyword"
            | "get_regulation"
            | "web_search"
            | "fetch_web_page"
    ) {
        return;
    }
    let Some(results) = output.get("results").and_then(|v| v.as_array()) else {
        if tool_name == "get_regulation" {
            if let Ok(packet) = serde_json::from_value::<ContextPacket>(output.clone()) {
                push_packet_dedup(acc, packet);
            } else if let Some(reg) = output.get("regulation") {
                if let Ok(packet) = serde_json::from_value::<ContextPacket>(reg.clone()) {
                    push_packet_dedup(acc, packet);
                }
            }
        }
        return;
    };
    for value in results {
        if let Ok(packet) = serde_json::from_value::<ContextPacket>(value.clone()) {
            push_packet_dedup(acc, packet);
        }
    }
}

fn push_packet_dedup(acc: &mut Vec<ContextPacket>, packet: ContextPacket) {
    if acc.iter().any(|p| p.id == packet.id) {
        return;
    }
    acc.push(packet);
}

fn build_initial_messages(
    scene: AiScene,
    environment: &str,
    cold_start_packets: &[ContextPacket],
    history: &[(String, String)],
    web_search_enabled: bool,
    skills_fragment: Option<&str>,
) -> Vec<LlmMessage> {
    let persona = ModelGateway::unified_persona(scene, web_search_enabled);
    let mut system_content = format!("{persona}\n\n{environment}");
    if let Some(skills) = skills_fragment {
        if !skills.is_empty() {
            system_content.push_str("\n\n");
            system_content.push_str(skills);
        }
    }
    let mut messages = vec![LlmMessage {
        role: MessageRole::System,
        content: system_content,
        tool_call_id: None,
        tool_calls: None,
    }];

    if !cold_start_packets.is_empty() {
        let hint = ModelGateway::format_evidence_packets(cold_start_packets);
        messages.push(LlmMessage {
            role: MessageRole::System,
            content: format!(
                "## 本地知识库检索材料\n\n\
                 以下是从你的笔记中预检索到的相关材料，请认真参考并在回答中引用；\
                 同时结合工具检索与网络搜索交叉验证。\n\n{hint}"
            ),
            tool_call_id: None,
            tool_calls: None,
        });
    }

    let compressed = compress_history_messages(history);
    for (role, content) in compressed {
        let r = match role.as_str() {
            "assistant" => MessageRole::Assistant,
            "tool" => MessageRole::Tool,
            _ => MessageRole::User,
        };
        messages.push(LlmMessage {
            role: r,
            content,
            tool_call_id: None,
            tool_calls: None,
        });
    }

    messages
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
    };

    let profile = resolve_scene(parent.scene);
    let _max_rounds = sub_rounds.min(profile.max_agentic_rounds);

    run_harness(state, app_handle, sub_input, provider_config, max_tokens).await
}

#[allow(clippy::too_many_arguments)]
fn save_round_checkpoint(
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
) -> AppResult<()> {
    let checkpoint = HarnessCheckpoint {
        meta: meta.clone(),
        round,
        messages: messages.to_vec(),
        tool_calls: tool_calls.to_vec(),
        tool_results: tool_results.to_vec(),
        evidence_packets: evidence_packets.to_vec(),
        usage: usage.clone(),
        bonus_round_used,
    };
    let _ = save_harness_checkpoint(&state.db, &input.request_id, &checkpoint);
    Ok(())
}

fn emit_thinking(
    app_handle: &AppHandle,
    request_id: &str,
    round: u32,
    content: &str,
) -> AppResult<()> {
    app_handle
        .emit(
            "ai:thinking",
            &serde_json::json!({
                "request_id": request_id,
                "round": round,
                "content": content,
            }),
        )
        .map_err(|e| AppError::msg(format!("emit thinking: {e}")))
}

#[allow(clippy::too_many_arguments)]
fn emit_trace_phase(
    app_handle: &AppHandle,
    request_id: &str,
    round: u32,
    phase: HarnessPhase,
    tool_name: &str,
    status: &str,
    message: Option<String>,
    output_preview: Option<String>,
) -> AppResult<()> {
    app_handle
        .emit(
            "ai:harness_trace",
            &HarnessTraceEvent {
                request_id: request_id.to_string(),
                round,
                phase,
                tool_name: tool_name.to_string(),
                status: status.to_string(),
                message,
                output_preview,
            },
        )
        .map_err(|e| AppError::msg(format!("emit harness trace: {e}")))
}

fn resolve_file_id(state: &AppState, note_path: Option<&str>) -> AppResult<Option<i64>> {
    let Some(path) = note_path else {
        return Ok(None);
    };
    state.db.with_conn(|conn| {
        Ok(conn
            .query_row("SELECT id FROM files WHERE path = ?1", [path], |r| {
                r.get::<_, i64>(0)
            })
            .ok())
    })
}

fn accumulate_usage(total: &mut TokenUsage, delta: &TokenUsage) {
    total.prompt_tokens += delta.prompt_tokens;
    total.completion_tokens += delta.completion_tokens;
    total.total_tokens += delta.total_tokens;
    total.prompt_cache_hit_tokens += delta.prompt_cache_hit_tokens;
    total.prompt_cache_miss_tokens += delta.prompt_cache_miss_tokens;
}
