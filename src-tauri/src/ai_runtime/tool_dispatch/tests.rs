use super::*;
use crate::ai_runtime::AiScene;
use crate::app::AppState;
use std::sync::Arc;

fn test_state() -> (Arc<AppState>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    let notes = vault.join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    std::fs::write(notes.join("test.md"), "# Test\nHello world").unwrap();
    let state = AppState::new(dir.path().to_path_buf()).unwrap();
    state.set_vault(vault).unwrap();
    (state, dir)
}

#[tokio::test]
async fn read_note_rejects_parent_dir_traversal() {
    let (state, _dir) = test_state();
    let args = serde_json::json!({ "path": "../../etc/passwd" });
    let result = note_impl::read_note(&state, &args).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("traversal") || !err.is_empty(),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn read_note_rejects_iris_metadata() {
    let (state, _dir) = test_state();
    let args = serde_json::json!({ "path": ".iris/versions/1/test.md" });
    let result = note_impl::read_note(&state, &args).await;
    assert!(result.is_err());
    assert!(!result.unwrap_err().to_string().is_empty());
}

#[tokio::test]
async fn read_note_accepts_valid_path() {
    let (state, _dir) = test_state();
    let args = serde_json::json!({ "path": "notes/test.md" });
    let result = note_impl::read_note(&state, &args).await;
    assert!(result.is_ok());
    let val = result.unwrap();
    assert_eq!(val["path"], "notes/test.md");
    assert_eq!(val["content"], "# Test\nHello world");
}

#[tokio::test]
async fn get_outline_rejects_iris_metadata() {
    let (state, _dir) = test_state();
    let args = serde_json::json!({ "path": ".iris/skills/my-skill/SKILL.md" });
    let result = note_impl::get_outline(&state, &args).await;
    assert!(result.is_err());
    assert!(!result.unwrap_err().to_string().is_empty());
}

#[tokio::test]
async fn get_backlinks_rejects_iris_metadata() {
    let (state, _dir) = test_state();
    let args = serde_json::json!({ "path": ".iris/versions/x.md" });
    let result = note_impl::get_backlinks(&state, &args).await;
    assert!(result.is_err());
    assert!(!result.unwrap_err().to_string().is_empty());
}

#[tokio::test]
async fn get_backlinks_rejects_parent_dir() {
    let (state, _dir) = test_state();
    let args = serde_json::json!({ "path": "../secret.md" });
    let result = note_impl::get_backlinks(&state, &args).await;
    assert!(result.is_err());
    assert!(!result.unwrap_err().to_string().is_empty());
}

#[tokio::test]
async fn get_block_links_rejects_parent_dir() {
    let (state, _dir) = test_state();
    let args = serde_json::json!({ "note_path": "../note.md" });
    let result = note_impl::get_block_links(&state, &args).await;
    assert!(result.is_err());
    assert!(!result.unwrap_err().to_string().is_empty());
}

#[tokio::test]
async fn get_block_links_rejects_iris_metadata() {
    let (state, _dir) = test_state();
    let args = serde_json::json!({ "note_path": ".iris/versions/x.md" });
    let result = note_impl::get_block_links(&state, &args).await;
    assert!(result.is_err());
    assert!(!result.unwrap_err().to_string().is_empty());
}

#[tokio::test]
async fn read_note_rejects_absolute_path() {
    let (state, _dir) = test_state();
    let args = serde_json::json!({ "path": "/etc/passwd" });
    let result = note_impl::read_note(&state, &args).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn mcp_capability_call_reports_missing_provider_through_dispatch() {
    let (state, _dir) = test_state();
    let ctx = ToolDispatchContext {
        scene: AiScene::DraftingAssist,
        note_path: None,
        file_id: None,
        web_search_enabled: false,
        cold_start_packets: &[],
        app_handle: None,
        attachment_count: 0,
        skill_activation_plan: None,
        embedding_state: None,
    };

    let result = dispatch_tool(
        &state,
        &ctx,
        "mcp_runtime_capability_call",
        &serde_json::json!({"capability": "web.search", "arguments": {"q": "iris"}}),
    )
    .await;

    assert!(!result.success);
    assert!(result
        .error
        .as_deref()
        .unwrap_or("")
        .contains("missing_mcp_profile"));
}

#[tokio::test]
async fn mcp_profile_management_tools_update_registry_after_confirmation() {
    use crate::ai_runtime::mcp_runtime_registry::{
        list_recent_health_events, list_runtime_profiles, upsert_server_catalog,
        McpServerCatalogInput,
    };

    let (state, _dir) = test_state();
    upsert_server_catalog(
        &state.db,
        &McpServerCatalogInput {
            id: "fake-server".into(),
            display_name: "Fake Server".into(),
            transport: "stdio".into(),
            command: Some("fake-mcp".into()),
            args_json: "[]".into(),
            url: None,
            env_schema_json: "{}".into(),
            capability_tags_json: "[\"web.search\"]".into(),
            source: "test".into(),
        },
    )
    .unwrap();

    let ctx = ToolDispatchContext {
        scene: AiScene::DraftingAssist,
        note_path: None,
        file_id: None,
        web_search_enabled: false,
        cold_start_packets: &[],
        app_handle: None,
        attachment_count: 0,
        skill_activation_plan: None,
        embedding_state: None,
    };

    let upserted = dispatch_tool(
        &state,
        &ctx,
        "mcp_runtime_profile_upsert",
        &serde_json::json!({
            "id": "fake-profile",
            "server_id": "fake-server",
            "vault_scope_hash": null,
            "display_name": "Fake Profile",
            "enabled": true,
            "transport_config_json": "{}",
            "env_bindings_json": "{}",
            "status": "unknown",
            "last_error": null
        }),
    )
    .await;
    assert!(upserted.success, "upsert failed: {:?}", upserted.error);
    assert_eq!(upserted.output["ok"], true);

    let profiles = list_runtime_profiles(&state.db).unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].id, "fake-profile");
    assert!(profiles[0].enabled);

    let toggled = dispatch_tool(
        &state,
        &ctx,
        "mcp_runtime_profile_toggle",
        &serde_json::json!({"profile_id": "fake-profile", "enabled": false}),
    )
    .await;
    assert!(toggled.success, "toggle failed: {:?}", toggled.error);
    assert!(!list_runtime_profiles(&state.db).unwrap()[0].enabled);

    let live_tools = dispatch_tool(
        &state,
        &ctx,
        "mcp_runtime_tools_list",
        &serde_json::json!({"profile_id": "fake-profile"}),
    )
    .await;
    assert!(!live_tools.success);
    let events = list_recent_health_events(&state.db, "fake-profile", 5).unwrap();
    assert_eq!(events[0].reason_code, "agent_live_tools_list_failed");
    assert_eq!(events[0].status.as_str(), "unavailable");

    let deleted = dispatch_tool(
        &state,
        &ctx,
        "mcp_runtime_profile_delete",
        &serde_json::json!({"profile_id": "fake-profile"}),
    )
    .await;
    assert!(deleted.success, "delete failed: {:?}", deleted.error);
    assert!(list_runtime_profiles(&state.db).unwrap().is_empty());
}

