//! Unified harness task request/result/artifact contract.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

use crate::ai_runtime::agent_task::{
    AgentTaskKind, AgentTaskRuntime, AgentTaskStatus, CreateTaskInput,
};
use crate::ai_runtime::assistant_facade::{
    parse_document_check_type, parse_organize_task_type, AssistantIntent,
};
use crate::ai_runtime::chapter_workflow::{self, ChapterInfo, ChapterWritingInput};
use crate::ai_runtime::document_workflow::DocumentCheckInput;
use crate::ai_runtime::document_workflow::DocumentCheckResult;
use crate::ai_runtime::model_gateway::TokenUsage;
use crate::ai_runtime::writing_state::{WritingState, WritingStateInput};
use crate::ai_runtime::writing_workflow::WritingTaskOutput;
use crate::ai_runtime::{
    CitationCheckInput, CitationCheckResult, CitationCheckScope, ContextPacket, OrganizeTaskInput,
    OrganizeTaskResult, OrganizeTaskScope, PatchProposal, SourceSpan, SourceType, TrustLevel,
};
use crate::ai_types::TaskPlanSummary;
use crate::ai_types::{LlmMessage, MessageRole};
use crate::app::AppState;
use crate::commands::assistant_commands::{
    validate_assistant_domain_boundary, AssistantAiDomain, AssistantExecuteRequest,
};
use crate::commands::writing_commands::WritingTaskInputIpc;
use crate::commands::{
    ai_commands, citation_commands, document_commands, organize_commands, research_commands,
    writing_commands,
};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

/// Unified run status for all assistant intents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessRunStatus {
    Completed,
    PendingConfirmation,
    Failed,
    Aborted,
}

/// Artifact kinds returned by harness task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HarnessArtifact {
    Message {
        content: String,
        citation_valid: bool,
    },
    Patches {
        patches: Vec<PatchProposal>,
    },
    CitationReport {
        report: CitationCheckResult,
    },
    OrganizeReport {
        report: OrganizeTaskResult,
    },
    ResearchReport {
        payload: serde_json::Value,
    },
    DocumentCheck {
        report: DocumentCheckResult,
    },
    ChapterWriting {
        payload: serde_json::Value,
    },
    ToolConfirmation {
        request_id: String,
        tool_call_id: String,
    },
    LegacyPayload {
        intent: String,
        json: serde_json::Value,
    },
}

/// Wire shape for optional frontend artifact panel (migration).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessArtifactWire {
    pub kind: String,
    pub title: String,
    pub status: String,
    pub source_task: String,
    pub evidence_count: u32,
    pub payload: serde_json::Value,
}

/// Unified harness task request (maps from `AssistantExecuteRequest`).
#[derive(Debug, Clone)]
pub(crate) struct HarnessTaskRequest {
    pub(crate) assistant: AssistantExecuteRequest,
    pub(crate) task_plan: TaskPlanSummary,
    pub(crate) routing_override: Option<ai_commands::AiSendRoutingOverride>,
}

impl HarnessTaskRequest {
    pub(crate) fn from_assistant_with_routing(
        assistant: AssistantExecuteRequest,
        task_plan: TaskPlanSummary,
        routing_override: ai_commands::AiSendRoutingOverride,
    ) -> Self {
        Self {
            assistant,
            task_plan,
            routing_override: Some(routing_override),
        }
    }
}

/// Internal task result before mapping to IPC.
#[derive(Debug, Clone, Serialize)]
pub struct HarnessTaskResult {
    pub request_id: String,
    pub task_id: Option<String>,
    pub run_status: HarnessRunStatus,
    pub artifacts: Vec<HarnessArtifact>,
    pub artifact_wires: Vec<HarnessArtifactWire>,
    pub evidence_packet_ids: Vec<String>,
    pub usage: Option<TokenUsage>,
    pub evidence_refresh_notice: Option<String>,
    pub chat_payload: Option<serde_json::Value>,
    pub writing: Option<WritingTaskOutput>,
    pub citation: Option<CitationCheckResult>,
    pub organize: Option<OrganizeTaskResult>,
    pub research: Option<serde_json::Value>,
    pub chapter: Option<serde_json::Value>,
    pub document: Option<DocumentCheckResult>,
}

/// Execute unified assistant task and collect artifacts.
fn run_status_wire(status: HarnessRunStatus) -> &'static str {
    match status {
        HarnessRunStatus::Completed => "completed",
        HarnessRunStatus::PendingConfirmation => "pending_confirmation",
        HarnessRunStatus::Failed => "failed",
        HarnessRunStatus::Aborted => "aborted",
    }
}

fn emit_workflow_trace(app_handle: &AppHandle, request_id: &str, task_name: &str, status: &str) {
    use crate::ai_runtime::harness::{HarnessPhase, HarnessTraceEvent};
    let _ = app_handle.emit(
        "ai:harness_trace",
        &HarnessTraceEvent {
            request_id: request_id.to_string(),
            round: 0,
            phase: HarnessPhase::ToolStart,
            tool_name: task_name.to_string(),
            status: status.to_string(),
            message: None,
            output_preview: None,
        },
    );
}

fn skill_scope_wire(scope: crate::ai_runtime::skills::SkillScope) -> &'static str {
    match scope {
        crate::ai_runtime::skills::SkillScope::Global => "Global",
        crate::ai_runtime::skills::SkillScope::Vault => "Vault",
    }
}

fn legacy_skill_overlay_from_plan(
    state: &AppState,
    scene: crate::ai_runtime::AiScene,
    user_message: &str,
    plan: Option<&crate::ai_types::SkillActivationPlanSummary>,
) -> String {
    let Some(plan) = plan.filter(|plan| !plan.activated_skills.is_empty()) else {
        return String::new();
    };
    let Ok(vault) = state.vault_path() else {
        return String::new();
    };
    let Ok(skills) = crate::ai_runtime::skills::scan_all(&vault) else {
        return String::new();
    };
    let selected: Vec<_> = skills
        .into_iter()
        .filter(|skill| {
            plan.activated_skills.iter().any(|active| {
                active.blocked_capabilities.is_empty()
                    && active.name == skill.name
                    && active.scope == skill_scope_wire(skill.scope)
            })
        })
        .collect();
    if selected.is_empty() {
        return String::new();
    }
    crate::ai_runtime::skills::inject_into_prompt(&vault, &selected, scene, user_message)
}

fn apply_skill_overlay_to_goal(goal: &str, overlay: &str) -> String {
    let overlay = overlay.trim();
    if overlay.is_empty() {
        goal.to_string()
    } else {
        format!("{goal}\n\n## Active Skill Guidance\n{overlay}")
    }
}

fn creates_own_runtime_task(intent: AssistantIntent) -> bool {
    !matches!(intent, AssistantIntent::Chat | AssistantIntent::Knowledge)
}

