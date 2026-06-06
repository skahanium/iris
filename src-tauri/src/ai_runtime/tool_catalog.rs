//! ToolCatalog — single source of truth for all built-in tool definitions.
//!
//! Every tool exposed to the LLM or harness must be registered here.
//! `ToolRegistry` is built from this catalog; `ToolPolicy` evaluates
//! availability from it. No tool definition should live outside this module.

use std::sync::LazyLock;

use crate::ai_runtime::{AiScene, ToolAccessLevel};

/// Implementation status of a catalog entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolImplementationStatus {
    /// Has a real handler in `dispatch_tool_inner`.
    Dispatchable,
    /// Handled inside the harness loop (e.g. `spawn_subagent`, `conclude_reasoning`).
    HarnessOnly,
    /// Registered for future implementation; not currently exposed.
    Planned,
}

/// A single entry in the global tool catalog.
#[derive(Debug, Clone)]
pub struct ToolCatalogEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: serde_json::Value,
    pub access_level: ToolAccessLevel,
    pub requires_confirmation: bool,
    pub implementation: ToolImplementationStatus,
    /// Whether this tool is available when no skill is active.
    pub default_enabled_without_skill: bool,
    /// Scenes where this tool is naturally relevant (superset of old scene_allowlist).
    pub scene_affinity: &'static [AiScene],
    /// Optional cap on result count passed to the retrieval layer.
    pub max_results: Option<u32>,
}