#[tokio::test]
async fn mcp_runtime_diagnostics_tools_return_registry_metadata_only() {
    use crate::ai_runtime::mcp_runtime_registry::{
        record_health_event, record_tool_inventory, upsert_runtime_profile, upsert_server_catalog,
        McpHealthEventInput, McpRuntimeProfileInput, McpRuntimeStatus, McpServerCatalogInput,
        McpToolInventoryInput,
    };

    let (state, _dir) = test_state();
    upsert_server_catalog(
        &state.db,
        &McpServerCatalogInput {
            id: "anysearch".into(),
            display_name: "AnySearch".into(),
            transport: "stdio".into(),
            command: Some("anysearch".into()),
            args_json: "[]".into(),
            url: None,
            env_schema_json: "{}".into(),
            capability_tags_json: "[\"web_search\"]".into(),
            source: "test".into(),
        },
    )
    .unwrap();
    upsert_runtime_profile(
        &state.db,
        &McpRuntimeProfileInput {
            id: "anysearch-local".into(),
            server_id: "anysearch".into(),
            vault_scope_hash: None,
            display_name: "AnySearch Local".into(),
            enabled: true,
            transport_config_json: "{}".into(),
            env_bindings_json: "{}".into(),
            status: McpRuntimeStatus::Ready,
            last_error: None,
        },
    )
    .unwrap();
    record_tool_inventory(
        &state.db,
        &McpToolInventoryInput {
            profile_id: "anysearch-local".into(),
            tool_name: "web_search".into(),
            schema_hash: "sha256:test".into(),
            capability_mapping_json: "{\"capability\":\"web_search\"}".into(),
            description: Some("Search the web".into()),
        },
    )
    .unwrap();
    record_health_event(
        &state.db,
        &McpHealthEventInput {
            profile_id: "anysearch-local".into(),
            status: McpRuntimeStatus::Ready,
            reason_code: "probe_ok".into(),
            message: Some("ready".into()),
            metadata_json: "{}".into(),
        },
    )
    .unwrap();

    let ctx = ToolDispatchContext {
        scene: AiScene::DraftingAssist,
        note_path: None,
        file_id: None,
        web_search_enabled: false,
        cold_start_packets: &[],
        app_handle: None,
        attachment_count: 0,
        skill_activation_plan: None,
        embedding_state: None,
    };

    let profiles = dispatch_tool(
        &state,
        &ctx,
        "mcp_runtime_profiles_list",
        &serde_json::json!({}),
    )
    .await;
    assert!(
        profiles.success,
        "profiles tool failed: {:?}",
        profiles.error
    );
    assert_eq!(profiles.output["profiles"][0]["id"], "anysearch-local");
    assert_eq!(profiles.output["profiles"][0]["status"], "ready");

    let diagnostics = dispatch_tool(
        &state,
        &ctx,
        "mcp_runtime_diagnostics",
        &serde_json::json!({ "profile_id": "anysearch-local", "health_limit": 5 }),
    )
    .await;
    assert!(
        diagnostics.success,
        "diagnostics tool failed: {:?}",
        diagnostics.error
    );
    assert_eq!(diagnostics.output["profile_id"], "anysearch-local");
    assert_eq!(diagnostics.output["tools"][0]["tool_name"], "web_search");
    assert_eq!(
        diagnostics.output["health_events"][0]["reason_code"],
        "probe_ok"
    );
}