fn create_workflow_runtime_task(
    db: &Database,
    request: &AssistantExecuteRequest,
    legacy_intent: AssistantIntent,
) -> AppResult<String> {
    let session_id = if let Some(session_id) = request.session_id {
        session_id
    } else {
        crate::ai_runtime::session::SessionManager::ensure(
            db,
            legacy_intent.scene(),
            request.note_path.as_deref(),
        )?
    };
    AgentTaskRuntime::create_task(
        db,
        CreateTaskInput {
            request_id: uuid::Uuid::new_v4().to_string(),
            session_id,
            kind: AgentTaskKind::Complex,
            user_input: request.message.clone(),
            budget_policy: serde_json::json!({
                "mode": "assistant_workflow",
                "intent": legacy_intent_wire(legacy_intent),
            }),
        },
    )
}

fn complete_workflow_runtime_task(
    db: &Database,
    task_id: &str,
    result: &HarnessTaskResult,
) -> AppResult<()> {
    AgentTaskRuntime::record_step(
        db,
        task_id,
        "assistant_workflow",
        AgentTaskStatus::Completed,
        "assistant workflow input summarized in agent_tasks",
        "assistant task completed; no process artifact generated for ordinary completion",
        serde_json::json!({
            "summary": "assistant workflow completed",
            "request_id": result.request_id,
            "artifact_kinds": result.artifacts.iter().map(artifact_kind).collect::<Vec<_>>(),
            "evidence_packet_ids": result.evidence_packet_ids,
        }),
    )?;
    AgentTaskRuntime::complete_task(db, task_id)
}

fn fail_workflow_runtime_task(db: &Database, task_id: &str, error_code: &str) -> AppResult<()> {
    AgentTaskRuntime::fail_safe(db, task_id, error_code)
}

fn read_classified_document(vault: &std::path::Path, note_path: &str) -> AppResult<String> {
    let abs_path = crate::storage::paths::resolve_vault_path(vault, note_path)?;
    let raw = std::fs::read(&abs_path)?;
    let vk_guard = crate::crypto::vault_key::VAULT_KEY
        .get()
        .ok_or_else(|| AppError::msg("保险库未初始化"))?;
    let vk = vk_guard
        .read()
        .map_err(|e| AppError::msg(format!("lock error: {e}")))?;
    let key = vk.key()?;
    let decrypted = crate::crypto::classified_io::decrypt_cef(&raw, key)?;
    String::from_utf8(decrypted).map_err(|_| AppError::msg("Classified note is not valid UTF-8"))
}

fn classified_query_requests_vault_scope(message: &str) -> bool {
    const KEYWORDS: [&str; 6] = [
        "涉密库",
        "保险库",
        "其他涉密文档",
        "全库",
        "所有涉密",
        "全部涉密",
    ];
    KEYWORDS.iter().any(|keyword| message.contains(keyword))
}

fn validate_classified_scope(
    scope: Option<&crate::ai_runtime::retrieval_scope::ContextScopeDto>,
) -> AppResult<bool> {
    let Some(scope) = scope else {
        return Ok(false);
    };
    for path in &scope.paths {
        if !crate::storage::paths::is_classified_note_path(path) {
            return Err(AppError::msg(
                "classified assistant contextScope must stay inside .classified",
            ));
        }
    }
    for prefix in &scope.path_prefixes {
        let normalized = prefix.replace('\\', "/");
        if normalized != ".classified" && !normalized.starts_with(".classified/") {
            return Err(AppError::msg(
                "classified assistant contextScope must stay inside .classified",
            ));
        }
    }
    if !scope.corpus_ids.is_empty() {
        return Err(AppError::msg(
            "classified assistant contextScope cannot use normal corpus IDs",
        ));
    }
    Ok(!scope.paths.is_empty() || !scope.path_prefixes.is_empty())
}

fn classified_scope_matches(
    hit_path: &str,
    scope: &crate::ai_runtime::retrieval_scope::ContextScopeDto,
) -> bool {
    if scope
        .paths
        .iter()
        .any(|path| hit_path == path.replace('\\', "/"))
    {
        return true;
    }
    scope
        .path_prefixes
        .iter()
        .map(|prefix| prefix.replace('\\', "/").trim_end_matches('/').to_string())
        .any(|prefix| hit_path == prefix || hit_path.starts_with(&format!("{prefix}/")))
}

fn scoped_classified_hits(
    vault: &std::path::Path,
    request: &AssistantExecuteRequest,
    note_path: &str,
) -> AppResult<Vec<crate::ai_runtime::classified_retrieval::ClassifiedSearchHit>> {
    let has_explicit_scope = validate_classified_scope(request.context_scope.as_ref())?;
    let chunks = crate::ai_runtime::classified_retrieval::build_classified_index(vault)?;
    let current_doc =
        if has_explicit_scope || classified_query_requests_vault_scope(&request.message) {
            None
        } else {
            Some(note_path)
        };
    let mut hits = crate::ai_runtime::classified_retrieval::search_chunks(
        &chunks,
        &request.message,
        current_doc,
        8,
    );
    if let Some(scope) = request.context_scope.as_ref() {
        hits.retain(|hit| classified_scope_matches(&hit.document_path, scope));
    }
    if current_doc.is_some() {
        hits.retain(|hit| hit.document_path == note_path);
    }
    hits.truncate(4);
    Ok(hits)
}

fn classified_hits_to_packets(
    hits: &[crate::ai_runtime::classified_retrieval::ClassifiedSearchHit],
) -> Vec<ContextPacket> {
    hits.iter()
        .enumerate()
        .map(|(index, hit)| ContextPacket {
            id: format!("classified-evidence-{}", index + 1),
            source_type: SourceType::Note,
            source_path: Some(hit.document_path.clone()),
            title: hit
                .heading
                .clone()
                .unwrap_or_else(|| "涉密文档片段".to_string()),
            heading_path: hit.heading.clone(),
            source_span: None,
            content_hash: crate::ai_runtime::writing_workflow::compute_content_hash(&hit.snippet),
            excerpt: hit.snippet.clone(),
            retrieval_reason: "classified scoped retrieval".to_string(),
            score: hit.score,
            trust_level: TrustLevel::UserNote,
            citation_label: format!("[C{}]", index + 1),
            stale: false,
            web: None,
            corpus: None,
        })
        .collect()
}

fn selection_range(document: &str, selection: &str) -> SourceSpan {
    if let Some(start) = document.find(selection) {
        return SourceSpan {
            start,
            end: start + selection.len(),
        };
    }
    SourceSpan {
        start: 0,
        end: selection.len().min(document.len()),
    }
}

struct ClassifiedWritingOutputInput<'a> {
    request_id: String,
    note_path: &'a str,
    document: &'a str,
    selection: &'a str,
    message: &'a str,
    intent: crate::ai_runtime::WritingIntent,
    replacement: String,
    evidence: Vec<ContextPacket>,
    usage: TokenUsage,
}

