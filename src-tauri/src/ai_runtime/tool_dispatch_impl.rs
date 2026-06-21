use crate::ai_runtime::ToolCallResult;
use crate::app::AppState;
use crate::error::{AppError, AppResult};
use std::time::Instant;
#[path = "tool_dispatch/boundary.rs"]
mod boundary_impl;
#[path = "tool_dispatch/context.rs"]
mod context_impl;
#[path = "tool_dispatch/markdown.rs"]
mod markdown_impl;
#[path = "tool_dispatch/memory.rs"]
mod memory_impl;
#[path = "tool_dispatch/note.rs"]
mod note_impl;
#[path = "tool_dispatch/runtime.rs"]
mod runtime_impl;
#[path = "tool_dispatch/schedule.rs"]
mod schedule_impl;
#[path = "tool_dispatch/search.rs"]
mod search_impl;
#[path = "tool_dispatch/skills.rs"]
mod skills_impl;
#[path = "tool_dispatch/vault.rs"]
mod vault_impl;
#[path = "tool_dispatch/web.rs"]
mod web_impl;

pub use context_impl::ToolDispatchContext;

#[rustfmt::skip]
pub const DISPATCHABLE_TOOL_NAMES: &[&str] = &[
    "search_hybrid", "search_semantic", "search_keyword", "get_regulation", "get_context_packets",
    "system_time_now", "app_context_read", "capabilities_read",
    "web_search", "fetch_web_page", "read_note", "list_vault", "get_outline", "get_backlinks",
    "get_block_links", "memory_read", "memory_write", "scheduled_task_create", "scheduled_task_list",
    "scheduled_task_delete", "web_fetch_batch", "readability_fetch", "rendered_fetch",
    "vault_create_note", "vault_rename_move", "vault_delete_to_trash", "vault_asset_write",
    "vault_version_list", "insert_text_at_cursor", "replace_selection", "skills_list", "skills_install",
    "skills_prepare_workspace", "skills_uninstall", "skills_update", "skills_toggle", "skills_read_resource",
    "skills_workspace_list", "skills_workspace_read", "skills_workspace_write", "git_read_status",
    "git_read_diff", "git_read_log", "secret_exists", "fs_import_to_vault", "fs_export",
    "fs_read_authorized_folder", "fs_write_authorized_export", "doc_normalize_markdown",
    "doc_extract_citations", "web_to_markdown", "web_download_to_assets", "web_citation_extract",
    "skill_request_capabilities", "process_run_readonly", "git_write_commit",
];
pub const HARNESS_ONLY_TOOL_NAMES: &[&str] = &["spawn_subagent", "conclude_reasoning"];
pub fn is_exposable_tool(name: &str) -> bool {
    crate::ai_runtime::tool_catalog::catalog_find(name).is_some_and(|entry| {
        entry.implementation != crate::ai_runtime::tool_catalog::ToolImplementationStatus::Planned
    })
}

fn is_retryable_tool_error(tool_name: &str, result: &ToolCallResult) -> bool {
    if result.success {
        return false;
    }
    let err = result.error.as_deref().unwrap_or("");
    (tool_name == "web_search" || tool_name == "fetch_web_page")
        && (err.contains("timeout") || err.contains("network") || err.contains("连接"))
}
pub async fn dispatch_tool_with_retry(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    tool_name: &str,
    args: &serde_json::Value,
) -> ToolCallResult {
    let mut result = dispatch_tool(state, ctx, tool_name, args).await;
    if is_retryable_tool_error(tool_name, &result) {
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        result = dispatch_tool(state, ctx, tool_name, args).await;
    }
    if !result.success && tool_name == "search_hybrid" {
        return dispatch_tool(state, ctx, "search_keyword", args).await;
    }
    result
}
pub async fn dispatch_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    tool_name: &str,
    args: &serde_json::Value,
) -> ToolCallResult {
    let start = Instant::now();
    let result = dispatch_tool_inner(state, ctx, tool_name, args).await;
    let duration_ms = start.elapsed().as_millis() as u64;
    match result {
        Ok(output) => ToolCallResult {
            tool_name: tool_name.to_string(),
            success: true,
            output,
            duration_ms,
            tokens_used: None,
            error: None,
        },
        Err(e) => ToolCallResult {
            tool_name: tool_name.to_string(),
            success: false,
            output: serde_json::Value::Null,
            duration_ms,
            tokens_used: None,
            error: Some(e.to_string()),
        },
    }
}

