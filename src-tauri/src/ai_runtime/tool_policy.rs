//! Stateless policy boundary for a future Run tool pipeline.
//!
//! Policy input is explicit Run capability data. It intentionally has no
//! deprecated routing metadata or implicit execution-state fallback.

use crate::ai_runtime::tool_catalog::{
    catalog_find, ToolCatalogEntry, ToolImplementationStatus, TOOL_CATALOG,
};
use crate::ai_runtime::{AutonomyLevel, ToolAccessLevel, ToolCapabilityAffinity};

/// Explicit capabilities authorized for one Run.
#[derive(Debug, Clone, Copy)]
pub struct ToolPolicyContext {
    pub autonomy_level: AutonomyLevel,
    pub web_search_enabled: bool,
    pub allow_writes: bool,
    pub allow_research: bool,
    pub allow_skill_management: bool,
    /// When false, vault read/search tools are denied unless another capability
    /// path authorizes them. Explicit `@`/`#` Runs keep this true and rely on
    /// `RetrievalScope` to constrain paths.
    pub allow_implicit_vault: bool,
}

impl Default for ToolPolicyContext {
    fn default() -> Self {
        Self {
            autonomy_level: AutonomyLevel::L0,
            web_search_enabled: false,
            allow_writes: false,
            allow_research: false,
            allow_skill_management: false,
            allow_implicit_vault: false,
        }
    }
}
/// Result of evaluating a tool against the Run policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolPolicyVerdict {
    AutoAllowed,
    RequiresConfirmation,
    Denied(DenialReason),
}

/// Reason a tool cannot enter a Run pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialReason {
    NotImplemented,
    CapabilityMismatch,
    InsufficientAutonomy,
    WebSearchDisabled,
    ImplicitVaultDenied,
}

/// User-safe explanation for a denied tool request.
pub fn denial_user_message(reason: DenialReason, tool_name: &str) -> String {
    match reason {
        DenialReason::WebSearchDisabled => {
            format!("Web search is disabled, so {tool_name} cannot be used.")
        }
        DenialReason::NotImplemented => format!("tool {tool_name} is not implemented"),
        DenialReason::CapabilityMismatch => {
            format!("tool {tool_name} is not authorized for this Run")
        }
        DenialReason::InsufficientAutonomy => {
            format!("current autonomy level is too low to use {tool_name}")
        }
        DenialReason::ImplicitVaultDenied => {
            format!("vault access is not authorized for this Run, so {tool_name} cannot be used")
        }
    }
}

const META_SKILL_TOOLS: &[&str] = &["skills_list"];

/// Evaluate one catalog tool using only explicit Run capabilities.
pub fn evaluate_tool(tool_name: &str, ctx: &ToolPolicyContext) -> ToolPolicyVerdict {
    let Some(entry) = catalog_find(tool_name) else {
        return ToolPolicyVerdict::Denied(DenialReason::NotImplemented);
    };
    evaluate_entry(entry, ctx)
}

fn evaluate_entry(entry: &ToolCatalogEntry, ctx: &ToolPolicyContext) -> ToolPolicyVerdict {
    if entry.implementation == ToolImplementationStatus::Planned {
        return ToolPolicyVerdict::Denied(DenialReason::NotImplemented);
    }
    if entry.access_level == ToolAccessLevel::Network && !ctx.web_search_enabled {
        return ToolPolicyVerdict::Denied(DenialReason::WebSearchDisabled);
    }
    if is_vault_read_tool(entry) && !ctx.allow_implicit_vault {
        return ToolPolicyVerdict::Denied(DenialReason::ImplicitVaultDenied);
    }
    if !META_SKILL_TOOLS.contains(&entry.name)
        && !entry
            .capability_affinity()
            .iter()
            .copied()
            .any(|capability| capability_allowed(capability, ctx))
        && !is_default_read_tool(entry, ctx)
    {
        return ToolPolicyVerdict::Denied(DenialReason::CapabilityMismatch);
    }
    if let Some(required) = required_autonomy(entry) {
        if ctx.autonomy_level < required {
            return ToolPolicyVerdict::Denied(DenialReason::InsufficientAutonomy);
        }
    }
    if entry.requires_confirmation {
        ToolPolicyVerdict::RequiresConfirmation
    } else {
        ToolPolicyVerdict::AutoAllowed
    }
}

