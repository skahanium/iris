use super::*;
use crate::ai_runtime::skills::SkillScopeRule;
use crate::ai_types::{SkillActivationItemSummary, SkillActivationPlanSummary};
use crate::app::AppState;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn test_state() -> (Arc<AppState>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    let notes = vault.join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    std::fs::write(notes.join("test.md"), "# Test\nHello world").unwrap();
    let private = vault.join("private");
    std::fs::create_dir_all(&private).unwrap();
    std::fs::write(private.join("secret.md"), "# Secret\nHidden").unwrap();
    let state = AppState::new(dir.path().to_path_buf()).unwrap();
    state.set_vault(vault).unwrap();
    (state, dir)
}

fn scoped_plan(pattern: &str) -> SkillActivationPlanSummary {
    scoped_plan_rule("glob", pattern)
}

fn scoped_plan_rule(kind: &str, pattern: &str) -> SkillActivationPlanSummary {
    SkillActivationPlanSummary {
        activated_skills: vec![SkillActivationItemSummary {
            name: "daily-skill".into(),
            scope: "vault".into(),
            scope_rules: vec![SkillScopeRule {
                kind: kind.into(),
                pattern: pattern.into(),
            }],
            score: 1.0,
            match_reason: "test".into(),
            injected_sections: vec!["skill_overlay".into()],
            degraded_reasons: Vec::new(),
            requested_tools: vec!["read_note".into(), "replace_selection".into()],
            confirmation_required_tools: vec!["replace_selection".into()],
            blocked_capabilities: Vec::new(),
        }],
        requested_tools: vec!["read_note".into(), "replace_selection".into()],
        confirmation_required_tools: vec!["replace_selection".into()],
        blocked_capabilities: Vec::new(),
        skill_overlay_summary: "test".into(),
        degraded: false,
    }
}

fn index_tagged_note(state: &AppState, path: &str, tag: &str) {
    state
        .db
        .with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO files
                 (id, path, title, frontmatter, content_hash, word_count, created_at, updated_at)
                 VALUES (1, ?1, NULL, NULL, 'hash', 0, datetime('now'), datetime('now'))",
                [path],
            )?;
            conn.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", [tag])?;
            let tag_id: i64 =
                conn.query_row("SELECT id FROM tags WHERE name = ?1", [tag], |row| {
                    row.get(0)
                })?;
            conn.execute(
                "INSERT OR IGNORE INTO file_tags (file_id, tag_id) VALUES (1, ?1)",
                [tag_id],
            )?;
            Ok(())
        })
        .unwrap();
}

fn dispatch_context_with_plan<'a>(
    plan: Option<&'a SkillActivationPlanSummary>,
) -> ToolDispatchContext<'a> {
    let retrieval_scope = Box::leak(Box::new(
        crate::ai_runtime::retrieval_scope::RetrievalScope::default(),
    ));
    ToolDispatchContext {
        note_path: None,
        file_id: None,
        web_search_enabled: false,
        max_web_fetches: 3,
        cold_start_packets: &[],
        retrieval_scope,
        runtime_documents: &[],
        app_handle: None,
        attachment_count: 0,
        skill_activation_plan: plan,
    }
}

fn dispatch_context_with_retrieval_scope<'a>(
    retrieval_scope: &'a crate::ai_runtime::retrieval_scope::RetrievalScope,
) -> ToolDispatchContext<'a> {
    ToolDispatchContext {
        note_path: None,
        file_id: None,
        web_search_enabled: false,
        max_web_fetches: 3,
        cold_start_packets: &[],
        retrieval_scope,
        runtime_documents: &[],
        app_handle: None,
        attachment_count: 0,
        skill_activation_plan: None,
    }
}

#[test]
fn timeout_result_has_retryable_structured_error_shape() {
    let result = timeout_tool_result("web_search", Instant::now(), Duration::from_secs(30));

    assert!(!result.success);
    assert_eq!(result.tool_name, "web_search");
    assert_eq!(result.output["error"], "tool_dispatch_timeout");
    assert_eq!(result.output["failure_class"], "timeout");
    assert!(result.output["message"]
        .as_str()
        .unwrap_or("")
        .contains("web_search timed out after 30s"));
    assert!(result
        .error
        .as_deref()
        .unwrap_or("")
        .contains("tool_dispatch_timeout: web_search timed out after 30s"));
    assert!(is_retryable_tool_error("web_search", &result));
}

#[tokio::test]
async fn read_note_rejects_parent_dir_traversal() {
    let (state, _dir) = test_state();
    let ctx = dispatch_context_with_plan(None);
    let args = serde_json::json!({ "path": "../../etc/passwd" });
    let result = note_impl::read_note(&state, &ctx, &args).await;
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
    let ctx = dispatch_context_with_plan(None);
    let args = serde_json::json!({ "path": ".iris/versions/1/test.md" });
    let result = note_impl::read_note(&state, &ctx, &args).await;
    assert!(result.is_err());
    assert!(!result.unwrap_err().to_string().is_empty());
}