/// The complete built-in tool catalog. Add new tools here only.
pub static TOOL_CATALOG: LazyLock<Vec<ToolCatalogEntry>> = LazyLock::new(|| {
    vec![
        // ─── Read-only retrieval ────────────────────────────────
        ToolCatalogEntry {
            name: "search_hybrid",
            description: "混合搜索：FTS + 向量 + 分数融合，搜索知识库中与查询相关的内容",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "搜索查询"}
                },
                "required": ["query"]
            }),
            access_level: ToolAccessLevel::ReadIndex,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[
                AiScene::KnowledgeLookup,
                AiScene::ExemplarLearning,
                AiScene::DraftingAssist,
                AiScene::ResearchSynthesis,
            ],
            max_results: Some(20),
        },
        ToolCatalogEntry {
            name: "search_semantic",
            description: "语义搜索知识库，查找与查询语义相似的笔记片段",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "default": 10}
                },
                "required": ["query"]
            }),
            access_level: ToolAccessLevel::ReadIndex,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: Some(20),
        },
        ToolCatalogEntry {
            name: "search_keyword",
            description: "关键词全文搜索，精确匹配特定术语或短语",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "default": 10}
                },
                "required": ["query"]
            }),
            access_level: ToolAccessLevel::ReadIndex,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: Some(20),
        },
        ToolCatalogEntry {
            name: "get_regulation",
            description: "根据法规名称和条款号获取精确条款原文",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "regulation_name": {"type": "string", "description": "法规名称"},
                    "article": {"type": "string", "description": "条号，如'第六条'"},
                    "paragraph": {"type": "string", "description": "款号，如'第一款'"}
                },
                "required": ["regulation_name", "article"]
            }),
            access_level: ToolAccessLevel::ReadNoteSpan,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[
                AiScene::KnowledgeLookup,
                AiScene::DraftingAssist,
                AiScene::ResearchSynthesis,
            ],
            max_results: Some(1),
        },
        ToolCatalogEntry {
            name: "get_context_packets",
            description: "返回当前会话已组装的证据包列表",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            access_level: ToolAccessLevel::ReadIndex,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "get_block_links",
            description: "获取笔记的显式或已确认块级链接",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "note_path": {"type": "string"}
                },
                "required": ["note_path"]
            }),
            access_level: ToolAccessLevel::ReadIndex,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[AiScene::KnowledgeLookup, AiScene::ResearchSynthesis],
            max_results: Some(50),
        },
        // ─── Network (read-only but external) ───────────────────
        ToolCatalogEntry {
            name: "web_search",
            description:
                "联网搜索实时信息；无需确认，直接调用。结果应与本地检索证据交叉引用、相互印证。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "搜索查询"}
                },
                "required": ["query"]
            }),
            access_level: ToolAccessLevel::Network,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[
                AiScene::KnowledgeLookup,
                AiScene::DraftingAssist,
                AiScene::ResearchSynthesis,
            ],
            max_results: Some(8),
        },
        ToolCatalogEntry {
            name: "fetch_web_page",
            description: "打开单个 HTTPS 网页并提取正文片段（需用户确认）。\
            仅在 web_search 或本地检索已给出 URL 且摘要不足时使用；\
            每轮最多 1～2 次，禁止批量爬取。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "HTTPS 页面 URL"},
                    "max_chars": {"type": "integer", "description": "最大正文字符数，默认 24000"},
                    "reason": {"type": "string", "description": "抓取原因（供用户确认）"}
                },
                "required": ["url"]
            }),
            access_level: ToolAccessLevel::Network,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[
                AiScene::KnowledgeLookup,
                AiScene::DraftingAssist,
                AiScene::ResearchSynthesis,
            ],
            max_results: Some(2),
        },
        // ─── Note reading ───────────────────────────────────────
        ToolCatalogEntry {
            name: "read_note",
            description: "读取指定笔记的 Markdown 全文（可截断）",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "max_chars": {"type": "integer", "default": 12000}
                },
                "required": ["path"]
            }),
            access_level: ToolAccessLevel::ReadNoteSpan,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "list_vault",
            description: "列出知识库中的笔记路径与标题",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "prefix": {"type": "string", "description": "路径前缀过滤"},
                    "limit": {"type": "integer", "default": 50}
                }
            }),
            access_level: ToolAccessLevel::ReadIndex,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: Some(100),
        },
        ToolCatalogEntry {
            name: "get_outline",
            description: "提取笔记的标题大纲（Markdown 标题层级）",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                },
                "required": ["path"]
            }),
            access_level: ToolAccessLevel::ReadNoteSpan,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "get_backlinks",
            description: "获取链接到指定笔记的反向链接",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                },
                "required": ["path"]
            }),
            access_level: ToolAccessLevel::ReadIndex,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: Some(50),
        },
        // ─── Harness-only (loop control) ────────────────────────
        ToolCatalogEntry {
            name: "conclude_reasoning",
            description:
                "当你认为已收集到足够信息、可以回答用户问题时调用，结束工具循环并生成最终回答。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "summary": {"type": "string", "description": "简要说明为何可以结束"}
                }
            }),
            access_level: ToolAccessLevel::ReadIndex,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::HarnessOnly,
            default_enabled_without_skill: true,
            scene_affinity: &[
                AiScene::KnowledgeLookup,
                AiScene::ExemplarLearning,
                AiScene::DraftingAssist,
                AiScene::ResearchSynthesis,
            ],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "spawn_subagent",
            description: "将子任务委派给独立 agent 并行执行。适用于多角度检索、子问题分解。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task": {"type": "string", "description": "子任务完整描述"},
                    "context_hint": {"type": "string", "description": "可选额外上下文"},
                    "max_rounds": {"type": "integer", "description": "子任务最大轮次", "default": 2}
                },
                "required": ["task"]
            }),
            access_level: ToolAccessLevel::ReadIndex,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::HarnessOnly,
            default_enabled_without_skill: true,
            scene_affinity: &[
                AiScene::KnowledgeLookup,
                AiScene::DraftingAssist,
                AiScene::ResearchSynthesis,
            ],
            max_results: None,
        },
        // ─── Write operations (all require confirmation) ────────
        // These are confirmed via the harness UI, not exposed directly to the model.
        // Harness handles confirmation → PendingToolCall → dispatch after user approval.
        ToolCatalogEntry {
            name: "insert_text_at_cursor",
            description: "在编辑器光标位置插入文本",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "text": {"type": "string", "description": "要插入的文本"},
                    "base_content_hash": {"type": "string", "description": "当前 Markdown 内容 hash（如可用）"},
                    "range": {"type": "object", "description": "插入位置范围（如可用）"},
                    "risk_level": {"type": "string", "enum": ["low", "medium", "high"]}
                },
                "required": ["text"]
            }),
            access_level: ToolAccessLevel::WriteMarkdown,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::HarnessOnly,
            default_enabled_without_skill: false,
            scene_affinity: &[AiScene::DraftingAssist],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "replace_selection",
            description: "替换编辑器当前选中文本",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "replacement": {"type": "string", "description": "替换文本"},
                    "base_content_hash": {"type": "string", "description": "当前 Markdown 内容 hash（如可用）"},
                    "range": {"type": "object", "description": "替换范围（如可用）"},
                    "risk_level": {"type": "string", "enum": ["low", "medium", "high"]}
                },
                "required": ["replacement"]
            }),
            access_level: ToolAccessLevel::WriteMarkdown,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::HarnessOnly,
            default_enabled_without_skill: false,
            scene_affinity: &[AiScene::DraftingAssist],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "add_tags",
            description: "为笔记添加标签（修改 frontmatter 或正文标签）",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "note_path": {"type": "string"},
                    "tags": {"type": "array", "items": {"type": "string"}}
                },
                "required": ["note_path", "tags"]
            }),
            access_level: ToolAccessLevel::WriteMarkdown,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::HarnessOnly,
            default_enabled_without_skill: false,
            scene_affinity: &[AiScene::ExemplarLearning],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "confirm_block_link",
            description: "确认一条 AI 建议的隐含块级链接",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "link_id": {"type": "integer"}
                },
                "required": ["link_id"]
            }),
            access_level: ToolAccessLevel::WriteCache,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::HarnessOnly,
            default_enabled_without_skill: false,
            scene_affinity: &[AiScene::ExemplarLearning],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "save_genre_template",
            description: "保存或更新文种模板",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "genre": {"type": "string"},
                    "structure": {"type": "object"}
                },
                "required": ["genre", "structure"]
            }),
            access_level: ToolAccessLevel::WriteCache,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::HarnessOnly,
            default_enabled_without_skill: false,
            scene_affinity: &[AiScene::ExemplarLearning],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "update_user_rule",
            description: "添加或更新用户长期规则",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "rule": {"type": "string", "description": "规则内容"},
                    "rule_type": {"type": "string", "enum": ["output_format", "citation_style", "tone", "tool_preference", "agent_behavior"]}
                },
                "required": ["rule", "rule_type"]
            }),
            access_level: ToolAccessLevel::WriteSettings,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::HarnessOnly,
            default_enabled_without_skill: false,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "create_note_from_deposit",
            description: "从 AI 收件箱创建新 .md 笔记",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "deposit_id": {"type": "integer"},
                    "target_path": {"type": "string"}
                },
                "required": ["deposit_id", "target_path"]
            }),
            access_level: ToolAccessLevel::WriteMarkdown,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::HarnessOnly,
            default_enabled_without_skill: false,
            scene_affinity: &[],
            max_results: None,
        },
        // ─── Skill management (meta tools) ─────────────────────
        ToolCatalogEntry {
            name: "skills_list",
            description: "列出已安装的 Agent Skills（全局 + 当前库）",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            access_level: ToolAccessLevel::ReadIndex,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "skills_install",
            description: "安装 Agent Skill（url / git / local / registry）。SkillHub: source=registry, registry=skillhub, path_or_url=<skill名或页面URL>",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": {"type": "string", "enum": ["url", "git", "local", "registry"]},
                    "path_or_url": {"type": "string"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "global"},
                    "subpath": {"type": "string"},
                    "registry": {"type": "string", "description": "registry 时必填，默认 skillhub"},
                    "reason": {"type": "string", "description": "展示于确认框"}
                },
                "required": ["source", "path_or_url"]
            }),
            access_level: ToolAccessLevel::ManageSkills,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "skills_uninstall",
            description: "卸载已安装的 Agent Skill",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "global"},
                    "reason": {"type": "string"}
                },
                "required": ["name"]
            }),
            access_level: ToolAccessLevel::ManageSkills,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "skills_toggle",
            description: "启用或禁用已安装的 Agent Skill",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "global"},
                    "enabled": {"type": "boolean"},
                    "reason": {"type": "string"}
                },
                "required": ["name", "enabled"]
            }),
            access_level: ToolAccessLevel::ManageSkills,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
    ]
});