fn is_vault_read_tool(entry: &ToolCatalogEntry) -> bool {
    entry.default_enabled_without_skill
        && matches!(
            entry.access_level,
            ToolAccessLevel::ReadIndex | ToolAccessLevel::ReadNoteSpan
        )
}

fn is_default_read_tool(entry: &ToolCatalogEntry, ctx: &ToolPolicyContext) -> bool {
    ctx.allow_implicit_vault
        && entry.default_enabled_without_skill
        && matches!(
            entry.access_level,
            ToolAccessLevel::ReadIndex
                | ToolAccessLevel::ReadNoteSpan
                | ToolAccessLevel::ReadProfile
        )
}

fn required_autonomy(entry: &ToolCatalogEntry) -> Option<AutonomyLevel> {
    match entry.access_level {
        ToolAccessLevel::ReadIndex
        | ToolAccessLevel::ReadNoteSpan
        | ToolAccessLevel::ReadProfile => None,
        ToolAccessLevel::Network
        | ToolAccessLevel::WriteCache
        | ToolAccessLevel::WriteMarkdown
        | ToolAccessLevel::WriteSettings => Some(AutonomyLevel::L2),
        ToolAccessLevel::ManageSkills => None,
    }
}

fn capability_allowed(capability: ToolCapabilityAffinity, ctx: &ToolPolicyContext) -> bool {
    use ToolCapabilityAffinity::*;
    match capability {
        ReadNotes | SearchNotes => ctx.allow_implicit_vault,
        WebFetch => ctx.web_search_enabled && ctx.allow_research,
        ResearchSynthesis => ctx.allow_research,
        SkillManagement => ctx.allow_skill_management,
        WriteNotes | PatchDocument => ctx.allow_writes,
        VaultOrganize => false,
    }
}

/// Return the tool names that are usable now and those requiring confirmation.
pub fn compute_available_tools(ctx: &ToolPolicyContext) -> (Vec<String>, Vec<String>) {
    let mut auto_allowed = Vec::new();
    let mut requires_confirmation = Vec::new();
    for entry in TOOL_CATALOG.iter() {
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

/// Whether a tool may be shown to a Run executor.
pub fn is_tool_exposed(tool_name: &str, ctx: &ToolPolicyContext) -> bool {
    !matches!(evaluate_tool(tool_name, ctx), ToolPolicyVerdict::Denied(_))
}

/// Whether a shown tool requires explicit user confirmation.
pub fn tool_requires_confirmation(tool_name: &str, ctx: &ToolPolicyContext) -> bool {
    matches!(
        evaluate_tool(tool_name, ctx),
        ToolPolicyVerdict::RequiresConfirmation
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_context() -> ToolPolicyContext {
        ToolPolicyContext {
            autonomy_level: AutonomyLevel::L2,
            allow_implicit_vault: true,
            ..ToolPolicyContext::default()
        }
    }

    #[test]
    fn read_tools_are_available_from_explicit_capabilities() {
        assert!(is_tool_exposed("search_hybrid", &read_context()));
        assert!(is_tool_exposed("read_note", &read_context()));
        assert!(!is_tool_exposed("insert_text_at_cursor", &read_context()));
    }

    #[test]
    fn vault_read_tools_are_denied_when_implicit_vault_is_disabled() {
        let ctx = ToolPolicyContext {
            autonomy_level: AutonomyLevel::L2,
            allow_implicit_vault: false,
            ..ToolPolicyContext::default()
        };
        assert_eq!(
            evaluate_tool("read_note", &ctx),
            ToolPolicyVerdict::Denied(DenialReason::ImplicitVaultDenied)
        );
        assert_eq!(
            evaluate_tool("search_hybrid", &ctx),
            ToolPolicyVerdict::Denied(DenialReason::ImplicitVaultDenied)
        );
    }

    #[test]
    fn write_capability_requires_explicit_authorization_and_confirmation() {
        let ctx = ToolPolicyContext {
            allow_writes: true,
            ..read_context()
        };
        assert_eq!(
            evaluate_tool("insert_text_at_cursor", &ctx),
            ToolPolicyVerdict::RequiresConfirmation
        );
    }

    #[test]
    fn network_capability_requires_explicit_research_authorization() {
        let blocked = ToolPolicyContext {
            web_search_enabled: false,
            ..read_context()
        };
        assert_eq!(
            evaluate_tool("web_search", &blocked),
            ToolPolicyVerdict::Denied(DenialReason::WebSearchDisabled)
        );
    }
}