fn build_classified_writing_output(input: ClassifiedWritingOutputInput<'_>) -> WritingTaskOutput {
    let base_content_hash =
        crate::ai_runtime::writing_workflow::compute_content_hash(input.document);
    let patch = crate::ai_runtime::writing_workflow::build_patch_proposal(
        input.note_path,
        &base_content_hash,
        input.selection,
        &input.replacement,
        selection_range(input.document, input.selection),
        input
            .evidence
            .iter()
            .map(|packet| packet.id.clone())
            .collect(),
    );
    let suggestions = vec![
        crate::ai_runtime::writing_workflow::build_writing_suggestion(
            input.intent.clone(),
            "涉密选区改写建议",
            0.86,
        ),
    ];
    let writing_state = WritingState::from_input(WritingStateInput {
        request_id: input.request_id.clone(),
        target_path: input.note_path.to_string(),
        base_content_hash,
        writing_goal: input.message.to_string(),
        intent: format!("{:?}", input.intent).to_ascii_lowercase(),
        evidence: input.evidence.clone(),
        patches: vec![patch.clone()],
    });
    WritingTaskOutput {
        request_id: input.request_id,
        suggestions,
        patches: vec![patch],
        evidence_used: input.evidence,
        total_tokens: input.usage,
        writing_state,
    }
}

async fn run_classified_writing_task(
    state: &Arc<AppState>,
    app_handle: &AppHandle,
    request: &AssistantExecuteRequest,
    routing_override: Option<&ai_commands::AiSendRoutingOverride>,
) -> AppResult<HarnessTaskResult> {
    let note_path = request
        .note_path
        .as_deref()
        .ok_or_else(|| AppError::msg("classified assistant requires notePath"))?;
    let selection = request
        .selection
        .as_deref()
        .filter(|selection| !selection.trim().is_empty())
        .ok_or_else(|| AppError::msg("classified writing requires selection"))?;
    let vault = state.vault_path()?;
    let document = read_classified_document(&vault, note_path)?;
    let hits = scoped_classified_hits(&vault, request, note_path)?;
    let evidence = classified_hits_to_packets(&hits);
    let route = routing_override
        .ok_or_else(|| AppError::msg("classified assistant requires resolved model route"))?;
    let provider_config = route.resolved.to_provider_config_for_slot(route.slot);
    let intent = crate::ai_runtime::writing_workflow::detect_writing_intent(
        &request.message,
        Some(selection),
    );
    let cursor_context = request
        .cursor_context
        .as_deref()
        .unwrap_or(document.as_str());
    let (replacement, usage) = crate::ai_runtime::writing_workflow::generate_replacement_with_llm(
        &state.db,
        app_handle,
        &provider_config,
        &intent,
        selection,
        cursor_context,
        &request.message,
        &evidence,
    )
    .await?;
    let request_id = uuid::Uuid::new_v4().to_string();
    let output = build_classified_writing_output(ClassifiedWritingOutputInput {
        request_id,
        note_path,
        document: &document,
        selection,
        message: &request.message,
        intent,
        replacement,
        evidence,
        usage,
    });
    Ok(task_result_from_writing(output))
}

async fn run_classified_chat_task(
    state: &Arc<AppState>,
    app_handle: &AppHandle,
    request: &AssistantExecuteRequest,
    routing_override: Option<&ai_commands::AiSendRoutingOverride>,
) -> AppResult<HarnessTaskResult> {
    let note_path = request
        .note_path
        .as_deref()
        .ok_or_else(|| AppError::msg("classified assistant requires notePath"))?;
    let vault = state.vault_path()?;
    let document = read_classified_document(&vault, note_path)?;
    let scoped_hits = scoped_classified_hits(&vault, request, note_path)?;
    let mut evidence = String::new();
    for (index, hit) in scoped_hits.iter().enumerate() {
        evidence.push_str(&format!(
            "\n[片段 {}] {}\n{}\n",
            index + 1,
            hit.heading.as_deref().unwrap_or("当前文档"),
            hit.snippet
        ));
    }
    let system_prompt = format!(
        "你是 Iris 的涉密域助手。只使用本次提供的当前涉密文档与涉密检索片段回答；不要请求或引用普通笔记、普通会话、外部缓存或工具。不要泄露内部保险库路径。\n\n当前涉密文档：\n{}\n\n涉密检索片段：\n{}",
        document, evidence
    );
    let request_id = uuid::Uuid::new_v4().to_string();
    let route = routing_override
        .ok_or_else(|| AppError::msg("classified assistant requires resolved model route"))?;
    let provider_config = route.resolved.to_provider_config_for_slot(route.slot);
    app_handle
        .emit(
            "ai:request_started",
            serde_json::json!({
                "request_id": request_id.clone(),
                "classified": true,
            }),
        )
        .ok();
    let gateway = crate::ai_runtime::model_gateway::ModelGateway::with_defaults(
        app_handle.clone(),
        vec![provider_config.clone()],
    )?;
    let response = gateway
        .send_classified_streaming_request(
            &request_id,
            crate::ai_runtime::model_gateway::GatewayRequest {
                provider: provider_config,
                messages: vec![
                    LlmMessage {
                        role: MessageRole::System,
                        content: system_prompt.into(),
                        ..Default::default()
                    },
                    LlmMessage {
                        role: MessageRole::User,
                        content: request.message.clone().into(),
                        ..Default::default()
                    },
                ],
                tools: Vec::new(),
                max_tokens: Some(route.resolved.output_budget),
                temperature: Some(0.2),
                stream: true,
                thinking: route.resolved.thinking,
                skip_stub_ids: Vec::new(),
            },
        )
        .await?;
    let content = response.content.unwrap_or_default();
    Ok(HarnessTaskResult {
        request_id: request_id.clone(),
        task_id: None,
        run_status: HarnessRunStatus::Completed,
        artifacts: vec![HarnessArtifact::Message {
            content: content.clone(),
            citation_valid: true,
        }],
        artifact_wires: Vec::new(),
        evidence_packet_ids: Vec::new(),
        usage: Some(response.usage.clone()),
        evidence_refresh_notice: None,
        chat_payload: Some(serde_json::json!({
            "request_id": request_id,
            "session_id": 0,
            "status": "completed",
            "content": content,
            "tool_calls": response.tool_calls,
            "tool_results": [],
            "harness_rounds": 1,
            "usage": response.usage,
            "usage_source": "provider",
            "citation_valid": true,
            "evidence_packets": [],
            "pending_confirmation": false,
        })),
        writing: None,
        citation: None,
        organize: None,
        research: None,
        chapter: None,
        document: None,
    })
}

