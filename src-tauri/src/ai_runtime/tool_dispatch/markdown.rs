use super::ToolDispatchContext;
use crate::ai_runtime::PatchApplyResult;
use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::note_operations::{apply_edit, NoteEdit};
use crate::storage::note_write::FileWriteIndexStatus;

pub(super) fn markdown_write_patch_apply(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    tool_name: &str,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let required = |key: &str| {
        args.get(key)
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::msg(format!("missing {key}")))
    };
    let target = args
        .get("target_path")
        .and_then(|v| v.as_str())
        .or(ctx.note_path)
        .ok_or_else(|| AppError::msg("missing target_path"))?;
    let range = args
        .get("range")
        .ok_or_else(|| AppError::msg("missing range"))?;
    let offset = |key: &str| {
        range
            .get(key)
            .and_then(|v| v.as_u64())
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| AppError::msg("invalid patch range"))
    };
    let edit = NoteEdit {
        path: target.to_string(),
        base_content_hash: required("base_content_hash")?.to_string(),
        range: offset("start")?..offset("end")?,
        original: args
            .get("original_text")
            .or_else(|| args.get("selection"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        replacement: required(if tool_name == "insert_text_at_cursor" {
            "text"
        } else {
            "replacement"
        })?
        .to_string(),
    };
    let vault = state.vault_path()?;
    let receipt = apply_edit(state, &vault, &edit, || {
        ctx.ensure_run_active()?;
        ctx.ensure_write_target_matches(target)?;
        ctx.ensure_document_capability(
            target,
            crate::ai_runtime::policy_decision_engine::DocumentCapability::ApplyChange,
        )?;
        ctx.ensure_active_skill_scope_allows_path(&state.db, target)
    })?;
    let warnings = if receipt.write.index_status == FileWriteIndexStatus::Degraded {
        vec!["文档已写入，但索引待修复。".to_string()]
    } else {
        Vec::new()
    };
    Ok(serde_json::json!({
        "type": "patch_apply", "tool_name": tool_name, "target_path": target,
        "receipt": receipt,
        "result": PatchApplyResult {
            success: true, new_content_hash: Some(receipt.after_hash.clone()), error: None, warnings,
        },
    }))
}
