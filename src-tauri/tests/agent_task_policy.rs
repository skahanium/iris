use iris_lib::ai_runtime::agent_task::AgentTaskKind;
use iris_lib::ai_runtime::agent_task_policy::{
    AgentTaskPolicy, AgentTaskPolicyInput, AgentTaskScope,
};
use iris_lib::ai_runtime::{AgentIntent, AutonomyLevel, CapabilitySlot, ContextStrategy};

#[test]
fn research_policy_is_derived_from_task_inputs_not_legacy_scene() {
    let policy = AgentTaskPolicy::from_input(AgentTaskPolicyInput {
        intent: AgentIntent::Research,
        task_kind: AgentTaskKind::Complex,
        scope: AgentTaskScope::Vault,
        web_authorized: true,
        has_attachments: false,
        write_permission_required: false,
        research_depth: 2,
    });

    assert_eq!(policy.intent, AgentIntent::Research);
    assert_eq!(policy.autonomy_level, AutonomyLevel::L3);
    assert_eq!(policy.model_slot, CapabilitySlot::Reasoner);
    assert_eq!(policy.context_strategy, ContextStrategy::Hybrid);
    assert_eq!(policy.max_agentic_rounds, 4);
    assert_eq!(policy.max_tool_calls_per_round, 6);
    assert_eq!(policy.max_fetch_per_round, 2);
    assert!(policy.default_token_budget >= 100_000);
}

#[test]
fn writing_and_exemplar_legacy_hint_do_not_change_task_policy() {
    let rewrite_policy = AgentTaskPolicy::from_input(AgentTaskPolicyInput {
        intent: AgentIntent::RewriteSelection,
        task_kind: AgentTaskKind::Lightweight,
        scope: AgentTaskScope::Selection,
        web_authorized: false,
        has_attachments: false,
        write_permission_required: true,
        research_depth: 0,
    });
    let write_policy = AgentTaskPolicy::from_input(AgentTaskPolicyInput {
        intent: AgentIntent::Write,
        task_kind: AgentTaskKind::Lightweight,
        scope: AgentTaskScope::Note,
        web_authorized: false,
        has_attachments: false,
        write_permission_required: true,
        research_depth: 0,
    });

    assert_eq!(rewrite_policy.model_slot, CapabilitySlot::Writer);
    assert_eq!(write_policy.model_slot, CapabilitySlot::Writer);
    assert_eq!(
        rewrite_policy.max_agentic_rounds,
        write_policy.max_agentic_rounds
    );
    assert_eq!(
        rewrite_policy.max_token_budget,
        write_policy.max_token_budget
    );
    assert!(!rewrite_policy
        .legacy_scene_hint
        .contains("exemplar_learning"));
    assert!(!write_policy.legacy_scene_hint.contains("exemplar_learning"));
}

#[test]
fn attachments_and_long_context_promote_capabilities_without_scene_switching() {
    let policy = AgentTaskPolicy::from_input(AgentTaskPolicyInput {
        intent: AgentIntent::VisionChat,
        task_kind: AgentTaskKind::Lightweight,
        scope: AgentTaskScope::Vault,
        web_authorized: false,
        has_attachments: true,
        write_permission_required: false,
        research_depth: 0,
    });

    assert_eq!(policy.model_slot, CapabilitySlot::Vision);
    assert_eq!(policy.context_strategy, ContextStrategy::Hybrid);
    assert_eq!(policy.autonomy_level, AutonomyLevel::L1);
}