// ─── Derived lists (computed at compile-time-like from the catalog) ──────

/// Tool names that have real `dispatch_tool_inner` handlers.
pub fn catalog_dispatchable_names() -> Vec<&'static str> {
    TOOL_CATALOG
        .iter()
        .filter(|e| e.implementation == ToolImplementationStatus::Dispatchable)
        .map(|e| e.name)
        .collect()
}

/// Tool names handled inside the harness loop (not via dispatch).
pub fn catalog_harness_only_names() -> Vec<&'static str> {
    TOOL_CATALOG
        .iter()
        .filter(|e| e.implementation == ToolImplementationStatus::HarnessOnly)
        .map(|e| e.name)
        .collect()
}

/// Tool names that can be exposed to the model (dispatchable or harness-only).
pub fn catalog_exposable_names() -> Vec<&'static str> {
    TOOL_CATALOG
        .iter()
        .filter(|e| e.implementation != ToolImplementationStatus::Planned)
        .map(|e| e.name)
        .collect()
}

/// Core read-only tools available without any skill activation.
pub fn catalog_default_readonly_names() -> Vec<&'static str> {
    TOOL_CATALOG
        .iter()
        .filter(|e| e.default_enabled_without_skill && !e.requires_confirmation)
        .map(|e| e.name)
        .collect()
}

