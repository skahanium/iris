use crate::ai_runtime::ToolAccessLevel;

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
            max_results: None,
        },
    ]
}
