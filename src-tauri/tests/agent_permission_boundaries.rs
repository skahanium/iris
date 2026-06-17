use iris_lib::ai_runtime::agent_permissions::{
    permission_profile_for_tool, preflight_tool_permission, AgentPermissionAtom,
    PermissionDecision, PermissionGrantInput, PermissionRiskLevel, PermissionScopeKind,
};
use iris_lib::ai_runtime::tool_catalog::{catalog_find, ToolImplementationStatus, TOOL_CATALOG};
use iris_lib::ai_runtime::tool_dispatch::{dispatch_tool, ToolDispatchContext};
use iris_lib::ai_runtime::{AiScene, SkillCapabilitySupportStatus};
use iris_lib::app::AppState;

fn test_state() -> (std::sync::Arc<AppState>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    let state = AppState::new(dir.path().to_path_buf()).unwrap();
    state.set_vault(vault).unwrap();
    (state, dir)
}

fn dispatchable(name: &str) -> bool {
    catalog_find(name)
        .is_some_and(|entry| entry.implementation == ToolImplementationStatus::Dispatchable)
}

fn ctx() -> ToolDispatchContext<'static> {
    ToolDispatchContext {
        scene: AiScene::DraftingAssist,
        note_path: None,
        file_id: None,
        web_search_enabled: false,
        cold_start_packets: &[],
        app_handle: None,
        attachment_count: 0,
        skill_activation_plan: None,
    }
}

#[test]
fn catalog_declares_phase5_remaining_permission_boundaries() {
    let required = [
        "fs_pick_file",
        "fs_pick_folder",
        "fs_import_to_vault",
        "fs_export",
        "fs_read_authorized_folder",
        "fs_write_authorized_export",
        "doc_convert",
        "doc_ocr",
        "doc_extract_pdf",
        "doc_extract_table",
        "doc_normalize_markdown",
        "doc_fix_links",
        "doc_extract_citations",
        "web_to_markdown",
        "web_download_to_assets",
        "web_citation_extract",
        "net_localhost",
        "skill_request_capabilities",
        "skill_execute_script_sandboxed",
        "skill_install_dependency",
        "skill_mcp_bridge",
        "process_run_markdown_tool",
        "process_run_readonly",
        "process_run_mutating",
        "process_run_network",
        "process_long_running",
        "process_kill_owned",
        "git_read_status",
        "git_read_diff",
        "git_read_log",
        "git_write_commit",
        "clipboard_write",
        "clipboard_read",
        "browser_read_page",
        "browser_screenshot",
        "browser_control_page",
        "secret_exists",
        "secret_use_named",
        "secret_create_update",
        "secret_read_plaintext",
    ];

    for name in required {
        assert!(catalog_find(name).is_some(), "{name} missing from catalog");
        let profile = permission_profile_for_tool(name)
            .unwrap_or_else(|| panic!("{name} missing permission profile"));
        assert!(
            !profile.atoms.is_empty(),
            "{name} must map to at least one permission atom"
        );
    }
}

#[test]
fn unsupported_high_risk_boundaries_are_planned_not_exposed() {
    for name in [
        "process_run_mutating",
        "process_run_network",
        "process_long_running",
        "skill_execute_script_sandboxed",
        "skill_install_dependency",
        "skill_mcp_bridge",
        "clipboard_read",
        "browser_control_page",
        "secret_read_plaintext",
    ] {
        let entry = catalog_find(name).unwrap();
        assert_eq!(entry.implementation, ToolImplementationStatus::Planned);
        let preflight = preflight_tool_permission(entry, &serde_json::json!({}), None);
        assert_eq!(preflight.decision, PermissionDecision::DenyOnce);
        assert!(preflight.blocked);
        assert!(preflight.effects[0].blocked_reason.is_some());
    }
}

