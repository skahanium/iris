//! Research Workflow — L3 limited agentic loop engine.
//!
//! The only scene that allows multi-round tool-calling loops.
//! Follows the pipeline:
//!   topic → sub-proposition decomposition → per-proposition retrieval →
//!   evidence matrix → gap identification → summary output
//!
//! Constraints (§11.4):
//! - max_agentic_rounds = 4 (default)
//! - max_tool_calls_per_round = 6
//! - web research requires explicit user authorization
//! - external web evidence has lower trust than user notes & local regulations
//! - no fabricated citations

use crate::ai_runtime::{
    model_gateway::{
        GatewayRequest, LlmMessage, MessageRole, ModelGateway, ProviderConfig, TokenUsage,
    },
    scene_router::resolve_scene,
    session::SessionManager,
    tool_executor::ToolRegistry,
    AiScene, ContextPacket, TrustLevel,
};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

// ─── Research Types ──────────────────────────────────────

/// A sub-proposition extracted from the research topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubProposition {
    pub id: String,
    pub statement: String,
    pub evidence: Vec<ContextPacket>,
    pub gaps: Vec<String>,
}

/// Evidence matrix: propositions × evidence sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceMatrix {
    pub topic: String,
    pub propositions: Vec<SubProposition>,
    pub global_gaps: Vec<String>,
    pub total_evidence_count: usize,
    pub coverage_score: f64,
}

/// Argument chain link between propositions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgumentLink {
    pub from_proposition_id: String,
    pub to_proposition_id: String,
    pub link_type: ArgumentLinkType,
    pub strength: f64,
    pub evidence_label: Option<String>,
}

/// Types of argument links.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgumentLinkType {
    /// A supports B
    Supports,
    /// A contradicts B
    Contradicts,
    /// A is a prerequisite for B
    Prerequisite,
    /// A is a consequence of B
    Consequence,
    /// A and B are parallel evidence
    Parallel,
}

/// Full argument chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgumentChain {
    pub links: Vec<ArgumentLink>,
    pub has_contradictions: bool,
    pub chain_strength: f64,
}

/// Research round state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchRound {
    pub round_number: u32,
    pub queries_executed: Vec<String>,
    pub packets_retrieved: Vec<ContextPacket>,
    pub tool_calls_made: u32,
    pub llm_output: Option<String>,
}

/// Research workflow result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchResult {
    pub request_id: String,
    pub topic: String,
    pub rounds: Vec<ResearchRound>,
    pub evidence_matrix: EvidenceMatrix,
    pub argument_chain: ArgumentChain,
    pub summary: String,
    pub total_tokens: TokenUsage,
}

/// Research workflow configuration.
#[derive(Debug, Clone)]
pub struct ResearchConfig {
    pub max_rounds: u32,
    pub max_tools_per_round: u32,
    pub web_research_authorized: bool,
    pub token_budget: usize,
}

impl Default for ResearchConfig {
    fn default() -> Self {
        Self {
            max_rounds: 4,
            max_tools_per_round: 6,
            web_research_authorized: false,
            token_budget: 40_000,
        }
    }
}

// ─── Research Workflow Engine ────────────────────────────