fn legacy_intent_wire(intent: AssistantIntent) -> &'static str {
    match intent {
        AssistantIntent::Chat => "chat",
        AssistantIntent::Knowledge => "knowledge",
        AssistantIntent::Writing => "writing",
        AssistantIntent::Citation => "citation",
        AssistantIntent::Organize => "organize",
        AssistantIntent::Research => "research",
        AssistantIntent::Chapter => "chapter",
        AssistantIntent::Document => "document",
    }
}

fn artifact_kind(artifact: &HarnessArtifact) -> &'static str {
    match artifact {
        HarnessArtifact::Message { .. } => "not_displayed",
        HarnessArtifact::Patches { .. } => "writing_change",
        HarnessArtifact::CitationReport { .. }
        | HarnessArtifact::OrganizeReport { .. }
        | HarnessArtifact::DocumentCheck { .. } => "structured_result",
        HarnessArtifact::ResearchReport { .. } => "evidence_sources",
        HarnessArtifact::ChapterWriting { .. } => "not_displayed",
        HarnessArtifact::ToolConfirmation { .. } => "task_process",
        HarnessArtifact::LegacyPayload { .. } => "not_displayed",
    }
}

/// Execute unified assistant task and collect artifacts.
pub(crate) async fn run_harness_task(
    state: &Arc<AppState>,
    app_handle: &AppHandle,
    task: HarnessTaskRequest,
) -> AppResult<HarnessTaskResult> {
    let request = task.assistant;
    let task_plan = task.task_plan;
    let routing_override = task.routing_override;
    validate_assistant_domain_boundary(&request)?;
    let legacy_intent = crate::ai_runtime::task_plan::legacy_intent_for_task_plan(&task_plan);
    let agent_intent = crate::ai_runtime::task_plan::agent_intent_for_task_plan(&task_plan);
    if request.ai_domain == AssistantAiDomain::Classified {
        if matches!(legacy_intent, AssistantIntent::Writing) {
            return run_classified_writing_task(
                state,
                app_handle,
                &request,
                routing_override.as_ref(),
            )
            .await;
        }
        return run_classified_chat_task(state, app_handle, &request, routing_override.as_ref())
            .await;
    }
    let skill_activation_plan = routing_override
        .as_ref()
        .and_then(|route| route.skill_activation_plan.as_ref());
    let skill_overlay = legacy_skill_overlay_from_plan(
        state,
        legacy_intent.scene(),
        &request.message,
        skill_activation_plan,
    );
    let runtime_task_id = if creates_own_runtime_task(legacy_intent) {
        Some(create_workflow_runtime_task(
            &state.db,
            &request,
            legacy_intent,
        )?)
    } else {
        None
    };
    let outcome = match legacy_intent {
        AssistantIntent::Writing => {
            let note_path = request
                .note_path
                .clone()
                .ok_or_else(|| AppError::msg("写作任务需要 notePath"))?;
            let selection = request
                .selection
                .as_ref()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| AppError::msg("写作任务需要选区"))?
                .clone();
            let cursor_context = request
                .cursor_context
                .clone()
                .or(request.note_content.clone())
                .unwrap_or_default();
            let input = WritingTaskInputIpc {
                target_path: note_path,
                base_content_hash: resolve_content_hash(
                    request.note_content.as_deref(),
                    request.base_content_hash.as_deref(),
                ),
                selection: Some(selection),
                cursor_context,
                writing_goal: apply_skill_overlay_to_goal(&request.message, &skill_overlay),
                web_authorized: request.web_authorized,
            };
            let trace_id = uuid::Uuid::new_v4().to_string();
            emit_workflow_trace(app_handle, &trace_id, "writing", "running");
            let payload = writing_commands::execute_writing_task(state, app_handle, input).await?;
            emit_workflow_trace(app_handle, &payload.request_id, "writing", "ok");
            Ok(task_result_from_writing(payload))
        }
        AssistantIntent::Citation => {
            let trace_id = uuid::Uuid::new_v4().to_string();
            emit_workflow_trace(app_handle, &trace_id, "citation", "running");
            let note_path = request
                .note_path
                .clone()
                .ok_or_else(|| AppError::msg("引用检查需要 notePath"))?;
            let paragraph_text = request
                .paragraph_text
                .clone()
                .or(request.selection.clone())
                .filter(|t| !t.is_empty())
                .ok_or_else(|| AppError::msg("引用检查需要段落或选区文本"))?;
            let input = CitationCheckInput {
                paragraph_text,
                document_path: note_path,
                scope: citation_scope_from_dto(request.context_scope.clone()),
                web_authorized: request.web_authorized,
            };
            let payload =
                citation_commands::execute_citation_check(state, app_handle, input).await?;
            emit_workflow_trace(app_handle, &payload.request_id, "citation", "ok");
            Ok(task_result_from_citation(payload))
        }
        AssistantIntent::Organize => {
            let trace_id = uuid::Uuid::new_v4().to_string();
            emit_workflow_trace(app_handle, &trace_id, "organize", "running");
            let input = OrganizeTaskInput {
                scope: organize_scope_from_dto(request.context_scope.clone()),
                task_type: parse_organize_task_type(request.organize_task_type.as_deref()),
            };
            let payload =
                organize_commands::execute_organize_task(state, app_handle, input).await?;
            emit_workflow_trace(app_handle, &payload.request_id, "organize", "ok");
            Ok(task_result_from_organize(payload))
        }
        AssistantIntent::Research => {
            let trace_id = uuid::Uuid::new_v4().to_string();
            emit_workflow_trace(app_handle, &trace_id, "research", "running");
            let payload = research_commands::execute_research_task(
                state,
                app_handle,
                apply_skill_overlay_to_goal(&request.message, &skill_overlay),
                Some(request.web_authorized),
            )
            .await?;
            let rid = if payload.request_id.is_empty() {
                trace_id.as_str()
            } else {
                payload.request_id.as_str()
            };
            emit_workflow_trace(app_handle, rid, "research", "ok");
            Ok(task_result_from_research(payload))
        }
        AssistantIntent::Chapter => {
            let trace_id = uuid::Uuid::new_v4().to_string();
            emit_workflow_trace(app_handle, &trace_id, "chapter", "running");
            let note_path = request
                .note_path
                .clone()
                .ok_or_else(|| AppError::msg("章节写作需要 notePath"))?;
            let chapter =
                resolve_chapter(request.chapter.clone(), request.note_content.as_deref())?;
            let input = ChapterWritingInput {
                target_path: note_path,
                base_content_hash: resolve_content_hash(
                    request.note_content.as_deref(),
                    request.base_content_hash.as_deref(),
                ),
                chapter,
                writing_goal: apply_skill_overlay_to_goal(&request.message, &skill_overlay),
                web_authorized: request.web_authorized,
            };
            let payload =
                document_commands::execute_chapter_writing(state, app_handle, input).await?;
            emit_workflow_trace(app_handle, &payload.request_id, "chapter", "ok");
            Ok(task_result_from_chapter(payload))
        }
        AssistantIntent::Document => {
            let trace_id = uuid::Uuid::new_v4().to_string();
            emit_workflow_trace(app_handle, &trace_id, "document", "running");
            let note_path = request
                .note_path
                .clone()
                .ok_or_else(|| AppError::msg("文档检查需要 notePath"))?;
            let note_content = request
                .note_content
                .clone()
                .ok_or_else(|| AppError::msg("文档检查需要 noteContent"))?;
            let input = DocumentCheckInput {
                target_path: note_path,
                content: note_content.clone(),
                check_type: parse_document_check_type(request.document_check_type.as_deref()),
                web_authorized: request.web_authorized,
                base_content_hash: resolve_content_hash(
                    Some(note_content.as_str()),
                    request.base_content_hash.as_deref(),
                ),
            };
            let payload =
                document_commands::execute_document_check(state, app_handle, input).await?;
            emit_workflow_trace(app_handle, &payload.request_id, "document", "ok");
            Ok(task_result_from_document(payload))
        }
        AssistantIntent::Chat | AssistantIntent::Knowledge => {
            let trace_id = uuid::Uuid::new_v4().to_string();
            emit_workflow_trace(app_handle, &trace_id, "chat", "running");
            let scene = legacy_intent.scene().profile().to_string();
            let payload = ai_commands::execute_ai_send_message_with_routing(
                state,
                app_handle,
                scene,
                Some(agent_intent),
                request.session_id,
                request.message.clone(),
                request.images.clone(),
                request.selected_packet_ids.clone(),
                request.note_path.clone(),
                request.context_scope.clone(),
                Some(request.web_authorized),
                Some(request.new_session),
                routing_override,
            )
            .await?;
            let rid = if payload.request_id.is_empty() {
                trace_id.as_str()
            } else {
                payload.request_id.as_str()
            };
            let status = if payload.pending_confirmation || payload.status == "pending_tools" {
                "pending"
            } else {
                "ok"
            };
            emit_workflow_trace(app_handle, rid, "chat", status);
            Ok(task_result_from_chat(payload))
        }
    };
    match outcome {
        Ok(mut result) => {
            if let Some(task_id) = runtime_task_id {
                complete_workflow_runtime_task(&state.db, &task_id, &result)?;
                result.task_id = Some(task_id);
            }
            Ok(result)
        }
        Err(err) => {
            if let Some(task_id) = runtime_task_id {
                let _ = fail_workflow_runtime_task(&state.db, &task_id, "ASSISTANT_WORKFLOW_ERROR");
            }
            Err(err)
        }
    }
}

