//! ToolPolicy 閳?hard security boundary for tool exposure and execution.
//!
//! Computes the set of tools available for a given request by intersecting:
//!
//! ```text
//! implemented/harness-only hard gate
//!   閳?task capability affinity
//!   閳?autonomy level
//!   閳?web_search_enabled
//!   閳?skill allowed-tools request
//!   閳?user settings
//! ```
//!
//! Key invariants:
//! - Skills cannot enable unimplemented tools.
//! - Skills cannot bypass `requires_confirmation`.
//! - Skills cannot auto-execute write tools at L1 autonomy.
//! - Without skills, the 8 core read-only tools are always available.

use crate::ai_runtime::tool_catalog::{
    catalog_find, ToolCatalogEntry, ToolImplementationStatus, TOOL_CATALOG,
};
use crate::ai_runtime::{
    agent_task::AgentTaskKind,
    agent_task_policy::{
        intent_from_legacy_scene, AgentTaskPolicy, AgentTaskPolicyInput, AgentTaskScope,
    },
    AiScene, AutonomyLevel, ToolAccessLevel, ToolCapabilityAffinity,
};

/// Evaluation context for a single tool policy decision.
#[derive(Debug, Clone)]
pub struct ToolPolicyContext {
    pub task_policy: Option<AgentTaskPolicy>,
    /// Legacy scene hint for old callers that have not moved to task policy.
    pub scene: AiScene,
    pub autonomy_level: AutonomyLevel,
    pub web_search_enabled: bool,
    /// Tools explicitly requested by active skills (via `allowed-tools`).
    pub skill_allowed_tools: Vec<String>,
    /// Depth of sub-agent nesting (閳?2 hides `spawn_subagent`).
    pub depth: u32,
}

/// Result of evaluating a single tool against the policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolPolicyVerdict {
    /// Tool is available and can be auto-executed (no confirmation needed).
    AutoAllowed,
    /// Tool is available but requires user confirmation before execution.
    RequiresConfirmation,
    /// Tool is not available for this request.
    Denied(DenialReason),
}

/// Why a tool was denied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialReason {
    /// Not in the catalog or marked as Planned.
    NotImplemented,
    /// Not relevant to the current task capability requirements.
    CapabilityMismatch,
    /// Autonomy level too low for this access level.
    InsufficientAutonomy,
    /// Network tool but web_search is disabled.
    WebSearchDisabled,
    /// Sub-agent depth 閳?2 hides spawn_subagent.
    DepthLimit,
    /// Not in skill allowed-tools when skills are active and tool is non-default.
    NotInSkillAllowlist,
}

/// User-facing hint when a tool is denied (also written into tool-role messages).
pub fn denial_user_message(reason: DenialReason, tool_name: &str) -> String {
    match reason {
        DenialReason::WebSearchDisabled => format!(
            "Web search is disabled, so {tool_name} cannot be used. Install SkillHub skills through skills_install(source=registry, registry=skillhub, path_or_url=<skill name>) instead of fetch_web_page."
        ),
        DenialReason::NotImplemented => format!("tool {tool_name} is not implemented"),
        DenialReason::CapabilityMismatch => {
            format!("tool {tool_name} is not available for this task")
        }
        DenialReason::InsufficientAutonomy => {
            format!("current autonomy level is too low to use {tool_name}")
        }
        DenialReason::DepthLimit => {
            format!("tool {tool_name} is unavailable at this sub-agent depth")
        }
        DenialReason::NotInSkillAllowlist => {
            format!("active Skills did not allow tool {tool_name}")
        }
    }
}
/// Core meta tools for skill management 閳?always available, not blocked by skill allowlist.
const META_SKILL_TOOLS: &[&str] = &[
    "skills_list",
    "skills_install",
    "skills_prepare_workspace",
    "skills_uninstall",
    "skills_update",
    "skills_toggle",
    "mcp_runtime_profiles_list",
    "mcp_runtime_diagnostics",
    "mcp_runtime_tools_list",
    "mcp_runtime_health_check",
    "mcp_runtime_capability_call",
    "mcp_runtime_profile_upsert",
    "mcp_runtime_profile_toggle",
    "mcp_runtime_profile_delete",
    "skills_workspace_list",
    "skills_workspace_read",
    "skills_workspace_write",
];

