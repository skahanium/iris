use crate::ai_runtime::ToolAccessLevel;

use super::{ToolCatalogEntry, ToolExecutionMetadata, ToolImplementationStatus};

pub(super) fn tools() -> Vec<ToolCatalogEntry> {
    vec![
        ToolCatalogEntry {
            name: "web_search",
            description: "搜索公开网页并返回本次 Run 的候选片段。结果不足时可调整 query 或来源方向；候选片段不能直接作为最终证据，选中候选后使用 web_fetch 读取正文。无需确认。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "搜索查询"}
                },
                "additionalProperties": false,
                "required": ["query"]
            }),
            access_level: ToolAccessLevel::Network,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            max_results: Some(8),
            execution_metadata: Some(ToolExecutionMetadata {
                cost_class: "network",
                output_policy: "bounded_packets",
                evidence_policy: "current_run_web",
            }),
        },
        ToolCatalogEntry {
            name: "web_fetch",
            description: "读取本次 Run 搜索候选或用户明确提供的公开 HTTPS URL 正文。可一次提交多个独立 URL；部分读取失败时会同时返回已取得的正文和失败 URL，便于换来源继续。无需确认。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "urls": {
                        "type": "array",
                        "items": {"type": "string", "format": "uri"},
                        "minItems": 1,
                        "description": "本次 Run 候选或用户消息中明确给出的公开 HTTPS URL"
                    }
                },
                "additionalProperties": false,
                "required": ["urls"]
            }),
            access_level: ToolAccessLevel::Network,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            max_results: Some(8),
            execution_metadata: Some(ToolExecutionMetadata {
                cost_class: "network",
                output_policy: "bounded_packets",
                evidence_policy: "current_run_web",
            }),
        },
    ]
}
