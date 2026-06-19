use crate::ai_runtime::{AiScene, ToolAccessLevel};

use super::{ToolCatalogEntry, ToolImplementationStatus};

pub(super) fn tools() -> Vec<ToolCatalogEntry> {
    vec![
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
            implementation: ToolImplementationStatus::Dispatchable,
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
            implementation: ToolImplementationStatus::Dispatchable,
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
            scene_affinity: &[],
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
            scene_affinity: &[],
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
            scene_affinity: &[],
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
    ]
}
