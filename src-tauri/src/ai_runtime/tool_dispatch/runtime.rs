use crate::ai_runtime::runtime_context;
use crate::ai_runtime::tool_dispatch::ToolDispatchContext;
use crate::app::AppState;
use crate::error::AppResult;

pub(crate) fn system_time_now_tool() -> AppResult<serde_json::Value> {
    serde_json::to_value(runtime_context::current_time_context()).map_err(Into::into)
}

pub(crate) fn app_context_read_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    serde_json::to_value(runtime_context::app_context_snapshot(
        state,
        ctx.note_path,
        ctx.file_id,
        ctx.attachment_count,
    ))
    .map_err(Into::into)
}

pub(crate) fn capabilities_read_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let all_tools = runtime_context::all_catalog_tools_as_specs();
    let tools = all_tools
        .into_iter()
        .filter(|tool| ctx.available_tool_names.contains(&tool.name))
        .collect::<Vec<_>>();
    let mut snapshot = serde_json::to_value(runtime_context::capability_snapshot(
        &state.db,
        ctx.web_search_enabled,
        &tools,
    ))?;
    // `request_tools` is a question, not a request for access. The Host answers
    // which of the named tools are already in this Run's surface and refuses to
    // widen it: silently ignoring the parameter would be the "declared but never
    // consumed" defect `G01` registers, and granting it would be a permission
    // escalation. Either way no tool, capability or budget changes here.
    if let Some(answer) = requested_tools_answer(args, ctx) {
        snapshot["requestedTools"] = answer;
    }
    Ok(snapshot)
}

/// Build the `requestedTools` answer for `capabilities_read`.
///
/// Returns `None` when the parameter is absent, so a caller that never uses it
/// sees exactly the snapshot it saw before this parameter existed.
fn requested_tools_answer(
    args: &serde_json::Value,
    ctx: &ToolDispatchContext<'_>,
) -> Option<serde_json::Value> {
    let requested = args.get("request_tools")?;
    let Some(names) = requested.as_array() else {
        return Some(serde_json::json!({
            "requested": [],
            "available": [],
            "unavailable": [],
            "rejectedReason": "capability_request_must_be_a_string_array",
            "note": "request_tools 必须是工具名数组；本次没有开放任何工具。",
        }));
    };
    let mut available: Vec<String> = Vec::new();
    let mut unavailable: Vec<serde_json::Value> = Vec::new();
    for entry in names {
        let Some(requested) = entry.as_str() else {
            unavailable.push(serde_json::json!({
                "name": entry,
                "reason": "capability_request_entry_must_be_a_string",
            }));
            continue;
        };
        if ctx
            .available_tool_names
            .iter()
            .any(|available| available == requested)
        {
            available.push(requested.to_string());
        } else if crate::ai_runtime::tool_catalog::catalog_find(requested).is_some() {
            unavailable.push(serde_json::json!({
                "name": requested,
                "reason": "capability_request_not_in_current_surface",
            }));
        } else {
            unavailable.push(serde_json::json!({
                "name": requested,
                "reason": "capability_request_unknown_tool",
            }));
        }
    }
    Some(serde_json::json!({
        "requested": names,
        "available": available,
        "unavailable": unavailable,
        "note": "只回报当前 Run 的工具面；本工具不开放工具或权限。",
    }))
}

#[cfg(test)]
mod tests {
    use crate::ai_runtime::tool_dispatch::{dispatch_tool, ToolDispatchContext};
    use crate::app::AppState;
    use std::sync::Arc;

    fn test_state() -> (Arc<AppState>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(vault.join("notes")).unwrap();
        std::fs::write(vault.join("notes/test.md"), "# Test\nHello world").unwrap();
        let state = AppState::new(dir.path().to_path_buf()).unwrap();
        state.set_vault(vault).unwrap();
        (state, dir)
    }

