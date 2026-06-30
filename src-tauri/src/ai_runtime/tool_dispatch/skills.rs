use crate::ai_runtime::skills::SkillScope;
use crate::app::AppState;
use crate::error::{AppError, AppResult};

use super::ToolDispatchContext;

pub(super) fn is_skill_tool(name: &str) -> bool {
    matches!(
        name,
        "skills_list"
            | "mcp_runtime_profiles_list"
            | "mcp_runtime_diagnostics"
            | "mcp_runtime_tool_inventory_list"
            | "mcp_runtime_health_events_list"
            | "mcp_runtime_tools_list"
            | "mcp_runtime_health_check"
            | "mcp_runtime_capability_call"
            | "mcp_server_catalog_upsert"
            | "mcp_runtime_profile_upsert"
            | "mcp_runtime_profile_toggle"
            | "mcp_runtime_profile_delete"
            | "skills_install"
            | "skills_prepare_workspace"
            | "skills_uninstall"
            | "skills_update"
            | "skills_toggle"
            | "skills_read_resource"
            | "skills_workspace_list"
            | "skills_workspace_read"
            | "skills_workspace_write"
    )
}

pub(super) async fn dispatch_skill_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    tool_name: &str,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    match tool_name {
        "skills_list" => skills_list_tool(state, ctx).await,
        "mcp_runtime_profiles_list" => mcp_runtime_profiles_list_tool(state).await,
        "mcp_runtime_diagnostics" => mcp_runtime_diagnostics_tool(state, args).await,
        "mcp_runtime_tool_inventory_list" => {
            mcp_runtime_tool_inventory_list_tool(state, args).await
        }
        "mcp_runtime_health_events_list" => mcp_runtime_health_events_list_tool(state, args).await,
        "mcp_runtime_tools_list" => mcp_runtime_tools_list_tool(state, args).await,
        "mcp_runtime_health_check" => mcp_runtime_health_check_tool(state, args).await,
        "mcp_runtime_capability_call" => mcp_runtime_capability_call_tool(state, args).await,
        "mcp_server_catalog_upsert" => mcp_server_catalog_upsert_tool(state, args).await,
        "mcp_runtime_profile_upsert" => mcp_runtime_profile_upsert_tool(state, args).await,
        "mcp_runtime_profile_toggle" => mcp_runtime_profile_toggle_tool(state, args).await,
        "mcp_runtime_profile_delete" => mcp_runtime_profile_delete_tool(state, args).await,
        "skills_install" => skills_install_tool(state, ctx, args).await,
        "skills_prepare_workspace" => skills_prepare_workspace_tool(state, ctx, args).await,
        "skills_uninstall" => skills_uninstall_tool(state, ctx, args).await,
        "skills_update" => skills_update_tool(state, ctx, args).await,
        "skills_toggle" => skills_toggle_tool(state, ctx, args).await,
        "skills_read_resource" => skills_read_resource_tool(state, ctx, args).await,
        "skills_workspace_list" => skills_workspace_list_tool(state, ctx, args).await,
        "skills_workspace_read" => skills_workspace_read_tool(state, ctx, args).await,
        "skills_workspace_write" => skills_workspace_write_tool(state, ctx, args).await,
        _ => Err(AppError::msg(format!("unknown tool: {tool_name}"))),
    }
}

pub(super) async fn skills_list_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    let _ = ctx;
    let vault = state.vault_path()?;
    let entries = crate::ai_runtime::skill_install_service::list_skills(&state.db, &vault, None)?;
    Ok(serde_json::to_value(&entries).unwrap_or_default())
}

fn sanitize_health_events(
    events: Vec<crate::ai_runtime::mcp_runtime_registry::McpHealthEventSummary>,
) -> Vec<crate::ai_runtime::mcp_runtime_registry::McpHealthEventSummary> {
    events
        .into_iter()
        .map(|mut event| {
            event.message = event
                .message
                .map(|message| crate::ai_runtime::trace::redact_classified_leaks(&message));
            event.metadata_json =
                crate::ai_runtime::trace::redact_classified_leaks(&event.metadata_json);
            event
        })
        .collect()
}

pub(super) async fn mcp_runtime_profiles_list_tool(
    state: &AppState,
) -> AppResult<serde_json::Value> {
    let profiles = crate::ai_runtime::mcp_runtime_registry::list_runtime_profiles(&state.db)?;
    Ok(serde_json::json!({ "profiles": profiles }))
}