/// Evaluate the policy verdict for a single tool.
pub fn evaluate_tool(tool_name: &str, ctx: &ToolPolicyContext) -> ToolPolicyVerdict {
    let Some(entry) = catalog_find(tool_name) else {
        return ToolPolicyVerdict::Denied(DenialReason::NotImplemented);
    };
    evaluate_entry(entry, ctx)
}

fn is_meta_skill_tool(name: &str) -> bool {
    META_SKILL_TOOLS.contains(&name)
}

/// Evaluate the policy verdict for a catalog entry.
fn evaluate_entry(entry: &ToolCatalogEntry, ctx: &ToolPolicyContext) -> ToolPolicyVerdict {
    // 1. Hard gate: must be implemented or harness-only
    if entry.implementation == ToolImplementationStatus::Planned {
        return ToolPolicyVerdict::Denied(DenialReason::NotImplemented);
    }

    // 2. Depth limit: spawn_subagent hidden at depth 閳?2
    if entry.name == "spawn_subagent" && ctx.depth >= 2 {
        return ToolPolicyVerdict::Denied(DenialReason::DepthLimit);
    }

    // 3. Web search gating
    if entry.access_level == ToolAccessLevel::Network && !ctx.web_search_enabled {
        return ToolPolicyVerdict::Denied(DenialReason::WebSearchDisabled);
    }

    // 4. Meta skill tools bypass task capability and skill allowlist gates.
    if is_meta_skill_tool(entry.name) {
        return if entry.requires_confirmation {
            ToolPolicyVerdict::RequiresConfirmation
        } else {
            ToolPolicyVerdict::AutoAllowed
        };
    }

    let task_policy = effective_task_policy(ctx);

    // 5. Capability affinity: task policy, permission preflight and skill
    // allowlists decide exposure. Legacy scene affinity is metadata only.
    let capability_affinity = entry.capability_affinity();
    if !capability_allowed_for_task(entry, &capability_affinity, &task_policy, ctx) {
        return ToolPolicyVerdict::Denied(DenialReason::CapabilityMismatch);
    }

    // 6. Autonomy level check
    if let Some(required) = required_autonomy(entry) {
        if ctx.autonomy_level < required {
            return ToolPolicyVerdict::Denied(DenialReason::InsufficientAutonomy);
        }
    }

    // 7. Skill allowlist check: if skills are active, non-default tools must be in allowlist
    if !ctx.skill_allowed_tools.is_empty()
        && !entry.default_enabled_without_skill
        && !ctx.skill_allowed_tools.contains(&entry.name.to_string())
    {
        return ToolPolicyVerdict::Denied(DenialReason::NotInSkillAllowlist);
    }

    // 8. Confirmation check
    if entry.requires_confirmation {
        ToolPolicyVerdict::RequiresConfirmation
    } else {
        ToolPolicyVerdict::AutoAllowed
    }
}

/// Minimum autonomy level required for a tool's access level.
fn required_autonomy(entry: &ToolCatalogEntry) -> Option<AutonomyLevel> {
    if entry.name == "web_search" {
        return Some(AutonomyLevel::L1);
    }

    match entry.access_level {
        ToolAccessLevel::ReadIndex
        | ToolAccessLevel::ReadNoteSpan
        | ToolAccessLevel::ReadProfile => {
            None // Always allowed at any autonomy
        }
        ToolAccessLevel::Network => Some(AutonomyLevel::L2),
        ToolAccessLevel::WriteCache => Some(AutonomyLevel::L2),
        ToolAccessLevel::WriteMarkdown => Some(AutonomyLevel::L2),
        ToolAccessLevel::WriteSettings => Some(AutonomyLevel::L2),
        ToolAccessLevel::ManageSkills => None,
    }
}

fn effective_task_policy(ctx: &ToolPolicyContext) -> AgentTaskPolicy {
    ctx.task_policy.clone().unwrap_or_else(|| {
        let intent = intent_from_legacy_scene(ctx.scene);
        AgentTaskPolicy::from_input(AgentTaskPolicyInput {
            intent,
            task_kind: match ctx.scene {
                AiScene::ResearchSynthesis => AgentTaskKind::Complex,
                _ => AgentTaskKind::Lightweight,
            },
            scope: AgentTaskScope::Vault,
            web_authorized: ctx.web_search_enabled,
            has_attachments: false,
            write_permission_required: matches!(ctx.scene, AiScene::DraftingAssist),
            research_depth: matches!(ctx.scene, AiScene::ResearchSynthesis) as u32,
        })
    })
}

