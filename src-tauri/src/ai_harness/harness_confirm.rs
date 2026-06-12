//! Tool confirmation: append tool results to checkpoint and resume harness.

use tauri::AppHandle;

use crate::ai_harness::tool_turn::skip_stub_ids_for_checkpoint;
use crate::ai_runtime::harness::{
    merge_tool_packets_into, run_harness, HarnessRunInput, HarnessRunResult,
};
use crate::ai_runtime::harness_support::{
    load_harness_checkpoint, save_harness_checkpoint, HarnessCheckpoint,
};
use crate::ai_runtime::model_gateway::{
    prepare_tool_api_messages, repair_tool_api_messages, LlmMessage, MessageRole,
};
use crate::ai_runtime::tool_dispatch::{dispatch_tool, ToolDispatchContext};
use crate::ai_runtime::tool_executor::ToolRegistry;
use crate::ai_runtime::tool_policy::ToolPolicyContext;
use crate::ai_runtime::trace::{TraceRecorder, TraceStatus};
use crate::ai_runtime::AiScene;
use crate::app::{AppState, PendingToolCall};
use crate::error::{AppError, AppResult};

/// Classify resume failures for frontend recovery and telemetry.
pub fn classify_resume_error(message: &str) -> &'static str {
    let lower = message.to_lowercase();
    if lower.contains("checkpoint") || message.contains("未找到可恢复") {
        "checkpoint_missing"
    } else if lower.contains("messages with role 'tool'")
        || lower.contains("tool_calls")
        || message.contains("工具续聊消息序列无效")
    {
        "invalid_tool_chain"
    } else if lower.contains("400") || lower.contains("bad request") {
        "provider_bad_request"
    } else {
        "resume_failed"
    }
}

/// Append a tool-role message to checkpoint and persist.
#[allow(clippy::too_many_arguments)]
pub fn append_tool_message_to_checkpoint(
    db: &crate::storage::db::Database,
    request_id: &str,
    tool_call_id: &str,
    tool_content: String,
    tool_result_status: &str,
    result_output: Option<serde_json::Value>,
    merge_packets: Option<(&str, &serde_json::Value)>,
    policy_ctx: Option<&ToolPolicyContext>,
) -> AppResult<HarnessCheckpoint> {
    let mut cp = load_harness_checkpoint(db, request_id)?
        .ok_or_else(|| AppError::msg("未找到可恢复的 checkpoint"))?;

    cp.messages.push(LlmMessage {
        role: MessageRole::Tool,
        content: tool_content,
        tool_call_id: Some(tool_call_id.to_string()),
        tool_calls: None,
        ..Default::default()
    });

    repair_tool_api_messages(&mut cp.messages);
    let registry = ToolRegistry::new();
    let ctx = policy_ctx
        .cloned()
        .unwrap_or_else(|| policy_ctx_from_checkpoint_meta(&cp.meta));
    let skip = skip_stub_ids_for_checkpoint(&cp, &registry, &ctx);
    prepare_tool_api_messages(&mut cp.messages, &skip);

    let mut entry = serde_json::json!({
        "tool_call_id": tool_call_id,
        "status": tool_result_status,
    });
    if let Some(out) = result_output {
        entry["result"] = out;
    }
    cp.tool_results.push(entry);

    if let Some((tool_name, output)) = merge_packets {
        merge_tool_packets_into(tool_name, output, &mut cp.evidence_packets);
    }

    save_harness_checkpoint(db, request_id, &cp)?;
    Ok(cp)
}

fn policy_ctx_from_checkpoint_meta(
    meta: &crate::ai_runtime::harness_support::HarnessCheckpointMeta,
) -> ToolPolicyContext {
    let scene: AiScene =
        serde_json::from_str(&format!("\"{}\"", meta.scene)).unwrap_or(AiScene::KnowledgeLookup);
    let profile = crate::ai_runtime::resolve_scene(scene);
    ToolPolicyContext {
        scene,
        autonomy_level: profile.autonomy_level,
        web_search_enabled: meta.web_search_enabled,
        skill_allowed_tools: meta
            .skill_activation_plan
            .as_ref()
            .map(crate::ai_runtime::SkillActivationPlanSummary::allowed_tools)
            .unwrap_or_default(),
        depth: meta.depth,
    }
}

