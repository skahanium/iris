//! Tool confirmation: append tool results to checkpoint and resume harness.

use tauri::AppHandle;

use crate::ai_runtime::harness::{merge_tool_packets_into, run_harness, HarnessRunInput, HarnessRunResult};
use crate::ai_runtime::harness_support::{
    load_harness_checkpoint, save_harness_checkpoint, HarnessCheckpoint,
};
use crate::ai_runtime::model_gateway::{LlmMessage, MessageRole};
use crate::ai_runtime::tool_dispatch::{dispatch_tool, ToolDispatchContext};
use crate::ai_runtime::trace::{TraceRecorder, TraceStatus};
use crate::ai_runtime::AiScene;
use crate::app::{AppState, PendingToolCall};
use crate::error::{AppError, AppResult};

/// Append a tool-role message to checkpoint and persist.
pub fn append_tool_message_to_checkpoint(
    db: &crate::storage::db::Database,
    request_id: &str,
    tool_call_id: &str,
    tool_content: String,
    tool_result_status: &str,
    result_output: Option<serde_json::Value>,
    merge_packets: Option<(&str, &serde_json::Value)>,
) -> AppResult<HarnessCheckpoint> {
    let mut cp = load_harness_checkpoint(db, request_id)?
        .ok_or_else(|| AppError::msg("未找到可恢复的 checkpoint"))?;

    cp.messages.push(LlmMessage {
        role: MessageRole::Tool,
        content: tool_content,
        tool_call_id: Some(tool_call_id.to_string()),
        tool_calls: None,
    });

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

    let resolved = crate::llm::config::resolve_for_scene(&state.db, scene)?;
    let provider_config = resolved.to_provider_config(scene);

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
            history_messages: vec![],
            depth: cp.meta.depth,
            resume_from_checkpoint: true,
            token_budget: None,
            max_rounds_override: None,
        },
        provider_config,
        Some(resolved.output_budget),
    )
    .await?;

    if !harness_result.pending_confirmation {
        TraceRecorder::update_status(&state.db, request_id, TraceStatus::Completed)?;
    }

    Ok(harness_result)
}

/// Dispatch an approved tool and append its result to checkpoint (does not resume).
pub async fn dispatch_approved_tool_to_checkpoint(
    state: &AppState,
    pending: &PendingToolCall,
    tool_call_id: &str,
    args: &serde_json::Value,
) -> AppResult<()> {
    let file_id = pending.file_id;
    let result = dispatch_tool(
        state,
        &ToolDispatchContext {
            scene: pending.scene,
            note_path: pending.note_path.as_deref(),
            file_id,
            web_search_enabled: pending.web_search_enabled,
            cold_start_packets: &[],
        },
        &pending.tool_name,
        args,
    )
    .await;

    let (tool_content, status, output, merge) = if result.success {
        let output_str =
            serde_json::to_string(&result.output).unwrap_or_else(|_| "{}".into());
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
    )?;
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
    )?;
    Ok(())
}