fn capability_allowed_for_task(
    entry: &ToolCatalogEntry,
    capability_affinity: &[ToolCapabilityAffinity],
    policy: &AgentTaskPolicy,
    ctx: &ToolPolicyContext,
) -> bool {
    if entry.default_enabled_without_skill
        && matches!(
            entry.access_level,
            ToolAccessLevel::ReadIndex
                | ToolAccessLevel::ReadNoteSpan
                | ToolAccessLevel::ReadProfile
        )
    {
        return true;
    }

    let skill_requested = ctx.skill_allowed_tools.contains(&entry.name.to_string());
    capability_affinity.iter().copied().any(|capability| {
        capability_allowed(capability, policy, skill_requested, ctx.web_search_enabled)
    })
}

fn capability_allowed(
    capability: ToolCapabilityAffinity,
    policy: &AgentTaskPolicy,
    skill_requested: bool,
    web_search_enabled: bool,
) -> bool {
    use crate::ai_runtime::AgentIntent;
    use ToolCapabilityAffinity::*;

    match capability {
        ReadNotes | SearchNotes => true,
        WebFetch => policy.web_authorized && web_search_enabled,
        ResearchSynthesis => {
            skill_requested
                || policy.research_depth > 0
                || matches!(
                    policy.intent,
                    AgentIntent::Research | AgentIntent::CitationCheck | AgentIntent::DocumentCheck
                )
        }
        SkillManagement => skill_requested || matches!(policy.intent, AgentIntent::SkillManagement),
        WriteNotes | PatchDocument => skill_requested || policy.write_permission_required,
        VaultOrganize => skill_requested || matches!(policy.intent, AgentIntent::Organize),
    }
}

/// Compute the set of tool names available for the given context.
/// Returns (auto_allowed, requires_confirmation) 閳?both subsets of exposable tools.
pub fn compute_available_tools(ctx: &ToolPolicyContext) -> (Vec<String>, Vec<String>) {
    let mut auto_allowed = Vec::new();
    let mut requires_confirmation = Vec::new();

    for entry in TOOL_CATALOG.iter() {
        if entry.implementation == ToolImplementationStatus::Planned {
            continue;
        }
        match evaluate_entry(entry, ctx) {
            ToolPolicyVerdict::AutoAllowed => auto_allowed.push(entry.name.to_string()),
            ToolPolicyVerdict::RequiresConfirmation => {
                requires_confirmation.push(entry.name.to_string())
            }
            ToolPolicyVerdict::Denied(_) => {}
        }
    }

    (auto_allowed, requires_confirmation)
}

/// Check whether a tool should be exposed to the model in the given context.
/// This is the top-level filter used by `tools_for_surface`.
pub fn is_tool_exposed(tool_name: &str, ctx: &ToolPolicyContext) -> bool {
    let verdict = evaluate_tool(tool_name, ctx);
    !matches!(verdict, ToolPolicyVerdict::Denied(_))
}

