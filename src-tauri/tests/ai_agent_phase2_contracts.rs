use iris_lib::ai_runtime::agent_permissions::{
    PermissionDecision, PermissionGrantInput, PermissionRiskLevel, PermissionScopeKind,
};
use iris_lib::ai_runtime::permission_decision::{
    decide_tool_permission, PermissionDecisionRequest, PermissionExecutionDecision,
};
use iris_lib::ai_runtime::tool_catalog::{ToolCatalogEntry, TOOL_CATALOG};
use iris_lib::ai_runtime::tool_execution_pipeline::{evaluate_tool_execution, ToolExecutionGate};
use iris_lib::ai_runtime::tool_policy::ToolPolicyContext;
use iris_lib::ai_runtime::trace::TraceRecorder;
use iris_lib::ai_runtime::{AiScene, AutonomyLevel};
use iris_lib::storage::db::Database;
use iris_lib::{
    ai_harness::tool_turn::pending_confirmation_position,
    ai_runtime::model_gateway::{LlmMessage, MessageRole, ToolCall},
    ai_runtime::tool_executor::ToolRegistry,
};

fn catalog_entry(name: &str) -> &'static ToolCatalogEntry {
    TOOL_CATALOG
        .iter()
        .find(|entry| entry.name == name)
        .unwrap_or_else(|| panic!("missing catalog entry {name}"))
}

fn policy_ctx(scene: AiScene, autonomy_level: AutonomyLevel) -> ToolPolicyContext {
    ToolPolicyContext {
        task_policy: None,
        scene,
        autonomy_level,
        web_search_enabled: true,
        skill_allowed_tools: vec![],
        depth: 0,
    }
}

#[test]
fn permission_decision_hard_denies_unsupported_profiles() {
    let db = Database::open_in_memory().unwrap();
    let ctx = policy_ctx(AiScene::DraftingAssist, AutonomyLevel::L2);
    let entry = catalog_entry("fs_pick_file");

    let outcome = decide_tool_permission(
        &db,
        PermissionDecisionRequest {
            request_id: "phase2-unsupported",
            entry,
            args: &serde_json::json!({ "reason": "attach source" }),
            policy_ctx: &ctx,
            skill_id: None,
        },
    )
    .unwrap();

    assert_eq!(outcome.decision, PermissionExecutionDecision::Denied);
    assert!(outcome.preflight.blocked);
    assert!(outcome
        .denied_reason
        .as_deref()
        .unwrap_or("")
        .contains("unsupported"));
}

#[test]
fn permission_decision_applies_exact_session_grants_only() {
    let db = Database::open_in_memory().unwrap();
    let ctx = policy_ctx(AiScene::DraftingAssist, AutonomyLevel::L2);
    let entry = catalog_entry("replace_selection");

    iris_lib::ai_runtime::agent_permissions::upsert_permission_grant(
        &db,
        &PermissionGrantInput {
            permission_name: "vault.write.patch",
            decision: PermissionDecision::AllowForSession,
            scope_kind: PermissionScopeKind::Session,
            scope_value: Some("other-session"),
            risk_level: PermissionRiskLevel::Medium,
            skill_id: None,
            expires_at: None,
        },
    )
    .unwrap();

    let without_matching_grant = decide_tool_permission(
        &db,
        PermissionDecisionRequest {
            request_id: "phase2-session-a",
            entry,
            args: &serde_json::json!({ "replacement": "text" }),
            policy_ctx: &ctx,
            skill_id: None,
        },
    )
    .unwrap();
    assert_eq!(
        without_matching_grant.decision,
        PermissionExecutionDecision::RequiresConfirmation
    );

    iris_lib::ai_runtime::agent_permissions::upsert_permission_grant(
        &db,
        &PermissionGrantInput {
            permission_name: "vault.write.patch",
            decision: PermissionDecision::AllowForSession,
            scope_kind: PermissionScopeKind::Session,
            scope_value: Some("phase2-session-a"),
            risk_level: PermissionRiskLevel::Medium,
            skill_id: None,
            expires_at: None,
        },
    )
    .unwrap();

    let matching_grant = decide_tool_permission(
        &db,
        PermissionDecisionRequest {
            request_id: "phase2-session-a",
            entry,
            args: &serde_json::json!({ "replacement": "text" }),
            policy_ctx: &ctx,
            skill_id: None,
        },
    )
    .unwrap();
    assert_eq!(
        matching_grant.decision,
        PermissionExecutionDecision::AutoAllowed
    );
}

