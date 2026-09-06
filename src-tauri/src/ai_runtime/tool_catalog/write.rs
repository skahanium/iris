use crate::ai_runtime::ToolAccessLevel;

use super::{ToolCatalogEntry, ToolImplementationStatus};

pub(super) fn tools() -> Vec<ToolCatalogEntry> {
    vec![
        ToolCatalogEntry {
            name: "insert_text_at_cursor",
            description:
                "在已授权 Markdown 的精确 UTF-8 字节位置插入文本；必须先读取基线，确认后执行",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "text": {"type": "string", "description": "要插入的文本"},
                    "target_path": {"type":"string", "description":"已授权的 Vault 相对路径"},
                    "base_content_hash": {"type": "string", "description": "read_note 返回的整篇内容 hash"},
                    "range": {"type": "object", "description": "UTF-8 字节位置，start 必须等于 end", "properties":{"start":{"type":"integer","minimum":0},"end":{"type":"integer","minimum":0}}, "required":["start","end"], "additionalProperties":false}
                },
                "required": ["text", "target_path", "base_content_hash", "range"]
            }),
            access_level: ToolAccessLevel::WriteMarkdown,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            max_results: None,
            execution_metadata: None,
        },
        ToolCatalogEntry {
            name: "replace_selection",
            description:
                "替换已授权 Markdown 的精确原文范围；必须先读取基线，不允许模糊替换，确认后执行",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "replacement": {"type": "string", "description": "替换文本"},
                    "target_path": {"type":"string", "description":"已授权的 Vault 相对路径"},
                    "base_content_hash": {"type": "string", "description": "read_note 返回的整篇内容 hash"},
                    "original_text":{"type":"string", "description":"字节范围内完全一致的原文"},
                    "range": {"type": "object", "description": "UTF-8 字节范围，end 不包含在内", "properties":{"start":{"type":"integer","minimum":0},"end":{"type":"integer","minimum":0}}, "required":["start","end"], "additionalProperties":false}
                },
                "required": ["replacement", "target_path", "base_content_hash", "range", "original_text"]
            }),
            access_level: ToolAccessLevel::WriteMarkdown,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            max_results: None,
            execution_metadata: None,
        },
    ]
}