/// Execute the research workflow as an L3 agentic loop.
pub async fn execute_research(
    db: &Database,
    app_handle: &AppHandle,
    request_id: &str,
    topic: &str,
    config: ResearchConfig,
    provider_config: ProviderConfig,
    web_authorized: bool,
) -> AppResult<ResearchResult> {
    let mut config = config;
    config.web_research_authorized = web_authorized;

    let _profile = resolve_scene(AiScene::ResearchSynthesis);
    let _registry = ToolRegistry::new();

    // Ensure session
    let _session_key = "research_synthesis:__global__".to_string();
    let sid = SessionManager::ensure(db, AiScene::ResearchSynthesis, None)?;

    // Save user topic
    SessionManager::append_message(db, sid, "user", topic, None)?;

    let mut rounds: Vec<ResearchRound> = Vec::new();
    let mut all_packets: Vec<ContextPacket> = Vec::new();
    let mut total_usage = TokenUsage::default();

    // ── Phase 1: Sub-proposition decomposition ──────────
    let sub_propositions = decompose_topic(
        app_handle,
        request_id,
        &provider_config,
        topic,
        &mut total_usage,
    )
    .await?;

    // ── Phase 2: Per-proposition retrieval (agentic loop) ──
    for round_num in 0..config.max_rounds {
        if sub_propositions.is_empty() {
            break;
        }

        let mut round = ResearchRound {
            round_number: round_num + 1,
            queries_executed: Vec::new(),
            packets_retrieved: Vec::new(),
            tool_calls_made: 0,
            llm_output: None,
        };

        // Retrieve evidence for each proposition
        for prop in &sub_propositions {
            if round.tool_calls_made >= config.max_tools_per_round {
                break;
            }

            let packets = db.with_conn(|conn| {
                let request = crate::ai_runtime::retrieval_broker::RetrievalRequest {
                    query: prop.statement.clone(),
                    max_results: 10,
                    layers: crate::ai_runtime::retrieval_broker::RetrievalLayers {
                        fts: true,
                        vector: true,
                        graph: true,
                        exact: true,
                        template: false,
                    },
                    note_context: None,
                    file_id_context: None,
                };
                crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request)
            })?;

            round.queries_executed.push(prop.statement.clone());
            round.packets_retrieved.extend(packets.clone());
            round.tool_calls_made += 1;
            all_packets.extend(packets);
        }

        // Deduplicate packets
        all_packets.dedup_by(|a, b| a.id == b.id);

        // Check if we need another round (only if we found new evidence)
        let new_evidence_count = round.packets_retrieved.len();
        if new_evidence_count == 0 {
            rounds.push(round);
            break;
        }

        rounds.push(round);
    }

    // ── Phase 3: Build evidence matrix ──────────────────
    let evidence_matrix = build_evidence_matrix(topic, &sub_propositions, &all_packets);

    // ── Phase 4: Argument chain detection ───────────────
    let argument_chain = detect_argument_chains(
        app_handle,
        request_id,
        &provider_config,
        &evidence_matrix,
        &mut total_usage,
    )
    .await?;

    // ── Phase 5: Synthesize final summary ───────────────
    let summary = synthesize_summary(
        app_handle,
        request_id,
        &provider_config,
        topic,
        &evidence_matrix,
        &argument_chain,
        &mut total_usage,
    )
    .await?;

    // Save assistant summary
    SessionManager::append_message(db, sid, "assistant", &summary, None)?;

    Ok(ResearchResult {
        request_id: request_id.to_string(),
        topic: topic.to_string(),
        rounds,
        evidence_matrix,
        argument_chain,
        summary,
        total_tokens: total_usage,
    })
}

// ─── Sub-proposition Decomposition ───────────────────────

async fn decompose_topic(
    app_handle: &AppHandle,
    _request_id: &str,
    provider: &ProviderConfig,
    topic: &str,
    usage: &mut TokenUsage,
) -> AppResult<Vec<SubProposition>> {
    let prompt = format!(
        r#"你是一个学术研究助理。请将以下研究主题分解为 3-7 个子命题，每个子命题应当：
1. 可以独立检索证据
2. 与主题有明确的逻辑关系
3. 互相之间有区分度

研究主题: {topic}

请以 JSON 数组格式返回子命题，每个元素包含 "id"(如 "P1") 和 "statement"(子命题陈述)。
只返回 JSON，不要其他文字。"#
    );

    let messages = vec![LlmMessage {
        role: MessageRole::User,
        content: prompt,
        tool_call_id: None,
        tool_calls: None,
    }];

    let request = GatewayRequest {
        provider: provider.clone(),
        messages,
        tools: vec![],
        max_tokens: Some(2000),
        temperature: Some(0.3),
        stream: false,
    };

    let gateway = ModelGateway::new(app_handle.clone(), vec![provider.clone()]);
    let response = gateway.send_request(request).await?;

    accumulate_usage(usage, &response.usage);

    let content = response.content.unwrap_or_default();
    parse_sub_propositions(&content)
}

