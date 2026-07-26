use crate::app::AppState;
use crate::error::{AppError, AppResult};

use super::ToolDispatchContext;

pub(super) fn is_skill_tool(name: &str) -> bool {
    matches!(name, "skills_list")
}

pub(super) async fn dispatch_skill_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    tool_name: &str,
    _args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    match tool_name {
        "skills_list" => skills_list_tool(state, ctx).await,
        _ => Err(AppError::msg(format!("unknown tool: {tool_name}"))),
    }
}

pub(super) async fn skills_list_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    let _ = ctx;
    let vault = state.vault_path()?;
    // Tool dispatch is inside a Run and must not scan a user-controlled
    // directory. The cache is populated at vault activation and explicit UI
    // refresh/confirmation boundaries.
    let skills = state.cached_skills_for_vault(&vault)?.unwrap_or_default();
    let entries = crate::ai_runtime::skills::skill_list_entries(skills);
    Ok(serde_json::to_value(&entries).unwrap_or_default())
}
