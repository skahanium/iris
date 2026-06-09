//! Research Workflow 鈥?L3 limited agentic loop engine.
//!
//! The only scene that allows multi-round tool-calling loops.
//! Follows the pipeline:
//!   topic 鈫?sub-proposition decomposition 鈫?per-proposition retrieval 鈫?
//!   evidence matrix 鈫?gap identification 鈫?summary output
//!
//! Constraints (搂11.4):
//! - max_agentic_rounds = 4 (default)
//! - max_tool_calls_per_round = 6
//! - web research uses the global bottom-bar toggle (injected context, not a tool)
//! - external web evidence has lower trust than user notes & local regulations
//! - no fabricated citations

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::ai_runtime::{
    model_gateway::{
        GatewayRequest, LlmMessage, LlmToolDef, MessageRole, ModelGateway, ProviderConfig,
        TokenUsage, ToolCall,
    },
    scene_router::resolve_scene,
    session::SessionManager,
    tool_executor::{ToolRegistry, ToolSurfaceFilter},
    AiScene, AutonomyLevel, ContextPacket, ResearchProgress, ResearchTaskState, TrustLevel,
};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

// 鈹€鈹€鈹€ Research Types 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// A sub-proposition extracted from the research topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubProposition {
    pub id: String,
    pub statement: String,
    pub evidence: Vec<ContextPacket>,
    pub gaps: Vec<String>,
}

/// Evidence matrix: propositions 脳 evidence sources.
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
            token_budget: 240_000,
        }
    }
}

// 鈹€鈹€鈹€ Research Workflow Engine 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

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

    let _profile = resolve_scene(AiScene::ResearchSynthesis);
    let registry = ToolRegistry::new();

    // Ensure session
    let _session_key = "research_synthesis:__global__".to_string();
    let sid = SessionManager::ensure(db, AiScene::ResearchSynthesis, None)?;

    // Save user topic
    SessionManager::append_message(db, sid, "user", topic, None)?;

    let mut rounds: Vec<ResearchRound> = Vec::new();
    let mut total_usage = TokenUsage::default();

    // 鈹€鈹€ Phase 1: Sub-proposition decomposition 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€
    let sub_propositions = decompose_topic(
        app_handle,
        request_id,
        &provider_config,
        topic,
        &mut total_usage,
    )
    .await?;

    // 鈹€鈹€ Phase 2: Agentic retrieval loop (LLM-driven tool calling) 鈹€鈹€
    let mut accumulated_evidence: Vec<ContextPacket> = Vec::new();
    if config.web_research_authorized {
        push_topic_web_evidence(db, topic, &mut accumulated_evidence).await;
    }
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
            r#"浣犳槸鐮旂┒鍔╃悊锛屾鍦ㄨ繘琛岀 {round_num} 杞绱紙鍏?{} 杞級銆?

鐮旂┒涓婚: {topic}

瀛愬懡棰?
{propositions_desc}

宸叉敹闆嗚瘉鎹憳瑕?
{evidence_summary}

璇蜂娇鐢ㄥ彲鐢ㄥ伐鍏风户缁绱㈣瘉鎹€傚鏋滆瘉鎹凡鍏呭垎锛岀洿鎺ヨ緭鍑?"EVIDENCE_SUFFICIENT"銆?
姣忚疆鏈€澶氳皟鐢?{} 涓伐鍏枫€?#,
            config.max_rounds, config.max_tools_per_round
        );

        let messages = vec![LlmMessage {
            role: MessageRole::User,
            content: prompt,
            tool_call_id: None,
            tool_calls: None,
            ..Default::default()
        }];

        let request = GatewayRequest {
            provider: provider_config.clone(),
            messages,
            tools: llm_tools.clone(),
            max_tokens: Some(2000),
            temperature: Some(0.3),
            stream: true,
            thinking: false,
            skip_stub_ids: vec![],
        };

        let gateway =
            ModelGateway::with_defaults(app_handle.clone(), vec![provider_config.clone()])?;
        let response = gateway.send_streaming_request(request_id, request).await?;

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

            // Check tool permission
            let policy_ctx = ToolPolicyContext {
                scene: AiScene::ResearchSynthesis, autonomy_level: AutonomyLevel::L3, web_search_enabled: true, skill_allowed_tools: vec![], depth: 0, };
            if registry.check_tool_policy(&tool_call.function.name, &policy_ctx).is_err() { continue; }
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

    // 鈹€鈹€ Phase 3: Build evidence matrix 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€
    let evidence_matrix = build_evidence_matrix(topic, &sub_propositions, &accumulated_evidence);

    // 鈹€鈹€ Phase 4: Argument chain detection 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€
    let argument_chain = detect_argument_chains(
        app_handle,
        request_id,
        &provider_config,
        &evidence_matrix,
        &mut total_usage,
    )
    .await?;

    // 鈹€鈹€ Phase 5: Synthesize final summary 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€
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