pub(super) async fn mcp_runtime_diagnostics_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let health_limit = args
        .get("health_limit")
        .and_then(|value| value.as_u64())
        .unwrap_or(20)
        .clamp(1, 50) as usize;
    let profiles = crate::ai_runtime::mcp_runtime_registry::list_runtime_profiles(&state.db)?;
    let Some(profile_id) = args.get("profile_id").and_then(|value| value.as_str()) else {
        return Ok(serde_json::json!({ "profiles": profiles }));
    };
    let profile = profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| AppError::msg(format!("unknown MCP runtime profile: {profile_id}")))?;
    let tools =
        crate::ai_runtime::mcp_runtime_registry::list_tool_inventory(&state.db, profile_id)?;
    let health_events = sanitize_health_events(
        crate::ai_runtime::mcp_runtime_registry::list_recent_health_events(
            &state.db,
            profile_id,
            health_limit,
        )?,
    );
    Ok(serde_json::json!({
        "profile_id": profile_id,
        "profile": profile,
        "tools": tools,
        "health_events": health_events
    }))
}

pub(super) async fn mcp_runtime_tool_inventory_list_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let profile_id = mcp_profile_id_arg(args, "mcp_runtime_tool_inventory_list")?;
    let tools =
        crate::ai_runtime::mcp_runtime_registry::list_tool_inventory(&state.db, &profile_id)?;
    Ok(serde_json::json!({
        "profile_id": profile_id,
        "tools": tools,
    }))
}

pub(super) async fn mcp_runtime_health_events_list_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let profile_id = mcp_profile_id_arg(args, "mcp_runtime_health_events_list")?;
    let health_limit = args
        .get("limit")
        .or_else(|| args.get("health_limit"))
        .and_then(|value| value.as_u64())
        .unwrap_or(20)
        .clamp(1, 50) as usize;
    let health_events = sanitize_health_events(
        crate::ai_runtime::mcp_runtime_registry::list_recent_health_events(
            &state.db,
            &profile_id,
            health_limit,
        )?,
    );
    Ok(serde_json::json!({
        "profile_id": profile_id,
        "health_events": health_events,
    }))
}

fn mcp_profile_id_arg(args: &serde_json::Value, tool_name: &str) -> AppResult<String> {
    args.get("profile_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| AppError::msg(format!("{tool_name} missing profile_id")))
}

fn mcp_runtime_options() -> crate::ai_runtime::mcp_host_runtime::McpHostRuntimeOptions {
    crate::ai_runtime::mcp_host_runtime::McpHostRuntimeOptions {
        request_timeout: std::time::Duration::from_secs(8),
        max_stdout_line_bytes: 64 * 1024,
        max_stderr_bytes: 8 * 1024,
        cwd: None,
    }
}

fn bool_arg(args: &serde_json::Value, key: &str, tool_name: &str) -> AppResult<bool> {
    args.get(key)
        .and_then(|value| value.as_bool())
        .ok_or_else(|| AppError::msg(format!("{tool_name} missing {key}")))
}

fn string_arg(args: &serde_json::Value, key: &str, tool_name: &str) -> AppResult<String> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| AppError::msg(format!("{tool_name} missing {key}")))
}

pub(super) async fn mcp_runtime_capability_call_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let capability = string_arg(args, "capability", "mcp_runtime_capability_call")?;
    let arguments = args
        .get("arguments")
        .cloned()
        .filter(|value| value.is_object())
        .ok_or_else(|| AppError::msg("mcp_runtime_capability_call missing arguments"))?;
    let result = crate::ai_runtime::mcp_host_runtime::call_required_capability(
        &state.db,
        &capability,
        arguments,
        mcp_runtime_options(),
    )
    .await?;
    Ok(serde_json::to_value(result).unwrap_or_default())
}
pub(super) async fn mcp_runtime_profile_upsert_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let input: crate::ai_runtime::mcp_runtime_registry::McpRuntimeProfileInput =
        serde_json::from_value(args.clone()).map_err(|err| {
            AppError::msg(format!("mcp_runtime_profile_upsert invalid input: {err}"))
        })?;
    crate::ai_runtime::mcp_runtime_registry::upsert_runtime_profile(&state.db, &input)?;
    Ok(serde_json::json!({
        "ok": true,
        "profile_id": input.id,
        "enabled": input.enabled
    }))
}

pub(super) async fn mcp_server_catalog_upsert_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let input: crate::ai_runtime::mcp_runtime_registry::McpServerCatalogInput =
        serde_json::from_value(args.clone()).map_err(|err| {
            AppError::msg(format!("mcp_server_catalog_upsert invalid input: {err}"))
        })?;
    crate::ai_runtime::mcp_runtime_registry::upsert_server_catalog(&state.db, &input)?;
    Ok(serde_json::json!({
        "ok": true,
        "server_id": input.id,
        "transport": input.transport,
    }))
}

pub(super) async fn mcp_runtime_profile_toggle_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let profile_id = mcp_profile_id_arg(args, "mcp_runtime_profile_toggle")?;
    let enabled = bool_arg(args, "enabled", "mcp_runtime_profile_toggle")?;
    crate::ai_runtime::mcp_runtime_registry::set_runtime_profile_enabled(
        &state.db,
        &profile_id,
        enabled,
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "profile_id": profile_id,
        "enabled": enabled
    }))
}