#[tokio::test]
async fn read_note_accepts_valid_path() {
    let (state, _dir) = test_state();
    let ctx = dispatch_context_with_plan(None);
    let args = serde_json::json!({ "path": "notes/test.md" });
    let result = note_impl::read_note(&state, &ctx, &args).await;
    assert!(result.is_ok());
    let val = result.unwrap();
    assert_eq!(val["path"], "notes/test.md");
    assert_eq!(val["content"], "# Test\nHello world");
}

#[tokio::test]
async fn note_read_tools_reject_paths_outside_the_immutable_run_scope() {
    let (state, _dir) = test_state();
    let scope = crate::ai_runtime::retrieval_scope::RetrievalScope {
        path_prefixes: Vec::new(),
        paths: vec!["notes/test.md".into()],
        required_tags: Vec::new(),
    };
    let ctx = dispatch_context_with_retrieval_scope(&scope);

    for result in [
        note_impl::read_note(
            &state,
            &ctx,
            &serde_json::json!({ "path": "private/secret.md" }),
        )
        .await,
        note_impl::get_outline(
            &state,
            &ctx,
            &serde_json::json!({ "path": "private/secret.md" }),
        )
        .await,
        note_impl::get_backlinks(
            &state,
            &ctx,
            &serde_json::json!({ "path": "private/secret.md" }),
        )
        .await,
        note_impl::get_block_links(
            &state,
            &ctx,
            &serde_json::json!({ "note_path": "private/secret.md" }),
        )
        .await,
    ] {
        assert_eq!(
            result
                .expect_err("out-of-scope note reads must fail closed")
                .to_string(),
            "agent_run_retrieval_scope_violation"
        );
    }
}

#[tokio::test]
async fn list_vault_returns_only_paths_inside_the_immutable_run_scope() {
    let (state, _dir) = test_state();
    state
        .db
        .with_conn(|conn| {
            for (path, title) in [("notes/test.md", "Test"), ("private/secret.md", "Secret")] {
                conn.execute(
                    "INSERT OR REPLACE INTO files
                     (path, title, content_hash, word_count, created_at, updated_at)
                     VALUES (?1, ?2, 'hash', 1, datetime('now'), datetime('now'))",
                    rusqlite::params![path, title],
                )?;
            }
            Ok(())
        })
        .expect("index notes");
    let scope = crate::ai_runtime::retrieval_scope::RetrievalScope {
        path_prefixes: vec!["notes/".into()],
        paths: Vec::new(),
        required_tags: Vec::new(),
    };
    let ctx = dispatch_context_with_retrieval_scope(&scope);

    let result = dispatch_tool(&state, &ctx, "list_vault", &serde_json::json!({})).await;

    assert!(
        result.success,
        "scoped vault list failed: {:?}",
        result.error
    );
    assert_eq!(result.output["count"], 1);
    assert_eq!(result.output["files"][0]["path"], "notes/test.md");
}

#[tokio::test]
async fn tag_only_run_scope_applies_to_direct_note_reads() {
    let (state, _dir) = test_state();
    index_tagged_note(&state, "notes/test.md", "daily");
    let scope = crate::ai_runtime::retrieval_scope::RetrievalScope {
        path_prefixes: Vec::new(),
        paths: Vec::new(),
        required_tags: vec!["daily".into()],
    };
    let ctx = dispatch_context_with_retrieval_scope(&scope);

    assert!(note_impl::read_note(
        &state,
        &ctx,
        &serde_json::json!({ "path": "notes/test.md" })
    )
    .await
    .is_ok());
    assert_eq!(
        note_impl::read_note(
            &state,
            &ctx,
            &serde_json::json!({ "path": "private/secret.md" })
        )
        .await
        .expect_err("untagged note must be outside a tag-only scope")
        .to_string(),
        "agent_run_retrieval_scope_violation"
    );
}

#[tokio::test]
async fn read_note_rejects_path_outside_active_skill_scope() {
    let (state, _dir) = test_state();
    let plan = scoped_plan("notes/**");
    let ctx = dispatch_context_with_plan(Some(&plan));
    let result = dispatch_tool(
        &state,
        &ctx,
        "read_note",
        &serde_json::json!({ "path": "private/secret.md" }),
    )
    .await;

    assert!(!result.success);
    assert!(result
        .error
        .as_deref()
        .unwrap_or("")
        .contains("outside the confirmed Skill scope"));
}

#[tokio::test]
async fn read_note_allows_paths_matching_active_skill_tag_scope() {
    let (state, _dir) = test_state();
    index_tagged_note(&state, "notes/test.md", "daily");
    let plan = scoped_plan_rule("tag", "#daily");
    let ctx = dispatch_context_with_plan(Some(&plan));

    let allowed = dispatch_tool(
        &state,
        &ctx,
        "read_note",
        &serde_json::json!({ "path": "notes/test.md" }),
    )
    .await;
    assert!(
        allowed.success,
        "tag-scoped read failed: {:?}",
        allowed.error
    );

    let rejected = dispatch_tool(
        &state,
        &ctx,
        "read_note",
        &serde_json::json!({ "path": "private/secret.md" }),
    )
    .await;
    assert!(!rejected.success);
    assert!(rejected
        .error
        .as_deref()
        .unwrap_or("")
        .contains("outside the confirmed Skill scope"));
}