fn parse_sub_propositions(json_str: &str) -> AppResult<Vec<SubProposition>> {
    // Try to extract JSON from the response (may be wrapped in markdown code block)
    let json_str = json_str.trim();
    let json_str = if json_str.starts_with("```") {
        let start = json_str.find('[').unwrap_or(0);
        let end = json_str.rfind(']').unwrap_or(json_str.len());
        &json_str[start..=end]
    } else {
        json_str
    };

    let parsed: Vec<serde_json::Value> = serde_json::from_str(json_str)
        .map_err(|e| AppError::msg(format!("failed to parse sub-propositions: {e}")))?;

    Ok(parsed
        .into_iter()
        .enumerate()
        .map(|(i, v)| SubProposition {
            id: v["id"]
                .as_str()
                .unwrap_or(&format!("P{}", i + 1))
                .to_string(),
            statement: v["statement"].as_str().unwrap_or("").to_string(),
            evidence: Vec::new(),
            gaps: Vec::new(),
        })
        .filter(|p| !p.statement.is_empty())
        .collect())
}

// ─── Evidence Matrix ─────────────────────────────────────

fn build_evidence_matrix(
    topic: &str,
    propositions: &[SubProposition],
    all_packets: &[ContextPacket],
) -> EvidenceMatrix {
    let mut props_with_evidence = Vec::new();
    let mut global_gaps = Vec::new();
    let mut total_evidence = 0;

    for prop in propositions {
        // Find packets relevant to this proposition
        let evidence: Vec<ContextPacket> = all_packets
            .iter()
            .filter(|p| {
                // Simple relevance: check if any key terms appear in the excerpt
                let prop_terms: Vec<&str> = prop
                    .statement
                    .split(|c: char| c.is_whitespace() || c == '，' || c == '。')
                    .filter(|s| s.len() >= 2)
                    .collect();
                prop_terms
                    .iter()
                    .any(|term| p.excerpt.contains(term) || p.title.contains(term))
            })
            .cloned()
            .collect();

        let gaps = if evidence.is_empty() {
            vec![format!("子命题「{}」缺少直接证据", prop.statement)]
        } else {
            // Check for trust level gaps
            let has_user_note = evidence
                .iter()
                .any(|e| matches!(e.trust_level, TrustLevel::UserNote));
            let has_regulation = evidence
                .iter()
                .any(|e| matches!(e.source_type, crate::ai_runtime::SourceType::Regulation));

            let mut gaps = Vec::new();
            if !has_user_note {
                gaps.push("缺少用户笔记支撑".to_string());
            }
            if !has_regulation {
                gaps.push("缺少法规条款引用".to_string());
            }
            gaps
        };

        total_evidence += evidence.len();

        props_with_evidence.push(SubProposition {
            id: prop.id.clone(),
            statement: prop.statement.clone(),
            evidence: evidence.clone(),
            gaps: gaps.clone(),
        });

        global_gaps.extend(gaps);
    }

    // Calculate coverage score
    let covered = props_with_evidence
        .iter()
        .filter(|p| !p.evidence.is_empty())
        .count();
    let coverage_score = if propositions.is_empty() {
        0.0
    } else {
        covered as f64 / propositions.len() as f64
    };

    EvidenceMatrix {
        topic: topic.to_string(),
        propositions: props_with_evidence,
        global_gaps,
        total_evidence_count: total_evidence,
        coverage_score,
    }
}

// ─── Argument Chain Detection ────────────────────────────

