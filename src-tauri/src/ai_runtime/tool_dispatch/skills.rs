use crate::ai_runtime::skills::SkillScope;
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
    use crate::ai_runtime::skill_install_service::{
        normalize_skill_scope_arg, SkillInstallRequest,
    };
    use crate::ai_runtime::skill_registry::SkillInstallSource;

    let source_str = args
        .get("source")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_install missing source"))?;
    let source = SkillInstallSource::parse(source_str)
        .ok_or_else(|| AppError::msg(format!("unknown source: {source_str}")))?;
    let path_or_url = args
        .get("path_or_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_install missing path_or_url"))?
        .to_string();
    let scope = normalize_skill_scope_arg(args.get("scope").and_then(|v| v.as_str()));
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

pub(super) async fn skills_prepare_workspace_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skill_install_service::{
        normalize_skill_scope_arg, prepare_skill_workspace,
    };

    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_prepare_workspace missing name"))?;
    let scope = normalize_skill_scope_arg(args.get("scope").and_then(|v| v.as_str()));
    let vault = state.vault_path()?;
    let result = prepare_skill_workspace(
        &vault,
        Some(&state.db),
        ctx.app_handle.as_ref(),
        name,
        scope,
    )?;
    Ok(serde_json::to_value(result).unwrap_or_default())
}

pub(super) async fn skills_uninstall_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skill_install_service::{normalize_skill_scope_arg, uninstall_skill};

    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_uninstall missing name"))?;
    let scope = normalize_skill_scope_arg(Some(
        args.get("scope")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::msg("skills_uninstall missing scope"))?,
    ));
    let vault = state.vault_path()?;
    uninstall_skill(&state.db, &vault, ctx.app_handle.as_ref(), name, scope)?;
    Ok(serde_json::json!({ "ok": true, "name": name }))
}

pub(super) async fn skills_update_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skill_install_service::{normalize_skill_scope_arg, update_skill};

    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_update missing name"))?;
    let scope = normalize_skill_scope_arg(Some(
        args.get("scope")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::msg("skills_update missing scope"))?,
    ));
    let vault = state.vault_path()?;
    let entry = update_skill(&state.db, &vault, ctx.app_handle.as_ref(), name, scope).await?;
    Ok(serde_json::to_value(&entry).unwrap_or_default())
}

pub(super) async fn skills_toggle_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skill_install_service::{normalize_skill_scope_arg, toggle_skill};

    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_toggle missing name"))?;
    let enabled = args
        .get("enabled")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| AppError::msg("skills_toggle missing enabled"))?;
    let scope = normalize_skill_scope_arg(Some(
        args.get("scope")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::msg("skills_toggle missing scope"))?,
    ));
    let vault = state.vault_path()?;
    toggle_skill(&vault, ctx.app_handle.as_ref(), name, scope, enabled)?;
    Ok(serde_json::json!({ "ok": true, "name": name, "enabled": enabled }))
}

fn read_resource_scope(
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
    name: &str,
) -> SkillScope {
    use crate::ai_runtime::skill_install_service::normalize_skill_scope_arg;

    if let Some(scope) = args.get("scope").and_then(|value| value.as_str()) {
        return normalize_skill_scope_arg(Some(scope));
    }
    ctx.skill_activation_plan
        .and_then(|plan| {
            plan.activated_skills
                .iter()
                .find(|skill| skill.name == name)
                .map(|skill| normalize_skill_scope_arg(Some(skill.scope.to_lowercase().as_str())))
        })
        .unwrap_or(SkillScope::Vault)
}

pub(super) async fn skills_read_resource_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skills::read_skill_resource;

    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_read_resource missing name"))?;
    let relative_path = args
        .get("relative_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_read_resource missing relative_path"))?;
    let scope = read_resource_scope(ctx, args, name);
    let vault = state.vault_path()?;
    let content = read_skill_resource(&vault, name, scope, relative_path)?;
    Ok(serde_json::json!({ "content": content }))
}
