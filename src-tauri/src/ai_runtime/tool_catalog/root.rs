use crate::ai_runtime::ToolAccessLevel;

use super::{ToolCatalogEntry, ToolImplementationStatus};

pub(super) fn tools() -> Vec<ToolCatalogEntry> {
    vec![
        ToolCatalogEntry {
            name: "system_time_now",
            description: "读取可信的本机当前日期、时间、星期与时区；回答“今天/现在/星期几”类问题时优先使用。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            access_level: ToolAccessLevel::ReadProfile,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: Some(1),
        },
        ToolCatalogEntry {
            name: "app_context_read",
            description: "读取当前 Iris 应用上下文摘要，包括 vault、当前笔记、文件 ID 与附件数量；不读取 API Key 明文。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            access_level: ToolAccessLevel::ReadProfile,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: Some(1),
        },
        ToolCatalogEntry {
            name: "capabilities_read",
            description: "读取当前 AI 能力摘要，包括联网开关、模型槽位配置状态、Vision 状态与可用工具；不读取凭据明文。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            access_level: ToolAccessLevel::ReadProfile,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: Some(1),
        },
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