// 鈹€鈹€鈹€ Agentic Loop Helpers 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

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
async fn push_topic_web_evidence(db: &Database, topic: &str, accumulated: &mut Vec<ContextPacket>) {
    let fetch = match crate::llm::search_web::fetch_search_context_for_db(db, topic).await {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!("Web search for research failed: {e}");
            return;
        }
    };
    let web_packets =
        crate::ai_runtime::evidence_mixer::web_packets_from_fetch(&fetch, topic, None);
    accumulated.extend(web_packets);
}

/// Format accumulated evidence into a concise summary for the LLM.
fn format_evidence_summary(packets: &[ContextPacket]) -> String {
    if packets.is_empty() {
        return "鏆傛棤宸叉敹闆嗚瘉鎹€?.to_string();
    }

    let mut summary = String::new();
    for (i, p) in packets.iter().take(20).enumerate() {
        summary.push_str(&format!(
            "{}. [{}] {} 鈥?{}\n",
            i + 1,
            p.citation_label,
            p.title,
            truncate_str(&p.excerpt, 100)
        ));
    }
    if packets.len() > 20 {
        summary.push_str(&format!("... 鍏?{} 鏉¤瘉鎹甛n", packets.len()));
    }
    summary
}

/// Truncate string helper for evidence summary.
fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        format!("{}鈥?, s.chars().take(max_chars).collect::<String>())
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
            let query = format!("銆妠reg_name}銆媨article}");
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
        _ => Ok(vec![]),
    }
}

// 鈹€鈹€鈹€ Sub-proposition Decomposition 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