async fn dispatch_tool_inner(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    tool_name: &str,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    match tool_name {
        "search_hybrid" | "search_semantic" | "search_keyword" => {
            search_impl::hybrid_search(state, tool_name, args, ctx).await
        }
        "get_regulation" => search_impl::regulation_lookup(state, args).await,
        "get_context_packets" => Ok(serde_json::json!({
            "packets": ctx.cold_start_packets,
            "count": ctx.cold_start_packets.len(),
        })),
        "system_time_now" => runtime_impl::system_time_now_tool(),
        "app_context_read" => runtime_impl::app_context_read_tool(state, ctx),
        "capabilities_read" => runtime_impl::capabilities_read_tool(state, ctx),
        "web_search" => web_impl::web_search_tool(state, args, ctx).await,
        "fetch_web_page" => web_impl::fetch_web_page_tool(state, args, ctx).await,
        "read_note" => note_impl::read_note(state, args).await,
        "list_vault" => note_impl::list_vault(state, args).await,
        "get_outline" => note_impl::get_outline(state, args).await,
        "get_backlinks" => note_impl::get_backlinks(state, args).await,
        "get_block_links" => note_impl::get_block_links(state, args).await,
        "memory_read" => memory_impl::memory_read_tool(state, args, ctx).await,
        "memory_write" => memory_impl::memory_write_tool(state, args, ctx).await,
        "scheduled_task_create" => schedule_impl::scheduled_task_create_tool(state, args).await,
        "scheduled_task_list" => schedule_impl::scheduled_task_list_tool(state, args).await,
        "scheduled_task_delete" => schedule_impl::scheduled_task_delete_tool(state, args).await,
        "web_fetch_batch" => web_impl::web_fetch_batch_tool(state, args, ctx).await,
        "readability_fetch" => web_impl::readability_fetch_tool(state, args, ctx, false).await,
        "rendered_fetch" => web_impl::readability_fetch_tool(state, args, ctx, true).await,
        "vault_create_note" => vault_impl::vault_create_note_tool(state, ctx, args),
        "vault_rename_move" => vault_impl::vault_rename_move_tool(state, ctx, args),
        "vault_delete_to_trash" => vault_impl::vault_delete_to_trash_tool(state, args),
        "vault_asset_write" => vault_impl::vault_asset_write_tool(state, args),
        "vault_version_list" => vault_impl::vault_version_list_tool(state, args),
        "insert_text_at_cursor" | "replace_selection" => {
            markdown_impl::markdown_write_patch_apply(state, ctx, tool_name, args)
        }
        name if skills_impl::is_skill_tool(name) => {
            skills_impl::dispatch_skill_tool(state, ctx, tool_name, args).await
        }
        "git_read_status" => boundary_impl::git_read_status_tool(state, args),
        "git_read_diff" => boundary_impl::git_read_diff_tool(state, args),
        "git_read_log" => boundary_impl::git_read_log_tool(state, args),
        "secret_exists" => boundary_impl::secret_exists_tool(state, args),
        "fs_import_to_vault" => boundary_impl::fs_import_to_vault_tool(state, ctx, args),
        "fs_export" => boundary_impl::fs_export_tool(args),
        "fs_read_authorized_folder" => boundary_impl::fs_read_authorized_folder_tool(args),
        "fs_write_authorized_export" => boundary_impl::fs_write_authorized_export_tool(args),
        "doc_normalize_markdown" => boundary_impl::doc_normalize_markdown_tool(args),
        "doc_extract_citations" => boundary_impl::doc_extract_citations_tool(args),
        "web_to_markdown" => boundary_impl::web_to_markdown_tool(state, args, ctx).await,
        "web_download_to_assets" => {
            boundary_impl::web_download_to_assets_tool(state, args, ctx).await
        }
        "web_citation_extract" => boundary_impl::web_citation_extract_tool(state, args, ctx).await,
        "skill_request_capabilities" => boundary_impl::skill_request_capabilities_tool(args),
        "process_run_readonly" => boundary_impl::process_run_readonly_tool(state, args),
        "git_write_commit" => boundary_impl::git_write_commit_tool(state, args),
        _ => Err(AppError::msg(format!("unknown tool: {tool_name}"))),
    }
}

#[cfg(test)]
mod tests {
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
            err.contains("traversal") || err.contains("元数据"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn read_note_rejects_iris_metadata() {
        let (state, _dir) = test_state();
        let args = serde_json::json!({ "path": ".iris/versions/1/test.md" });
        let result = note_impl::read_note(&state, &args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("元数据"));
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
        assert!(result.unwrap_err().to_string().contains("元数据"));
    }

    #[tokio::test]
    async fn get_backlinks_rejects_iris_metadata() {
        let (state, _dir) = test_state();
        let args = serde_json::json!({ "path": ".iris/versions/x.md" });
        let result = note_impl::get_backlinks(&state, &args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("元数据"));
    }

    #[tokio::test]
    async fn get_backlinks_rejects_parent_dir() {
        let (state, _dir) = test_state();
        let args = serde_json::json!({ "path": "../secret.md" });
        let result = note_impl::get_backlinks(&state, &args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("traversal"));
    }

    #[tokio::test]
    async fn get_block_links_rejects_parent_dir() {
        let (state, _dir) = test_state();
        let args = serde_json::json!({ "note_path": "../note.md" });
        let result = note_impl::get_block_links(&state, &args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("traversal"));
    }

    #[tokio::test]
    async fn get_block_links_rejects_iris_metadata() {
        let (state, _dir) = test_state();
        let args = serde_json::json!({ "note_path": ".iris/versions/x.md" });
        let result = note_impl::get_block_links(&state, &args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("元数据"));
    }

    #[tokio::test]
    async fn read_note_rejects_absolute_path() {
        let (state, _dir) = test_state();
        let args = serde_json::json!({ "path": "/etc/passwd" });
        let result = note_impl::read_note(&state, &args).await;
        assert!(result.is_err());
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
            error.contains("hash") || error.contains("哈希"),
            "unexpected error: {error}"
        );
        let content =
            std::fs::read_to_string(state.vault_path().unwrap().join("notes/test.md")).unwrap();
        assert_eq!(content, "# Test\nHello world");
    }
}
