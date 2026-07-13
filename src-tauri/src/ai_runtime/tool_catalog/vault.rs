use crate::ai_runtime::ToolAccessLevel;

use super::{ToolCatalogEntry, ToolImplementationStatus};

pub(super) fn tools() -> Vec<ToolCatalogEntry> {
    vec![
        ToolCatalogEntry {
            name: "vault_create_note",
            description: "在 Markdown vault 中创建新的 .md 笔记",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "target_path": {"type": "string", "description": "vault-relative .md path"},
                    "content": {"type": "string", "description": "initial Markdown body"}
                },
                "required": ["target_path"]
            }),
            access_level: ToolAccessLevel::WriteMarkdown,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            max_results: None,
        },
        ToolCatalogEntry {
            name: "vault_rename_move",
            description: "重命名或移动用户笔记，并返回 backlinks / wikilinks 影响摘要",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "new_path": {"type": "string"}
                },
                "required": ["path", "new_path"]
            }),
            access_level: ToolAccessLevel::WriteMarkdown,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            max_results: None,
        },
        ToolCatalogEntry {
            name: "vault_delete_to_trash",
            description: "将用户笔记移入 Iris 回收站，而不是永久删除",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                },
                "required": ["path"]
            }),
            access_level: ToolAccessLevel::WriteMarkdown,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            max_results: None,
        },
        ToolCatalogEntry {
            name: "vault_asset_write",
            description: "将二进制资源写入 vault assets/ 目录",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "vault-relative asset path, e.g. assets/image.png"},
                    "data_base64": {"type": "string", "description": "base64 encoded binary data"}
                },
                "required": ["path", "data_base64"]
            }),
            access_level: ToolAccessLevel::WriteMarkdown,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            max_results: None,
        },
        ToolCatalogEntry {
            name: "vault_version_list",
            description: "列出指定用户笔记的版本快照",
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
            max_results: Some(50),
        },
    ]
}