#[test]
fn host_dependent_or_policy_blocked_boundaries_stay_planned() {
    for name in [
        "fs_pick_file",
        "fs_pick_folder",
        "doc_convert",
        "doc_ocr",
        "doc_extract_pdf",
        "doc_extract_table",
        "doc_fix_links",
        "net_localhost",
        "skill_execute_script_sandboxed",
        "skill_install_dependency",
        "skill_mcp_bridge",
        "process_run_markdown_tool",
        "process_run_mutating",
        "process_run_network",
        "process_long_running",
        "process_kill_owned",
        "clipboard_write",
        "clipboard_read",
        "browser_read_page",
        "browser_screenshot",
        "browser_control_page",
        "secret_use_named",
        "secret_create_update",
        "secret_read_plaintext",
    ] {
        let entry = catalog_find(name).unwrap();
        assert_eq!(entry.implementation, ToolImplementationStatus::Planned);
        let preflight = preflight_tool_permission(entry, &serde_json::json!({}), None);
        assert!(
            preflight.blocked,
            "{name} must stay blocked until implemented"
        );
        assert_eq!(preflight.decision, PermissionDecision::DenyOnce);
    }
}

#[test]
fn skill_compatibility_reports_planned_phase5_boundaries() {
    let status =
        iris_lib::ai_runtime::skills::support_status_for_capability("process_run_readonly");
    assert_eq!(
        status,
        SkillCapabilitySupportStatus::SupportedWithConfirmation
    );

    let blocked =
        iris_lib::ai_runtime::skills::support_status_for_capability("secret_read_plaintext");
    assert_eq!(blocked, SkillCapabilitySupportStatus::Planned);
}

#[tokio::test]
async fn git_read_status_is_vault_scoped_and_content_free() {
    let (state, _dir) = test_state();
    let vault = state.vault_path().unwrap();
    std::fs::write(vault.join("note.md"), "# Note").unwrap();
    let _ = std::process::Command::new("git")
        .arg("init")
        .current_dir(&vault)
        .output()
        .unwrap();

    let result = dispatch_tool(
        &state,
        &ctx(),
        "git_read_status",
        &serde_json::json!({ "max_chars": 2000 }),
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["type"], "git_read_status");
    assert!(result.output["status"]
        .as_str()
        .unwrap()
        .contains("note.md"));
    assert!(!result.output["status"].as_str().unwrap().contains("# Note"));
}

#[tokio::test]
async fn secret_exists_checks_named_credential_without_plaintext() {
    let (state, _dir) = test_state();
    let result = dispatch_tool(
        &state,
        &ctx(),
        "secret_exists",
        &serde_json::json!({ "service": "iris.llm.phase5_missing" }),
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["type"], "secret_exists");
    assert_eq!(result.output["exists"], false);
    assert!(result.output.get("value").is_none());
}

#[test]
fn git_and_secret_tools_have_expected_permission_risk() {
    let git = permission_profile_for_tool("git_read_status").unwrap();
    assert_eq!(git.risk_level, PermissionRiskLevel::Low);
    assert!(git.atoms.contains(&AgentPermissionAtom::GitReadStatus));

    let secret = permission_profile_for_tool("secret_exists").unwrap();
    assert_eq!(secret.risk_level, PermissionRiskLevel::Low);
    assert!(secret.atoms.contains(&AgentPermissionAtom::SecretExists));
}

#[test]
fn all_non_planned_catalog_entries_keep_permission_profiles() {
    for entry in TOOL_CATALOG.iter() {
        if entry.implementation == ToolImplementationStatus::Planned {
            continue;
        }
        let profile = permission_profile_for_tool(entry.name)
            .unwrap_or_else(|| panic!("missing permission profile for {}", entry.name));
        assert!(!profile.atoms.is_empty());
    }
}

#[tokio::test]
async fn external_import_requires_authorized_root_and_writes_vault_note() {
    let (state, dir) = test_state();
    let external = dir.path().join("external");
    std::fs::create_dir_all(&external).unwrap();
    let source = external.join("source.md");
    std::fs::write(&source, "# Imported\n\nBody").unwrap();

    let result = dispatch_tool(
        &state,
        &ctx(),
        "fs_import_to_vault",
        &serde_json::json!({
            "source_path": source,
            "authorized_root": external,
            "target_path": "imports/source.md"
        }),
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["type"], "fs_import_to_vault");
    assert_eq!(result.output["path"], "imports/source.md");
    assert_eq!(
        std::fs::read_to_string(state.vault_path().unwrap().join("imports/source.md")).unwrap(),
        "# Imported\n\nBody"
    );

    let blocked = dispatch_tool(
        &state,
        &ctx(),
        "fs_import_to_vault",
        &serde_json::json!({
            "source_path": source,
            "authorized_root": external.join("nested"),
            "target_path": "imports/blocked.md"
        }),
    )
    .await;
    assert!(!blocked.success);
}