#[test]
fn tool_execution_pipeline_records_denied_permission_and_tool_audit() {
    let db = Database::open_in_memory().unwrap();
    TraceRecorder::start(&db, "phase2-pipeline-deny", AiScene::DraftingAssist).unwrap();
    let ctx = policy_ctx(AiScene::DraftingAssist, AutonomyLevel::L2);
    let entry = catalog_entry("fs_pick_file");

    let gate = evaluate_tool_execution(
        &db,
        ToolExecutionGate {
            request_id: "phase2-pipeline-deny",
            harness_round: 3,
            entry,
            args: &serde_json::json!({ "reason": "import" }),
            policy_ctx: &ctx,
            skill_id: None,
            scene: Some(AiScene::DraftingAssist.profile()),
            subagent_depth: 0,
        },
    )
    .unwrap();

    assert_eq!(gate.decision.decision, PermissionExecutionDecision::Denied);
    assert!(gate.tool_result.is_some());

    let permission_count: i64 = db
        .with_read_conn(|conn| {
            let count = conn.query_row(
                "SELECT COUNT(*) FROM agent_permission_audit WHERE request_id = ?1",
                ["phase2-pipeline-deny"],
                |row| row.get(0),
            )?;
            Ok(count)
        })
        .unwrap();
    assert!(permission_count > 0);

    let tool_count: i64 = db
        .with_read_conn(|conn| {
            let count = conn.query_row(
                "SELECT COUNT(*) FROM tool_audit WHERE request_id = ?1 AND tool_name = ?2",
                ("phase2-pipeline-deny", "fs_pick_file"),
                |row| row.get(0),
            )?;
            Ok(count)
        })
        .unwrap();
    assert_eq!(tool_count, 1);
}

#[test]
fn source_paths_use_tool_execution_pipeline_for_runtime_gates() {
    let run_loop = include_str!("../src/ai_harness/harness/run.rs");
    let confirm = include_str!("../src/ai_harness/harness_confirm.rs");

    assert!(run_loop.contains("evaluate_tool_execution"));
    assert!(run_loop.contains("audit_dispatched_tool"));
    assert!(confirm.contains("evaluate_tool_execution"));
    assert!(confirm.contains("audit_dispatched_tool"));
}

#[test]
fn pending_confirmation_position_reports_true_serial_progress() {
    let registry = ToolRegistry::new();
    let ctx = policy_ctx(AiScene::DraftingAssist, AutonomyLevel::L2);
    let fetch_a = ToolCall::new(
        "call_fetch_a",
        "insert_text_at_cursor",
        r#"{"target_path":"notes/a.md","text":"A"}"#,
    );
    let fetch_b = ToolCall::new(
        "call_fetch_b",
        "replace_selection",
        r#"{"target_path":"notes/b.md","replacement":"B"}"#,
    );
    let mut messages = vec![LlmMessage {
        role: MessageRole::Assistant,
        content: "fetch both".into(),
        tool_call_id: None,
        tool_calls: Some(vec![fetch_a.clone(), fetch_b.clone()]),
        ..Default::default()
    }];

    let first = pending_confirmation_position(&registry, &messages, &ctx, &fetch_a.id)
        .expect("first pending confirmation");
    assert_eq!(first.index, 1);
    assert_eq!(first.count, 2);

    messages.push(LlmMessage {
        role: MessageRole::Tool,
        content: r#"{"title":"A"}"#.into(),
        tool_call_id: Some(fetch_a.id.clone()),
        tool_calls: None,
        ..Default::default()
    });

    let second = pending_confirmation_position(&registry, &messages, &ctx, &fetch_b.id)
        .expect("second pending confirmation");
    assert_eq!(second.index, 2);
    assert_eq!(second.count, 2);
}
