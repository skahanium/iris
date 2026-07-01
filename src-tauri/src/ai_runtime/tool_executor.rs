//! Tool definitions, permission checks, and execution dispatch.
//!
//! All tool definitions live here. The ToolExecutor handles:
//! 1. Building the capability-policy driven tool surface
//! 2. Formatting tool specs for LLM function-calling
//! 3. Routing confirmed tool calls to Rust command handlers

use crate::ai_runtime::tool_catalog::{ToolImplementationStatus, TOOL_CATALOG};
use crate::ai_runtime::tool_dispatch::is_exposable_tool;
use crate::ai_runtime::tool_policy::{self, DenialReason, ToolPolicyContext, ToolPolicyVerdict};
use crate::ai_runtime::{AiScene, ToolSpec};

/// Filters applied when building the tool surface for LLM / IPC listing.
#[derive(Debug, Clone, Copy, Default)]
pub struct ToolSurfaceFilter {
    pub web_search_enabled: bool,
    /// Sub-agent nesting depth (hide spawn_subagent when >= 2).
    pub depth: u32,
    /// When true, only tools that do not require user confirmation (research loop).
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

    /// Tools exposed to the model: task policy + dispatch/harness handler + runtime filters.
    pub fn tools_for_surface(&self, scene: AiScene, filter: ToolSurfaceFilter) -> Vec<ToolSpec> {
        let ctx = ToolPolicyContext {
            task_policy: None,
            scene,
            autonomy_level: scene.autonomy_level(),
            web_search_enabled: filter.web_search_enabled,
            depth: filter.depth,
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
                scene_affinity: entry.scene_affinity.to_vec(),
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
    use crate::ai_runtime::{
        agent_task::AgentTaskKind,
        agent_task_policy::{AgentTaskPolicy, AgentTaskPolicyInput, AgentTaskScope},
    };
    use crate::ai_types::{AgentIntent, AutonomyLevel};

    #[test]
    fn registry_catalog_entries_are_policy_neutral() {
        let reg = ToolRegistry::new();
        let names: Vec<&str> = reg
            .catalog_entries()
            .iter()
            .map(|t| t.name.as_str())
            .collect();

        assert!(names.contains(&"search_hybrid"));
        assert!(names.contains(&"insert_text_at_cursor"));
    }

    #[test]
    fn write_tools_require_confirmation() {
        let reg = ToolRegistry::new();
        assert!(reg.requires_confirmation("insert_text_at_cursor"));
        assert!(reg.requires_confirmation("replace_selection"));
        assert!(reg.requires_confirmation("add_tags"));
        assert!(reg.requires_confirmation("update_user_rule"));
    }

    #[test]
    fn read_tools_no_confirmation() {
        let reg = ToolRegistry::new();
        assert!(!reg.requires_confirmation("search_hybrid"));
        assert!(!reg.requires_confirmation("get_regulation"));
    }

    #[test]
    fn unknown_tool_defaults_to_confirmation() {
        let reg = ToolRegistry::new();
        assert!(reg.requires_confirmation("nonexistent_tool"));
    }

    #[test]
    fn write_markdown_forbidden_at_l1() {
        let reg = ToolRegistry::new();
        let l2_ctx = ToolPolicyContext {
            task_policy: None,
            scene: AiScene::DraftingAssist,
            autonomy_level: AutonomyLevel::L2,
            web_search_enabled: true,
            depth: 0,
        };
        let l1_ctx = ToolPolicyContext {
            autonomy_level: AutonomyLevel::L1,
            ..l2_ctx.clone()
        };
        assert!(reg
            .check_tool_policy("insert_text_at_cursor", &l2_ctx)
            .is_ok());
        assert!(reg
            .check_tool_policy("insert_text_at_cursor", &l1_ctx)
            .is_err());
    }

    #[test]
    fn ask_notes_task_policy_blocks_write_capability() {
        let reg = ToolRegistry::new();
        let ctx = ToolPolicyContext {
            task_policy: Some(AgentTaskPolicy::from_input(AgentTaskPolicyInput {
                intent: AgentIntent::AskNotes,
                task_kind: AgentTaskKind::Lightweight,
                scope: AgentTaskScope::Vault,
                web_authorized: true,
                has_attachments: false,
                write_permission_required: false,
                research_depth: 0,
            })),
            scene: AiScene::KnowledgeLookup,
            autonomy_level: AutonomyLevel::L2,
            web_search_enabled: true,
            depth: 0,
        };
        assert!(reg
            .check_tool_policy("insert_text_at_cursor", &ctx)
            .is_err());
    }

    #[test]
    fn auto_tools_excludes_confirmation_tools() {
        let reg = ToolRegistry::new();
        let auto = reg.confirmation_free_catalog_entries();
        let names: Vec<&str> = auto.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"search_hybrid"));
        assert!(!names.contains(&"insert_text_at_cursor")); // requires confirmation
    }