#[tokio::test]
async fn external_export_stays_inside_authorized_root() {
    let (state, dir) = test_state();
    let export_root = dir.path().join("exports");

    let result = dispatch_tool(
        &state,
        &ctx(),
        "fs_export",
        &serde_json::json!({
            "dest_path": export_root.join("out.md"),
            "authorized_root": export_root,
            "content": "# Export"
        }),
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["type"], "fs_export");
    assert_eq!(
        std::fs::read_to_string(export_root.join("out.md")).unwrap(),
        "# Export"
    );

    let blocked = dispatch_tool(
        &state,
        &ctx(),
        "fs_export",
        &serde_json::json!({
            "dest_path": dir.path().join("outside.md"),
            "authorized_root": export_root,
            "content": "# Escape"
        }),
    )
    .await;
    assert!(!blocked.success);
}

#[tokio::test]
async fn doc_normalize_markdown_is_content_only() {
    assert!(dispatchable("doc_normalize_markdown"));
    let (state, _dir) = test_state();
    let result = dispatch_tool(
        &state,
        &ctx(),
        "doc_normalize_markdown",
        &serde_json::json!({"content": "# Title\r\n\r\n\r\nBody   \r\n"}),
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["type"], "doc_normalize_markdown");
    assert_eq!(result.output["markdown"], "# Title\n\nBody\n");
}

#[tokio::test]
async fn web_download_to_assets_rejects_non_asset_path_before_network() {
    assert!(dispatchable("web_download_to_assets"));
    let (state, _dir) = test_state();
    let result = dispatch_tool(
        &state,
        &ctx(),
        "web_download_to_assets",
        &serde_json::json!({
            "url": "https://example.com/file.txt",
            "asset_path": "notes/file.txt"
        }),
    )
    .await;

    assert!(!result.success);
    assert!(result.error.unwrap().contains("assets"));
}

#[tokio::test]
async fn web_to_markdown_requires_web_authorization() {
    assert!(dispatchable("web_to_markdown"));
    let (state, _dir) = test_state();
    let result = dispatch_tool(
        &state,
        &ctx(),
        "web_to_markdown",
        &serde_json::json!({"url": "https://example.com"}),
    )
    .await;

    assert!(!result.success);
    assert!(result.error.unwrap().contains("not enabled"));
}

#[tokio::test]
async fn skill_request_capabilities_reports_supported_and_planned() {
    assert!(dispatchable("skill_request_capabilities"));
    let (state, _dir) = test_state();
    let result = dispatch_tool(
        &state,
        &ctx(),
        "skill_request_capabilities",
        &serde_json::json!({
            "capabilities": ["read", "process_run_readonly", "secret_read_plaintext"]
        }),
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["type"], "skill_request_capabilities");
    assert_eq!(result.output["results"][0]["status"], "supported");
    assert_eq!(
        result.output["results"][1]["status"],
        "supported_with_confirmation"
    );
    assert_eq!(result.output["results"][2]["status"], "planned");
}

#[tokio::test]
async fn process_run_readonly_is_allowlisted_and_vault_scoped() {
    assert!(dispatchable("process_run_readonly"));
    let (state, _dir) = test_state();
    let vault = state.vault_path().unwrap();
    std::fs::write(vault.join("note.md"), "one\ntwo\n").unwrap();
    let _ = std::process::Command::new("git")
        .arg("init")
        .current_dir(&vault)
        .output()
        .unwrap();

    let _ = std::process::Command::new("git")
        .arg("init")
        .current_dir(&vault)
        .output();

    let result = dispatch_tool(
        &state,
        &ctx(),
        "process_run_readonly",
        &serde_json::json!({
            "program": "git",
            "args": ["status"],
            "max_chars": 2000
        }),
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["type"], "process_run_readonly");
    assert!(result.output["stdout"]
        .as_str()
        .unwrap()
        .contains("note.md"));

    let blocked = dispatch_tool(
        &state,
        &ctx(),
        "process_run_readonly",
        &serde_json::json!({
            "program": "sh",
            "args": ["-c", "echo nope"]
        }),
    )
    .await;
    assert!(!blocked.success);
}