/// Whether the tool requires user confirmation in the given context.
pub fn tool_requires_confirmation(tool_name: &str, ctx: &ToolPolicyContext) -> bool {
    let verdict = evaluate_tool(tool_name, ctx);
    matches!(verdict, ToolPolicyVerdict::RequiresConfirmation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::{
        agent_task::AgentTaskKind,
        agent_task_policy::{AgentTaskPolicyInput, AgentTaskScope},
        AgentIntent, AutonomyLevel,
    };

    fn policy_for(intent: AgentIntent, write_permission_required: bool) -> AgentTaskPolicy {
        AgentTaskPolicy::from_input(AgentTaskPolicyInput {
            intent,
            task_kind: if matches!(
                intent,
                AgentIntent::Research | AgentIntent::CitationCheck | AgentIntent::DocumentCheck
            ) {
                AgentTaskKind::Complex
            } else {
                AgentTaskKind::Lightweight
            },
            scope: AgentTaskScope::Vault,
            web_authorized: true,
            has_attachments: false,
            write_permission_required,
            research_depth: matches!(intent, AgentIntent::Research | AgentIntent::CitationCheck)
                as u32,
        })
    }

    fn default_ctx() -> ToolPolicyContext {
        let task_policy = policy_for(AgentIntent::AskNotes, false);
        ToolPolicyContext {
            task_policy: Some(task_policy.clone()),
            scene: AiScene::KnowledgeLookup,
            autonomy_level: task_policy.autonomy_level,
            web_search_enabled: true,
            skill_allowed_tools: vec![],
            depth: 0,
        }
    }

    fn drafting_ctx() -> ToolPolicyContext {
        let task_policy = policy_for(AgentIntent::Write, true);
        ToolPolicyContext {
            task_policy: Some(task_policy.clone()),
            scene: AiScene::DraftingAssist,
            autonomy_level: task_policy.autonomy_level,
            web_search_enabled: true,
            skill_allowed_tools: vec![],
            depth: 0,
        }
    }

    fn chat_ctx(web_search_enabled: bool) -> ToolPolicyContext {
        let task_policy = policy_for(AgentIntent::Chat, false);
        ToolPolicyContext {
            task_policy: Some(task_policy.clone()),
            scene: AiScene::KnowledgeLookup,
            autonomy_level: task_policy.autonomy_level,
            web_search_enabled,
            skill_allowed_tools: vec![],
            depth: 0,
        }
    }

    // 閳光偓閳光偓 Hard gate 閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓

    #[test]
    fn meta_skill_tools_always_available_with_active_skill_allowlist() {
        let ctx = ToolPolicyContext {
            task_policy: None,
            scene: AiScene::KnowledgeLookup,
            autonomy_level: AutonomyLevel::L2,
            web_search_enabled: true,
            skill_allowed_tools: vec!["search_hybrid".into()],
            depth: 0,
        };
        assert_eq!(
            evaluate_tool("skills_list", &ctx),
            ToolPolicyVerdict::AutoAllowed
        );
        assert_eq!(
            evaluate_tool("skills_install", &ctx),
            ToolPolicyVerdict::RequiresConfirmation
        );
        assert_eq!(
            evaluate_tool("mcp_runtime_profiles_list", &ctx),
            ToolPolicyVerdict::AutoAllowed
        );
        assert_eq!(
            evaluate_tool("mcp_runtime_tools_list", &ctx),
            ToolPolicyVerdict::RequiresConfirmation
        );
        assert_eq!(
            evaluate_tool("mcp_runtime_health_check", &ctx),
            ToolPolicyVerdict::RequiresConfirmation
        );
        assert_eq!(
            evaluate_tool("mcp_runtime_capability_call", &ctx),
            ToolPolicyVerdict::RequiresConfirmation
        );
        assert_eq!(
            evaluate_tool("mcp_runtime_profile_upsert", &ctx),
            ToolPolicyVerdict::RequiresConfirmation
        );
        assert_eq!(
            evaluate_tool("mcp_runtime_profile_toggle", &ctx),
            ToolPolicyVerdict::RequiresConfirmation
        );
        assert_eq!(
            evaluate_tool("mcp_runtime_profile_delete", &ctx),
            ToolPolicyVerdict::RequiresConfirmation
        );
    }

    #[test]
    fn meta_skill_tools_available_when_web_search_disabled() {
        let ctx = ToolPolicyContext {
            web_search_enabled: false,
            ..default_ctx()
        };
        assert_eq!(
            evaluate_tool("skills_list", &ctx),
            ToolPolicyVerdict::AutoAllowed
        );
        assert_eq!(
            evaluate_tool("skills_install", &ctx),
            ToolPolicyVerdict::RequiresConfirmation
        );
    }

    #[test]
    fn nonexistent_tool_denied() {
        let ctx = default_ctx();
        assert_eq!(
            evaluate_tool("nonexistent", &ctx),
            ToolPolicyVerdict::Denied(DenialReason::NotImplemented)
        );
    }

    // 閳光偓閳光偓 Capability affinity 閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓

    #[test]
    fn ask_notes_policy_allows_search_notes() {
        let ctx = default_ctx();
        assert!(is_tool_exposed("search_hybrid", &ctx));
    }

    #[test]
    fn ask_notes_policy_denies_write_without_write_permission() {
        let ctx = default_ctx();
        assert!(!is_tool_exposed("insert_text_at_cursor", &ctx));
    }

    #[test]
    fn write_policy_allows_insert_text() {
        let ctx = drafting_ctx();
        assert!(is_tool_exposed("insert_text_at_cursor", &ctx));
    }

    #[test]
    fn search_notes_capability_available_without_legacy_scene_affinity() {
        let ctx = default_ctx();
        assert!(is_tool_exposed("search_semantic", &ctx));
    }

    // 閳光偓閳光偓 Autonomy level 閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓

    #[test]
    fn write_tool_denied_at_l1() {
        let ctx = ToolPolicyContext {
            autonomy_level: AutonomyLevel::L1,
            ..drafting_ctx()
        };
        assert_eq!(
            evaluate_tool("insert_text_at_cursor", &ctx),
            ToolPolicyVerdict::Denied(DenialReason::InsufficientAutonomy)
        );
    }

    #[test]
    fn write_tool_allowed_at_l2() {
        let ctx = drafting_ctx();
        assert_eq!(
            evaluate_tool("insert_text_at_cursor", &ctx),
            ToolPolicyVerdict::RequiresConfirmation
        );
    }

    #[test]
    fn chat_with_web_authorized_exposes_web_search_at_l1() {
        let ctx = chat_ctx(true);

        assert_eq!(ctx.autonomy_level, AutonomyLevel::L1);
        assert_eq!(
            evaluate_tool("web_search", &ctx),
            ToolPolicyVerdict::AutoAllowed
        );
    }

    #[test]
    fn chat_without_web_authorization_does_not_expose_web_search() {
        let ctx = chat_ctx(false);

        assert_eq!(
            evaluate_tool("web_search", &ctx),
            ToolPolicyVerdict::Denied(DenialReason::WebSearchDisabled)
        );
    }

    #[test]
    fn chat_does_not_auto_expose_page_fetch_tools() {
        let ctx = chat_ctx(true);

        assert!(!is_tool_exposed("fetch_web_page", &ctx));
        assert!(!is_tool_exposed("web_fetch_batch", &ctx));
        assert!(!is_tool_exposed("readability_fetch", &ctx));
        assert!(!is_tool_exposed("rendered_fetch", &ctx));
    }

    #[test]
    fn page_fetch_tool_denied_at_l1() {
        let ctx = ToolPolicyContext {
            autonomy_level: AutonomyLevel::L1,
            ..default_ctx()
        };
        assert_eq!(
            evaluate_tool("fetch_web_page", &ctx),
            ToolPolicyVerdict::Denied(DenialReason::InsufficientAutonomy)
        );
    }

    #[test]
    fn read_tool_allowed_at_l0() {
        let ctx = ToolPolicyContext {
            autonomy_level: AutonomyLevel::L0,
            ..default_ctx()
        };
        assert!(is_tool_exposed("search_hybrid", &ctx));
    }

    // 閳光偓閳光偓 Web search gating 閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓

    #[test]
    fn web_search_denied_when_disabled() {
        let ctx = ToolPolicyContext {
            web_search_enabled: false,
            ..default_ctx()
        };
        assert_eq!(
            evaluate_tool("web_search", &ctx),
            ToolPolicyVerdict::Denied(DenialReason::WebSearchDisabled)
        );
    }

    #[test]
    fn fetch_web_page_denied_by_web_flag() {
        let ctx = ToolPolicyContext {
            web_search_enabled: false,
            ..default_ctx()
        };
        assert_eq!(
            evaluate_tool("fetch_web_page", &ctx),
            ToolPolicyVerdict::Denied(DenialReason::WebSearchDisabled)
        );
    }

    // 閳光偓閳光偓 Confirmation 閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓

    #[test]
    fn read_tool_auto_allowed() {
        let ctx = default_ctx();
        assert_eq!(
            evaluate_tool("read_note", &ctx),
            ToolPolicyVerdict::AutoAllowed
        );
    }

    #[test]
    fn write_tool_requires_confirmation() {
        let ctx = drafting_ctx();
        assert_eq!(
            evaluate_tool("insert_text_at_cursor", &ctx),
            ToolPolicyVerdict::RequiresConfirmation
        );
    }

    #[test]
    fn conclude_reasoning_auto_allowed() {
        let ctx = default_ctx();
        assert_eq!(
            evaluate_tool("conclude_reasoning", &ctx),
            ToolPolicyVerdict::AutoAllowed
        );
    }

    // 閳光偓閳光偓 Depth limit 閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓

    #[test]
    fn spawn_subagent_hidden_at_depth_2() {
        let ctx = ToolPolicyContext {
            depth: 2,
            ..default_ctx()
        };
        assert_eq!(
            evaluate_tool("spawn_subagent", &ctx),
            ToolPolicyVerdict::Denied(DenialReason::DepthLimit)
        );
    }

    #[test]
    fn spawn_subagent_allowed_at_depth_1() {
        let ctx = ToolPolicyContext {
            depth: 1,
            ..default_ctx()
        };
        assert!(is_tool_exposed("spawn_subagent", &ctx));
    }

    // 閳光偓閳光偓 Skill allowlist 閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓

    #[test]
    fn non_default_tool_denied_without_skill_allowlist() {
        let ctx = default_ctx();
        assert!(!is_tool_exposed("insert_text_at_cursor", &ctx));
    }

    #[test]
    fn skill_can_enable_non_default_tool_when_capability_allows_it() {
        let ctx = ToolPolicyContext {
            task_policy: Some(policy_for(AgentIntent::Write, true)),
            scene: AiScene::DraftingAssist,
            skill_allowed_tools: vec!["insert_text_at_cursor".to_string()],
            ..default_ctx()
        };
        assert!(is_tool_exposed("insert_text_at_cursor", &ctx));
    }

    #[test]
    fn skill_cannot_enable_non_default_tool_not_in_allowlist() {
        let ctx = ToolPolicyContext {
            task_policy: Some(policy_for(AgentIntent::Write, true)),
            scene: AiScene::DraftingAssist,
            skill_allowed_tools: vec!["some_other_tool".to_string()],
            ..default_ctx()
        };
        // insert_text_at_cursor is not default, not in skill allowlist
        assert!(!is_tool_exposed("insert_text_at_cursor", &ctx));
    }

    // 閳光偓閳光偓 compute_available_tools 閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓

    #[test]
    fn compute_available_includes_core_defaults() {
        let ctx = default_ctx();
        let (auto, confirm) = compute_available_tools(&ctx);
        for name in [
            "search_hybrid",
            "read_note",
            "list_vault",
            "get_outline",
            "get_backlinks",
        ] {
            assert!(
                auto.contains(&name.to_string()),
                "{name} should be auto-allowed"
            );
        }
        // fetch_web_page requires confirmation
        assert!(confirm.contains(&"fetch_web_page".to_string()));
    }

    #[test]
    fn compute_available_excludes_denied() {
        let ctx = default_ctx();
        let (auto, confirm) = compute_available_tools(&ctx);
        assert!(!auto.contains(&"insert_text_at_cursor".to_string()));
        assert!(!confirm.contains(&"insert_text_at_cursor".to_string()));
    }

    // 閳光偓閳光偓 Core default tools invariant 閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓

    #[test]
    fn core_defaults_always_available_without_skills() {
        let required_auto = [
            "search_hybrid",
            "search_semantic",
            "search_keyword",
            "read_note",
            "list_vault",
            "get_outline",
            "get_backlinks",
        ];
        let ctx = default_ctx();
        let (auto, _) = compute_available_tools(&ctx);
        for name in required_auto {
            assert!(
                auto.contains(&name.to_string()),
                "core tool '{name}' must be auto-allowed"
            );
        }
    }

    #[test]
    fn conclude_reasoning_always_available() {
        let ctx = default_ctx();
        let (auto, _) = compute_available_tools(&ctx);
        assert!(auto.contains(&"conclude_reasoning".to_string()));
    }

    // 閳光偓閳光偓 Writing task specifics 閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓

    #[test]
    fn write_policy_write_tools_need_confirmation() {
        let ctx = drafting_ctx();
        let (auto, confirm) = compute_available_tools(&ctx);
        assert!(confirm.contains(&"insert_text_at_cursor".to_string()));
        assert!(confirm.contains(&"replace_selection".to_string()));
        // Read tools still auto
        assert!(auto.contains(&"search_hybrid".to_string()));
    }

    // 閳光偓閳光偓 Ask-notes task specifics 閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓閳光偓

    #[test]
    fn ask_notes_policy_no_write_tools() {
        let ctx = default_ctx();
        let (auto, confirm) = compute_available_tools(&ctx);
        assert!(!auto.contains(&"insert_text_at_cursor".to_string()));
        assert!(!confirm.contains(&"insert_text_at_cursor".to_string()));
        assert!(!auto.contains(&"replace_selection".to_string()));
        assert!(!confirm.contains(&"replace_selection".to_string()));
    }
}
