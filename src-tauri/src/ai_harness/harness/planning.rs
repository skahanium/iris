//! Harness round budget and agent-loop planning limits.

use crate::ai_runtime::agent_task_policy::AgentTaskPolicy;

/// Resolve effective max agentic rounds (respects override and task policy cap).
pub(crate) fn resolve_max_rounds(
    policy: &AgentTaskPolicy,
    max_rounds_override: Option<u32>,
) -> u32 {
    max_rounds_override
        .unwrap_or(policy.max_agentic_rounds)
        .min(policy.max_agentic_rounds)
}

/// Token budget for a harness run (defaults to task policy).
pub(crate) fn resolve_token_budget(policy: &AgentTaskPolicy, token_budget: Option<u32>) -> u32 {
    token_budget
        .unwrap_or(policy.default_token_budget)
        .min(policy.max_token_budget)
}
