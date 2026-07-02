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
//! - web research uses the global bottom-bar toggle (injected context, not a tool)
//! - external web evidence has lower trust than user notes & local regulations
//! - no fabricated citations

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::ai_runtime::web_evidence_broker::{
    collect_web_evidence, web_evidence_items_to_packets, WebEvidenceBrokerInput,
};
use crate::ai_runtime::{
    model_gateway::{
        GatewayRequest, LlmMessage, LlmToolDef, MessageRole, ModelGateway, ProviderConfig,
        TokenUsage, ToolCall,
    },
    research_state::{save_research_state, ResearchState, ResearchStateInput},
    session::SessionManager,
    tool_executor::{ToolRegistry, ToolSurfaceFilter},
    tool_policy::ToolPolicyContext,
    AiScene, AutonomyLevel, ContextPacket, ResearchProgress, ResearchTaskState, TrustLevel,
};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

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
    pub research_state: ResearchState,
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
            token_budget: 240_000,
        }
    }
}

// ─── Research Workflow Engine ────────────────────────────

/// Execute the research workflow as an L3 agentic loop.
///
/// Supports cancel_token for abort and emits `ai:research_progress` events per round.
#[allow(clippy::too_many_arguments)]
pub async fn execute_research(
    db: &Database,
    app_handle: &AppHandle,
    request_id: &str,
    topic: &str,
    config: ResearchConfig,
    provider_config: ProviderConfig,
    web_authorized: bool,
    cancel_token: Option<Arc<AtomicBool>>,
) -> AppResult<ResearchResult> {
    let mut config = config;
    config.web_research_authorized = web_authorized;

    let registry = ToolRegistry::new();

    // Ensure session
    let _session_key = "research_synthesis:__global__".to_string();
    let sid = SessionManager::ensure(db, AiScene::ResearchSynthesis, None)?;

    // Save user topic
    SessionManager::append_message(db, sid, "user", topic, None, None)?;

    let mut rounds: Vec<ResearchRound> = Vec::new();
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

    // ── Phase 2: Agentic retrieval loop (LLM-driven tool calling) ──
    let mut accumulated_evidence: Vec<ContextPacket> = Vec::new();
    push_topic_web_evidence(
        db,
        topic,
        config.web_research_authorized,
        &mut accumulated_evidence,
    )
    .await;
    let llm_tools = build_research_tool_defs(&registry, config.web_research_authorized);

    for round_num in 0..config.max_rounds {
        // Check abort signal before each round
        if let Some(ref token) = cancel_token {
            if token.load(Ordering::Relaxed) {
                tracing::info!("Research {request_id} aborted at round {round_num}");
                break;
            }
        }

        // Check token budget before each round
        if total_usage.total_tokens as usize >= config.token_budget {
            break;
        }

        let mut round = ResearchRound {
            round_number: round_num + 1,
            queries_executed: Vec::new(),
            packets_retrieved: Vec::new(),
            tool_calls_made: 0,
            llm_output: None,
        };

        // Build context from accumulated evidence
        let evidence_summary = format_evidence_summary(&accumulated_evidence);
        let propositions_desc = sub_propositions
            .iter()
            .map(|p| format!("- {}: {}", p.id, p.statement))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"你是研究助理，正在进行第 {round_num} 轮检索（共 {} 轮）。

研究主题: {topic}

子命题:
{propositions_desc}

已收集证据摘要:
{evidence_summary}

请使用可用工具继续检索证据。如果证据已充分，直接输出 "EVIDENCE_SUFFICIENT"。
每轮最多调用 {} 个工具。"#,
            config.max_rounds, config.max_tools_per_round
        );

        let messages = vec![LlmMessage {
            role: MessageRole::User,
            content: prompt.into(),
            reasoning_content: None,
            tool_call_id: None,
            tool_calls: None,
        }];

        let request = GatewayRequest {
            provider: provider_config.clone(),
            messages,
            tools: llm_tools.clone(),
            max_tokens: Some(2000),
            temperature: Some(0.3),
            stream: false,
            thinking: false,
            skip_stub_ids: vec![],
        };

        let gateway =
            ModelGateway::with_defaults(app_handle.clone(), vec![provider_config.clone()])?;
        let response = gateway.send_request(request).await?;

        accumulate_usage(&mut total_usage, &response.usage);

        // Process tool calls from LLM
        if let Some(content) = &response.content {
            if content.contains("EVIDENCE_SUFFICIENT") {
                round.llm_output = Some(content.clone());

                // Capture values before move
                let queries = round.queries_executed.clone();
                let new_evidence = round.packets_retrieved.len();
                let total_evidence = accumulated_evidence.len();
                rounds.push(round);

                let _ = app_handle
                    .emit(
                        "ai:research_progress",
                        &ResearchProgress {
                            request_id: request_id.to_string(),
                            topic: topic.to_string(),
                            state: ResearchTaskState::Completed,
                            current_round: round_num + 1,
                            max_rounds: config.max_rounds,
                            queries_executed: queries,
                            new_evidence_count: new_evidence,
                            total_evidence_count: total_evidence,
                            tokens_used: total_usage.total_tokens,
                            token_budget: config.token_budget,
                            progress_pct: 1.0,
                            round_terminated_early: true,
                        },
                    )
                    .ok();

                break;
            }
        }

        for tool_call in &response.tool_calls {
            if round.tool_calls_made >= config.max_tools_per_round {
                break;
            }
            if total_usage.total_tokens as usize >= config.token_budget {
                break;
            }

            let policy_ctx = ToolPolicyContext {
                task_policy: None,
                scene: AiScene::ResearchSynthesis,
                autonomy_level: AutonomyLevel::L3,
                web_search_enabled: config.web_research_authorized,
                depth: 0,
            };
            if registry
                .check_tool_policy(&tool_call.function.name, &policy_ctx)
                .is_err()
            {
                continue;
            }

            let new_packets =
                execute_tool_call(db, app_handle, &provider_config, tool_call, &config)
                    .await
                    .unwrap_or_default();

            round.queries_executed.push(format!(
                "{}({})",
                tool_call.function.name, tool_call.function.arguments
            ));
            round.packets_retrieved.extend(new_packets.clone());
            round.tool_calls_made += 1;
            accumulated_evidence.extend(new_packets);
        }

        // Deduplicate
        accumulated_evidence.dedup_by(|a, b| a.id == b.id);

        let new_evidence_count = round.packets_retrieved.len();
        round.llm_output = response.content.clone();
        rounds.push(round);

        // Emit per-round progress event to frontend
        let progress = ResearchProgress {
            request_id: request_id.to_string(),
            topic: topic.to_string(),
            state: ResearchTaskState::Retrieving,
            current_round: round_num + 1,
            max_rounds: config.max_rounds,
            queries_executed: rounds
                .last()
                .map(|r| r.queries_executed.clone())
                .unwrap_or_default(),
            new_evidence_count,
            total_evidence_count: accumulated_evidence.len(),
            tokens_used: total_usage.total_tokens,
            token_budget: config.token_budget,
            progress_pct: (round_num + 1) as f64 / config.max_rounds as f64,
            round_terminated_early: false,
        };
        let _ = app_handle.emit("ai:research_progress", &progress);

        if new_evidence_count == 0 {
            break;
        }
    }

    // ── Phase 3: Build evidence matrix ──────────────────
    let mut evidence_matrix =
        build_evidence_matrix(topic, &sub_propositions, &accumulated_evidence);
    if !config.web_research_authorized && evidence_matrix.total_evidence_count == 0 {
        evidence_matrix
            .global_gaps
            .push("联网关闭，未检索外部来源".to_string());
    }

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
    SessionManager::append_message(db, sid, "assistant", &summary, None, None)?;
    let research_state = ResearchState::from_input(ResearchStateInput {
        request_id: request_id.to_string(),
        topic: topic.to_string(),
        questions: evidence_matrix
            .propositions
            .iter()
            .map(|proposition| format!("{}: {}", proposition.id, proposition.statement))
            .collect(),
        evidence: evidence_matrix
            .propositions
            .iter()
            .flat_map(|proposition| proposition.evidence.clone())
            .collect(),
        global_gaps: evidence_matrix.global_gaps.clone(),
        has_contradictions: argument_chain.has_contradictions,
        summary: summary.clone(),
    });
    let _ = save_research_state(db, &research_state);

    Ok(ResearchResult {
        request_id: request_id.to_string(),
        topic: topic.to_string(),
        rounds,
        evidence_matrix,
        argument_chain,
        summary,
        total_tokens: total_usage,
        research_state,
    })
}