fn resolve_content_hash(note_content: Option<&str>, provided: Option<&str>) -> String {
    if let Some(hash) = provided.filter(|h| !h.is_empty()) {
        return hash.to_string();
    }
    note_content
        .map(crate::ai_runtime::writing_workflow::compute_content_hash)
        .unwrap_or_default()
}

fn citation_scope_from_dto(
    dto: Option<crate::ai_runtime::retrieval_scope::ContextScopeDto>,
) -> Option<CitationCheckScope> {
    dto.map(|scope| CitationCheckScope {
        paths: scope.paths,
        path_prefixes: scope.path_prefixes,
        corpus_ids: if scope.corpus_ids.is_empty() {
            None
        } else {
            Some(scope.corpus_ids)
        },
    })
}

fn organize_scope_from_dto(
    dto: Option<crate::ai_runtime::retrieval_scope::ContextScopeDto>,
) -> Option<OrganizeTaskScope> {
    dto.map(|scope| OrganizeTaskScope {
        paths: scope.paths,
        path_prefixes: scope.path_prefixes,
        corpus_ids: if scope.corpus_ids.is_empty() {
            None
        } else {
            Some(scope.corpus_ids)
        },
    })
}

fn resolve_chapter(
    chapter: Option<ChapterInfo>,
    note_content: Option<&str>,
) -> AppResult<ChapterInfo> {
    if let Some(ch) = chapter {
        return Ok(ch);
    }
    let content = note_content.ok_or_else(|| AppError::msg("章节任务需要笔记正文"))?;
    chapter_workflow::parse_chapters(content)
        .into_iter()
        .next()
        .ok_or_else(|| AppError::msg("当前文档没有可识别的章节结构"))
}

fn task_result_from_chat(
    payload: crate::commands::ai_commands::AiChatResponse,
) -> HarnessTaskResult {
    let payload_json = serde_json::to_value(&payload).unwrap_or_default();
    let request_id = payload.request_id.clone();
    let pending = payload.pending_confirmation || payload.status == "pending_tools";
    let run_status = if pending {
        HarnessRunStatus::PendingConfirmation
    } else {
        HarnessRunStatus::Completed
    };
    let content = payload.content.clone();
    let citation_valid = payload.citation_valid;
    let packet_ids: Vec<String> = payload
        .evidence_packets
        .iter()
        .map(|packet| packet.id.clone())
        .collect();
    let usage: Option<TokenUsage> = Some(payload.usage.clone());
    let mut artifacts = vec![HarnessArtifact::Message {
        content: content.clone(),
        citation_valid,
    }];
    if pending {
        if let Some(tc) = payload.tool_calls.first() {
            if !payload.request_id.is_empty() {
                artifacts.push(HarnessArtifact::ToolConfirmation {
                    request_id: payload.request_id.clone(),
                    tool_call_id: tc.id.clone(),
                });
            }
        }
    }
    let artifact_wires = artifacts_to_wires(&artifacts, "chat");
    HarnessTaskResult {
        request_id,
        task_id: payload.task_id.clone(),
        run_status,
        artifacts,
        artifact_wires,
        evidence_packet_ids: packet_ids,
        usage,
        evidence_refresh_notice: None,
        chat_payload: Some(payload_json),
        writing: None,
        citation: None,
        organize: None,
        research: None,
        chapter: None,
        document: None,
    }
}

fn task_result_from_writing(payload: WritingTaskOutput) -> HarnessTaskResult {
    let artifacts = vec![
        HarnessArtifact::Patches {
            patches: payload.patches.clone(),
        },
        HarnessArtifact::LegacyPayload {
            intent: "writing".into(),
            json: serde_json::to_value(&payload).unwrap_or_default(),
        },
    ];
    let artifact_wires = artifacts_to_wires(&artifacts, "writing");
    HarnessTaskResult {
        request_id: payload.request_id.clone(),
        task_id: None,
        run_status: HarnessRunStatus::Completed,
        artifacts,
        artifact_wires,
        evidence_packet_ids: payload.evidence_used.iter().map(|p| p.id.clone()).collect(),
        usage: Some(payload.total_tokens.clone()),
        evidence_refresh_notice: None,
        chat_payload: None,
        writing: Some(payload),
        citation: None,
        organize: None,
        research: None,
        chapter: None,
        document: None,
    }
}

