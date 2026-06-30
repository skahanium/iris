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
    let entries = crate::ai_runtime::skills::list_skills(&state.db, &vault, None)?;
    Ok(serde_json::to_value(&entries).unwrap_or_default())
}
