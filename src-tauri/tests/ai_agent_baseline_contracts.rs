use iris_lib::ai_harness::tool_turn::{outstanding_confirm_ids, outstanding_confirm_tool};
use iris_lib::ai_runtime::agent_permissions::{
    permission_profile_for_tool, AgentPermissionAtom, PermissionRiskLevel,
};
use iris_lib::ai_runtime::model_gateway::{LlmMessage, MessageRole, ToolCall};
use iris_lib::ai_runtime::tool_catalog::{ToolImplementationStatus, TOOL_CATALOG};
use iris_lib::ai_runtime::tool_executor::ToolRegistry;
use iris_lib::ai_runtime::tool_policy::{
    evaluate_tool, DenialReason, ToolPolicyContext, ToolPolicyVerdict,
};
use iris_lib::ai_runtime::{
    agent_task::AgentTaskKind,
    agent_task_policy::{AgentTaskPolicy, AgentTaskPolicyInput, AgentTaskScope},
    AgentIntent, AiScene, AutonomyLevel,
};

fn policy_ctx(depth: u32, web_search_enabled: bool) -> ToolPolicyContext {
    ToolPolicyContext {
        task_policy: None,
        scene: AiScene::KnowledgeLookup,
        autonomy_level: AutonomyLevel::L2,
        web_search_enabled,
        depth,
    }
}

fn write_policy_ctx() -> ToolPolicyContext {
    let task_policy = AgentTaskPolicy::from_input(AgentTaskPolicyInput {
        intent: AgentIntent::Write,
        task_kind: AgentTaskKind::Lightweight,
        scope: AgentTaskScope::Vault,
        web_authorized: true,
        has_attachments: false,
        write_permission_required: true,
        research_depth: 0,
    });
    ToolPolicyContext {
        task_policy: Some(task_policy.clone()),
        scene: AiScene::DraftingAssist,
        autonomy_level: task_policy.autonomy_level,
        web_search_enabled: true,
        depth: 0,
    }
}

fn tool_call(name: &str) -> ToolCall {
    ToolCall::new(format!("call-{name}"), name, "{}")
}

fn assistant_with_tool_calls(calls: Vec<ToolCall>) -> Vec<LlmMessage> {
    vec![LlmMessage {
        role: MessageRole::Assistant,
        content: String::new().into(),
        tool_call_id: None,
        tool_calls: Some(calls),
        ..Default::default()
    }]
}

#[test]
fn run_harness_keeps_subagents_partitioned_and_joined() {
    let run_rs = include_str!("../src/ai_harness/harness/run.rs");

    assert!(run_rs.contains(".partition(|tc| tc.function.name == \"spawn_subagent\")"));
    assert!(run_rs.contains("SubAgentCoordinator::plan(&subagent_specs)"));
    assert!(run_rs.contains("join_all(sub_futures).await"));
    assert!(run_rs.contains("parent.session_id"));
    assert!(run_rs.contains("parent.cold_start_packets.clone()"));
    assert!(!run_rs.contains("evidence_ledger.packets(),\n        web_search_enabled"));
}

#[test]
fn tool_policy_depth_limit_returns_visible_denial_for_subagents() {
    let verdict = evaluate_tool("spawn_subagent", &policy_ctx(2, true));

    assert_eq!(verdict, ToolPolicyVerdict::Denied(DenialReason::DepthLimit));
}

#[test]
fn implemented_tools_keep_permission_profiles_and_unsupported_secret_stays_closed() {
    for entry in TOOL_CATALOG.iter() {
        if entry.implementation == ToolImplementationStatus::Planned {
            continue;
        }

        let profile = permission_profile_for_tool(entry.name)
            .unwrap_or_else(|| panic!("missing permission profile for {}", entry.name));
        assert!(
            !profile.atoms.is_empty(),
            "{} must map to at least one permission atom",
            entry.name
        );
    }

    let secret = permission_profile_for_tool("secret.read_plaintext").unwrap();
    assert_eq!(secret.risk_level, PermissionRiskLevel::Critical);
    assert!(secret
        .atoms
        .contains(&AgentPermissionAtom::SecretReadPlaintext));
    assert!(!secret.supported);
}

#[test]
fn confirmation_helpers_track_one_active_prompt_and_remaining_pending_ids() {
    let registry = ToolRegistry::new();
    let first_write = tool_call("replace_selection");
    let second_write = tool_call("vault_delete_to_trash");
    let mut messages = assistant_with_tool_calls(vec![first_write.clone(), second_write.clone()]);

    let ctx = write_policy_ctx();
    let first = outstanding_confirm_tool(&registry, &messages, &ctx).expect("first confirmation");
    assert_eq!(first.id, first_write.id);

    messages.push(LlmMessage {
        role: MessageRole::Tool,
        content: r#"{"results":[]}"#.into(),
        tool_call_id: Some(first_write.id),
        tool_calls: None,
        ..Default::default()
    });

    let ids = outstanding_confirm_ids(&registry, &messages, &ctx);
    assert_eq!(ids, vec![second_write.id]);
}

#[test]
fn facade_modules_remain_thin_reexports_for_stage_zero_baseline() {
    let model_gateway = include_str!("../src/ai_runtime/model_gateway.rs");
    let skills = include_str!("../src/ai_runtime/skills.rs");
    let retrieval_broker = include_str!("../src/ai_runtime/retrieval_broker.rs");

    for facade in [model_gateway, skills, retrieval_broker] {
        assert!(facade.contains("#[path = "));
        assert!(facade.contains("mod implementation;"));
        assert!(facade.contains("pub use implementation::*;"));
    }
}