fn task_result_from_citation(payload: CitationCheckResult) -> HarnessTaskResult {
    let artifacts = vec![HarnessArtifact::CitationReport {
        report: payload.clone(),
    }];
    let artifact_wires = artifacts_to_wires(&artifacts, "citation");
    HarnessTaskResult {
        request_id: payload.request_id.clone(),
        task_id: None,
        run_status: HarnessRunStatus::Completed,
        artifacts,
        artifact_wires,
        evidence_packet_ids: vec![],
        usage: None,
        evidence_refresh_notice: None,
        chat_payload: None,
        writing: None,
        citation: Some(payload),
        organize: None,
        research: None,
        chapter: None,
        document: None,
    }
}

fn task_result_from_organize(payload: OrganizeTaskResult) -> HarnessTaskResult {
    let artifacts = vec![HarnessArtifact::OrganizeReport {
        report: payload.clone(),
    }];
    let artifact_wires = artifacts_to_wires(&artifacts, "organize");
    HarnessTaskResult {
        request_id: "organize".into(),
        task_id: None,
        run_status: HarnessRunStatus::Completed,
        artifacts,
        artifact_wires,
        evidence_packet_ids: vec![],
        usage: None,
        evidence_refresh_notice: None,
        chat_payload: None,
        writing: None,
        citation: None,
        organize: Some(payload),
        research: None,
        chapter: None,
        document: None,
    }
}

fn task_result_from_research(
    payload: crate::commands::research_commands::ResearchExecuteResponse,
) -> HarnessTaskResult {
    let request_id = payload.request_id.clone();
    let payload_json = serde_json::to_value(&payload).unwrap_or_default();
    let mut artifacts = Vec::new();
    if research_payload_has_real_evidence(&payload_json) {
        artifacts.push(HarnessArtifact::ResearchReport {
            payload: payload_json.clone(),
        });
    }
    artifacts.push(HarnessArtifact::LegacyPayload {
        intent: "research".into(),
        json: payload_json.clone(),
    });
    let artifact_wires = artifacts_to_wires(&artifacts, "research");
    HarnessTaskResult {
        request_id,
        task_id: None,
        run_status: HarnessRunStatus::Completed,
        artifacts,
        artifact_wires,
        evidence_packet_ids: vec![],
        usage: None,
        evidence_refresh_notice: None,
        chat_payload: None,
        writing: None,
        citation: None,
        organize: None,
        research: Some(payload_json),
        chapter: None,
        document: None,
    }
}

fn task_result_from_chapter(
    payload: crate::ai_runtime::chapter_workflow::ChapterWritingResult,
) -> HarnessTaskResult {
    let artifacts = vec![
        HarnessArtifact::ChapterWriting {
            payload: serde_json::to_value(&payload).unwrap_or_default(),
        },
        HarnessArtifact::Patches {
            patches: payload.patches.clone(),
        },
    ];
    let artifact_wires = artifacts_to_wires(&artifacts, "chapter");
    HarnessTaskResult {
        request_id: payload.request_id.clone(),
        task_id: None,
        run_status: HarnessRunStatus::Completed,
        artifacts,
        artifact_wires,
        evidence_packet_ids: payload.evidence_used.iter().map(|p| p.id.clone()).collect(),
        usage: Some(payload.total_tokens.clone()),
        evidence_refresh_notice: None,
        chat_payload: None,
        writing: None,
        citation: None,
        organize: None,
        research: None,
        chapter: Some(serde_json::to_value(&payload).unwrap_or_default()),
        document: None,
    }
}

fn task_result_from_document(payload: DocumentCheckResult) -> HarnessTaskResult {
    let artifacts = vec![HarnessArtifact::DocumentCheck {
        report: payload.clone(),
    }];
    let artifact_wires = artifacts_to_wires(&artifacts, "document");
    HarnessTaskResult {
        request_id: payload.request_id.clone(),
        task_id: None,
        run_status: HarnessRunStatus::Completed,
        artifacts,
        artifact_wires,
        evidence_packet_ids: vec![],
        usage: None,
        evidence_refresh_notice: None,
        chat_payload: None,
        writing: None,
        citation: None,
        organize: None,
        research: None,
        chapter: None,
        document: Some(payload),
    }
}

fn artifacts_to_wires(
    artifacts: &[HarnessArtifact],
    source_task: &str,
) -> Vec<HarnessArtifactWire> {
    artifacts
        .iter()
        .filter_map(|a| {
            let (kind, title, evidence_count, payload) = match a {
                HarnessArtifact::Message { .. } | HarnessArtifact::LegacyPayload { .. } => {
                    return None;
                }
                HarnessArtifact::Patches { patches } => {
                    if patches.is_empty() {
                        return None;
                    }
                    (
                        "writing_change",
                        "写作修改",
                        0,
                        serde_json::json!({
                            "schema": "writing_change",
                            "patches": patches,
                        }),
                    )
                }
                HarnessArtifact::CitationReport { report } => (
                    "structured_result",
                    "引用检查",
                    0,
                    serde_json::json!({
                        "schema": "citation_report",
                        "result": report,
                    }),
                ),
                HarnessArtifact::OrganizeReport { report } => {
                    if report.batch.suggestions.is_empty() {
                        return None;
                    }
                    (
                        "structured_result",
                        "整理建议",
                        0,
                        serde_json::json!({
                            "schema": "organize_result",
                            "suggestions": report.batch.suggestions,
                        }),
                    )
                }
                HarnessArtifact::ResearchReport { payload } => {
                    if !research_payload_has_real_evidence(payload) {
                        return None;
                    }
                    (
                        "evidence_sources",
                        "证据来源",
                        research_payload_evidence_count(payload),
                        payload.clone(),
                    )
                }
                HarnessArtifact::DocumentCheck { report } => {
                    let issues = document_issue_list(report);
                    if issues.is_empty() && report.analysis_summary.is_none() {
                        return None;
                    }
                    (
                        "structured_result",
                        "文档检查",
                        report.evidence_used.len() as u32,
                        serde_json::json!({
                            "resultKind": "document_issues",
                            "summary": report.analysis_summary,
                            "issues": issues,
                        }),
                    )
                }
                HarnessArtifact::ChapterWriting { .. } => return None,
                HarnessArtifact::ToolConfirmation {
                    request_id,
                    tool_call_id,
                } => (
                    "task_process",
                    "工具确认",
                    0,
                    serde_json::json!({
                        "schema": "task_process",
                        "status": "pending_confirmation",
                        "request_id": request_id,
                        "tool_call_id": tool_call_id,
                    }),
                ),
            };
            Some(HarnessArtifactWire {
                kind: kind.into(),
                title: title.into(),
                status: "ready".into(),
                source_task: source_task.into(),
                evidence_count,
                payload,
            })
        })
        .collect()
}

