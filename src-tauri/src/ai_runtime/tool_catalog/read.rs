use crate::ai_runtime::ToolAccessLevel;

use super::{ToolCatalogEntry, ToolImplementationStatus};

pub(super) fn tools() -> Vec<ToolCatalogEntry> {
    vec![
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
            max_results: None,
        },
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
            max_results: Some(50),
        },
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
            max_results: None,
        },
        ToolCatalogEntry {
            name: "spawn_subagent",
            description: "将复杂只读子任务委派给独立 agent。可提交单个 task，或用 tasks 一次提交最多 3 个并发子任务；适用于独立推理、交叉验证或长任务拆分。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task": {"type": "string", "description": "子任务完整描述"},
                    "tasks": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 3,
                        "description": "按请求顺序排列的子任务；与 task 互斥",
                        "items": {
                            "type": "object",
                            "properties": {
                                "task": {"type": "string", "description": "子任务完整描述"},
                                "role": {"type": "string", "description": "子 agent 角色，例如 researcher / verifier / writer"},
                                "context_hint": {"type": "string", "description": "可选额外上下文"},
                                "allowed_tools": {
                                    "type": "array",
                                    "items": {"type": "string"},
                                    "description": "允许子 agent 使用的工具子集；只能收窄当前可用工具面"
                                },
                                "resource_locks": {
                                    "type": "array",
                                    "description": "可选只读资源声明；任何 write 请求都会被拒绝，不提供写入协调",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "resource_type": {"type": "string", "default": "note"},
                                            "resource_id": {"type": "string"},
                                            "access": {"type": "string", "enum": ["read", "write"], "default": "read"}
                                        },
                                        "required": ["resource_id"]
                                    }
                                }
                            },
                            "required": ["task"]
                        }
                    },
                    "role": {"type": "string", "description": "子 agent 角色，例如 researcher / verifier / writer"},
                    "context_hint": {"type": "string", "description": "可选额外上下文"},
                    "max_rounds": {"type": "integer", "description": "子任务最大轮次", "default": 2},
                    "allowed_tools": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "允许子 agent 使用的工具子集；只能收窄当前可用工具面"
                    },
                    "resource_locks": {
                        "type": "array",
                        "description": "可选只读资源声明；任何 write 请求都会被拒绝，不提供写入协调",
                        "items": {
                            "type": "object",
                            "properties": {
                                "resource_type": {"type": "string", "default": "note"},
                                "resource_id": {"type": "string"},
                                "access": {"type": "string", "enum": ["read", "write"], "default": "read"}
                            },
                            "required": ["resource_id"]
                        }
                    }
                }
            }),
            access_level: ToolAccessLevel::ReadIndex,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::HarnessOnly,
            default_enabled_without_skill: true,
            max_results: None,
        },
    ]
}
