//! Tool definitions, permission checks, and execution dispatch.
//!
//! All tool definitions live here. The ToolExecutor handles:
//! 1. Filtering available tools by scene and access level
//! 2. Formatting tool specs for LLM function-calling
//! 3. Routing confirmed tool calls to Rust command handlers

use crate::ai_runtime::{AiScene, AutonomyLevel, ToolAccessLevel, ToolSpec};

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

    // ─── private ─────────────────────────────────────

    fn builtin_tools() -> Vec<ToolSpec> {
        vec![
            // ─── 只读查询 ───
            ToolSpec {
                name: "search_hybrid".into(),
                description: "混合搜索：FTS + 向量 + 分数融合，搜索知识库中与查询相关的内容".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "搜索查询"}
                    },
                    "required": ["query"]
                }),
                access_level: ToolAccessLevel::ReadIndex,
                scene_allowlist: vec![
                    AiScene::KnowledgeLookup,
                    AiScene::ExemplarLearning,
                    AiScene::DraftingAssist,
                    AiScene::ResearchSynthesis,
                ],
                requires_confirmation: false,
                max_results: Some(20),
            },
            ToolSpec {
                name: "search_semantic".into(),
                description: "语义搜索知识库，查找与查询语义相似的笔记片段".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "limit": {"type": "integer", "default": 10}
                    },
                    "required": ["query"]
                }),
                access_level: ToolAccessLevel::ReadIndex,
                scene_allowlist: vec![],
                requires_confirmation: false,
                max_results: Some(20),
            },
            ToolSpec {
                name: "search_keyword".into(),
                description: "关键词全文搜索，精确匹配特定术语或短语".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "limit": {"type": "integer", "default": 10}
                    },
                    "required": ["query"]
                }),
                access_level: ToolAccessLevel::ReadIndex,
                scene_allowlist: vec![],
                requires_confirmation: false,
                max_results: Some(20),
            },
            ToolSpec {
                name: "get_regulation".into(),
                description: "根据法规名称和条款号获取精确条款原文".into(),
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
                scene_allowlist: vec![
                    AiScene::KnowledgeLookup,
                    AiScene::DraftingAssist,
                    AiScene::ResearchSynthesis,
                ],
                requires_confirmation: false,
                max_results: Some(1),
            },
            ToolSpec {
                name: "get_context_packets".into(),
                description: "返回当前会话已组装的证据包列表".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
                access_level: ToolAccessLevel::ReadIndex,
                scene_allowlist: vec![],
                requires_confirmation: false,
                max_results: None,
            },
            ToolSpec {
                name: "get_block_links".into(),
                description: "获取笔记的显式或已确认块级链接".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "note_path": {"type": "string"}
                    },
                    "required": ["note_path"]
                }),
                access_level: ToolAccessLevel::ReadIndex,
                scene_allowlist: vec![AiScene::KnowledgeLookup, AiScene::ResearchSynthesis],
                requires_confirmation: false,
                max_results: Some(50),
            },
            ToolSpec {
                name: "web_search".into(),
                description: "联网搜索外部信息（需用户授权）".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"}
                    },
                    "required": ["query"]
                }),
                access_level: ToolAccessLevel::Network,
                scene_allowlist: vec![AiScene::ResearchSynthesis],
                requires_confirmation: true,
                max_results: Some(5),
            },
            // ─── 写入操作 (均需确认) ───
            ToolSpec {
                name: "insert_text_at_cursor".into(),
                description: "在编辑器光标位置插入文本".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": {"type": "string", "description": "要插入的文本"}
                    },
                    "required": ["text"]
                }),
                access_level: ToolAccessLevel::WriteMarkdown,
                scene_allowlist: vec![AiScene::DraftingAssist],
                requires_confirmation: true,
                max_results: None,
            },
            ToolSpec {
                name: "replace_selection".into(),
                description: "替换编辑器当前选中文本".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "replacement": {"type": "string", "description": "替换文本"}
                    },
                    "required": ["replacement"]
                }),
                access_level: ToolAccessLevel::WriteMarkdown,
                scene_allowlist: vec![AiScene::DraftingAssist],
                requires_confirmation: true,
                max_results: None,
            },
            ToolSpec {
                name: "add_tags".into(),
                description: "为笔记添加标签（修改 frontmatter 或正文标签）".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "note_path": {"type": "string"},
                        "tags": {"type": "array", "items": {"type": "string"}}
                    },
                    "required": ["note_path", "tags"]
                }),
                access_level: ToolAccessLevel::WriteMarkdown,
                scene_allowlist: vec![AiScene::ExemplarLearning],
                requires_confirmation: true,
                max_results: None,
            },
            ToolSpec {
                name: "confirm_block_link".into(),
                description: "确认一条 AI 建议的隐含块级链接".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "link_id": {"type": "integer"}
                    },
                    "required": ["link_id"]
                }),
                access_level: ToolAccessLevel::WriteCache,
                scene_allowlist: vec![AiScene::ExemplarLearning],
                requires_confirmation: true,
                max_results: None,
            },
            ToolSpec {
                name: "save_genre_template".into(),
                description: "保存或更新文种模板".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "genre": {"type": "string"},
                        "structure": {"type": "object"}
                    },
                    "required": ["genre", "structure"]
                }),
                access_level: ToolAccessLevel::WriteCache,
                scene_allowlist: vec![AiScene::ExemplarLearning],
                requires_confirmation: true,
                max_results: None,
            },
            ToolSpec {
                name: "update_user_rule".into(),
                description: "添加或更新用户长期规则".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "rule": {"type": "string", "description": "规则内容"},
                        "rule_type": {"type": "string", "enum": ["output_format", "citation_style", "tone", "tool_preference", "agent_behavior"]}
                    },
                    "required": ["rule", "rule_type"]
                }),
                access_level: ToolAccessLevel::WriteSettings,
                scene_allowlist: vec![],
                requires_confirmation: true,
                max_results: None,
            },
            ToolSpec {
                name: "create_note_from_deposit".into(),
                description: "从 AI 收件箱创建新 .md 笔记".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "deposit_id": {"type": "integer"},
                        "target_path": {"type": "string"}
                    },
                    "required": ["deposit_id", "target_path"]
                }),
                access_level: ToolAccessLevel::WriteMarkdown,
                scene_allowlist: vec![],
                requires_confirmation: true,
                max_results: None,
            },
        ]
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

    // 2. 自治等级检查：L3 以下不允许 Network 工具
    if tool.access_level == ToolAccessLevel::Network && allowed_level < AutonomyLevel::L3 {
        return Err(ToolPermissionError::InsufficientAutonomy {
            tool: tool.name.clone(),
            required: AutonomyLevel::L3,
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
    fn network_tool_requires_l3() {
        let reg = ToolRegistry::new();
        let web = reg.find("web_search").unwrap();
        assert!(check_tool_permission(web, AiScene::ResearchSynthesis, AutonomyLevel::L3).is_ok());
        assert!(check_tool_permission(web, AiScene::ResearchSynthesis, AutonomyLevel::L2).is_err());
        assert!(check_tool_permission(web, AiScene::ResearchSynthesis, AutonomyLevel::L1).is_err());
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
}