fn research_payload_evidence_count(payload: &serde_json::Value) -> u32 {
    payload
        .pointer("/evidence_matrix/total_evidence_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default() as u32
}

fn research_payload_has_real_evidence(payload: &serde_json::Value) -> bool {
    if research_payload_evidence_count(payload) > 0 {
        return true;
    }
    let has_sources = payload
        .pointer("/research_state/sources")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|sources| !sources.is_empty());
    let has_conflicts = payload
        .pointer("/research_state/conflicts")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|conflicts| !conflicts.is_empty());
    let has_actionable_gap = payload
        .pointer("/research_state/evidence_gaps")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|gaps| has_actionable_evidence_gap(gaps.as_slice()))
        || payload
            .pointer("/evidence_matrix/global_gaps")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|gaps| has_actionable_evidence_gap(gaps.as_slice()));
    has_sources || has_conflicts || has_actionable_gap
}

fn has_actionable_evidence_gap(gaps: &[serde_json::Value]) -> bool {
    gaps.iter().any(|gap| {
        gap.as_str()
            .is_some_and(|text| !is_mechanical_evidence_gap(text))
    })
}

fn is_mechanical_evidence_gap(text: &str) -> bool {
    let normalized = text.to_lowercase();
    let mentions_evidence =
        normalized.contains("evidence") || text.contains("证据") || text.contains("来源");
    if !mentions_evidence {
        return false;
    }
    normalized.contains("no evidence")
        || normalized.contains("no source")
        || text.contains("未授权")
        || text.contains("未检索")
        || text.contains("未找到")
        || text.contains("暂无")
        || text.contains("没有")
}

fn document_issue_list(report: &DocumentCheckResult) -> Vec<String> {
    let mut issues = Vec::new();
    if let Some(outline) = &report.outline_result {
        issues.extend(
            outline
                .issues
                .iter()
                .map(|issue| format!("[大纲] {}", issue.description)),
        );
    }
    if let Some(citation) = &report.citation_gap_result {
        issues.extend(
            citation
                .uncited_claims
                .iter()
                .map(|claim| format!("[引用缺口] {}", claim.statement)),
        );
        issues.extend(
            citation
                .weak_citations
                .iter()
                .map(|weak| format!("[弱引用] {}", weak.reason)),
        );
    }
    if let Some(style) = &report.style_result {
        issues.extend(
            style
                .inconsistencies
                .iter()
                .map(|item| format!("[风格] {}", item.description)),
        );
    }
    issues
}

pub fn map_task_result_to_response(
    result: HarnessTaskResult,
) -> crate::commands::assistant_commands::AssistantExecuteResponse {
    use crate::commands::assistant_commands::AssistantExecuteResponse;
    let body = task_result_to_body(&result);
    AssistantExecuteResponse {
        body,
        request_id: result.request_id,
        task_id: result.task_id,
        run_status: run_status_wire(result.run_status).to_string(),
        evidence_refresh_notice: result.evidence_refresh_notice,
        artifacts: result.artifact_wires,
        intent_detection: None,
        task_plan: None,
        run_plan_summary: None,
        permission_preflight_summary: None,
    }
}