/// Look up a catalog entry by name.
pub fn catalog_find(name: &str) -> Option<&'static ToolCatalogEntry> {
    TOOL_CATALOG.iter().find(|e| e.name == name)
}

/// Total number of catalog entries.
pub fn catalog_total_count() -> usize {
    TOOL_CATALOG.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::tool_dispatch::{DISPATCHABLE_TOOL_NAMES, HARNESS_ONLY_TOOL_NAMES};

    #[test]
    fn catalog_has_all_dispatchable_tools() {
        let catalog_disp = catalog_dispatchable_names();
        for name in DISPATCHABLE_TOOL_NAMES {
            assert!(
                catalog_disp.contains(name),
                "dispatch tool '{name}' missing from catalog dispatchable list"
            );
        }
    }

    #[test]
    fn catalog_has_all_harness_only_tools() {
        let catalog_ho = catalog_harness_only_names();
        for name in HARNESS_ONLY_TOOL_NAMES {
            assert!(
                catalog_ho.contains(name),
                "harness-only tool '{name}' missing from catalog harness-only list"
            );
        }
    }

    #[test]
    fn dispatch_list_matches_catalog() {
        // Every tool in DISPATCHABLE_TOOL_NAMES must be in the catalog as Dispatchable.
        let catalog_disp = catalog_dispatchable_names();
        for name in DISPATCHABLE_TOOL_NAMES {
            assert!(catalog_disp.contains(name), "{name} not in catalog");
        }
        // Every catalog Dispatchable tool must be in DISPATCHABLE_TOOL_NAMES.
        for name in &catalog_disp {
            assert!(
                DISPATCHABLE_TOOL_NAMES.contains(name),
                "catalog dispatchable '{name}' not in DISPATCHABLE_TOOL_NAMES"
            );
        }
    }

    #[test]
    fn harness_only_list_matches_catalog() {
        let catalog_ho = catalog_harness_only_names();
        // Every HARNESS_ONLY_TOOL_NAMES entry must be in catalog as HarnessOnly
        for name in HARNESS_ONLY_TOOL_NAMES {
            assert!(catalog_ho.contains(name), "{name} not in catalog");
        }
        // Every catalog HarnessOnly entry must be either:
        // - in HARNESS_ONLY_TOOL_NAMES (loop control), OR
        // - requires_confirmation (write tools confirmed via harness UI)
        for name in &catalog_ho {
            let in_harness_list = HARNESS_ONLY_TOOL_NAMES.contains(name);
            let entry = catalog_find(name).unwrap();
            let is_write_tool = entry.requires_confirmation;
            assert!(
                in_harness_list || is_write_tool,
                "catalog harness-only '{name}' is neither in HARNESS_ONLY_TOOL_NAMES nor a write tool"
            );
        }
    }

    #[test]
    fn no_duplicate_names() {
        let mut seen = Vec::new();
        for entry in TOOL_CATALOG.iter() {
            assert!(
                !seen.contains(&entry.name),
                "duplicate tool name: {}",
                entry.name
            );
            seen.push(entry.name);
        }
    }

    #[test]
    fn default_readonly_tools_present() {
        let defaults = catalog_default_readonly_names();
        // These must always be available without skills (plan §2.2)
        let required = [
            "search_hybrid",
            "search_semantic",
            "search_keyword",
            "read_note",
            "list_vault",
            "get_outline",
            "get_backlinks",
            "conclude_reasoning",
        ];
        for name in required {
            assert!(
                defaults.contains(&name),
                "core default tool '{name}' missing from default_readonly list"
            );
        }
    }

    #[test]
    fn write_tools_not_in_default_readonly() {
        let defaults = catalog_default_readonly_names();
        let write_tools = [
            "insert_text_at_cursor",
            "replace_selection",
            "add_tags",
            "confirm_block_link",
            "save_genre_template",
            "update_user_rule",
            "create_note_from_deposit",
        ];
        for name in write_tools {
            assert!(
                !defaults.contains(&name),
                "write tool '{name}' should not be in default_readonly"
            );
        }
    }

    #[test]
    fn total_catalog_count() {
        // 14 read-only + 7 write + 4 skills meta = 25 total tools
        assert_eq!(
            catalog_total_count(),
            25,
            "catalog should have exactly 25 tools"
        );
    }

    #[test]
    fn catalog_find_works() {
        assert!(catalog_find("read_note").is_some());
        assert!(catalog_find("nonexistent_tool").is_none());
    }
}