    #[test]
    fn tools_for_surface_includes_confirmation_gated_write_tools() {
        let reg = ToolRegistry::new();
        let tools = reg.tools_for_surface(AiScene::DraftingAssist, ToolSurfaceFilter::default());
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        // Write tools are exposed in drafting mode, but the harness pauses for confirmation.
        assert!(names.contains(&"insert_text_at_cursor"));
        assert!(names.contains(&"replace_selection"));
        assert!(names.contains(&"search_hybrid"));
    }

    #[test]
    fn tools_for_surface_hides_web_when_disabled() {
        let reg = ToolRegistry::new();
        let tools = reg.tools_for_surface(
            AiScene::KnowledgeLookup,
            ToolSurfaceFilter {
                web_search_enabled: false,
                ..Default::default()
            },
        );
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(!names.contains(&"web_search"));
        assert!(!names.contains(&"fetch_web_page"));
    }

    #[test]
    fn policy_surface_uses_tool_policy_context() {
        let reg = ToolRegistry::new();
        let ctx = ToolPolicyContext {
            task_policy: None,
            scene: AiScene::KnowledgeLookup,
            autonomy_level: AutonomyLevel::L1,
            web_search_enabled: true,
            depth: 0,
        };
        let tools = reg.tools_for_policy_surface(&ctx, false);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

        assert!(names.contains(&"search_hybrid"));
        assert!(names.contains(&"web_search"));
        assert!(!names.contains(&"fetch_web_page"));
    }

    #[test]
    fn ordinary_policy_surface_hides_low_level_fetch_tools() {
        let reg = ToolRegistry::new();
        let ctx = ToolPolicyContext {
            task_policy: None,
            scene: AiScene::ResearchSynthesis,
            autonomy_level: AutonomyLevel::L3,
            web_search_enabled: true,
            depth: 0,
        };
        let tools = reg.tools_for_policy_surface(&ctx, false);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

        assert!(names.contains(&"web_search"));
        assert!(!names.contains(&"fetch_web_page"));
        assert!(!names.contains(&"web_fetch_batch"));
        assert!(!names.contains(&"readability_fetch"));
        assert!(!names.contains(&"rendered_fetch"));
    }

    #[test]
    fn policy_surface_follows_task_policy() {
        let reg = ToolRegistry::new();
        let task_policy = AgentTaskPolicy::from_input(AgentTaskPolicyInput {
            intent: AgentIntent::Write,
            task_kind: AgentTaskKind::Lightweight,
            scope: AgentTaskScope::Vault,
            web_authorized: true,
            has_attachments: false,
            write_permission_required: true,
            research_depth: 0,
        });
        let ctx = ToolPolicyContext {
            task_policy: Some(task_policy),
            scene: AiScene::DraftingAssist,
            autonomy_level: AutonomyLevel::L2,
            web_search_enabled: true,
            depth: 0,
        };
        let tools = reg.tools_for_policy_surface(&ctx, false);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

        assert!(names.contains(&"insert_text_at_cursor"));
        assert!(names.contains(&"replace_selection"));
    }