/// Resume harness from checkpoint after tool confirm (approve / reject / modify).
pub async fn resume_harness_after_tool_confirm(
    state: &AppState,
    app_handle: &AppHandle,
    request_id: &str,
) -> AppResult<HarnessRunResult> {
    let cp = load_harness_checkpoint(&state.db, request_id)?
        .ok_or_else(|| AppError::msg("未找到可恢复的 checkpoint"))?;
    let scene: AiScene = serde_json::from_str(&format!("\"{}\"", cp.meta.scene))
        .map_err(|e| AppError::msg(format!("invalid scene in checkpoint: {e}")))?;

    TraceRecorder::update_status(&state.db, request_id, TraceStatus::ModelCalled)?;

    let (resolved, provider_config, thinking) =
        if let (Some(provider_id), Some(model), Some(slot)) = (
            cp.meta.provider_id.as_deref(),
            cp.meta.model.as_deref(),
            cp.meta.capability_slot,
        ) {
            let mut resolved =
                crate::llm::config::resolve_for_provider(&state.db, provider_id, Some(model))?;
            if let Some(thinking) = cp.meta.thinking {
                resolved.thinking = thinking;
            }
            if let Some(output_budget) = cp.meta.output_budget {
                resolved.output_budget = output_budget;
            }
            if let Some(endpoint_family) = cp.meta.endpoint_family {
                resolved.endpoint_family = endpoint_family;
            }
            let thinking = cp.meta.thinking.unwrap_or(resolved.thinking);
            let provider_config = resolved.to_provider_config_for_slot(slot);
            (resolved, provider_config, thinking)
        } else {
            let resolved = crate::llm::config::resolve_for_scene(&state.db, scene)?;
            let provider_config = resolved.to_provider_config(scene);
            let thinking = resolved.thinking;
            (resolved, provider_config, thinking)
        };
    let user_message = latest_user_message(&cp.messages);

    let harness_result = run_harness(
        state,
        app_handle,
        HarnessRunInput {
            request_id: request_id.to_string(),
            scene,
            session_id: cp.meta.session_id,
            note_path: cp.meta.note_path.clone(),
            note_title: cp.meta.note_title.clone(),
            selection_excerpt: cp.meta.selection_excerpt.clone(),
            cold_start_packets: cp.meta.cold_start_packets.clone(),
            web_search_enabled: cp.meta.web_search_enabled,
            user_message,
            history_messages: vec![],
            depth: cp.meta.depth,
            resume_from_checkpoint: true,
            token_budget: None,
            max_rounds_override: None,
            skill_activation_plan: cp.meta.skill_activation_plan.clone(),
        },
        provider_config,
        Some(resolved.output_budget),
        thinking,
    )
    .await?;

    if !harness_result.pending_confirmation {
        TraceRecorder::update_status(&state.db, request_id, TraceStatus::Completed)?;
    }

    Ok(harness_result)
}

fn latest_user_message(messages: &[LlmMessage]) -> String {
    messages
        .iter()
        .rev()
        .find(|message| matches!(message.role, MessageRole::User))
        .map(|message| message.content.clone())
        .unwrap_or_default()
}

/// Resume harness; on failure restore trace to awaiting confirm so checkpoint stays loadable.
pub async fn resume_harness_after_tool_confirm_or_restore(
    state: &AppState,
    app_handle: &AppHandle,
    request_id: &str,
) -> AppResult<HarnessRunResult> {
    match resume_harness_after_tool_confirm(state, app_handle, request_id).await {
        Ok(result) => Ok(result),
        Err(e) => {
            let _ = TraceRecorder::update_status(
                &state.db,
                request_id,
                TraceStatus::AwaitingToolConfirmation,
            );
            let code = classify_resume_error(&e.to_string());
            Err(AppError::msg(format!("{code}: {e}")))
        }
    }
}

