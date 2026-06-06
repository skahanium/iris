//! Tool definitions, permission checks, and execution dispatch.
//!
//! All tool definitions live here. The ToolExecutor handles:
//! 1. Filtering available tools by scene and access level
//! 2. Formatting tool specs for LLM function-calling
//! 3. Routing confirmed tool calls to Rust command handlers

use std::time::Instant;

use crate::ai_runtime::tool_catalog::{ToolImplementationStatus, TOOL_CATALOG};
use crate::ai_runtime::tool_dispatch::is_exposable_tool;
use crate::ai_runtime::tool_policy::{self, DenialReason, ToolPolicyContext, ToolPolicyVerdict};
use crate::ai_runtime::{AiScene, AutonomyLevel, ToolAccessLevel, ToolCallResult, ToolSpec};
use crate::error::AppResult;

/// Filters applied when building the tool surface for LLM / IPC listing.
#[derive(Debug, Clone, Copy, Default)]
pub struct ToolSurfaceFilter {
    pub web_search_enabled: bool,
    /// Sub-agent nesting depth (hide spawn_subagent when >= 2).
    pub depth: u32,
    /// When true, only tools that do not require user confirmation (research loop).
    pub only_auto: bool,
}

// ─── Tool Registry ───────────────────────────────────────

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

    /// 返回指定场景可用的工具列表。
    pub fn for_scene(&self, scene: AiScene) -> Vec<&ToolSpec> {
        self.tools
            .iter()
            .filter(|t| t.scene_allowlist.is_empty() || t.scene_allowlist.contains(&scene))
            .collect()
    }

    /// Tools exposed to the model: scene allowlist + dispatch/harness handler + runtime filters.
    pub fn tools_for_surface(&self, scene: AiScene, filter: ToolSurfaceFilter) -> Vec<ToolSpec> {
        let ctx = ToolPolicyContext {
            scene,
            autonomy_level: crate::ai_runtime::resolve_scene(scene).autonomy_level,
            web_search_enabled: filter.web_search_enabled,
            skill_allowed_tools: vec![],
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

    /// 返回指定场景中不需要用户确认的工具（只读自动执行）。
    pub fn auto_tools_for_scene(&self, scene: AiScene) -> Vec<&ToolSpec> {
        self.for_scene(scene)
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
            .unwrap_or(true) // 未知工具默认需要确认
    }

    /// 执行指定工具并记录耗时。
    ///
    /// 调用方负责填充 `tokens_used`（如果可用）。
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> AppResult<ToolCallResult> {
        let start = Instant::now();

        let spec = self.find(tool_name);
        if spec.is_none() {
            let duration_ms = start.elapsed().as_millis() as u64;
            return Ok(ToolCallResult {
                tool_name: tool_name.to_string(),
                success: false,
                output: serde_json::Value::Null,
                duration_ms,
                tokens_used: None,
                error: Some(format!("unknown tool: {tool_name}")),
            });
        }

        // Actual dispatch is handled by the caller (model_gateway / agent loop).
        // This method provides the timing wrapper and result structure.
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ToolCallResult {
            tool_name: tool_name.to_string(),
            success: true,
            output: args,
            duration_ms,
            tokens_used: None,
            error: None,
        })
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

    // ─── private ─────────────────────────────────────

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
                scene_allowlist: entry.scene_affinity.to_vec(),
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

// ─── Permission Check ────────────────────────────────────

/// 检查工具在当前场景和自治等级下是否允许执行。
pub fn check_tool_permission(
    tool: &ToolSpec,
    scene: AiScene,
    allowed_level: AutonomyLevel,
) -> Result<(), ToolPermissionError> {
    // 1. 场景白名单检查
    if !tool.scene_allowlist.is_empty() && !tool.scene_allowlist.contains(&scene) {
        return Err(ToolPermissionError::SceneNotAllowed {
            tool: tool.name.clone(),
            scene,
        });
    }

    // 2. 联网只读工具（web_search）在 L2 及以上可用
    if tool.access_level == ToolAccessLevel::Network && allowed_level < AutonomyLevel::L2 {
        return Err(ToolPermissionError::InsufficientAutonomy {
            tool: tool.name.clone(),
            required: AutonomyLevel::L2,
            current: allowed_level,
        });
    }

    // 3. WriteMarkdown + WriteSettings 在 L1 下禁止
    if matches!(
        tool.access_level,
        ToolAccessLevel::WriteMarkdown | ToolAccessLevel::WriteSettings
    ) && allowed_level < AutonomyLevel::L2
    {
        return Err(ToolPermissionError::InsufficientAutonomy {
            tool: tool.name.clone(),
            required: AutonomyLevel::L2,
            current: allowed_level,
        });
    }

    Ok(())
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ToolPermissionError {
    #[error("tool '{tool}' not allowed in scene {scene:?}")]
    SceneNotAllowed { tool: String, scene: AiScene },
    #[error("tool '{tool}' requires autonomy {required:?}, current is {current:?}")]
    InsufficientAutonomy {
        tool: String,
        required: AutonomyLevel,
        current: AutonomyLevel,
    },
}

/// Error from the new policy engine (Phase 2).
#[derive(Debug, Clone, thiserror::Error)]
#[error("tool '{tool}' denied by policy: {reason:?}")]
pub struct ToolPolicyDeniedError {
    pub tool: String,
    pub reason: DenialReason,
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_filters_by_scene() {
        let reg = ToolRegistry::new();
        let tools = reg.for_scene(AiScene::KnowledgeLookup);
        // KnowledgeLookup should have search tools + get_regulation + get_block_links
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"search_hybrid"));
        assert!(names.contains(&"get_regulation"));
        assert!(names.contains(&"get_block_links"));
        // — but NOT insert_text_at_cursor (DraftingAssist only)
        assert!(!names.contains(&"insert_text_at_cursor"));
    }

    #[test]
    fn drafting_scene_has_write_tools() {
        let reg = ToolRegistry::new();
        let tools = reg.for_scene(AiScene::DraftingAssist);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"insert_text_at_cursor"));
        assert!(names.contains(&"replace_selection"));
        assert!(names.contains(&"search_hybrid"));
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
        let insert = reg.find("insert_text_at_cursor").unwrap();
        assert!(check_tool_permission(insert, AiScene::DraftingAssist, AutonomyLevel::L2).is_ok());
        assert!(check_tool_permission(insert, AiScene::DraftingAssist, AutonomyLevel::L1).is_err());
    }

    #[test]
    fn tool_not_in_scene_allowlist_blocked() {
        let reg = ToolRegistry::new();
        let insert = reg.find("insert_text_at_cursor").unwrap();
        // insert_text_at_cursor only for DraftingAssist
        assert!(
            check_tool_permission(insert, AiScene::KnowledgeLookup, AutonomyLevel::L2).is_err()
        );
    }

    #[test]
    fn auto_tools_excludes_confirmation_tools() {
        let reg = ToolRegistry::new();
        let auto = reg.auto_tools_for_scene(AiScene::DraftingAssist);
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
            scene: AiScene::KnowledgeLookup,
            autonomy_level: AutonomyLevel::L1,
            web_search_enabled: true,
            skill_allowed_tools: vec![],
            depth: 0,
        };
        let tools = reg.tools_for_policy_surface(&ctx, false);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

        assert!(names.contains(&"search_hybrid"));
        assert!(!names.contains(&"web_search"));
        assert!(!names.contains(&"fetch_web_page"));
    }

    #[test]
    fn policy_surface_intersects_skill_allowed_tools() {
        let reg = ToolRegistry::new();
        let ctx = ToolPolicyContext {
            scene: AiScene::DraftingAssist,
            autonomy_level: AutonomyLevel::L2,
            web_search_enabled: true,
            skill_allowed_tools: vec!["insert_text_at_cursor".to_string()],
            depth: 0,
        };
        let tools = reg.tools_for_policy_surface(&ctx, false);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

        assert!(names.contains(&"insert_text_at_cursor"));
        assert!(!names.contains(&"replace_selection"));
    }

    #[test]
    fn every_exposed_tool_has_handler() {
        use crate::ai_runtime::tool_dispatch::{
            is_exposable_tool, DISPATCHABLE_TOOL_NAMES, HARNESS_ONLY_TOOL_NAMES,
        };
        let reg = ToolRegistry::new();
        for scene in [
            AiScene::KnowledgeLookup,
            AiScene::ExemplarLearning,
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

    // ── Policy integration tests ──────────────────────────

    #[test]
    fn policy_allows_read_tool_in_knowledge() {
        let reg = ToolRegistry::new();
        let ctx = ToolPolicyContext {
            scene: AiScene::KnowledgeLookup,
            autonomy_level: AutonomyLevel::L2,
            web_search_enabled: true,
            skill_allowed_tools: vec![],
            depth: 0,
        };
        assert!(reg.check_tool_policy("search_hybrid", &ctx).is_ok());
        assert!(reg.check_tool_policy("read_note", &ctx).is_ok());
    }

    #[test]
    fn policy_denies_write_tool_in_knowledge() {
        let reg = ToolRegistry::new();
        let ctx = ToolPolicyContext {
            scene: AiScene::KnowledgeLookup,
            autonomy_level: AutonomyLevel::L2,
            web_search_enabled: true,
            skill_allowed_tools: vec![],
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
            scene: AiScene::DraftingAssist,
            autonomy_level: AutonomyLevel::L2,
            web_search_enabled: true,
            skill_allowed_tools: vec![],
            depth: 0,
        };
        assert!(reg.check_tool_policy("insert_text_at_cursor", &ctx).is_ok());
    }

    #[test]
    fn policy_denies_write_at_l1() {
        let reg = ToolRegistry::new();
        let ctx = ToolPolicyContext {
            scene: AiScene::DraftingAssist,
            autonomy_level: AutonomyLevel::L1,
            web_search_enabled: true,
            skill_allowed_tools: vec![],
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