pub(super) async fn mcp_runtime_profile_delete_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let profile_id = mcp_profile_id_arg(args, "mcp_runtime_profile_delete")?;
    crate::ai_runtime::mcp_runtime_registry::delete_runtime_profile(&state.db, &profile_id)?;
    Ok(serde_json::json!({
        "ok": true,
        "profile_id": profile_id
    }))
}
pub(super) async fn mcp_runtime_tools_list_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::mcp_runtime_registry::{
        record_health_event, McpHealthEventInput, McpRuntimeStatus,
    };

    let profile_id = mcp_profile_id_arg(args, "mcp_runtime_tools_list")?;
    let discovery = match crate::ai_runtime::mcp_host_runtime::discover_profile_tools(
        &state.db,
        &profile_id,
        mcp_runtime_options(),
    )
    .await
    {
        Ok(discovery) => discovery,
        Err(err) => {
            let message = crate::ai_runtime::trace::redact_classified_leaks(&err.to_string());
            let _ = record_health_event(
                &state.db,
                &McpHealthEventInput {
                    profile_id: profile_id.clone(),
                    status: McpRuntimeStatus::Unavailable,
                    reason_code: "agent_live_tools_list_failed".into(),
                    message: Some(message.clone()),
                    metadata_json: serde_json::json!({"tool_count": 0}).to_string(),
                },
            );
            return Err(AppError::msg(message));
        }
    };
    Ok(serde_json::json!({
        "profile_id": profile_id,
        "protocol_version": discovery.protocol_version,
        "server_name": discovery.server_name,
        "server_version": discovery.server_version,
        "tools": discovery.tools,
        "tool_count": discovery.tools.len()
    }))
}

pub(super) async fn mcp_runtime_health_check_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::mcp_runtime_registry::{
        record_health_event, McpHealthEventInput, McpRuntimeStatus,
    };

    let profile_id = mcp_profile_id_arg(args, "mcp_runtime_health_check")?;
    match crate::ai_runtime::mcp_host_runtime::discover_profile_tools(
        &state.db,
        &profile_id,
        mcp_runtime_options(),
    )
    .await
    {
        Ok(discovery) => {
            record_health_event(
                &state.db,
                &McpHealthEventInput {
                    profile_id: profile_id.clone(),
                    status: McpRuntimeStatus::Ready,
                    reason_code: "agent_live_tools_list_ok".into(),
                    message: Some(format!("{} MCP tools discovered", discovery.tools.len())),
                    metadata_json: serde_json::json!({
                        "tool_count": discovery.tools.len(),
                        "protocol_version": discovery.protocol_version,
                        "server_name": discovery.server_name,
                    })
                    .to_string(),
                },
            )?;
            Ok(serde_json::json!({
                "profile_id": profile_id,
                "status": McpRuntimeStatus::Ready,
                "tool_count": discovery.tools.len(),
                "message": null
            }))
        }
        Err(err) => {
            let message = crate::ai_runtime::trace::redact_classified_leaks(&err.to_string());
            let _ = record_health_event(
                &state.db,
                &McpHealthEventInput {
                    profile_id: profile_id.clone(),
                    status: McpRuntimeStatus::Unavailable,
                    reason_code: "agent_live_tools_list_failed".into(),
                    message: Some(message.clone()),
                    metadata_json: serde_json::json!({"tool_count": 0}).to_string(),
                },
            );
            Ok(serde_json::json!({
                "profile_id": profile_id,
                "status": McpRuntimeStatus::Unavailable,
                "tool_count": 0,
                "message": message
            }))
        }
    }
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

pub(super) async fn skills_workspace_list_tool(
    state: &AppState,
    _ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skills::list_workspace_files;

    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_workspace_list missing name"))?;
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let vault = state.vault_path()?;
    let files = list_workspace_files(&vault, name, path)?;
    Ok(serde_json::json!({ "files": files }))
}

pub(super) async fn skills_workspace_read_tool(
    state: &AppState,
    _ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skills::read_workspace_file;

    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_workspace_read missing name"))?;
    let relative_path = args
        .get("relative_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_workspace_read missing relative_path"))?;
    let vault = state.vault_path()?;
    let content = read_workspace_file(&vault, name, relative_path)?;
    Ok(serde_json::json!({ "content": content }))
}

pub(super) async fn skills_workspace_write_tool(
    state: &AppState,
    _ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skills::{read_workspace_file, write_workspace_file};

    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_workspace_write missing name"))?;
    let relative_path = args
        .get("relative_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_workspace_write missing relative_path"))?;
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("skills_workspace_write missing content"))?;
    let mode = args
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("overwrite");
    let vault = state.vault_path()?;
    if mode == "create" && read_workspace_file(&vault, name, relative_path).is_ok() {
        return Err(AppError::msg("skill workspace file already exists"));
    }
    if mode != "create" && mode != "overwrite" {
        return Err(AppError::msg("unknown skills_workspace_write mode"));
    }
    let path = write_workspace_file(&vault, name, relative_path, content)?;
    Ok(serde_json::json!({ "ok": true, "path": path }))
}
