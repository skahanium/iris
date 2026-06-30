use crate::ai_runtime::{AiScene, ToolAccessLevel};

use super::{ToolCatalogEntry, ToolImplementationStatus};

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
                    }
                },
                "required": ["query"]
            }),
            access_level: ToolAccessLevel::Network,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[
                AiScene::KnowledgeLookup,
                AiScene::DraftingAssist,
                AiScene::ResearchSynthesis,
            ],
            max_results: Some(8),
        },
    ]
}
