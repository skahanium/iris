use crate::app::AppState;
use crate::error::{AppError, AppResult};

use super::ToolDispatchContext;

pub(super) async fn skills_list_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    let _ = ctx;
    let vault = state.vault_path()?;
    let entries = crate::ai_runtime::skill_install_service::list_skills(&state.db, &vault, None)?;
    Ok(serde_json::to_value(&entries).unwrap_or_default())
}

pub(super) async fn skills_install_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skill_install_service::{parse_scope, SkillInstallRequest};
    use crate::ai_runtime::skill_registry::SkillInstallSource;

    let source_str = args
        .get("source")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_install 缂哄皯 source"))?;
    let source = SkillInstallSource::parse(source_str)
        .ok_or_else(|| AppError::msg(format!("鏈煡 source: {source_str}")))?;
    let path_or_url = args
        .get("path_or_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_install 缂哄皯 path_or_url"))?
        .to_string();
    let scope = parse_scope(
        args.get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("global"),
    );
    let req = SkillInstallRequest {
        source,
        path_or_url,
        scope,
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
    let vault = state.vault_path()?;
    let entry = crate::ai_runtime::skill_install_service::install_skill(
        &state.db,
        &vault,
        ctx.app_handle.as_ref(),
        req,
    )
    .await?;
    Ok(serde_json::to_value(&entry).unwrap_or_default())
}

pub(super) async fn skills_uninstall_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skill_install_service::parse_scope;

    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_uninstall 缂哄皯 name"))?;
    let scope = parse_scope(
        args.get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("global"),
    );
    let vault = state.vault_path()?;
    crate::ai_runtime::skill_install_service::uninstall_skill(
        &state.db,
        &vault,
        ctx.app_handle.as_ref(),
        name,
        scope,
    )?;
    Ok(serde_json::json!({ "ok": true, "name": name }))
}

pub(super) async fn skills_update_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skill_install_service::{parse_scope, update_skill};

    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_update 缂哄皯 name"))?;
    let scope = parse_scope(
        args.get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("global"),
    );
    let vault = state.vault_path()?;
    let entry = update_skill(&state.db, &vault, ctx.app_handle.as_ref(), name, scope).await?;
    Ok(serde_json::to_value(&entry).unwrap_or_default())
}

pub(super) async fn skills_toggle_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skill_install_service::parse_scope;

    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_toggle 缂哄皯 name"))?;
    let enabled = args
        .get("enabled")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| AppError::msg("skills_toggle 缂哄皯 enabled"))?;
    let scope = parse_scope(
        args.get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("global"),
    );
    let vault = state.vault_path()?;
    crate::ai_runtime::skill_install_service::toggle_skill(
        &vault,
        ctx.app_handle.as_ref(),
        name,
        scope,
        enabled,
    )?;
    Ok(serde_json::json!({ "ok": true, "name": name, "enabled": enabled }))
}

pub(super) async fn skills_read_resource_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skill_install_service::parse_scope;
    use crate::ai_runtime::skills::read_skill_resource;

    let _ = ctx;
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_read_resource 缂哄皯 name"))?;
    let relative_path = args
        .get("relative_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_read_resource 缂哄皯 relative_path"))?;
    let scope = parse_scope(
        args.get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("global"),
    );
    let vault = state.vault_path()?;
    let content = read_skill_resource(&vault, name, scope, relative_path)?;
    Ok(serde_json::json!({ "content": content }))
}