// ─── Agentic Loop Helpers ────────────────────────────────

/// Build LLM tool definitions for the research agentic loop (read-only auto tools only).
fn build_research_tool_defs(registry: &ToolRegistry, web_search_enabled: bool) -> Vec<LlmToolDef> {
    let tools = registry.tools_for_surface(
        AiScene::ResearchSynthesis,
        ToolSurfaceFilter {
            web_search_enabled,
            depth: 0,
            only_auto: true,
        },
    );
    ModelGateway::tools_to_llm_format(&tools)
}

/// Pre-fetch web search results for the research topic when the global toggle is on.
async fn push_topic_web_evidence(
    db: &Database,
    topic: &str,
    enabled: bool,
    accumulated: &mut Vec<ContextPacket>,
) {
    let evidence = match collect_web_evidence(
        db,
        WebEvidenceBrokerInput {
            query: topic.to_string(),
            urls: Vec::new(),
            enabled,
            max_search_results: 8,
            max_fetches: 3,
        },
    )
    .await
    {
        Ok(items) => items,
        Err(error) => {
            tracing::warn!("Web evidence broker failed: {error}");
            return;
        }
    };
    accumulated.extend(web_evidence_items_to_packets(topic, &evidence));
}

/// Format accumulated evidence into a concise summary for the LLM.
fn format_evidence_summary(packets: &[ContextPacket]) -> String {
    if packets.is_empty() {
        return "暂无已收集证据。".to_string();
    }

    let mut summary = String::new();
    for (i, p) in packets.iter().take(20).enumerate() {
        summary.push_str(&format!(
            "{}. [{}] {} — {}\n",
            i + 1,
            p.citation_label,
            p.title,
            truncate_str(&p.excerpt, 100)
        ));
    }
    if packets.len() > 20 {
        summary.push_str(&format!("... 共 {} 条证据\n", packets.len()));
    }
    summary
}