    #[test]
    fn every_exposed_tool_has_handler() {
        use crate::ai_runtime::tool_dispatch::{
            is_exposable_tool, DISPATCHABLE_TOOL_NAMES, HARNESS_ONLY_TOOL_NAMES,
        };
        let reg = ToolRegistry::new();
        for scene in [
            AiScene::KnowledgeLookup,
            AiScene::DraftingAssist,
            AiScene::ResearchSynthesis,
        ] {
            let tools = reg.tools_for_surface(scene, ToolSurfaceFilter::default());
            for t in tools {
                assert!(
                    is_exposable_tool(&t.name),
                    "exposed tool {} must be dispatchable or harness-only",
                    t.name
                );
                if !t.requires_confirmation && !HARNESS_ONLY_TOOL_NAMES.contains(&t.name.as_str()) {
                    assert!(
                        DISPATCHABLE_TOOL_NAMES.contains(&t.name.as_str()),
                        "auto tool {} must have dispatch handler",
                        t.name
                    );
                }
            }
        }
    }

    // 鈹€鈹€ Policy integration tests 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

    #[test]
    fn policy_allows_read_tool_in_knowledge() {
        let reg = ToolRegistry::new();
        let ctx = ToolPolicyContext {
            task_policy: None,
            scene: AiScene::KnowledgeLookup,
            autonomy_level: AutonomyLevel::L2,
            web_search_enabled: true,
            depth: 0,
        };
        assert!(reg.check_tool_policy("search_hybrid", &ctx).is_ok());
        assert!(reg.check_tool_policy("read_note", &ctx).is_ok());
    }

    #[test]
    fn policy_denies_write_tool_in_knowledge() {
        let reg = ToolRegistry::new();
        let ctx = ToolPolicyContext {
            task_policy: None,
            scene: AiScene::KnowledgeLookup,
            autonomy_level: AutonomyLevel::L2,
            web_search_enabled: true,
            depth: 0,
        };
        assert!(reg
            .check_tool_policy("insert_text_at_cursor", &ctx)
            .is_err());
    }

    #[test]
    fn policy_allows_write_tool_in_drafting() {
        let reg = ToolRegistry::new();
        let ctx = ToolPolicyContext {
            task_policy: None,
            scene: AiScene::DraftingAssist,
            autonomy_level: AutonomyLevel::L2,
            web_search_enabled: true,
            depth: 0,
        };
        assert!(reg.check_tool_policy("insert_text_at_cursor", &ctx).is_ok());
    }

    #[test]
    fn policy_denies_write_at_l1() {
        let reg = ToolRegistry::new();
        let ctx = ToolPolicyContext {
            task_policy: None,
            scene: AiScene::DraftingAssist,
            autonomy_level: AutonomyLevel::L1,
            web_search_enabled: true,
            depth: 0,
        };
        assert!(reg
            .check_tool_policy("insert_text_at_cursor", &ctx)
            .is_err());
    }

    #[test]
    fn policy_registry_count_matches_catalog() {
        let reg = ToolRegistry::new();
        // Registry should have all non-planned tools from catalog
        let catalog_count = TOOL_CATALOG
            .iter()
            .filter(|e| e.implementation != ToolImplementationStatus::Planned)
            .count();
        assert_eq!(
            reg.tools.len(),
            catalog_count,
            "registry count should match catalog non-planned count"
        );
    }

    #[test]
    fn catalog_and_registry_agree_on_tools() {
        let reg = ToolRegistry::new();
        for entry in TOOL_CATALOG.iter() {
            if entry.implementation == ToolImplementationStatus::Planned {
                continue;
            }
            let spec = reg.find(entry.name);
            assert!(
                spec.is_some(),
                "catalog tool '{}' not found in registry",
                entry.name
            );
            let spec = spec.unwrap();
            assert_eq!(spec.requires_confirmation, entry.requires_confirmation);
            assert_eq!(spec.access_level, entry.access_level);
        }
    }
}