#[test]
fn write_tool_approval_applies_patch_with_cas() {
    let (state, _dir) = test_state();
    let base = "# Test\nHello world";
    let base_hash = crate::ai_runtime::writing_workflow::compute_content_hash(base);
    let ctx = ToolDispatchContext {
        scene: AiScene::DraftingAssist,
        note_path: Some("notes/test.md"),
        file_id: None,
        web_search_enabled: false,
        cold_start_packets: &[],
        app_handle: None,
        attachment_count: 0,
        skill_activation_plan: None,
        embedding_state: None,
    };
    let result = markdown_impl::markdown_write_patch_apply(
        &state,
        &ctx,
        "replace_selection",
        &serde_json::json!({
            "replacement": "Hi",
            "base_content_hash": base_hash,
            "range": {"start": 7, "end": 12},
            "original_text": "Hello",
            "risk_level": "low"
        }),
    )
    .unwrap();

    assert_eq!(result["type"], "patch_apply");
    assert_eq!(result["result"]["success"], true);
    let content =
        std::fs::read_to_string(state.vault_path().unwrap().join("notes/test.md")).unwrap();
    assert_eq!(content, "# Test\nHi world");
}

#[test]
fn write_tool_approval_reports_hash_conflict_without_writing() {
    let (state, _dir) = test_state();
    let ctx = ToolDispatchContext {
        scene: AiScene::DraftingAssist,
        note_path: Some("notes/test.md"),
        file_id: None,
        web_search_enabled: false,
        cold_start_packets: &[],
        app_handle: None,
        attachment_count: 0,
        skill_activation_plan: None,
        embedding_state: None,
    };
    let result = markdown_impl::markdown_write_patch_apply(
        &state,
        &ctx,
        "replace_selection",
        &serde_json::json!({
            "replacement": "Hi",
            "base_content_hash": "stale",
            "range": {"start": 7, "end": 12},
            "original_text": "Hello",
        }),
    )
    .unwrap();

    assert_eq!(result["type"], "patch_apply");
    assert_eq!(result["result"]["success"], false);
    let error = result["result"]["error"].as_str().unwrap_or("");
    assert!(
        error.contains("hash") || !error.is_empty(),
        "unexpected error: {error}"
    );
    let content =
        std::fs::read_to_string(state.vault_path().unwrap().join("notes/test.md")).unwrap();
    assert_eq!(content, "# Test\nHello world");
}
