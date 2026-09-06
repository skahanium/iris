use super::ToolDispatchContext;
use crate::ai_runtime::policy_decision_engine::DocumentCapability;
use crate::app::AppState;
use crate::commands::file::is_vault_asset_path;
use crate::error::{AppError, AppResult};
use crate::storage::atomic_write::with_vault_move_lock;
use crate::storage::note_move::{execute_move_locked, prepare_move, NoteMovePlan};
use crate::storage::note_operations::{create_note, trash_note};
use crate::storage::paths::{resolve_vault_path, validate_user_note_relative_path};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
const MAX_ASSET_BYTES: usize = 20 * 1024 * 1024;

fn authorize(state: &AppState, ctx: &ToolDispatchContext<'_>, path: &str) -> AppResult<()> {
    ctx.ensure_run_active()?;
    ctx.ensure_write_target_matches(path)?;
    ctx.ensure_document_capability(path, DocumentCapability::ApplyChange)?;
    ctx.ensure_active_skill_scope_allows_path(&state.db, path)
}
fn argument<'a>(args: &'a serde_json::Value, key: &str) -> AppResult<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg(format!("missing {key}")))
}

pub(super) fn vault_create_note_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let path = argument(args, "target_path")?;
    let vault = state.vault_path()?;
    let receipt = create_note(
        state,
        &vault,
        path,
        args["content"].as_str().unwrap_or(""),
        || authorize(state, ctx, path),
    )?;
    Ok(serde_json::json!({"type":"vault_create_note", "path":path, "receipt":receipt}))
}

pub(super) fn vault_rename_move_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let path = argument(args, "path")?;
    let new_path = argument(args, "new_path")?;
    with_vault_move_lock(|| {
        let plan: NoteMovePlan = if let Some(value) = args.get("frozen_note_move") {
            serde_json::from_value(value.clone())?
        } else {
            prepare_move(state, path, new_path)?
        };
        if plan.path != path || plan.new_path != new_path {
            return Err(AppError::msg("note_move_identity_conflict"));
        }
        authorize(state, ctx, path)?;
        authorize(state, ctx, new_path)?;
        for edit in &plan.backlinks {
            authorize(state, ctx, &edit.path)?;
        }
        let receipt = execute_move_locked(state, &plan)?;
        Ok(
            serde_json::json!({"type":"vault_rename_move", "path":new_path, "previousPath":path, "receipt":receipt}),
        )
    })
}

pub(super) fn vault_delete_to_trash_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let path = argument(args, "path")?;
    let base_hash = argument(args, "base_content_hash")?;
    let vault = state.vault_path()?;
    let receipt = trash_note(state, &vault, path, base_hash, || {
        authorize(state, ctx, path)
    })?;
    Ok(
        serde_json::json!({"type":"vault_delete_to_trash", "path":path, "trashId":receipt.trash_id, "receipt":receipt}),
    )
}
pub(super) fn vault_asset_write_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing path"))?;
    let data_base64 = args["data_base64"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing data_base64"))?;
    if !is_vault_asset_path(path) {
        return Err(AppError::msg("资源路径必须位于 assets/ 下"));
    }
    let bytes = STANDARD
        .decode(data_base64.trim())
        .map_err(|e| AppError::msg(format!("无效的资源数据: {e}")))?;
    if bytes.is_empty() {
        return Err(AppError::msg("资源数据为空"));
    }
    if bytes.len() > MAX_ASSET_BYTES {
        return Err(AppError::msg("资源超过 20MB 限制"));
    }

    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, path)?;
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::storage::atomic_write::atomic_write(&abs, &bytes)?;

    Ok(serde_json::json!({
        "type": "vault_asset_write",
        "path": path,
        "bytes": bytes.len(),
    }))
}

pub(super) fn vault_version_list_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing path"))?;
    let vault = state.vault_path()?;
    let _abs = validate_user_note_relative_path(&vault, path)?;
    let versions = crate::version::version_list(state, path)?;
    Ok(serde_json::json!({
        "type": "vault_version_list",
        "path": path,
        "versions": versions,
        "count": versions.len(),
    }))
}