/// Dispatch an approved tool and append its result to checkpoint (does not resume).
pub async fn dispatch_approved_tool_to_checkpoint(
    state: &AppState,
    app_handle: &AppHandle,
    pending: &PendingToolCall,
    tool_call_id: &str,
    args: &serde_json::Value,
) -> AppResult<()> {
    let registry = crate::ai_runtime::tool_executor::ToolRegistry::new();
    let policy_ctx = crate::ai_runtime::tool_policy::ToolPolicyContext {
        scene: pending.scene,
        autonomy_level: pending.autonomy_level,
        web_search_enabled: pending.web_search_enabled,
        skill_allowed_tools: pending.skill_allowed_tools.clone(),
        depth: 0,
    };

    if let Err(denied) = registry.check_tool_policy(&pending.tool_name, &policy_ctx) {
        let hint =
            crate::ai_runtime::tool_policy::denial_user_message(denied.reason, &pending.tool_name);
        let payload = serde_json::json!({ "error": hint, "policy_denied": true });
        let content = serde_json::to_string(&payload).unwrap_or_default();
        append_tool_message_to_checkpoint(
            &state.db,
            &pending.request_id,
            tool_call_id,
            content,
            "error",
            Some(payload.clone()),
            None,
            Some(&policy_ctx),
        )?;
        let _ = crate::ai_runtime::tool_audit::record_audit(
            &state.db,
            &crate::ai_runtime::tool_audit::ToolAuditInput {
                request_id: &pending.request_id,
                harness_round: 0,
                tool_name: &pending.tool_name,
                arguments: args,
                result: &payload,
                success: false,
                duration_ms: 0,
                scene: Some(pending.scene.profile()),
                subagent_depth: 0,
            },
        );
        return Ok(());
    }

    let file_id = pending.file_id;
    let result = dispatch_tool(
        state,
        &ToolDispatchContext {
            scene: pending.scene,
            note_path: pending.note_path.as_deref(),
            file_id,
            web_search_enabled: pending.web_search_enabled,
            cold_start_packets: &[],
            app_handle: Some(app_handle.clone()),
        },
        &pending.tool_name,
        args,
    )
    .await;

    let audit_result = if result.success {
        result.output.clone()
    } else {
        serde_json::json!({ "error": result.error.as_deref().unwrap_or("unknown") })
    };

    let (tool_content, status, output, merge) = if result.success {
        let output_str = serde_json::to_string(&result.output).unwrap_or_else(|_| "{}".into());
        (
            output_str,
            "completed",
            Some(result.output.clone()),
            Some((pending.tool_name.as_str(), &result.output)),
        )
    } else {
        let err = result.error.as_deref().unwrap_or("unknown");
        (
            format!(
                "{{\"error\": {}}}",
                serde_json::to_string(err).unwrap_or_default()
            ),
            "error",
            None,
            None,
        )
    };

    append_tool_message_to_checkpoint(
        &state.db,
        &pending.request_id,
        tool_call_id,
        tool_content,
        status,
        output,
        merge,
        Some(&policy_ctx),
    )?;

    let _ = crate::ai_runtime::tool_audit::record_audit(
        &state.db,
        &crate::ai_runtime::tool_audit::ToolAuditInput {
            request_id: &pending.request_id,
            harness_round: 0,
            tool_name: &pending.tool_name,
            arguments: args,
            result: &audit_result,
            success: result.success,
            duration_ms: result.duration_ms,
            scene: Some(pending.scene.profile()),
            subagent_depth: 0,
        },
    );

    Ok(())
}

/// Append a rejected tool result and prepare for resume.
pub fn append_rejected_tool_to_checkpoint(
    state: &AppState,
    request_id: &str,
    tool_call_id: &str,
) -> AppResult<()> {
    let content = serde_json::json!({
        "status": "rejected",
        "message": "用户已拒绝执行此工具，请在不使用该工具的前提下继续回答。",
    });
    let content_str = serde_json::to_string(&content).unwrap_or_default();
    append_tool_message_to_checkpoint(
        &state.db,
        request_id,
        tool_call_id,
        content_str,
        "rejected",
        Some(content),
        None,
        None,
    )?;
    let _ = crate::ai_runtime::tool_audit::record_audit(
        &state.db,
        &crate::ai_runtime::tool_audit::ToolAuditInput {
            request_id,
            harness_round: 0,
            tool_name: "tool_confirmation",
            arguments: &serde_json::json!({ "tool_call_id": tool_call_id }),
            result: &serde_json::json!({ "error": "rejected" }),
            success: false,
            duration_ms: 0,
            scene: None,
            subagent_depth: 0,
        },
    );
    Ok(())
}