/// Truncate string helper for evidence summary.
fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_chars).collect::<String>())
    }
}

/// Execute a single tool call from the LLM during the agentic loop.
async fn execute_tool_call(
    db: &Database,
    _app_handle: &AppHandle,
    _provider_config: &ProviderConfig,
    tool_call: &ToolCall,
    _config: &ResearchConfig,
) -> AppResult<Vec<ContextPacket>> {
    let args: serde_json::Value =
        serde_json::from_str(&tool_call.function.arguments).unwrap_or(serde_json::json!({}));

    match tool_call.function.name.as_str() {
        "search_hybrid" | "search_semantic" | "search_keyword" => {
            let query = args["query"].as_str().unwrap_or("");
            db.with_conn(|conn| {
                let request = crate::ai_runtime::retrieval_broker::RetrievalRequest {
                    scope: crate::ai_runtime::retrieval_scope::RetrievalScope::default(),
                    query: query.to_string(),
                    max_results: 10,
                    layers: crate::ai_runtime::retrieval_broker::RetrievalLayers {
                        fts: tool_call.function.name == "search_hybrid"
                            || tool_call.function.name == "search_keyword",
                        vector: tool_call.function.name == "search_hybrid"
                            || tool_call.function.name == "search_semantic",
                        graph: false,
                        exact: false,
                        template: false,
                    },
                    note_context: None,
                    file_id_context: None,
                };
                crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request)
            })
        }
        "get_regulation" => {
            let reg_name = args["regulation_name"].as_str().unwrap_or("");
            let article = args["article"].as_str().unwrap_or("");
            let query = format!("《{reg_name}》{article}");
            db.with_conn(|conn| {
                let request = crate::ai_runtime::retrieval_broker::RetrievalRequest {
                    scope: crate::ai_runtime::retrieval_scope::RetrievalScope::default(),
                    query,
                    max_results: 5,
                    layers: crate::ai_runtime::retrieval_broker::RetrievalLayers {
                        fts: false,
                        vector: false,
                        graph: false,
                        exact: true,
                        template: false,
                    },
                    note_context: None,
                    file_id_context: None,
                };
                crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request)
            })
        }
        "get_context_packets" => {
            // Return already-accumulated packets (no-op, they're tracked externally)
            Ok(vec![])
        }
        "web_search" => {
            execute_web_search_tool_call(db, &args, _config.web_research_authorized).await
        }
        _ => Ok(vec![]),
    }
}

async fn execute_web_search_tool_call(
    db: &Database,
    args: &serde_json::Value,
    enabled: bool,
) -> AppResult<Vec<ContextPacket>> {
    let input = web_search_broker_input(args, enabled);
    let query = input.query.clone();
    let evidence = collect_web_evidence(db, input).await?;
    Ok(web_evidence_items_to_packets(&query, &evidence))
}