#[tokio::test]
async fn git_write_commit_only_commits_explicit_vault_paths() {
    assert!(dispatchable("git_write_commit"));
    let (state, _dir) = test_state();
    let vault = state.vault_path().unwrap();
    let _ = std::process::Command::new("git")
        .arg("init")
        .current_dir(&vault)
        .output()
        .unwrap();
    std::fs::write(vault.join("note.md"), "# Commit me").unwrap();

    let result = dispatch_tool(
        &state,
        &ctx(),
        "git_write_commit",
        &serde_json::json!({
            "message": "test commit",
            "paths": ["note.md"]
        }),
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["type"], "git_write_commit");
    assert!(result.output["commit"].as_str().unwrap().len() >= 7);

    let blocked = dispatch_tool(
        &state,
        &ctx(),
        "git_write_commit",
        &serde_json::json!({
            "message": "bad",
            "paths": ["../outside.md"]
        }),
    )
    .await;
    assert!(!blocked.success);
}

#[test]
fn permission_grants_affect_preflight_without_storing_bodies() {
    let (state, _dir) = test_state();
    iris_lib::ai_runtime::agent_permissions::upsert_permission_grant(
        &state.db,
        &PermissionGrantInput {
            permission_name: "vault.write.patch",
            decision: PermissionDecision::AllowForSession,
            scope_kind: PermissionScopeKind::Session,
            scope_value: Some("session-1"),
            risk_level: PermissionRiskLevel::Medium,
            skill_id: None,
            expires_at: None,
        },
    )
    .unwrap();

    let entry = catalog_find("replace_selection").unwrap();
    let preflight = preflight_tool_permission(
        entry,
        &serde_json::json!({
            "target_path": "notes/a.md",
            "replacement": "secret note body that must not be persisted"
        }),
        None,
    );
    assert_eq!(preflight.decision, PermissionDecision::AllowOnce);

    let grant = iris_lib::ai_runtime::agent_permissions::find_permission_grant(
        &state.db,
        "vault.write.patch",
        PermissionScopeKind::Session,
        Some("session-1"),
        None,
    )
    .unwrap();
    assert_eq!(grant.unwrap().decision, PermissionDecision::AllowForSession);

    let rows: i64 = state
        .db
        .with_read_conn(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM agent_permission_grants WHERE scope_value LIKE '%note body%'",
                [],
                |row| row.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(rows, 0);
}

#[test]
fn permission_audit_records_decision_metadata_only() {
    let (state, _dir) = test_state();
    iris_lib::ai_runtime::trace::TraceRecorder::start(
        &state.db,
        "perm-audit-1",
        AiScene::DraftingAssist,
    )
    .unwrap();

    iris_lib::ai_runtime::agent_permissions::record_permission_audit(
        &state.db,
        &iris_lib::ai_runtime::agent_permissions::PermissionAuditInput {
            request_id: "perm-audit-1",
            skill_id: None,
            tool_name: "replace_selection",
            permission_name: "vault.write.patch",
            decision: PermissionDecision::AllowOnce,
            scope_summary: "path=notes/a.md",
            risk_level: PermissionRiskLevel::Medium,
            result_status: "completed",
        },
    )
    .unwrap();

    let row: (String, String, String) = state
        .db
        .with_read_conn(|conn| {
            Ok(conn.query_row(
                "SELECT permission_name, decision, scope_summary FROM agent_permission_audit WHERE request_id = 'perm-audit-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?)
        })
        .unwrap();
    assert_eq!(row.0, "vault.write.patch");
    assert_eq!(row.1, "allow_once");
    assert_eq!(row.2, "path=notes/a.md");
}
