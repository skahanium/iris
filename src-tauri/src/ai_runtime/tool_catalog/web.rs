use crate::ai_runtime::{AiScene, ToolAccessLevel};

use super::{ToolCatalogEntry, ToolImplementationStatus};

pub(super) fn tools() -> Vec<ToolCatalogEntry> {
    vec![
        ToolCatalogEntry {
            name: "web_search",
            description:
                "网络证据代理：检索实时外部来源并返回可追溯证据；无需确认，直接调用。结果应与本地检索证据交叉引用、相互印证。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "搜索查询"}
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
        ToolCatalogEntry {
            name: "fetch_web_page",
            description: "网络证据代理内部能力：打开单个 HTTPS 网页并提取正文片段（需用户确认）。\
            仅在网络证据代理或本地检索已给出 URL 且摘要不足时使用；\
            每轮最多 1～2 次，禁止批量爬取。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "HTTPS 页面 URL"},
                    "max_chars": {"type": "integer", "description": "最大正文字符数，默认 24000"},
                    "reason": {"type": "string", "description": "抓取原因（供用户确认）"}
                },
                "required": ["url"]
            }),
            access_level: ToolAccessLevel::Network,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[
                AiScene::KnowledgeLookup,
                AiScene::DraftingAssist,
                AiScene::ResearchSynthesis,
            ],
            max_results: Some(2),
        },
        ToolCatalogEntry {
            name: "web_fetch_batch",
            description: "网络证据代理内部能力：批量抓取少量 HTTPS 页面并返回正文证据包（需确认，限量）",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "urls": {"type": "array", "items": {"type": "string"}, "maxItems": 5},
                    "max_chars": {"type": "integer", "default": 12000},
                    "reason": {"type": "string"}
                },
                "required": ["urls"]
            }),
            access_level: ToolAccessLevel::Network,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            scene_affinity: &[AiScene::KnowledgeLookup, AiScene::ResearchSynthesis],
            max_results: Some(5),
        },
        ToolCatalogEntry {
            name: "readability_fetch",
            description: "网络证据代理内部能力：抓取 HTTPS 页面并提取适合阅读的正文（需确认）",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "max_chars": {"type": "integer", "default": 24000},
                    "reason": {"type": "string"}
                },
                "required": ["url"]
            }),
            access_level: ToolAccessLevel::Network,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            scene_affinity: &[AiScene::KnowledgeLookup, AiScene::ResearchSynthesis],
            max_results: Some(1),
        },
        ToolCatalogEntry {
            name: "rendered_fetch",
            description:
                "网络证据代理内部能力：Static readability HTTPS fetch; not JavaScript-rendered. 抓取正文并标记未渲染",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "max_chars": {"type": "integer", "default": 24000},
                    "reason": {"type": "string"}
                },
                "required": ["url"]
            }),
            access_level: ToolAccessLevel::Network,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            scene_affinity: &[AiScene::KnowledgeLookup, AiScene::ResearchSynthesis],
            max_results: Some(1),
        },
    ]
}