fn web_search_broker_input(args: &serde_json::Value, enabled: bool) -> WebEvidenceBrokerInput {
    WebEvidenceBrokerInput {
        query: args["query"].as_str().unwrap_or("").to_string(),
        urls: args["urls"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        enabled,
        max_search_results: 8,
        max_fetches: 3,
    }
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
        content: prompt.into(),
        reasoning_content: None,
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
        thinking: false,
        skip_stub_ids: vec![],
    };

    let gateway = ModelGateway::with_defaults(app_handle.clone(), vec![provider.clone()])?;
    let response = gateway.send_request(request).await?;

    accumulate_usage(usage, &response.usage);

    let content = response.content.unwrap_or_default();
    match parse_sub_propositions(&content) {
        Ok(props) if !props.is_empty() => Ok(props),
        Ok(_) => Ok(fallback_sub_propositions(topic)),
        Err(error) => {
            tracing::warn!("sub-proposition decomposition degraded to fallback: {error}");
            Ok(fallback_sub_propositions(topic))
        }
    }
}

fn parse_sub_propositions(json_str: &str) -> AppResult<Vec<SubProposition>> {
    // Try to extract JSON from the response (may be wrapped in markdown code block)
    let json_str = json_str.trim();
    let json_str = extract_json_array(json_str)
        .ok_or_else(|| AppError::msg("failed to parse sub-propositions: missing JSON array"))?;

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

fn extract_json_array(input: &str) -> Option<&str> {
    let start = input.find('[')?;
    let end = input.rfind(']')?;
    if start > end {
        return None;
    }
    Some(&input[start..=end])
}

fn fallback_sub_propositions(topic: &str) -> Vec<SubProposition> {
    let statement = topic
        .split(['。', '，', ',', ';', '；', '\n'])
        .map(str::trim)
        .find(|part| !part.is_empty())
        .unwrap_or(topic)
        .trim();
    vec![SubProposition {
        id: "P1".to_string(),
        statement: if statement.is_empty() {
            "研究主题".to_string()
        } else {
            statement.to_string()
        },
        evidence: Vec::new(),
        gaps: Vec::new(),
    }]
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
            Vec::new()
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
        content: prompt.into(),
        reasoning_content: None,
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
        thinking: false,
        skip_stub_ids: vec![],
    };

    let gateway = ModelGateway::with_defaults(app_handle.clone(), vec![provider.clone()])?;
    let response = gateway.send_request(request).await?;

    accumulate_usage(usage, &response.usage);

    let content = response.content.unwrap_or_default();
    parse_argument_chain(&content)
}