async fn decompose_topic(
    app_handle: &AppHandle,
    _request_id: &str,
    provider: &ProviderConfig,
    topic: &str,
    usage: &mut TokenUsage,
) -> AppResult<Vec<SubProposition>> {
    let prompt = format!(
        r#"浣犳槸涓€涓鏈爺绌跺姪鐞嗐€傝灏嗕互涓嬬爺绌朵富棰樺垎瑙ｄ负 3-7 涓瓙鍛介锛屾瘡涓瓙鍛介搴斿綋锛?
1. 鍙互鐙珛妫€绱㈣瘉鎹?
2. 涓庝富棰樻湁鏄庣‘鐨勯€昏緫鍏崇郴
3. 浜掔浉涔嬮棿鏈夊尯鍒嗗害

鐮旂┒涓婚: {topic}

璇蜂互 JSON 鏁扮粍鏍煎紡杩斿洖瀛愬懡棰橈紝姣忎釜鍏冪礌鍖呭惈 "id"(濡?"P1") 鍜?"statement"(瀛愬懡棰橀檲杩?銆?
鍙繑鍥?JSON锛屼笉瑕佸叾浠栨枃瀛椼€?#
    );

    let messages = vec![LlmMessage {
        role: MessageRole::User,
        content: prompt,
        tool_call_id: None,
        tool_calls: None,
        ..Default::default()
    }];

    let request = GatewayRequest {
        provider: provider.clone(),
        messages,
        tools: vec![],
        max_tokens: Some(2000),
        temperature: Some(0.3),
        stream: true,
        thinking: false,
        skip_stub_ids: vec![],
    };

    let gateway = ModelGateway::with_defaults(app_handle.clone(), vec![provider.clone()])?;
    let response = gateway.send_streaming_request(_request_id, request).await?;

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

// 鈹€鈹€鈹€ Evidence Matrix 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

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
                    .split(|c: char| c.is_whitespace() || c == '锛? || c == '銆?)
                    .filter(|s| s.len() >= 2)
                    .collect();
                prop_terms
                    .iter()
                    .any(|term| p.excerpt.contains(term) || p.title.contains(term))
            })
            .cloned()
            .collect();

        let gaps = if evidence.is_empty() {
            vec![format!("瀛愬懡棰樸€寋}銆嶇己灏戠洿鎺ヨ瘉鎹?, prop.statement)]
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
                gaps.push("缂哄皯鐢ㄦ埛绗旇鏀拺".to_string());
            }
            if !has_regulation {
                gaps.push("缂哄皯娉曡鏉℃寮曠敤".to_string());
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

// 鈹€鈹€鈹€ Argument Chain Detection 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

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
                "- {}: {} (璇佹嵁: {})",
                p.id,
                p.statement,
                evidence_summary.join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        r#"鍒嗘瀽浠ヤ笅瀛愬懡棰樹箣闂寸殑璁鸿瘉鍏崇郴銆傚姣忓鐩稿叧鍛介锛屽垽鏂叧绯荤被鍨嬪苟璇勪及寮哄害銆?

瀛愬懡棰?
{propositions_desc}

璇蜂互 JSON 鏁扮粍鏍煎紡杩斿洖璁鸿瘉閾炬帴锛屾瘡涓厓绱犲寘鍚?
- "from": 婧愬懡棰?ID
- "to": 鐩爣鍛介 ID
- "link_type": "supports" | "contradicts" | "prerequisite" | "consequence" | "parallel"
- "strength": 0.0-1.0 鐨勬诞鐐规暟

鍙繑鍥炵浉鍏崇殑閾炬帴锛坰trength > 0.3锛夛紝鍙繑鍥?JSON锛屼笉瑕佸叾浠栨枃瀛椼€?#
    );

    let messages = vec![LlmMessage {
        role: MessageRole::User,
        content: prompt,
        tool_call_id: None,
        tool_calls: None,
        ..Default::default()
    }];

    let request = GatewayRequest {
        provider: provider.clone(),
        messages,
        tools: vec![],
        max_tokens: Some(2000),
        temperature: Some(0.2),
        stream: true,
        thinking: false,
        skip_stub_ids: vec![],
    };

    let gateway = ModelGateway::with_defaults(app_handle.clone(), vec![provider.clone()])?;
    let response = gateway.send_streaming_request(_request_id, request).await?;

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

// 鈹€鈹€鈹€ Summary Synthesis 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

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
                "## {} {}\n璇佹嵁: {}\n缂哄彛: {}",
                p.id,
                p.statement,
                if evidence_citations.is_empty() {
                    "鏃?.to_string()
                } else {
                    evidence_citations.join("; ")
                },
                if p.gaps.is_empty() {
                    "鏃?.to_string()
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
            .map(|l| format!("{} 涓?{} 鐭涚浘", l.from_proposition_id, l.to_proposition_id))
            .collect();
        format!("\n\n## 娉ㄦ剰锛氬瓨鍦ㄧ煕鐩綷n{}", contradictions.join("\n"))
    } else {
        String::new()
    };

    let prompt = format!(
        r#"鍩轰簬浠ヤ笅鐮旂┒鍒嗘瀽锛屾挵鍐欎竴浠界粨鏋勫寲鐨勭爺绌剁患杩般€?

# 鐮旂┒涓婚
{topic}

# 瀛愬懡棰樹笌璇佹嵁
{propositions_text}
{contradictions_text}

璇锋挵鍐欑患杩帮紝瑕佹眰锛?
1. 寮曠敤璇佹嵁鏃朵娇鐢?[citation_label] 鏍煎紡
2. 鎸囧嚭璇佹嵁缂哄彛
3. 濡傛灉瀛樺湪鐭涚浘锛屾槑纭寚鍑?
4. 鎬荤粨涓昏鍙戠幇鍜屽缓璁殑鍚庣画鐮旂┒鏂瑰悜"#
    );

    let messages = vec![LlmMessage {
        role: MessageRole::User,
        content: prompt,
        tool_call_id: None,
        tool_calls: None,
        ..Default::default()
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
        .unwrap_or_else(|| "鏃犳硶鐢熸垚缁艰堪".to_string()))
}

// 鈹€鈹€鈹€ Helpers 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

fn accumulate_usage(total: &mut TokenUsage, addition: &TokenUsage) {
    total.prompt_tokens += addition.prompt_tokens;
    total.completion_tokens += addition.completion_tokens;
    total.total_tokens += addition.total_tokens;
}

// 鈹€鈹€鈹€ Tests 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sub_propositions_valid_json() {
        let json = r#"[
            {"id": "P1", "statement": "缁勭粐绾緥鐨勬牳蹇冭姹?},
            {"id": "P2", "statement": "杩濆弽缁勭粐绾緥鐨勫父瑙佹儏褰?}
        ]"#;
        let props = parse_sub_propositions(json).unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(props[0].id, "P1");
    }

    #[test]
    fn parse_sub_propositions_with_code_block() {
        let json = r#"```json
[{"id": "P1", "statement": "娴嬭瘯鍛介"}]
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
                statement: "缁勭粐绾緥".into(),
                evidence: vec![],
                gaps: vec![],
            },
            SubProposition {
                id: "P2".into(),
                statement: "寤夋磥绾緥".into(),
                evidence: vec![],
                gaps: vec![],
            },
        ];
        let packets = vec![ContextPacket {
            id: "pkt-1".into(),
            source_type: crate::ai_runtime::SourceType::Note,
            source_path: Some("test.md".into()),
            title: "缁勭粐绾緥姒傝堪".into(),
            heading_path: None,
            source_span: None,
            content_hash: "h1".into(),
            excerpt: "缁勭粐绾緥鏄厷鐨勭邯寰嬬殑閲嶈缁勬垚閮ㄥ垎".into(),
            retrieval_reason: "fts".into(),
            score: 0.9,
            trust_level: TrustLevel::UserNote,
            citation_label: "[1]".into(),
            stale: false,
            web: None,
        }];

        let matrix = build_evidence_matrix("test topic", &props, &packets);
        assert_eq!(matrix.propositions.len(), 2);
        // P1 should match because "缁勭粐绾緥" appears in the packet
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