async fn detect_argument_chains(
    app_handle: &AppHandle,
    _request_id: &str,
    provider: &ProviderConfig,
    matrix: &EvidenceMatrix,
    usage: &mut TokenUsage,
) -> AppResult<ArgumentChain> {
    if matrix.propositions.len() < 2 {
        return Ok(ArgumentChain {
            links: Vec::new(),
            has_contradictions: false,
            chain_strength: 1.0,
        });
    }

    let propositions_desc: String = matrix
        .propositions
        .iter()
        .map(|p| {
            let evidence_summary: Vec<String> =
                p.evidence.iter().take(3).map(|e| e.title.clone()).collect();
            format!(
                "- {}: {} (证据: {})",
                p.id,
                p.statement,
                evidence_summary.join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        r#"分析以下子命题之间的论证关系。对每对相关命题，判断关系类型并评估强度。

子命题:
{propositions_desc}

请以 JSON 数组格式返回论证链接，每个元素包含:
- "from": 源命题 ID
- "to": 目标命题 ID
- "link_type": "supports" | "contradicts" | "prerequisite" | "consequence" | "parallel"
- "strength": 0.0-1.0 的浮点数

只返回相关的链接（strength > 0.3），只返回 JSON，不要其他文字。"#
    );

    let messages = vec![LlmMessage {
        role: MessageRole::User,
        content: prompt,
        tool_call_id: None,
        tool_calls: None,
    }];

    let request = GatewayRequest {
        provider: provider.clone(),
        messages,
        tools: vec![],
        max_tokens: Some(2000),
        temperature: Some(0.2),
        stream: false,
    };

    let gateway = ModelGateway::new(app_handle.clone(), vec![provider.clone()]);
    let response = gateway.send_request(request).await?;

    accumulate_usage(usage, &response.usage);

    let content = response.content.unwrap_or_default();
    parse_argument_chain(&content)
}

fn parse_argument_chain(json_str: &str) -> AppResult<ArgumentChain> {
    let json_str = json_str.trim();
    let json_str = if json_str.starts_with("```") {
        let start = json_str.find('[').unwrap_or(0);
        let end = json_str.rfind(']').unwrap_or(json_str.len());
        &json_str[start..=end]
    } else {
        json_str
    };

    let parsed: Vec<serde_json::Value> = serde_json::from_str(json_str)
        .map_err(|e| AppError::msg(format!("failed to parse argument chain: {e}")))?;

    let links: Vec<ArgumentLink> = parsed
        .into_iter()
        .filter_map(|v| {
            let link_type_str = v["link_type"].as_str()?;
            let link_type = match link_type_str {
                "supports" => ArgumentLinkType::Supports,
                "contradicts" => ArgumentLinkType::Contradicts,
                "prerequisite" => ArgumentLinkType::Prerequisite,
                "consequence" => ArgumentLinkType::Consequence,
                "parallel" => ArgumentLinkType::Parallel,
                _ => return None,
            };

            Some(ArgumentLink {
                from_proposition_id: v["from"].as_str()?.to_string(),
                to_proposition_id: v["to"].as_str()?.to_string(),
                link_type,
                strength: v["strength"].as_f64().unwrap_or(0.5),
                evidence_label: v["evidence_label"].as_str().map(|s| s.to_string()),
            })
        })
        .collect();

    let has_contradictions = links
        .iter()
        .any(|l| matches!(l.link_type, ArgumentLinkType::Contradicts));

    let chain_strength = if links.is_empty() {
        0.0
    } else {
        links.iter().map(|l| l.strength).sum::<f64>() / links.len() as f64
    };

    Ok(ArgumentChain {
        links,
        has_contradictions,
        chain_strength,
    })
}

// ─── Summary Synthesis ───────────────────────────────────

async fn synthesize_summary(
    app_handle: &AppHandle,
    _request_id: &str,
    provider: &ProviderConfig,
    topic: &str,
    matrix: &EvidenceMatrix,
    chain: &ArgumentChain,
    usage: &mut TokenUsage,
) -> AppResult<String> {
    let propositions_text: String = matrix
        .propositions
        .iter()
        .map(|p| {
            let evidence_citations: Vec<String> = p
                .evidence
                .iter()
                .map(|e| format!("[{}] {}", e.citation_label, e.title))
                .collect();
            format!(
                "## {} {}\n证据: {}\n缺口: {}",
                p.id,
                p.statement,
                if evidence_citations.is_empty() {
                    "无".to_string()
                } else {
                    evidence_citations.join("; ")
                },
                if p.gaps.is_empty() {
                    "无".to_string()
                } else {
                    p.gaps.join("; ")
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let contradictions_text = if chain.has_contradictions {
        let contradictions: Vec<String> = chain
            .links
            .iter()
            .filter(|l| matches!(l.link_type, ArgumentLinkType::Contradicts))
            .map(|l| format!("{} 与 {} 矛盾", l.from_proposition_id, l.to_proposition_id))
            .collect();
        format!("\n\n## 注意：存在矛盾\n{}", contradictions.join("\n"))
    } else {
        String::new()
    };

    let prompt = format!(
        r#"基于以下研究分析，撰写一份结构化的研究综述。

# 研究主题
{topic}

# 子命题与证据
{propositions_text}
{contradictions_text}

请撰写综述，要求：
1. 引用证据时使用 [citation_label] 格式
2. 指出证据缺口
3. 如果存在矛盾，明确指出
4. 总结主要发现和建议的后续研究方向"#
    );

    let messages = vec![LlmMessage {
        role: MessageRole::User,
        content: prompt,
        tool_call_id: None,
        tool_calls: None,
    }];

    let request = GatewayRequest {
        provider: provider.clone(),
        messages,
        tools: vec![],
        max_tokens: Some(4000),
        temperature: Some(0.5),
        stream: false,
    };

    let gateway = ModelGateway::new(app_handle.clone(), vec![provider.clone()]);
    let response = gateway.send_request(request).await?;

    accumulate_usage(usage, &response.usage);

    Ok(response
        .content
        .unwrap_or_else(|| "无法生成综述".to_string()))
}

// ─── Helpers ─────────────────────────────────────────────

fn accumulate_usage(total: &mut TokenUsage, addition: &TokenUsage) {
    total.prompt_tokens += addition.prompt_tokens;
    total.completion_tokens += addition.completion_tokens;
    total.total_tokens += addition.total_tokens;
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sub_propositions_valid_json() {
        let json = r#"[
            {"id": "P1", "statement": "组织纪律的核心要求"},
            {"id": "P2", "statement": "违反组织纪律的常见情形"}
        ]"#;
        let props = parse_sub_propositions(json).unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(props[0].id, "P1");
    }

    #[test]
    fn parse_sub_propositions_with_code_block() {
        let json = r#"```json
[{"id": "P1", "statement": "测试命题"}]
```"#;
        let props = parse_sub_propositions(json).unwrap();
        assert_eq!(props.len(), 1);
    }

    #[test]
    fn build_evidence_matrix_empty() {
        let matrix = build_evidence_matrix("test", &[], &[]);
        assert_eq!(matrix.coverage_score, 0.0);
        assert_eq!(matrix.total_evidence_count, 0);
    }

    #[test]
    fn build_evidence_matrix_with_propositions() {
        let props = vec![
            SubProposition {
                id: "P1".into(),
                statement: "组织纪律".into(),
                evidence: vec![],
                gaps: vec![],
            },
            SubProposition {
                id: "P2".into(),
                statement: "廉洁纪律".into(),
                evidence: vec![],
                gaps: vec![],
            },
        ];
        let packets = vec![ContextPacket {
            id: "pkt-1".into(),
            source_type: crate::ai_runtime::SourceType::Note,
            source_path: Some("test.md".into()),
            title: "组织纪律概述".into(),
            heading_path: None,
            source_span: None,
            content_hash: "h1".into(),
            excerpt: "组织纪律是党的纪律的重要组成部分".into(),
            retrieval_reason: "fts".into(),
            score: 0.9,
            trust_level: TrustLevel::UserNote,
            citation_label: "[1]".into(),
            stale: false,
        }];

        let matrix = build_evidence_matrix("test topic", &props, &packets);
        assert_eq!(matrix.propositions.len(), 2);
        // P1 should match because "组织纪律" appears in the packet
        assert!(!matrix.propositions[0].evidence.is_empty());
    }

    #[test]
    fn parse_argument_chain_valid() {
        let json = r#"[
            {"from": "P1", "to": "P2", "link_type": "supports", "strength": 0.8}
        ]"#;
        let chain = parse_argument_chain(json).unwrap();
        assert_eq!(chain.links.len(), 1);
        assert!(!chain.has_contradictions);
    }

    #[test]
    fn parse_argument_chain_with_contradiction() {
        let json = r#"[
            {"from": "P1", "to": "P2", "link_type": "contradicts", "strength": 0.7}
        ]"#;
        let chain = parse_argument_chain(json).unwrap();
        assert!(chain.has_contradictions);
    }

    #[test]
    fn research_config_defaults() {
        let config = ResearchConfig::default();
        assert_eq!(config.max_rounds, 4);
        assert_eq!(config.max_tools_per_round, 6);
        assert!(!config.web_research_authorized);
    }
}
