//! Tool definitions, permission checks, and execution dispatch.
//!
//! All tool definitions live here. The ToolExecutor handles:
//! 1. Building the capability-policy driven tool surface
//! 2. Formatting tool specs for LLM function-calling
//! 3. Routing confirmed tool calls to Rust command handlers

use crate::ai_runtime::tool_catalog::{ToolImplementationStatus, TOOL_CATALOG};
use crate::ai_runtime::tool_dispatch::is_exposable_tool;
use crate::ai_runtime::tool_policy::{self, DenialReason, ToolPolicyContext, ToolPolicyVerdict};
use crate::ai_runtime::{AutonomyLevel, ToolSpec};

/// Filters applied when building the tool surface for LLM / IPC listing.
#[derive(Debug, Clone, Copy, Default)]
pub struct ToolSurfaceFilter {
    pub web_search_enabled: bool,
    pub allow_writes: bool,
    pub allow_research: bool,
    pub allow_skill_management: bool,
    /// When true, only tools that do not require user confirmation are returned.
    pub only_auto: bool,
}

// Tool Registry

/// 内置工具注册表。所有工具在此声明。
pub struct ToolRegistry {
    tools: Vec<ToolSpec>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: Self::builtin_tools(),
        }
    }

    /// Catalog view before policy filtering.
    ///
    /// Availability is decided by ToolPolicy.
    pub fn catalog_entries(&self) -> Vec<&ToolSpec> {
        self.tools.iter().collect()
    }

    /// Tools exposed to a Run executor from explicit capabilities only.
    pub fn tools_for_surface(&self, filter: ToolSurfaceFilter) -> Vec<ToolSpec> {
        let ctx = ToolPolicyContext {
            autonomy_level: AutonomyLevel::L2,
            web_search_enabled: filter.web_search_enabled,
            allow_writes: filter.allow_writes,
            allow_research: filter.allow_research,
            allow_skill_management: filter.allow_skill_management,
        };
        self.tools_for_policy_surface(&ctx, filter.only_auto)
    }

    /// Tools exposed to the model after evaluating the full ToolPolicy.
    pub fn tools_for_policy_surface(
        &self,
        ctx: &ToolPolicyContext,
        only_auto: bool,
    ) -> Vec<ToolSpec> {
        self.tools
            .iter()
            .filter(|t| is_exposable_tool(&t.name))
            .filter(|t| {
                let verdict = tool_policy::evaluate_tool(&t.name, ctx);
                match verdict {
                    ToolPolicyVerdict::AutoAllowed => true,
                    ToolPolicyVerdict::RequiresConfirmation => !only_auto,
                    ToolPolicyVerdict::Denied(_) => false,
                }
            })
            .cloned()
            .collect()
    }

    /// Catalog entries that do not need user confirmation.
    pub fn confirmation_free_catalog_entries(&self) -> Vec<&ToolSpec> {
        self.catalog_entries()
            .into_iter()
            .filter(|t| !t.requires_confirmation)
            .collect()
    }

    /// 按名称查找工具。
    pub fn find(&self, name: &str) -> Option<&ToolSpec> {
        self.tools.iter().find(|t| t.name == name)
    }

    /// 判断指定工具的写入是否需要确认。
    pub fn requires_confirmation(&self, tool_name: &str) -> bool {
        self.find(tool_name)
            .map(|t| t.requires_confirmation)
            .unwrap_or(true)
    }

    /// Check tool permission using the new policy engine (Phase 2).
    ///
    /// Returns `Ok(())` if the tool is allowed (auto or confirmation-required),
    /// and `Err(...)` if it's denied by the policy.
    pub fn check_tool_policy(
        &self,
        tool_name: &str,
        ctx: &ToolPolicyContext,
    ) -> Result<(), ToolPolicyDeniedError> {
        match tool_policy::evaluate_tool(tool_name, ctx) {
            ToolPolicyVerdict::AutoAllowed | ToolPolicyVerdict::RequiresConfirmation => Ok(()),
            ToolPolicyVerdict::Denied(reason) => Err(ToolPolicyDeniedError {
                tool: tool_name.to_string(),
                reason,
            }),
        }
    }

    // private

    /// Build tool list from the global `TOOL_CATALOG` (single source of truth).
    fn builtin_tools() -> Vec<ToolSpec> {
        TOOL_CATALOG
            .iter()
            .filter(|e| e.implementation != ToolImplementationStatus::Planned)
            .map(|entry| ToolSpec {
                name: entry.name.to_string(),
                description: entry.description.to_string(),
                input_schema: entry.input_schema.clone(),
                access_level: entry.access_level,
                requires_confirmation: entry.requires_confirmation,
                max_results: entry.max_results,
                capability_affinity: entry.capability_affinity(),
            })
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Error from the new policy engine (Phase 2).
#[derive(Debug, Clone, thiserror::Error)]
#[error("tool '{tool}' denied by policy: {reason:?}")]
pub struct ToolPolicyDeniedError {
    pub tool: String,
    pub reason: DenialReason,
}

// 鈹€鈹€鈹€ Tests 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_entries_are_policy_neutral() {
        let registry = ToolRegistry::new();
        assert!(registry.find("search_hybrid").is_some());
        assert!(registry.find("insert_text_at_cursor").is_some());
    }

    #[test]
    fn explicit_run_capabilities_control_write_exposure() {
        let registry = ToolRegistry::new();
        let read_only = registry.tools_for_surface(ToolSurfaceFilter::default());
        assert!(!read_only
            .iter()
            .any(|tool| tool.name == "insert_text_at_cursor"));

        let writable = registry.tools_for_surface(ToolSurfaceFilter {
            allow_writes: true,
            ..ToolSurfaceFilter::default()
        });
        assert!(writable
            .iter()
            .any(|tool| tool.name == "insert_text_at_cursor"));
    }

    #[test]
    fn harness_only_controls_are_never_exposed_to_the_model() {
        let registry = ToolRegistry::new();
        let surface = registry.tools_for_surface(ToolSurfaceFilter {
            web_search_enabled: true,
            allow_writes: true,
            allow_research: true,
            allow_skill_management: true,
            only_auto: false,
        });

        assert!(!surface.iter().any(|tool| tool.name == "spawn_subagent"));
        assert!(!surface.iter().any(|tool| tool.name == "conclude_reasoning"));
    }
}
