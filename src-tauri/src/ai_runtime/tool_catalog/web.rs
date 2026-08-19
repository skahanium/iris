use crate::ai_runtime::ToolAccessLevel;

use super::{ToolCatalogEntry, ToolExecutionMetadata, ToolImplementationStatus};

pub(super) fn tools() -> Vec<ToolCatalogEntry> {
    vec![
        ToolCatalogEntry {
            name: "web_search",
            description:
                "网络证据代理 WebEvidenceBroker：检索实时外部来源、读取明确 URL，并返回可追溯证据；无需确认，直接调用。结果应与本地检索证据交叉引用、相互印证。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "搜索查询"},
                    "urls": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "用户明确要求深读的公开 HTTPS URL"
                    },
                    "gap": {
                        "type": "string",
                        "enum": [
                            "missing_entity",
                            "missing_location",
                            "location_coverage",
                            "missing_timestamp",
                            "stale_observation",
                            "missing_unit",
                            "missing_channel",
                            "missing_independent_source",
                            "source_conflict"
                        ],
                        "description": "当前搜索要解决的证据缺口（仅 current-fact 研究循环使用）"
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
