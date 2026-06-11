use crate::ai_runtime::ToolAccessLevel;

use super::{ToolCatalogEntry, ToolImplementationStatus};

pub(super) fn tools() -> Vec<ToolCatalogEntry> {
    vec![
        ToolCatalogEntry {
            name: "memory_read",
            description: "读取用户确认保存的长期 AI 经验/记忆条目",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": {"type": "string", "description": "可选，精确读取某条记忆"},
                    "query": {"type": "string", "description": "可选，按关键词过滤"},
                    "limit": {"type": "integer", "default": 20}
                }
            }),
            access_level: ToolAccessLevel::ReadProfile,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            scene_affinity: &[],
            max_results: Some(50),
        },
        ToolCatalogEntry {
            name: "memory_write",
            description: "写入或更新用户确认的长期 AI 经验/记忆条目",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": {"type": "string"},
                    "content": {"type": "string"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "global"}
                },
                "required": ["key", "content"]
            }),
            access_level: ToolAccessLevel::WriteSettings,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "scheduled_task_create",
            description: "创建用户确认的主动 Agent 计划任务（仅登记，不后台自动执行）",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "title": {"type": "string"},
                    "prompt": {"type": "string"},
                    "schedule": {"type": "string", "description": "自然语言或 cron 风格描述"}
                },
                "required": ["title", "prompt", "schedule"]
            }),
            access_level: ToolAccessLevel::WriteSettings,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "scheduled_task_list",
            description: "列出已登记的主动 Agent 计划任务",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "include_disabled": {"type": "boolean", "default": false}
                }
            }),
            access_level: ToolAccessLevel::ReadProfile,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            scene_affinity: &[],
            max_results: Some(50),
        },
        ToolCatalogEntry {
            name: "scheduled_task_delete",
            description: "删除用户确认的主动 Agent 计划任务",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "integer"}
                },
                "required": ["id"]
            }),
            access_level: ToolAccessLevel::WriteSettings,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            scene_affinity: &[],
            max_results: None,
        },
    ]
}