fn task_result_to_body(
    result: &HarnessTaskResult,
) -> crate::commands::assistant_commands::AssistantExecuteBody {
    use crate::commands::assistant_commands::AssistantExecuteBody;
    if let Some(payload) = &result.chat_payload {
        return AssistantExecuteBody::Chat {
            payload: payload.clone(),
        };
    }
    if let Some(payload) = &result.writing {
        return AssistantExecuteBody::Writing {
            payload: payload.clone(),
        };
    }
    if let Some(payload) = &result.citation {
        return AssistantExecuteBody::Citation {
            payload: payload.clone(),
        };
    }
    if let Some(payload) = &result.organize {
        return AssistantExecuteBody::Organize {
            payload: payload.clone(),
        };
    }
    if let Some(payload) = &result.research {
        return AssistantExecuteBody::Research {
            payload: payload.clone(),
        };
    }
    if let Some(payload) = &result.chapter {
        return AssistantExecuteBody::Chapter {
            payload: serde_json::from_value(payload.clone()).unwrap_or_else(|_| {
                crate::ai_runtime::chapter_workflow::ChapterWritingResult {
                    request_id: result.request_id.clone(),
                    suggestions: vec![],
                    patches: vec![],
                    evidence_used: vec![],
                    total_tokens: TokenUsage::default(),
                }
            }),
        };
    }
    if let Some(payload) = &result.document {
        return AssistantExecuteBody::Document {
            payload: payload.clone(),
        };
    }
    AssistantExecuteBody::Chat {
        payload: serde_json::json!({
            "request_id": result.request_id,
            "status": "completed",
            "content": "",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::agent_task::{AgentTaskRuntime, AgentTaskStatus};
    use crate::storage::db::Database;

    #[test]
    fn chat_pending_task_status() {
        let payload = crate::commands::ai_commands::AiChatResponse {
            request_id: "r1".to_string(),
            task_id: Some("r1".to_string()),
            session_id: 1,
            status: "pending_tools".to_string(),
            content: "partial".to_string(),
            tool_calls: vec![crate::ai_runtime::model_gateway::ToolCall::new(
                "tc1",
                "search_hybrid",
                "{}",
            )],
            tool_results: vec![],
            usage: TokenUsage::default(),
            usage_source: crate::ai_runtime::harness::UsageSource::Provider,
            citation_valid: true,
            harness_rounds: 1,
            evidence_packets: vec![],
            pending_confirmation: true,
            deliberation_state: None,
            verification_summary: None,
            evidence_refresh_notice: None,
            tool_call_id: None,
            decision: None,
            resumed: None,
            installed_skill: None,
        };
        let r = task_result_from_chat(payload);
        assert_eq!(r.run_status, HarnessRunStatus::PendingConfirmation);
    }

    #[test]
    fn workflow_runtime_task_is_created_for_non_chat_intent() {
        let db = Database::open_in_memory().unwrap();
        let request = AssistantExecuteRequest {
            ai_domain: AssistantAiDomain::Normal,
            agent_intent: None,
            intent: Some(AssistantIntent::Research),
            intent_detection: None,
            task_plan: None,
            context_references: Vec::new(),
            message: "研究这个问题".into(),
            note_path: None,
            note_content: None,
            web_authorized: false,
            selection: None,
            cursor_context: None,
            paragraph_text: None,
            context_scope: None,
            session_id: None,
            selected_packet_ids: None,
            chapter: None,
            document_check_type: None,
            organize_task_type: None,
            base_content_hash: None,
            new_session: false,
            images: None,
        };

        let task_id =
            create_workflow_runtime_task(&db, &request, AssistantIntent::Research).unwrap();
        let task = AgentTaskRuntime::get_task(&db, &task_id).unwrap().unwrap();

        assert_eq!(task.status, AgentTaskStatus::Running);
        assert_eq!(
            task.kind,
            crate::ai_runtime::agent_task::AgentTaskKind::Complex
        );
        assert_eq!(task.budget_policy["intent"], "research");
    }

    #[test]
    fn workflow_runtime_task_failure_is_safe_and_terminal() {
        let db = Database::open_in_memory().unwrap();
        let request = AssistantExecuteRequest {
            ai_domain: AssistantAiDomain::Normal,
            agent_intent: None,
            intent: Some(AssistantIntent::Writing),
            intent_detection: None,
            task_plan: None,
            context_references: Vec::new(),
            message: "改写这段文字".into(),
            note_path: Some("notes/draft.md".into()),
            note_content: None,
            web_authorized: false,
            selection: Some("hello".into()),
            cursor_context: None,
            paragraph_text: None,
            context_scope: None,
            session_id: None,
            selected_packet_ids: None,
            chapter: None,
            document_check_type: None,
            organize_task_type: None,
            base_content_hash: None,
            new_session: false,
            images: None,
        };
        let task_id =
            create_workflow_runtime_task(&db, &request, AssistantIntent::Writing).unwrap();

        fail_workflow_runtime_task(&db, &task_id, "MISSING_REQUIRED_INPUT").unwrap();
        let task = AgentTaskRuntime::get_task(&db, &task_id).unwrap().unwrap();

        assert_eq!(task.status, AgentTaskStatus::FailedSafe);
        assert_eq!(task.error_code.as_deref(), Some("MISSING_REQUIRED_INPUT"));
    }

    #[test]
    fn classified_writing_intent_is_not_rejected_at_dispatch_boundary() {
        let source = include_str!("harness_task.rs");
        let legacy_rejection = concat!(
            "classified assistant currently supports chat and knowledge intents ",
            "only"
        );
        assert!(
            !source.contains(legacy_rejection),
            "classified domain must support writing/rewrite instead of hard-rejecting non-chat intents"
        );
    }

    #[test]
    fn classified_scope_validation_accepts_only_classified_paths() {
        let scope = crate::ai_runtime::retrieval_scope::ContextScopeDto {
            paths: vec![".classified/a.md".into()],
            path_prefixes: vec![".classified/projects".into()],
            corpus_ids: vec![],
        };
        assert!(validate_classified_scope(Some(&scope)).unwrap());

        let ordinary_path = crate::ai_runtime::retrieval_scope::ContextScopeDto {
            paths: vec!["normal.md".into()],
            path_prefixes: vec![],
            corpus_ids: vec![],
        };
        assert!(validate_classified_scope(Some(&ordinary_path)).is_err());

        let ordinary_prefix = crate::ai_runtime::retrieval_scope::ContextScopeDto {
            paths: vec![],
            path_prefixes: vec!["notes/".into()],
            corpus_ids: vec![],
        };
        assert!(validate_classified_scope(Some(&ordinary_prefix)).is_err());

        let corpus_scope = crate::ai_runtime::retrieval_scope::ContextScopeDto {
            paths: vec![],
            path_prefixes: vec![".classified".into()],
            corpus_ids: vec!["default".into()],
        };
        assert!(validate_classified_scope(Some(&corpus_scope)).is_err());
    }

    #[test]
    fn classified_scope_keywords_and_matching_are_strict() {
        assert!(classified_query_requests_vault_scope("请检索所有涉密材料"));
        assert!(classified_query_requests_vault_scope("到涉密库里查一下"));
        assert!(!classified_query_requests_vault_scope("只看当前这篇"));

        let scope = crate::ai_runtime::retrieval_scope::ContextScopeDto {
            paths: vec![".classified/exact.md".into()],
            path_prefixes: vec![".classified/team".into()],
            corpus_ids: vec![],
        };
        assert!(classified_scope_matches(".classified/exact.md", &scope));
        assert!(classified_scope_matches(".classified/team/a.md", &scope));
        assert!(!classified_scope_matches(".classified/other.md", &scope));
    }

    #[test]
    fn classified_writing_output_builds_controlled_patch() {
        let output = build_classified_writing_output(ClassifiedWritingOutputInput {
            request_id: "classified-request-1".into(),
            note_path: ".classified/secret.md",
            document: "# Secret\n\n原文内容\n",
            selection: "原文内容",
            message: "改写得更清晰",
            intent: crate::ai_runtime::WritingIntent::Rewrite,
            replacement: "涉密改写结果".into(),
            evidence: vec![],
            usage: TokenUsage::default(),
        });

        assert_eq!(output.request_id, "classified-request-1");
        assert_eq!(output.patches.len(), 1);
        let patch = &output.patches[0];
        assert_eq!(patch.target_path, ".classified/secret.md");
        assert_eq!(patch.original_text, "原文内容");
        assert_eq!(patch.replacement_text, "涉密改写结果");
        assert_eq!(patch.range, SourceSpan { start: 10, end: 22 });
        assert_eq!(
            output.writing_state.target_path,
            ".classified/secret.md".to_string()
        );
        assert!(output.evidence_used.is_empty());
    }

    #[test]
    fn research_wire_uses_evidence_sources_for_real_sources() {
        let payload = serde_json::json!({
            "topic": "行业资料",
            "summary": "有来源的研究摘要",
            "evidence_matrix": {
                "total_evidence_count": 1,
                "global_gaps": [],
            },
            "research_state": {
                "sources": [{ "title": "来源 A", "url": "https://example.com" }],
                "conflicts": [],
                "evidence_gaps": [],
            },
        });
        let artifacts = vec![HarnessArtifact::ResearchReport { payload }];
        let wires = artifacts_to_wires(&artifacts, "research");

        assert_eq!(wires.len(), 1);
        assert_eq!(wires[0].kind, "evidence_sources");
        assert_eq!(wires[0].evidence_count, 1);
    }

    #[test]
    fn research_wire_drops_mechanical_gap_without_sources() {
        let payload = serde_json::json!({
            "topic": "行业资料",
            "summary": "无来源研究摘要",
            "evidence_matrix": {
                "total_evidence_count": 0,
                "global_gaps": ["未授权联网，未检索到可用证据来源"],
            },
            "research_state": {
                "sources": [],
                "conflicts": [],
                "evidence_gaps": ["no evidence source was generated"],
            },
        });
        let artifacts = vec![HarnessArtifact::ResearchReport { payload }];

        assert!(artifacts_to_wires(&artifacts, "research").is_empty());
    }

    #[test]
    fn ordinary_completion_summary_is_not_a_process_wire() {
        let artifacts = vec![
            HarnessArtifact::Message {
                content: "普通回答".into(),
                citation_valid: true,
            },
            HarnessArtifact::LegacyPayload {
                intent: "chat".into(),
                json: serde_json::json!({
                    "output_summary": "assistant task completed; no process artifact generated for ordinary completion"
                }),
            },
        ];

        assert!(artifacts_to_wires(&artifacts, "chat").is_empty());
    }
}
