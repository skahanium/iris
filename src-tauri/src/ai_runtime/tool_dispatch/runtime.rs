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
) -> AppResult<serde_json::Value> {
    let tools = runtime_context::all_catalog_tools_as_specs();
    serde_json::to_value(runtime_context::capability_snapshot(
        &state.db,
        ctx.web_search_enabled,
        &tools,
    ))
    .map_err(Into::into)
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
    async fn runtime_context_tools_return_structured_readonly_state() {
        let (state, _dir) = test_state();
        let retrieval_scope = crate::ai_runtime::retrieval_scope::RetrievalScope::default();
        let ctx = ToolDispatchContext {
            note_path: Some("notes/test.md"),
            file_id: Some(7),
            web_search_enabled: true,
            max_web_fetches: 3,
            cold_start_packets: &[],
            retrieval_scope: &retrieval_scope,
            runtime_documents: &[],
            app_handle: None,
            attachment_count: 2,
            skill_activation_plan: None,
            embedding_state: None,
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
        assert_eq!(capabilities.output["vision"]["configured"], false);
        assert!(capabilities.output["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "system_time_now"));
    }
}