    #[tokio::test]
    async fn capabilities_read_empty_surface_returns_no_tools() {
        let (state, _dir) = test_state();
        let mut routing = crate::llm::config::deepseek_defaults();
        routing.providers.insert(
            "deepseek".to_string(),
            crate::llm::config::ProviderOverride {
                enabled_models: Some(vec!["deepseek-v4-flash".to_string()]),
                ..Default::default()
            },
        );
        crate::llm::config::save(&state.db, &routing).expect("save enabled model pool");
        let retrieval_scope = crate::ai_runtime::retrieval_scope::RetrievalScope::default();
        let ctx = ToolDispatchContext {
            db: None,

            selected_web_provider_id: None,

            note_path: Some("notes/test.md"),
            file_id: Some(7),
            run_id: None,
            write_target_path: None,
            confirmed_write_targets: None,
            confirmed_vault_id: None,
            document_policy: None,
            web_search_enabled: true,
            available_tool_names: &[],
            max_web_fetches: 3,
            cold_start_packets: &[],
            retrieval_scope: &retrieval_scope,
            runtime_documents: &[],
            app_handle: None,
            attachment_count: 2,
            skill_activation_plan: None,
        };

        let time = dispatch_tool(&state, &ctx, "system_time_now", &serde_json::json!({})).await;
        assert!(time.success, "{:?}", time.error);
        assert_eq!(time.output["kind"], "system_time");
        assert!(time.output["local_date"]
            .as_str()
            .unwrap_or("")
            .contains('-'));
        assert!(time.output["weekday_zh"]
            .as_str()
            .unwrap_or("")
            .starts_with("星期"));

        let app = dispatch_tool(&state, &ctx, "app_context_read", &serde_json::json!({})).await;
        assert!(app.success, "{:?}", app.error);
        assert_eq!(app.output["note_path"], "notes/test.md");
        assert_eq!(app.output["attachment_count"], 2);
        assert!(app.output["vault_path"]
            .as_str()
            .unwrap_or("")
            .contains("vault"));

        let capabilities =
            dispatch_tool(&state, &ctx, "capabilities_read", &serde_json::json!({})).await;
        assert!(capabilities.success, "{:?}", capabilities.error);
        assert_eq!(capabilities.output["web_search_enabled"], true);
        assert!(capabilities.output.get("vision").is_none());
        assert!(capabilities.output["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|model| {
                model["provider_id"] == "deepseek"
                    && model["model"] == "deepseek-v4-flash"
                    && model["configured"].is_boolean()
            }));
        assert!(capabilities.output["tools"]
            .as_array()
            .expect("capabilities tools")
            .is_empty());
    }

    #[tokio::test]
    async fn capabilities_read_reports_current_surface_only() {
        let (state, _dir) = test_state();
        let retrieval_scope = crate::ai_runtime::retrieval_scope::RetrievalScope::default();
        let available_tool_names = vec!["system_time_now".to_string()];
        let ctx = ToolDispatchContext {
            db: None,

            selected_web_provider_id: None,

            note_path: None,
            file_id: None,
            run_id: None,
            write_target_path: None,
            confirmed_write_targets: None,
            confirmed_vault_id: None,
            document_policy: None,
            web_search_enabled: false,
            available_tool_names: &available_tool_names,
            max_web_fetches: 0,
            cold_start_packets: &[],
            retrieval_scope: &retrieval_scope,
            runtime_documents: &[],
            app_handle: None,
            attachment_count: 0,
            skill_activation_plan: None,
        };

        let capabilities =
            dispatch_tool(&state, &ctx, "capabilities_read", &serde_json::json!({})).await;
        assert!(capabilities.success, "{:?}", capabilities.error);
        assert_eq!(capabilities.output["web_search_enabled"], false);

        let tools = capabilities.output["tools"]
            .as_array()
            .expect("capabilities_read tools array");
        assert!(
            !tools.iter().any(|tool| tool["name"] == "web_search"),
            "capabilities_read must not advertise web_search when web is disabled"
        );
        assert!(
            !tools.iter().any(|tool| tool["name"] == "search_hybrid"),
            "capabilities_read must not advertise vault search without a current Run surface"
        );
    }

    /// Build the same context the surface tests use, with a chosen surface.
    fn surface_ctx<'a>(
        available_tool_names: &'a [String],
        retrieval_scope: &'a crate::ai_runtime::retrieval_scope::RetrievalScope,
    ) -> ToolDispatchContext<'a> {
        ToolDispatchContext {
            db: None,
            selected_web_provider_id: None,
            note_path: None,
            file_id: None,
            run_id: None,
            write_target_path: None,
            confirmed_write_targets: None,
            confirmed_vault_id: None,
            document_policy: None,
            web_search_enabled: false,
            available_tool_names,
            max_web_fetches: 0,
            cold_start_packets: &[],
            retrieval_scope,
            runtime_documents: &[],
            app_handle: None,
            attachment_count: 0,
            skill_activation_plan: None,
        }
    }

    /// `request_tools` is a question. It must be answered, and answering it must
    /// not change the surface, the capability set or the answer to the same
    /// question asked without it (`G01`: declared parameters must be consumed or
    /// explicitly rejected — never silently ignored, never a back door).
    #[tokio::test]
    async fn capabilities_read_answers_request_tools_without_widening_the_surface() {
        let (state, _dir) = test_state();
        let retrieval_scope = crate::ai_runtime::retrieval_scope::RetrievalScope::default();
        let available_tool_names = vec!["system_time_now".to_string()];

        let baseline = dispatch_tool(
            &state,
            &surface_ctx(&available_tool_names, &retrieval_scope),
            "capabilities_read",
            &serde_json::json!({}),
        )
        .await;
        assert!(baseline.success, "{:?}", baseline.error);
        assert!(
            baseline.output.get("requestedTools").is_none(),
            "omitting request_tools must return the snapshot unchanged"
        );

        let asked = dispatch_tool(
            &state,
            &surface_ctx(&available_tool_names, &retrieval_scope),
            "capabilities_read",
            &serde_json::json!({
                "request_tools": ["system_time_now", "web_search", "not_a_tool"]
            }),
        )
        .await;
        assert!(asked.success, "{:?}", asked.error);

        let answer = &asked.output["requestedTools"];
        assert_eq!(
            answer["available"],
            serde_json::json!(["system_time_now"]),
            "a tool already in this Run's surface is reported as available"
        );
        let unavailable = answer["unavailable"].as_array().expect("unavailable list");
        assert!(
            unavailable.iter().any(|entry| {
                entry["name"] == "web_search"
                    && entry["reason"] == "capability_request_not_in_current_surface"
            }),
            "a declared tool outside this Run's surface must be refused with a \
             stable reason, not silently dropped: {unavailable:?}"
        );
        assert!(
            unavailable.iter().any(|entry| {
                entry["name"] == "not_a_tool"
                    && entry["reason"] == "capability_request_unknown_tool"
            }),
            "an unknown tool must be distinguished from an out-of-surface one: {unavailable:?}"
        );

        // The surface itself is unchanged: same tool list as the baseline ask.
        assert_eq!(
            asked.output["tools"], baseline.output["tools"],
            "request_tools must not widen the advertised tool surface"
        );
    }

    #[tokio::test]
    async fn capabilities_read_rejects_a_malformed_request_tools_value() {
        let (state, _dir) = test_state();
        let retrieval_scope = crate::ai_runtime::retrieval_scope::RetrievalScope::default();
        let available_tool_names = vec!["system_time_now".to_string()];

        let refused = dispatch_tool(
            &state,
            &surface_ctx(&available_tool_names, &retrieval_scope),
            "capabilities_read",
            &serde_json::json!({ "request_tools": "system_time_now" }),
        )
        .await;
        assert!(refused.success, "{:?}", refused.error);
        assert_eq!(
            refused.output["requestedTools"]["rejectedReason"],
            "capability_request_must_be_a_string_array",
            "a non-array request_tools must be rejected explicitly: {}",
            refused.output["requestedTools"]
        );
        assert!(
            refused.output["requestedTools"]["available"]
                .as_array()
                .is_some_and(|list| list.is_empty()),
            "a rejected request must not report anything as granted"
        );
    }
}