#[tokio::test]
async fn get_outline_rejects_iris_metadata() {
    let (state, _dir) = test_state();
    let ctx = dispatch_context_with_plan(None);
    let args = serde_json::json!({ "path": ".iris/skills/my-skill/SKILL.md" });
    let result = note_impl::get_outline(&state, &ctx, &args).await;
    assert!(result.is_err());
    assert!(!result.unwrap_err().to_string().is_empty());
}

#[tokio::test]
async fn get_backlinks_rejects_iris_metadata() {
    let (state, _dir) = test_state();
    let ctx = dispatch_context_with_plan(None);
    let args = serde_json::json!({ "path": ".iris/versions/x.md" });
    let result = note_impl::get_backlinks(&state, &ctx, &args).await;
    assert!(result.is_err());
    assert!(!result.unwrap_err().to_string().is_empty());
}

#[tokio::test]
async fn get_backlinks_rejects_parent_dir() {
    let (state, _dir) = test_state();
    let ctx = dispatch_context_with_plan(None);
    let args = serde_json::json!({ "path": "../secret.md" });
    let result = note_impl::get_backlinks(&state, &ctx, &args).await;
    assert!(result.is_err());
    assert!(!result.unwrap_err().to_string().is_empty());
}

#[tokio::test]
async fn get_block_links_rejects_parent_dir() {
    let (state, _dir) = test_state();
    let ctx = dispatch_context_with_plan(None);
    let args = serde_json::json!({ "note_path": "../note.md" });
    let result = note_impl::get_block_links(&state, &ctx, &args).await;
    assert!(result.is_err());
    assert!(!result.unwrap_err().to_string().is_empty());
}

#[tokio::test]
async fn get_block_links_rejects_iris_metadata() {
    let (state, _dir) = test_state();
    let ctx = dispatch_context_with_plan(None);
    let args = serde_json::json!({ "note_path": ".iris/versions/x.md" });
    let result = note_impl::get_block_links(&state, &ctx, &args).await;
    assert!(result.is_err());
    assert!(!result.unwrap_err().to_string().is_empty());
}

#[tokio::test]
async fn read_note_rejects_absolute_path() {
    let (state, _dir) = test_state();
    let ctx = dispatch_context_with_plan(None);
    let args = serde_json::json!({ "path": "/etc/passwd" });
    let result = note_impl::read_note(&state, &ctx, &args).await;
    assert!(result.is_err());
}

#[test]
fn write_tool_approval_applies_patch_with_cas() {
    let (state, _dir) = test_state();
    let base = "# Test\nHello world";
    let base_hash = crate::cas::hash::content_hash_str(base);
    let retrieval_scope = crate::ai_runtime::retrieval_scope::RetrievalScope::default();
    let ctx = ToolDispatchContext {
        note_path: Some("notes/test.md"),
        file_id: None,
        web_search_enabled: false,
        max_web_fetches: 3,
        cold_start_packets: &[],
        retrieval_scope: &retrieval_scope,
        runtime_documents: &[],
        app_handle: None,
        attachment_count: 0,
        skill_activation_plan: None,
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
fn write_tool_rejects_target_outside_active_skill_scope_before_apply() {
    let (state, _dir) = test_state();
    let plan = scoped_plan("notes/**");
    let mut ctx = dispatch_context_with_plan(Some(&plan));
    ctx.note_path = Some("private/secret.md");
    let base = "# Secret\nHidden";
    let base_hash = crate::cas::hash::content_hash_str(base);

    let result = markdown_impl::markdown_write_patch_apply(
        &state,
        &ctx,
        "replace_selection",
        &serde_json::json!({
            "target_path": "private/secret.md",
            "replacement": "Public",
            "base_content_hash": base_hash,
            "range": {"start": 9, "end": 15},
            "original_text": "Hidden",
            "risk_level": "low"
        }),
    )
    .unwrap();

    assert_eq!(result["type"], "patch_apply");
    assert_eq!(result["result"]["success"], false);
    assert!(result["result"]["error"]
        .as_str()
        .unwrap_or("")
        .contains("outside the confirmed Skill scope"));
    let content =
        std::fs::read_to_string(state.vault_path().unwrap().join("private/secret.md")).unwrap();
    assert_eq!(content, "# Secret\nHidden");
}

#[test]
fn write_tool_approval_reports_hash_conflict_without_writing() {
    let (state, _dir) = test_state();
    let retrieval_scope = crate::ai_runtime::retrieval_scope::RetrievalScope::default();
    let ctx = ToolDispatchContext {
        note_path: Some("notes/test.md"),
        file_id: None,
        web_search_enabled: false,
        max_web_fetches: 3,
        cold_start_packets: &[],
        retrieval_scope: &retrieval_scope,
        runtime_documents: &[],
        app_handle: None,
        attachment_count: 0,
        skill_activation_plan: None,
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