fn parse_argument_chain(json_str: &str) -> AppResult<ArgumentChain> {
    let json_str = json_str.trim();
    if json_str.is_empty() {
        return Ok(empty_argument_chain());
    }
    let json_str = if json_str.starts_with("```") {
        match (json_str.find('['), json_str.rfind(']')) {
            (Some(start), Some(end)) if start <= end => &json_str[start..=end],
            _ => return Ok(empty_argument_chain()),
        }
    } else {
        json_str
    };
    if json_str.is_empty() {
        return Ok(empty_argument_chain());
    }

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

fn empty_argument_chain() -> ArgumentChain {
    ArgumentChain {
        links: Vec::new(),
        has_contradictions: false,
        chain_strength: 0.0,
    }
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
        content: prompt.into(),
        reasoning_content: None,
        tool_call_id: None,
        tool_calls: None,
    }];

    let request = GatewayRequest {
        provider: provider.clone(),
        messages,
        tools: vec![],
        max_tokens: Some(4000),
        temperature: Some(0.5),
        stream: true,
        thinking: false,
        skip_stub_ids: vec![],
    };

    let gateway = ModelGateway::with_defaults(app_handle.clone(), vec![provider.clone()])?;
    let response = gateway.send_streaming_request(_request_id, request).await?;

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
    fn parse_sub_propositions_extracts_json_array_from_model_text() {
        let json = r#"可以，分解如下：
[
  {"id": "P1", "statement": "Chatbot Arena 最新排名"},
  {"id": "P2", "statement": "SWE-bench Live 最新排名"}
]
以上子命题可独立检索。"#;
        let props = parse_sub_propositions(json).unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(props[0].statement, "Chatbot Arena 最新排名");
    }

    #[test]
    fn fallback_sub_propositions_keeps_research_running_after_bad_json() {
        let props = fallback_sub_propositions("2026年6月最新模型榜单、消息速度和来源");

        assert!(!props.is_empty());
        assert_eq!(props[0].id, "P1");
        assert!(props[0].statement.contains("2026年6月最新模型榜单"));
        assert!(props.iter().all(|prop| prop.evidence.is_empty()));
        assert!(props.iter().all(|prop| prop.gaps.is_empty()));
    }

    #[test]
    fn build_evidence_matrix_empty() {
        let matrix = build_evidence_matrix("test", &[], &[]);
        assert_eq!(matrix.coverage_score, 0.0);
        assert_eq!(matrix.total_evidence_count, 0);
        assert!(matrix.global_gaps.is_empty());
    }

    #[test]
    fn empty_evidence_does_not_create_matrix_artifact() {
        let props = vec![SubProposition {
            id: "P1".into(),
            statement: "需要真实资料支撑的命题".into(),
            evidence: vec![],
            gaps: vec![],
        }];

        let matrix = build_evidence_matrix("test topic", &props, &[]);

        assert_eq!(matrix.total_evidence_count, 0);
        assert_eq!(matrix.coverage_score, 0.0);
        assert!(matrix.global_gaps.is_empty());
        assert!(matrix.propositions[0].gaps.is_empty());
    }

    #[test]
    fn mechanical_gap_without_source_is_not_a_displayable_gap() {
        let props = vec![SubProposition {
            id: "P1".into(),
            statement: "模型拆出来但没有来源的子命题".into(),
            evidence: vec![],
            gaps: vec!["子命题缺少直接证据".into()],
        }];

        let matrix = build_evidence_matrix("test topic", &props, &[]);

        assert!(matrix.global_gaps.is_empty());
        assert!(matrix.propositions[0].gaps.is_empty());
    }

    #[test]
    fn evidence_sources_include_real_source_count() {
        let props = vec![SubProposition {
            id: "P1".into(),
            statement: "组织纪律".into(),
            evidence: vec![],
            gaps: vec![],
        }];
        let packets = vec![
            ContextPacket {
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
                web: None,
                corpus: None,
            },
            ContextPacket {
                id: "pkt-2".into(),
                source_type: crate::ai_runtime::SourceType::Note,
                source_path: Some("test-2.md".into()),
                title: "组织纪律案例".into(),
                heading_path: None,
                source_span: None,
                content_hash: "h2".into(),
                excerpt: "组织纪律案例材料".into(),
                retrieval_reason: "fts".into(),
                score: 0.8,
                trust_level: TrustLevel::UserNote,
                citation_label: "[2]".into(),
                stale: false,
                web: None,
                corpus: None,
            },
        ];

        let matrix = build_evidence_matrix("test topic", &props, &packets);

        assert_eq!(matrix.total_evidence_count, 2);
        assert_eq!(matrix.coverage_score, 1.0);
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
            web: None,
            corpus: None,
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
    fn parse_argument_chain_empty_response_degrades_to_empty_chain() {
        let chain = parse_argument_chain("").unwrap();
        assert!(chain.links.is_empty());
        assert!(!chain.has_contradictions);
        assert_eq!(chain.chain_strength, 0.0);
    }

    #[test]
    fn research_web_search_tool_builds_broker_input_without_page_fetches() {
        let args = serde_json::json!({ "query": "网络证据代理" });
        let input = web_search_broker_input(&args, true);

        assert_eq!(input.query, "网络证据代理");
        assert!(input.enabled);
        assert_eq!(input.max_search_results, 8);
        assert_eq!(input.max_fetches, 3);
    }

    #[test]
    fn research_web_search_tool_converts_broker_items_to_packets() {
        let items = vec![crate::ai_runtime::web_evidence_broker::WebEvidenceItem {
            url: "https://example.com/source".into(),
            canonical_url: "https://example.com/source".into(),
            title: "Source".into(),
            domain: "example.com".into(),
            snippet: "Evidence snippet".into(),
            fetched_excerpt: None,
            provider_id: "anysearch".into(),
            provider_kind: "mcp".into(),
            cost_class: "free".into(),
            raw_result_hash: "hash".into(),
            extraction_method: "search_snippet".into(),
            trust_level: "external_untrusted".into(),
            retrieval_reason: "web.search".into(),
            search_backend: crate::ai_runtime::WebSearchBackend::Provider,
            source_rank: crate::ai_runtime::WebSourceRank::Unknown,
            freshness_label: None,
            failure_reason: None,
            conflict_group: None,
            conflict_note: None,
        }];

        let packets = web_evidence_items_to_packets("query", &items);

        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].retrieval_reason, "web_evidence_broker");
        assert_eq!(packets[0].excerpt, "Evidence snippet");
    }

    #[test]
    fn research_config_defaults() {
        let config = ResearchConfig::default();
        assert_eq!(config.max_rounds, 4);
        assert_eq!(config.max_tools_per_round, 6);
        assert!(!config.web_research_authorized);
    }
}
