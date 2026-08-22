use crate::ai_runtime::ToolAccessLevel;

use super::{ToolCatalogEntry, ToolExecutionMetadata, ToolImplementationStatus};

pub(super) fn tools() -> Vec<ToolCatalogEntry> {
    vec![
        ToolCatalogEntry {
            name: "weather_lookup",
            description: "查询当前天气或短期天气预报；需要城市/地区，返回可追溯的当前事实证据。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["weather.current", "weather.forecast"],
                        "description": "查询当前实况或预报"
                    },
                    "location": {
                        "type": "string",
                        "description": "城市或地区，例如“北京”"
                    },
                    "days": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 7,
                        "default": 1,
                        "description": "预报天数，1-7 天"
                    }
                },
                "required": ["operation"],
                "additionalProperties": false
            }),
            access_level: ToolAccessLevel::Network,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            max_results: Some(8),
            execution_metadata: Some(ToolExecutionMetadata {
                cost_class: "network",
                output_policy: "bounded_packets",
                evidence_policy: "current_run_domain",
            }),
        },
        ToolCatalogEntry {
            name: "news_lookup",
            description:
                "检索近期新闻；可按主题、地点、时间范围和数量约束，返回可追溯的当前事实证据。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["news.search"],
                        "description": "固定为新闻检索"
                    },
                    "topic": {"type": "string", "description": "新闻主题"},
                    "location": {"type": "string", "description": "地点/地区"},
                    "start": {
                        "type": "string",
                        "description": "RFC 3339 或 YYYY-MM-DD 开始时间"
                    },
                    "end": {
                        "type": "string",
                        "description": "RFC 3339 或 YYYY-MM-DD 结束时间"
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 20,
                        "default": 10,
                        "description": "返回条数上限"
                    }
                },
                "additionalProperties": false
            }),
            access_level: ToolAccessLevel::Network,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            max_results: Some(20),
            execution_metadata: Some(ToolExecutionMetadata {
                cost_class: "network",
                output_policy: "bounded_packets",
                evidence_policy: "current_run_domain",
            }),
        },
        ToolCatalogEntry {
            name: "finance_lookup",
            description: "查询金融行情、指标或相关新闻；需要明确 instrument 与稳定 operation。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["finance.quote", "finance.metrics", "finance.news"],
                        "description": "行情、指标或新闻"
                    },
                    "instrument": {
                        "type": "string",
                        "description": "证券/资产代码或名称，例如 AAPL"
                    },
                    "assetKind": {
                        "type": "string",
                        "description": "资产类别，例如 equity、etf、forex、crypto"
                    }
                },
                "required": ["operation", "instrument"],
                "additionalProperties": false
            }),
            access_level: ToolAccessLevel::Network,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            max_results: Some(10),
            execution_metadata: Some(ToolExecutionMetadata {
                cost_class: "network",
                output_policy: "bounded_packets",
                evidence_policy: "current_run_domain",
            }),
        },
        ToolCatalogEntry {
            name: "entertainment_lookup",
            description: "查询影视排片、即将上映或流媒体可看内容；附近影院查询需要城市/地区。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": [
                            "entertainment.now_playing",
                            "entertainment.upcoming",
                            "entertainment.streaming"
                        ],
                        "description": "正在上映、即将上映或流媒体可看"
                    },
                    "title": {"type": "string", "description": "影视作品标题"},
                    "location": {"type": "string", "description": "城市/地区，附近影院查询必填"},
                    "channel": {"type": "string", "description": "频道/流媒体平台"}
                },
                "required": ["operation"],
                "additionalProperties": false
            }),
            access_level: ToolAccessLevel::Network,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            max_results: Some(20),
            execution_metadata: Some(ToolExecutionMetadata {
                cost_class: "network",
                output_policy: "bounded_packets",
                evidence_policy: "current_run_domain",
            }),
        },
        ToolCatalogEntry {
            name: "sports_lookup",
            description: "查询体育赛程或比分；可按比赛、参赛方和日期约束。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["sports.schedule", "sports.score"],
                        "description": "赛程或比分"
                    },
                    "competition": {"type": "string", "description": "赛事/联赛名称"},
                    "participant": {"type": "string", "description": "参赛方/球队"},
                    "date": {
                        "type": "string",
                        "description": "YYYY-MM-DD 或 RFC 3339 日期，必须在冻结窗口内"
                    }
                },
                "required": ["operation"],
                "additionalProperties": false
            }),
            access_level: ToolAccessLevel::Network,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            max_results: Some(10),
            execution_metadata: Some(ToolExecutionMetadata {
                cost_class: "network",
                output_policy: "bounded_packets",
                evidence_policy: "current_run_domain",
            }),
        },
    ]
}
