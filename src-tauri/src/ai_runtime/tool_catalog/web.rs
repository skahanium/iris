use crate::ai_runtime::ToolAccessLevel;

use super::{ToolCatalogEntry, ToolExecutionMetadata, ToolImplementationStatus};

pub(super) fn tools() -> Vec<ToolCatalogEntry> {
    vec![
        ToolCatalogEntry {
            name: "web_search",
            description:
                "网络检索与读取：不带 urls 时返回本次 Run 的候选网页片段；结果不足时可调整 query。选中候选后，在后续调用的 urls 中传入这些候选 URL 读取正文，只有正文读取结果可作为最终引用证据。无需确认。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "搜索查询"},
                    "urls": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "本次 Run 搜索结果中选中的 URL，或用户消息中明确给出的公开 HTTPS URL；用于读取正文"
                    }
                },
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
    ]
}
