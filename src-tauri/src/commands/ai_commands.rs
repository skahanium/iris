//! AI Runtime IPC commands.
//!
//! These commands expose the ai_runtime pipeline to the React frontend
//! through typed Tauri IPC. Phase C: full LLM pipeline with streaming.

use crate::ai_runtime::{
    agent_task::{
        AgentTask, AgentTaskKind, AgentTaskResumePlan, AgentTaskResumePreflight, AgentTaskRuntime,
        AgentTaskStatus, BudgetPauseCheckpointInput, CreateTaskInput, TaskListFilter,
    },
    agent_task_policy::{
        intent_from_legacy_scene, resolve_for_task_policy, AgentTaskPolicy, AgentTaskPolicyInput,
        AgentTaskScope,
    },
    context_cache::ContextAssemblyCacheKey,
    context_planner::plan_context_for_policy,
    guardrails::{self, GuardResult},
    harness::{run_harness, HarnessRunInput},
    packet_builder::{build_context_packets, max_results_from_budget, ContextBuildOptions},
    retrieval_scope::ContextScopeDto,
    session::{SessionManager, SessionMessage, SessionSummary},
    tool_executor::ToolRegistry,
    trace::{TraceRecorder, TraceStatus},
    AgentIntent, AiScene, AssembledContext, ContextPacket, TokenUsage, ToolAccessLevel,
};
use std::path::Path;
use std::sync::Arc;

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::llm::config::ResolvedLlmConfig;
use crate::storage::paths::{is_user_note_path, resolve_vault_path};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{Emitter, State};
use tracing::info;

fn parse_ai_scene(scene: &str) -> AppResult<AiScene> {
    AiScene::parse_wire(scene).ok_or_else(|| AppError::msg(format!("invalid scene: {scene}")))
}

/// AI runtime only accepts ordinary user notes as note-scoped context.
pub(crate) fn validate_ai_note_path(note_path: Option<&str>) -> AppResult<()> {
    if let Some(path) = note_path {
        if !is_user_note_path(path) {
            return Err(AppError::msg("涉密笔记不能进入 AI 管道"));
        }
    }
    Ok(())
}

fn capability_slot_wire(slot: crate::ai_types::CapabilitySlot) -> &'static str {
    match slot {
        crate::ai_types::CapabilitySlot::Fast => "fast",
        crate::ai_types::CapabilitySlot::Writer => "writer",
        crate::ai_types::CapabilitySlot::Reasoner => "reasoner",
        crate::ai_types::CapabilitySlot::LongContext => "long_context",
        crate::ai_types::CapabilitySlot::Vision => "vision",
        crate::ai_types::CapabilitySlot::AgentTools => "agent_tools",
        crate::ai_types::CapabilitySlot::Embedding => "embedding",
        crate::ai_types::CapabilitySlot::Reranker => "reranker",
        crate::ai_types::CapabilitySlot::LocalPrivate => "local_private",
    }
}

fn vault_scope_hash(vault: &Path) -> String {
    let normalized = vault
        .canonicalize()
        .unwrap_or_else(|_| vault.to_path_buf())
        .to_string_lossy()
        .to_string();
    let digest = Sha256::digest(normalized.as_bytes());
    hex::encode(&digest[..12])
}

fn skill_names_for_policy(
    plan: Option<&crate::ai_types::SkillActivationPlanSummary>,
) -> Vec<String> {
    plan.map(|plan| {
        plan.activated_skills
            .iter()
            .map(|skill| skill.name.clone())
            .collect()
    })
    .unwrap_or_default()
}

fn required_permissions_for_policy(web_search: bool) -> Vec<String> {
    if web_search {
        vec![
            crate::ai_runtime::agent_permissions::AgentPermissionAtom::WebSearch
                .as_str()
                .to_string(),
        ]
    } else {
        Vec::new()
    }
}

fn build_task_budget_policy(
    policy: &AgentTaskPolicy,
    vault: &Path,
    note_path: Option<&str>,
    web_search: bool,
    skill_activation_plan: Option<&crate::ai_types::SkillActivationPlanSummary>,
) -> serde_json::Value {
    serde_json::json!({
        "mode": "lightweight",
        "agent_intent": format!("{:?}", policy.intent),
        "legacy_scene_hint": policy.legacy_scene_hint,
        "vault_scope_hash": vault_scope_hash(vault),
        "note_path": note_path,
        "required_model_slot": capability_slot_wire(policy.model_slot),
        "required_permissions": required_permissions_for_policy(web_search),
        "required_skills": skill_names_for_policy(skill_activation_plan),
    })
}

fn task_scope_for_request(note_path: Option<&str>, has_selection: bool) -> AgentTaskScope {
    if has_selection {
        AgentTaskScope::Selection
    } else if note_path.is_some() {
        AgentTaskScope::Note
    } else {
        AgentTaskScope::Vault
    }
}

fn task_kind_for_intent(intent: crate::ai_types::AgentIntent) -> AgentTaskKind {
    match intent {
        crate::ai_types::AgentIntent::Research
        | crate::ai_types::AgentIntent::CitationCheck
        | crate::ai_types::AgentIntent::DocumentCheck
        | crate::ai_types::AgentIntent::Chapter => AgentTaskKind::Complex,
        _ => AgentTaskKind::Lightweight,
    }
}

fn legacy_task_policy_from_scene(
    scene: AiScene,
    note_path: Option<&str>,
    web_search: bool,
    has_attachments: bool,
) -> AgentTaskPolicy {
    let intent = intent_from_legacy_scene(scene);
    AgentTaskPolicy::from_input(AgentTaskPolicyInput {
        intent,
        task_kind: task_kind_for_intent(intent),
        scope: task_scope_for_request(note_path, false),
        web_authorized: web_search,
        has_attachments,
        write_permission_required: matches!(
            intent,
            crate::ai_types::AgentIntent::RewriteSelection
                | crate::ai_types::AgentIntent::Write
                | crate::ai_types::AgentIntent::Chapter
                | crate::ai_types::AgentIntent::DocumentCheck
        ),
        research_depth: matches!(
            intent,
            crate::ai_types::AgentIntent::Research | crate::ai_types::AgentIntent::CitationCheck
        ) as u32,
    })
}

fn infer_agent_intent_for_new_request(
    message: &str,
    note_path: Option<&str>,
    has_attachments: bool,
) -> AgentIntent {
    if has_attachments {
        return AgentIntent::VisionChat;
    }

    let text = message.to_lowercase();
    let contains_any = |needles: &[&str]| needles.iter().any(|needle| text.contains(needle));
    if contains_any(&["skillhub", "技能", "安装 skill", "install skill"]) {
        AgentIntent::SkillManagement
    } else if contains_any(&["研究", "调研", "综述", "对比", "深挖"]) {
        AgentIntent::Research
    } else if contains_any(&["引用", "引证", "依据", "出处", "核查", "citation"]) {
        AgentIntent::CitationCheck
    } else if contains_any(&["全文检查", "文档检查", "大纲检查", "风格一致"]) {
        AgentIntent::DocumentCheck
    } else if contains_any(&["章节", "这一章", "本章", "chapter"]) {
        AgentIntent::Chapter
    } else if contains_any(&["整理", "归档", "标签", "分类", "知识库"]) {
        AgentIntent::Organize
    } else if contains_any(&["改写", "重写", "润色", "续写", "扩写", "写一段"]) {
        AgentIntent::Write
    } else if note_path.is_some()
        || contains_any(&["查一下", "查阅", "搜索", "找一下", "库里", "笔记"])
    {
        AgentIntent::AskNotes
    } else {
        AgentIntent::Chat
    }
}

fn derive_task_policy_for_new_request(
    intent: AgentIntent,
    note_path: Option<&str>,
    has_selection: bool,
    web_search: bool,
    has_attachments: bool,
) -> AgentTaskPolicy {
    AgentTaskPolicy::from_input(AgentTaskPolicyInput {
        intent,
        task_kind: task_kind_for_intent(intent),
        scope: task_scope_for_request(note_path, has_selection),
        web_authorized: web_search,
        has_attachments,
        write_permission_required: matches!(
            intent,
            AgentIntent::RewriteSelection
                | AgentIntent::Write
                | AgentIntent::Chapter
                | AgentIntent::DocumentCheck
        ),
        research_depth: if matches!(intent, AgentIntent::Research | AgentIntent::CitationCheck) {
            2
        } else {
            0
        },
    })
}

fn budget_pause_checkpoint(
    finish_reason: crate::ai_runtime::harness::HarnessFinishReason,
    selected_packet_ids: Vec<String>,
    evidence_packet_ids: Vec<String>,
    prompt_tokens: u32,
    completion_tokens: u32,
) -> serde_json::Value {
    AgentTaskRuntime::build_budget_pause_checkpoint(BudgetPauseCheckpointInput {
        finish_reason: match finish_reason {
            crate::ai_runtime::harness::HarnessFinishReason::BudgetExhausted => "budget_exhausted",
            crate::ai_runtime::harness::HarnessFinishReason::RoundLimit => "round_limit",
            crate::ai_runtime::harness::HarnessFinishReason::AwaitingConfirmation => {
                "awaiting_confirmation"
            }
            crate::ai_runtime::harness::HarnessFinishReason::Completed => "completed",
        },
        selected_packet_ids,
        evidence_packet_ids: evidence_packet_ids.clone(),
        evidence_ledger_summary: format!(
            "{} evidence packet ids retained; raw note bodies excluded",
            evidence_packet_ids.len()
        ),
        continuation_goal: "continue the paused task from compacted evidence and prior safe step"
            .into(),
        last_safe_step: "harness_segment_paused".into(),
        next_action: "resume task with compacted context".into(),
        remaining_budget_hint: serde_json::json!({
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
        }),
    })
}

fn accessible_note_paths_for_resume(vault: &Path, plan: &AgentTaskResumePlan) -> Vec<String> {
    let Some(note_path) = plan
        .budget_policy
        .get("note_path")
        .and_then(serde_json::Value::as_str)
    else {
        return Vec::new();
    };
    if validate_ai_note_path(Some(note_path)).is_err() {
        return Vec::new();
    }
    match resolve_vault_path(vault, note_path) {
        Ok(abs) if abs.exists() => vec![note_path.to_string()],
        _ => Vec::new(),
    }
}

fn enabled_skill_names_for_resume(vault: &Path) -> AppResult<Vec<String>> {
    Ok(crate::ai_runtime::skills::scan_all(vault)?
        .into_iter()
        .filter(|skill| skill.enabled)
        .map(|skill| skill.name)
        .collect())
}

fn intrinsic_resume_permission(permission: &str) -> bool {
    matches!(
        permission,
        "vault.read"
            | "vault.search"
            | "runtime.context.read"
            | "web.search"
            | "skill.read_resource"
            | "app_state.read"
            | "git.read_status"
            | "git.read_diff"
            | "git.read_log"
    )
}

fn permission_has_active_grant(
    state: &AppState,
    plan: &AgentTaskResumePlan,
    permission: &str,
) -> AppResult<bool> {
    use crate::ai_runtime::agent_permissions::{find_permission_grant, PermissionScopeKind};

    if intrinsic_resume_permission(permission) {
        return Ok(true);
    }

    let session_scope = plan.session_id.to_string();
    let vault_scope = plan.vault_scope_hash.as_deref();
    for (kind, value) in [
        (PermissionScopeKind::Request, Some(plan.request_id.as_str())),
        (PermissionScopeKind::Session, Some(session_scope.as_str())),
        (PermissionScopeKind::Vault, vault_scope),
        (PermissionScopeKind::Global, None),
    ] {
        if find_permission_grant(&state.db, permission, kind, value, None)?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn active_permissions_for_resume(
    state: &AppState,
    plan: &AgentTaskResumePlan,
) -> AppResult<Vec<String>> {
    let mut active = Vec::new();
    for permission in &plan.required_permissions {
        if permission_has_active_grant(state, plan, permission)? {
            active.push(permission.clone());
        }
    }
    Ok(active)
}

fn current_model_slot_for_resume(state: &AppState, plan: &AgentTaskResumePlan) -> Option<String> {
    if let Ok(Some(checkpoint)) =
        crate::ai_runtime::harness_support::load_harness_checkpoint(&state.db, &plan.request_id)
    {
        if let Some(policy) = checkpoint.meta.task_policy {
            let _ = resolve_for_task_policy(&state.db, &policy).ok()?;
            return Some(capability_slot_wire(policy.model_slot).to_string());
        }
    }
    let scene = load_scene_from_checkpoint(state, &plan.request_id)
        .ok()
        .or_else(|| plan.legacy_scene_hint.clone())
        .and_then(|scene| parse_ai_scene(&scene).ok())?;
    let fallback = legacy_task_policy_from_scene(scene, None, false, false);
    let _ = resolve_for_task_policy(&state.db, &fallback).ok()?;
    Some(capability_slot_wire(fallback.model_slot).to_string())
}

fn preflight_agent_task_resume(state: &AppState, plan: &AgentTaskResumePlan) -> AppResult<()> {
    let vault = state.vault_path()?;
    let preflight = AgentTaskResumePreflight {
        current_vault_scope_hash: Some(vault_scope_hash(&vault)),
        accessible_note_paths: accessible_note_paths_for_resume(&vault, plan),
        available_packet_ids: plan.evidence_refs.clone(),
        enabled_skill_names: enabled_skill_names_for_resume(&vault)?,
        active_permissions: active_permissions_for_resume(state, plan)?,
        current_model_slot: current_model_slot_for_resume(state, plan),
    };
    AgentTaskRuntime::validate_resume_preflight(plan, &preflight)
}

#[allow(clippy::too_many_arguments)]
fn build_context_packets_cached(
    state: &AppState,
    vault: &Path,
    scene: AiScene,
    note_path: Option<&str>,
    file_id: Option<i64>,
    query: &str,
    user_scope: &ContextScopeDto,
    build_opts: ContextBuildOptions,
) -> AppResult<(
    Vec<crate::ai_runtime::ContextPacket>,
    crate::ai_runtime::ContextStatus,
)> {
    let scope_json = serde_json::to_string(user_scope).unwrap_or_default();
    let cache_key = ContextAssemblyCacheKey::new(
        scene,
        note_path,
        query,
        &scope_json,
        &format!("{:?}", build_opts.strategy),
        build_opts.input_budget as u32,
    );
    if let Ok(mut cache) = state.ai.context_cache.lock() {
        if let Some(cached) = cache.get(&cache_key) {
            return Ok(cached);
        }
    }

    let built = state.db.with_conn(|conn| {
        build_context_packets(
            conn, vault, scene, note_path, file_id, query, user_scope, build_opts,
        )
    })?;

    if let Ok(mut cache) = state.ai.context_cache.lock() {
        cache.insert(cache_key, built.0.clone(), built.1.clone());
    }
    Ok(built)
}

/// Assemble context with intent detection and retrieval planning.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn context_assemble(
    state: State<'_, Arc<AppState>>,
    scene: String,
    agent_intent: Option<AgentIntent>,
    note_path: Option<String>,
    _note_content_hash: Option<String>,
    query: String,
    session_id: Option<i64>,
    context_scope: Option<ContextScopeDto>,
    web_search: Option<bool>,
) -> AppResult<AssembledContext> {
    validate_ai_note_path(note_path.as_deref())?;

    let scene = parse_ai_scene(&scene)?;
    let web_search_enabled = web_search.unwrap_or(false);
    let task_policy = derive_task_policy_for_new_request(
        agent_intent.unwrap_or_else(|| {
            infer_agent_intent_for_new_request(&query, note_path.as_deref(), false)
        }),
        note_path.as_deref(),
        false,
        web_search_enabled,
        false,
    );

    let registry = ToolRegistry::new();
    let skill_allowed_tools = state
        .vault_path()
        .ok()
        .and_then(|vault| {
            crate::ai_runtime::skills::active_skill_allowed_tools_for_task(
                &vault,
                task_policy.intent,
                Some(&state.db),
                &query,
                &[],
            )
            .ok()
        })
        .unwrap_or_default();
    let policy_ctx = crate::ai_runtime::tool_policy::ToolPolicyContext {
        task_policy: Some(task_policy.clone()),
        scene,
        autonomy_level: task_policy.autonomy_level,
        web_search_enabled,
        skill_allowed_tools,
        depth: 0,
    };
    let tools: Vec<_> = registry.tools_for_policy_surface(&policy_ctx, false);

    // Run intent detection and context planning
    let plan = plan_context_for_policy(&query, &task_policy, note_path.as_deref())?;

    // Only return an execution plan when there are multiple sub-queries,
    // so the frontend can show a preview for complex queries.
    // Single sub-queries (the common case) skip the plan and execute directly.
    let execution_plan = if plan.sub_queries.len() > 1 {
        Some(crate::ai_runtime::execution_plan::execution_plan_from_context_plan(&plan))
    } else {
        None
    };

    // Resolve file_id for graph layer
    let file_id = match &note_path {
        Some(path) => state
            .db
            .with_conn(|conn| {
                Ok(conn
                    .query_row(
                        "SELECT id FROM files WHERE path = ?1",
                        [path.as_str()],
                        |r| r.get::<_, i64>(0),
                    )
                    .ok())
            })
            .unwrap_or(None),
        None => None,
    };

    // Build context packets using the first sub-query (original)
    let primary_query = plan
        .sub_queries
        .first()
        .map(|sq| sq.query.as_str())
        .unwrap_or(&query);

    let vault = state.vault_path()?;
    let user_scope = context_scope.unwrap_or_default();
    let route = resolve_for_task_policy(&state.db, &task_policy)?;
    let resolved = route.resolved;
    let build_opts = ContextBuildOptions {
        max_results: max_results_from_budget(
            resolved.input_budget,
            scene,
            task_policy.context_strategy,
        ),
        strategy: task_policy.context_strategy,
        input_budget: resolved.input_budget,
    };
    let (packets, context_status) = build_context_packets_cached(
        state.inner().as_ref(),
        &vault,
        scene,
        note_path.as_deref(),
        file_id,
        primary_query,
        &user_scope,
        build_opts,
    )?;

    // Session is created explicitly in execute_ai_send_message via create_fresh().
    // Do NOT call SessionManager::ensure() here — it would recreate a deleted
    // session with the same session_key, causing the phantom session bug.
    let _sid = session_id;

    Ok(AssembledContext {
        provisional: true,
        packets,
        tools,
        context_status,
        execution_plan,
    })
}

/// Send an AI message with full LLM pipeline (shared by IPC and assistant facade).
#[derive(Debug, Clone)]
pub(crate) struct AiSendRoutingOverride {
    pub resolved: ResolvedLlmConfig,
    pub slot: crate::ai_types::CapabilitySlot,
    pub task_policy: AgentTaskPolicy,
    pub skill_activation_plan: Option<crate::ai_types::SkillActivationPlanSummary>,
}

/// 前端传入的图片附件 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Cluster C/D 将在后续使用此结构体
pub struct ImageAttachmentDto {
    /// UUID 标识
    pub id: String,
    /// 纯 base64 数据（不含 `data:` 前缀）
    pub data_base64: String,
    /// MIME 类型："image/png" | "image/jpeg" | "image/webp" | "image/gif"
    pub mime_type: String,
    /// 原始文件名
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    /// 文件字节数
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiToolInfo {
    pub name: String,
    pub description: String,
    pub requires_confirmation: bool,
    pub access_level: ToolAccessLevel,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct KnowledgeReindexResponse {
    pub anchors: usize,
    pub regulations: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiChatResponse {
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub session_id: i64,
    pub status: String,
    pub content: String,
    pub tool_calls: Vec<crate::ai_runtime::model_gateway::ToolCall>,
    pub tool_results: Vec<serde_json::Value>,
    pub usage: TokenUsage,
    pub usage_source: crate::ai_runtime::harness::UsageSource,
    pub citation_valid: bool,
    pub harness_rounds: u32,
    pub evidence_packets: Vec<ContextPacket>,
    pub pending_confirmation: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_refresh_notice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resumed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_skill: Option<String>,
}

impl AiChatResponse {
    fn from_harness_result(
        harness_result: &crate::ai_runtime::harness::HarnessRunResult,
        evidence_refresh_notice: Option<String>,
    ) -> Self {
        Self {
            request_id: harness_result.request_id.clone(),
            task_id: None,
            session_id: harness_result.session_id,
            status: if harness_result.pending_confirmation {
                "pending_tools".to_string()
            } else {
                "completed".to_string()
            },
            content: harness_result.content.clone(),
            tool_calls: harness_result.tool_calls.clone(),
            tool_results: harness_result.tool_results.clone(),
            usage: harness_result.usage.clone(),
            usage_source: harness_result.usage_source,
            citation_valid: harness_result.citation_valid,
            harness_rounds: harness_result.harness_rounds,
            evidence_packets: harness_result.evidence_packets.clone(),
            pending_confirmation: harness_result.pending_confirmation,
            evidence_refresh_notice,
            tool_call_id: None,
            decision: None,
            resumed: None,
            installed_skill: None,
        }
    }

    fn with_tool_confirmation(
        mut self,
        tool_call_id: String,
        decision: impl Into<String>,
        installed_skill: Option<String>,
    ) -> Self {
        self.tool_call_id = Some(tool_call_id);
        self.decision = Some(decision.into());
        self.resumed = Some(true);
        self.installed_skill = installed_skill;
        self
    }
}

#[allow(dead_code)] // Cluster C/D 将在后续使用这些方法
impl ImageAttachmentDto {
    /// 构造用于 LLM API 的 data URL。
    pub fn data_url(&self) -> String {
        format!("data:{};base64,{}", self.mime_type, self.data_base64)
    }

    /// 转换为多模态 ContentPart。
    pub fn to_content_part(&self) -> crate::ai_types::ContentPart {
        crate::ai_types::ContentPart::ImageUrl {
            image_url: crate::ai_types::ImageUrlPayload {
                url: self.data_url(),
                detail: Some("auto".into()),
            },
        }
    }
}

/// Send an AI message with full LLM pipeline (shared by IPC and assistant facade).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_ai_send_message(
    state: &AppState,
    app_handle: &tauri::AppHandle,
    scene: String,
    agent_intent: Option<AgentIntent>,
    session_id: Option<i64>,
    message: String,
    images: Option<Vec<ImageAttachmentDto>>,
    selected_packet_ids: Option<Vec<String>>,
    note_path: Option<String>,
    context_scope: Option<ContextScopeDto>,
    web_search: Option<bool>,
    new_session: Option<bool>,
) -> AppResult<AiChatResponse> {
    execute_ai_send_message_with_routing(
        state,
        app_handle,
        scene,
        agent_intent,
        session_id,
        message,
        images,
        selected_packet_ids,
        note_path,
        context_scope,
        web_search,
        new_session,
        None,
    )
    .await
}

/// Send an AI message using an already resolved capability route.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_ai_send_message_with_routing(
    state: &AppState,
    app_handle: &tauri::AppHandle,
    scene: String,
    agent_intent: Option<AgentIntent>,
    session_id: Option<i64>,
    message: String,
    images: Option<Vec<ImageAttachmentDto>>,
    selected_packet_ids: Option<Vec<String>>,
    note_path: Option<String>,
    context_scope: Option<ContextScopeDto>,
    web_search: Option<bool>,
    new_session: Option<bool>,
    routing_override: Option<AiSendRoutingOverride>,
) -> AppResult<AiChatResponse> {
    validate_ai_note_path(note_path.as_deref())?;

    let web_search = web_search.unwrap_or(false);
    let new_session = new_session.unwrap_or(false);
    let request_id = uuid::Uuid::new_v4().to_string();
    let scene = parse_ai_scene(&scene)?;
    let has_attachments = images.as_ref().is_some_and(|items| !items.is_empty());
    let task_policy = routing_override
        .as_ref()
        .map(|route| route.task_policy.clone())
        .unwrap_or_else(|| {
            derive_task_policy_for_new_request(
                agent_intent.unwrap_or_else(|| {
                    infer_agent_intent_for_new_request(
                        &message,
                        note_path.as_deref(),
                        has_attachments,
                    )
                }),
                note_path.as_deref(),
                false,
                web_search,
                has_attachments,
            )
        });

    // Start trace
    TraceRecorder::start(&state.db, &request_id, scene)?;

    app_handle
        .emit(
            "ai:request_started",
            &serde_json::json!({ "request_id": request_id }),
        )
        .map_err(|e| AppError::msg(format!("emit request_started: {e}")))?;

    // Create the durable task before guardrails or model routing so early
    // failures still have a safe lifecycle record. The user message is only
    // appended to chat history after guardrails pass.
    let sid = if new_session {
        SessionManager::create_fresh(&state.db, scene, note_path.as_deref())?
    } else if let Some(id) = session_id {
        id
    } else {
        SessionManager::ensure(&state.db, scene, note_path.as_deref())?
    };
    let task_id = AgentTaskRuntime::create_task(
        &state.db,
        CreateTaskInput {
            request_id: request_id.clone(),
            session_id: sid,
            kind: AgentTaskKind::Lightweight,
            user_input: message.clone(),
            budget_policy: serde_json::json!({
                "mode": "lightweight",
                "state": "pending_model_resolution",
            }),
        },
    )?;
    AgentTaskRuntime::record_event(
        &state.db,
        &task_id,
        "status",
        "started",
        serde_json::json!({ "request_id": request_id.clone() }),
    )?;

    // Sanitize query for injection attempts
    match guardrails::sanitize_query(&message) {
        GuardResult::Block { reason } => {
            let _ = AgentTaskRuntime::fail_safe(&state.db, &task_id, "INJECTION_BLOCKED");
            TraceRecorder::complete(
                &state.db,
                &request_id,
                TraceStatus::Failed,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some("INJECTION_BLOCKED"),
            )?;
            return Err(AppError::msg(format!(
                "query blocked by guardrails: {}",
                reason
            )));
        }
        GuardResult::Warn { reason } => {
            tracing::warn!("query warning: {}", reason);
        }
        GuardResult::Pass => {}
    }

    // Ensure session（新对话线程 vs 续接同 key 的历史）
    // Save user message (with content_parts if images present)
    let content_parts_json: Option<String> = images.as_ref().map(|imgs| {
        let parts: Vec<crate::ai_types::ContentPart> =
            imgs.iter().map(|img| img.to_content_part()).collect();
        serde_json::to_string(&parts).unwrap_or_default()
    });
    if let Err(err) = SessionManager::append_message(
        &state.db,
        sid,
        "user",
        &message,
        content_parts_json.as_deref(),
        None,
    ) {
        let _ = AgentTaskRuntime::fail_safe(&state.db, &task_id, "SESSION_APPEND_ERROR");
        return Err(err);
    }

    // Get session history for context
    let history = match SessionManager::recent_messages(&state.db, sid, 20) {
        Ok(history) => history,
        Err(err) => {
            let _ = AgentTaskRuntime::fail_safe(&state.db, &task_id, "SESSION_HISTORY_ERROR");
            return Err(err);
        }
    };

    // Build context packets
    let vault = match state.vault_path() {
        Ok(vault) => vault,
        Err(err) => {
            let _ = AgentTaskRuntime::fail_safe(&state.db, &task_id, "VAULT_SCOPE_ERROR");
            return Err(err);
        }
    };
    let user_scope = context_scope.unwrap_or_default();
    let file_id = match &note_path {
        Some(path) => state
            .db
            .with_conn(|conn| {
                Ok(conn
                    .query_row(
                        "SELECT id FROM files WHERE path = ?1",
                        [path.as_str()],
                        |r| r.get::<_, i64>(0),
                    )
                    .ok())
            })
            .unwrap_or(None),
        None => None,
    };
    let skill_activation_plan = routing_override
        .as_ref()
        .and_then(|route| route.skill_activation_plan.clone());
    let (resolved, route_slot) = if let Some(route) = routing_override {
        (route.resolved, route.slot)
    } else {
        match resolve_for_task_policy(&state.db, &task_policy) {
            Ok(route) => (route.resolved, route.summary.slot),
            Err(err) => {
                let _ = AgentTaskRuntime::fail_safe(&state.db, &task_id, "MODEL_CONFIG_ERROR");
                return Err(err);
            }
        }
    };
    AgentTaskRuntime::record_event(
        &state.db,
        &task_id,
        "budget",
        "resolved",
        serde_json::json!({
            "input_budget": resolved.input_budget,
            "output_budget": resolved.output_budget,
        }),
    )?;
    let build_opts = ContextBuildOptions {
        max_results: max_results_from_budget(
            resolved.input_budget,
            scene,
            task_policy.context_strategy,
        ),
        strategy: task_policy.context_strategy,
        input_budget: resolved.input_budget,
    };
    let (packets, _context_status) = match build_context_packets_cached(
        state,
        &vault,
        scene,
        note_path.as_deref(),
        file_id,
        &message,
        &user_scope,
        build_opts,
    ) {
        Ok(built) => built,
        Err(err) => {
            let _ = AgentTaskRuntime::fail_safe(&state.db, &task_id, "CONTEXT_BUILD_ERROR");
            return Err(err);
        }
    };

    let ledger = crate::ai_runtime::evidence_ledger::EvidenceLedger::new(packets);
    let (resolved_ids, evidence_refresh_notice) = if let Some(ids) = &selected_packet_ids {
        ledger.resolve_selected_packet_ids(ids, ledger.packets())?
    } else {
        (vec![], None)
    };
    let filtered_packets: Vec<_> = if resolved_ids.is_empty() {
        ledger.packets().to_vec()
    } else {
        ledger
            .packets()
            .iter()
            .filter(|p| resolved_ids.contains(&p.id))
            .cloned()
            .collect()
    };

    let note_title = note_path.as_ref().and_then(|p| {
        state
            .db
            .with_conn(|conn| {
                Ok(conn
                    .query_row(
                        "SELECT title FROM files WHERE path = ?1",
                        [p.as_str()],
                        |r| r.get::<_, String>(0),
                    )
                    .ok())
            })
            .ok()
            .flatten()
    });

    let history_messages: Vec<(String, String)> = history
        .iter()
        .map(|m| (m.role.clone(), m.content.clone()))
        .collect();

    let provider_config = resolved.to_provider_config_for_slot(route_slot);
    let provider_name = provider_config.name.clone();
    if let Err(err) = AgentTaskRuntime::update_budget_policy(
        &state.db,
        &task_id,
        build_task_budget_policy(
            &task_policy,
            &vault,
            note_path.as_deref(),
            web_search,
            skill_activation_plan.as_ref(),
        ),
    ) {
        let _ = AgentTaskRuntime::fail_safe(&state.db, &task_id, "TASK_POLICY_ERROR");
        return Err(err);
    }

    TraceRecorder::update_status(&state.db, &request_id, TraceStatus::ContextAssembled)?;

    let harness_result = match run_harness(
        state,
        app_handle,
        HarnessRunInput {
            request_id: request_id.clone(),
            scene,
            session_id: sid,
            note_path: note_path.clone(),
            note_title,
            selection_excerpt: None,
            cold_start_packets: filtered_packets.clone(),
            web_search_enabled: web_search,
            user_message: message.clone(),
            images,
            history_messages,
            depth: 0,
            resume_from_checkpoint: false,
            token_budget: None,
            max_rounds_override: None,
            skill_activation_plan,
            task_policy: task_policy.clone(),
        },
        provider_config,
        Some(resolved.output_budget),
        resolved.thinking,
    )
    .await
    {
        Ok(result) => result,
        Err(err) => {
            let _ = AgentTaskRuntime::fail_safe(&state.db, &task_id, "HARNESS_ERROR");
            return Err(err);
        }
    };

    TraceRecorder::update_status(&state.db, &request_id, TraceStatus::ModelCalled)?;
    let task_status = if harness_result.pending_confirmation {
        AgentTaskStatus::AwaitingConfirmation
    } else if matches!(
        harness_result.finish_reason,
        crate::ai_runtime::harness::HarnessFinishReason::BudgetExhausted
            | crate::ai_runtime::harness::HarnessFinishReason::RoundLimit
    ) {
        AgentTaskStatus::PausedBudget
    } else {
        AgentTaskStatus::Completed
    };
    let evidence_packet_ids = filtered_packets
        .iter()
        .map(|p| p.id.clone())
        .collect::<Vec<_>>();
    let resume_selected_packet_ids = if resolved_ids.is_empty() {
        evidence_packet_ids.clone()
    } else {
        resolved_ids.clone()
    };
    let pause_checkpoint = budget_pause_checkpoint(
        harness_result.finish_reason,
        resume_selected_packet_ids,
        evidence_packet_ids.clone(),
        harness_result.usage.prompt_tokens,
        harness_result.usage.completion_tokens,
    );
    AgentTaskRuntime::record_step(
        &state.db,
        &task_id,
        "respond",
        task_status,
        "user message summarized in agent_tasks",
        "assistant response summarized by session message",
        if task_status == AgentTaskStatus::PausedBudget {
            pause_checkpoint.clone()
        } else {
            serde_json::json!({
            "summary": if harness_result.pending_confirmation {
                "awaiting tool confirmation"
            } else if task_status == AgentTaskStatus::PausedBudget {
                "paused after segment budget exhaustion"
            } else {
                "assistant response completed"
            },
            "packet_ids": evidence_packet_ids,
            "finish_reason": harness_result.finish_reason,
            })
        },
    )?;

    if task_status == AgentTaskStatus::PausedBudget {
        AgentTaskRuntime::pause_budget(
            &state.db,
            &task_id,
            "segment paused before producing a reliable final answer",
            pause_checkpoint,
        )?;
    } else if !harness_result.pending_confirmation {
        if let Err(err) = finalize_chat_harness_run(
            state,
            &request_id,
            sid,
            route_slot,
            &provider_name,
            &harness_result,
            &filtered_packets,
        ) {
            let _ = AgentTaskRuntime::fail_safe(&state.db, &task_id, "FINALIZE_ERROR");
            return Err(err);
        }
        AgentTaskRuntime::complete_task(&state.db, &task_id)?;
    } else {
        AgentTaskRuntime::await_confirmation(&state.db, &task_id)?;
    }

    info!(
        scene = ?scene,
        provider = %provider_name,
        harness_rounds = harness_result.harness_rounds,
        pending_confirmation = harness_result.pending_confirmation,
        tokens_input = %harness_result.usage.prompt_tokens,
        tokens_output = %harness_result.usage.completion_tokens,
        "AI harness request completed"
    );

    let mut response =
        AiChatResponse::from_harness_result(&harness_result, evidence_refresh_notice);
    response.request_id = request_id;
    response.task_id = Some(task_id);
    response.session_id = sid;
    response.status = match task_status {
        AgentTaskStatus::AwaitingConfirmation => "pending_tools",
        AgentTaskStatus::PausedBudget => "paused_budget",
        AgentTaskStatus::Completed => "completed",
        AgentTaskStatus::PausedRecoverable => "paused_recoverable",
        AgentTaskStatus::FailedSafe => "failed_safe",
        AgentTaskStatus::Aborted => "aborted",
        AgentTaskStatus::Queued => "queued",
        AgentTaskStatus::Running => "running",
    }
    .to_string();
    Ok(response)
}

/// Send an AI message with full LLM pipeline.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn ai_send_message(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    scene: String,
    agent_intent: Option<AgentIntent>,
    session_id: Option<i64>,
    message: String,
    images: Option<Vec<ImageAttachmentDto>>,
    selected_packet_ids: Option<Vec<String>>,
    note_path: Option<String>,
    context_scope: Option<ContextScopeDto>,
    web_search: Option<bool>,
    new_session: Option<bool>,
) -> AppResult<AiChatResponse> {
    execute_ai_send_message(
        state.inner().as_ref(),
        &app_handle,
        scene,
        agent_intent,
        session_id,
        message,
        images,
        selected_packet_ids,
        note_path,
        context_scope,
        web_search,
        new_session,
    )
    .await
}

/// Persist session + trace after a completed harness run (not pending confirmation).
fn finalize_chat_harness_run(
    state: &AppState,
    request_id: &str,
    session_id: i64,
    capability_slot: crate::ai_types::CapabilitySlot,
    provider_name: &str,
    harness_result: &crate::ai_runtime::harness::HarnessRunResult,
    filtered_packets: &[crate::ai_runtime::ContextPacket],
) -> AppResult<()> {
    let tool_calls_value: Option<serde_json::Value> = if harness_result.tool_calls.is_empty() {
        None
    } else {
        Some(serde_json::to_value(&harness_result.tool_calls).unwrap_or_default())
    };
    SessionManager::append_message(
        &state.db,
        session_id,
        "assistant",
        &harness_result.content,
        None,
        tool_calls_value.as_ref(),
    )?;

    if harness_result.usage.prompt_cache_hit_tokens > 0
        || harness_result.usage.prompt_cache_miss_tokens > 0
    {
        let _ = crate::llm::config::save_usage_last(
            &state.db,
            harness_result.usage.prompt_cache_hit_tokens,
            harness_result.usage.prompt_cache_miss_tokens,
        );
    }

    TraceRecorder::complete(
        &state.db,
        request_id,
        TraceStatus::Completed,
        Some(&format!("{capability_slot:?}")),
        Some(provider_name),
        Some(
            &harness_result
                .tool_calls
                .iter()
                .map(|tc| tc.function.name.clone())
                .collect::<Vec<_>>(),
        ),
        Some(
            &filtered_packets
                .iter()
                .map(|p| p.id.clone())
                .collect::<Vec<_>>(),
        ),
        None,
        Some(harness_result.usage.prompt_tokens),
        Some(harness_result.usage.completion_tokens),
        None,
    )?;
    Ok(())
}

fn harness_run_to_chat_response(
    state: &AppState,
    harness_result: &crate::ai_runtime::harness::HarnessRunResult,
) -> AppResult<AiChatResponse> {
    let mut response = AiChatResponse::from_harness_result(harness_result, None);
    response.task_id =
        AgentTaskRuntime::task_id_for_request(&state.db, &harness_result.request_id)?;
    if matches!(
        harness_result.finish_reason,
        crate::ai_runtime::harness::HarnessFinishReason::BudgetExhausted
            | crate::ai_runtime::harness::HarnessFinishReason::RoundLimit
    ) {
        response.status = "paused_budget".to_string();
    }
    Ok(response)
}

fn harness_has_reliable_final_answer(
    harness_result: &crate::ai_runtime::harness::HarnessRunResult,
) -> bool {
    !harness_result.pending_confirmation
        && matches!(
            harness_result.finish_reason,
            crate::ai_runtime::harness::HarnessFinishReason::Completed
        )
}

fn sync_agent_task_after_harness(
    state: &AppState,
    harness_result: &crate::ai_runtime::harness::HarnessRunResult,
) -> AppResult<()> {
    let Some(task_id) =
        AgentTaskRuntime::task_id_for_request(&state.db, &harness_result.request_id)?
    else {
        return Ok(());
    };
    if harness_result.pending_confirmation {
        return AgentTaskRuntime::await_confirmation(&state.db, &task_id);
    }
    if matches!(
        harness_result.finish_reason,
        crate::ai_runtime::harness::HarnessFinishReason::BudgetExhausted
            | crate::ai_runtime::harness::HarnessFinishReason::RoundLimit
    ) {
        let packet_ids = harness_result
            .evidence_packets
            .iter()
            .map(|p| p.id.clone())
            .collect::<Vec<_>>();
        return AgentTaskRuntime::pause_budget(
            &state.db,
            &task_id,
            "resumed segment paused before producing a reliable final answer",
            budget_pause_checkpoint(
                harness_result.finish_reason,
                packet_ids.clone(),
                packet_ids,
                harness_result.usage.prompt_tokens,
                harness_result.usage.completion_tokens,
            ),
        );
    }
    AgentTaskRuntime::complete_task(&state.db, &task_id)
}

/// Handle tool confirmation from the user.
#[tauri::command]
pub async fn tool_confirm(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    request_id: String,
    tool_call_id: String,
    decision: String,
    modified_args: Option<serde_json::Value>,
) -> AppResult<AiChatResponse> {
    use crate::ai_runtime::harness_confirm::{
        append_rejected_tool_to_checkpoint, dispatch_approved_tool_to_checkpoint,
        resume_harness_after_tool_confirm_or_restore,
    };

    if decision == "reject" {
        crate::llm::safe_lock(&state.ai.pending_tool_calls).remove(&tool_call_id);
        append_rejected_tool_to_checkpoint(state.inner(), &request_id, &tool_call_id)?;
        let harness_result =
            resume_harness_after_tool_confirm_or_restore(state.inner(), &app_handle, &request_id)
                .await?;
        if harness_has_reliable_final_answer(&harness_result) {
            finalize_chat_harness_run_from_task_policy(
                state.inner(),
                &request_id,
                &harness_result,
            )?;
        }
        sync_agent_task_after_harness(state.inner(), &harness_result)?;
        return Ok(
            harness_run_to_chat_response(state.inner(), &harness_result)?.with_tool_confirmation(
                tool_call_id,
                "reject",
                None,
            ),
        );
    }

    let pending = crate::llm::safe_lock(&state.ai.pending_tool_calls).remove(&tool_call_id);
    let Some(pending) = pending else {
        return Err(AppError::msg(format!(
            "no pending tool call for id: {tool_call_id}"
        )));
    };

    let args: serde_json::Value = if let Some(args) = modified_args {
        args
    } else {
        serde_json::from_str(&pending.arguments).unwrap_or_default()
    };

    dispatch_approved_tool_to_checkpoint(
        state.inner(),
        &app_handle,
        &pending,
        &tool_call_id,
        &args,
    )
    .await?;

    let mut installed_skill: Option<String> = None;
    if pending.tool_name == "skills_install" {
        if let Ok(Some(cp)) =
            crate::ai_runtime::harness_support::load_harness_checkpoint(&state.db, &request_id)
        {
            if let Some(msg) = cp.messages.iter().rev().find(|m| {
                matches!(m.role, crate::ai_runtime::model_gateway::MessageRole::Tool)
                    && m.tool_call_id.as_deref() == Some(tool_call_id.as_str())
            }) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(msg.content.as_str()) {
                    installed_skill = json.get("name").and_then(|n| n.as_str()).map(String::from);
                }
            }
        }
    }

    let harness_result =
        resume_harness_after_tool_confirm_or_restore(state.inner(), &app_handle, &request_id)
            .await?;

    if harness_has_reliable_final_answer(&harness_result) {
        finalize_chat_harness_run_from_task_policy(state.inner(), &request_id, &harness_result)?;
    }
    sync_agent_task_after_harness(state.inner(), &harness_result)?;

    let out = harness_run_to_chat_response(state.inner(), &harness_result)?.with_tool_confirmation(
        tool_call_id,
        if decision == "modify" {
            "modify"
        } else {
            "approve"
        },
        installed_skill,
    );

    // Migration-period event; frontend should use resumed harness payload.
    let _ = app_handle.emit("ai:tool_result", &out);

    Ok(out)
}

fn load_scene_from_checkpoint(state: &AppState, request_id: &str) -> AppResult<String> {
    use crate::ai_runtime::harness_support::load_harness_checkpoint;
    let cp = load_harness_checkpoint(&state.db, request_id)?
        .ok_or_else(|| AppError::msg("checkpoint missing"))?;
    Ok(cp.meta.scene)
}

fn task_policy_from_checkpoint(state: &AppState, request_id: &str) -> AppResult<AgentTaskPolicy> {
    use crate::ai_runtime::harness_support::load_harness_checkpoint;
    let cp = load_harness_checkpoint(&state.db, request_id)?
        .ok_or_else(|| AppError::msg("checkpoint missing"))?;
    if let Some(policy) = cp.meta.task_policy {
        return Ok(policy);
    }

    let scene = parse_ai_scene(&cp.meta.scene)?;
    Ok(legacy_task_policy_from_scene(
        scene,
        cp.meta.note_path.as_deref(),
        cp.meta.web_search_enabled,
        false,
    ))
}

fn finalize_chat_harness_run_from_task_policy(
    state: &AppState,
    request_id: &str,
    harness_result: &crate::ai_runtime::harness::HarnessRunResult,
) -> AppResult<()> {
    let policy = task_policy_from_checkpoint(state, request_id)?;
    let route = resolve_for_task_policy(&state.db, &policy)?;
    let provider_config = route
        .resolved
        .to_provider_config_for_slot(route.summary.slot);
    finalize_chat_harness_run(
        state,
        request_id,
        harness_result.session_id,
        route.summary.slot,
        &provider_config.name,
        harness_result,
        &harness_result.evidence_packets,
    )
}

/// Get available tools for a scene (for frontend display).
#[tauri::command]
pub fn ai_list_tools(state: State<'_, Arc<AppState>>, scene: String) -> AppResult<Vec<AiToolInfo>> {
    let scene = parse_ai_scene(&scene)?;
    let registry = ToolRegistry::new();
    let task_policy = legacy_task_policy_from_scene(scene, None, true, false);
    let skill_allowed_tools = state
        .vault_path()
        .ok()
        .and_then(|vault| {
            crate::ai_runtime::skills::active_skill_allowed_tools_for_task(
                &vault,
                task_policy.intent,
                Some(&state.db),
                scene.profile(),
                &[],
            )
            .ok()
        })
        .unwrap_or_default();
    let ctx = crate::ai_runtime::tool_policy::ToolPolicyContext {
        task_policy: Some(task_policy.clone()),
        scene,
        autonomy_level: task_policy.autonomy_level,
        web_search_enabled: true,
        skill_allowed_tools,
        depth: 0,
    };
    let tools: Vec<_> = registry
        .tools_for_policy_surface(&ctx, false)
        .iter()
        .map(|t| AiToolInfo {
            name: t.name.clone(),
            description: t.description.clone(),
            requires_confirmation: t.requires_confirmation,
            access_level: t.access_level,
        })
        .collect();
    Ok(tools)
}

/// Re-index all knowledge: anchors, regulations, block links.
#[tauri::command]
pub async fn knowledge_reindex(
    state: State<'_, Arc<AppState>>,
) -> AppResult<KnowledgeReindexResponse> {
    let vault = state.vault_path()?;
    let mut stats = KnowledgeReindexResponse::default();

    state.db.with_conn(|conn| {
        // Re-index regulations
        match crate::knowledge::regulations::reindex_all_regulations(conn, &vault) {
            Ok(count) => {
                stats.regulations = count;
            }
            Err(e) => tracing::warn!("regulation reindex error: {e}"),
        }
        Ok::<_, crate::error::AppError>(())
    })?;
    if let Ok(mut cache) = state.ai.context_cache.lock() {
        cache.clear();
    }

    Ok(stats)
}

/// Hybrid search across all knowledge layers.
#[tauri::command]
pub async fn search_hybrid(
    state: State<'_, Arc<AppState>>,
    query: String,
    scene: Option<String>,
    note_path: Option<String>,
    limit: Option<usize>,
) -> AppResult<Vec<serde_json::Value>> {
    validate_ai_note_path(note_path.as_deref())?;

    let _scene: AiScene = scene
        .as_deref()
        .map(parse_ai_scene)
        .transpose()?
        .unwrap_or(AiScene::KnowledgeLookup);

    let file_id = match &note_path {
        Some(path) => state
            .db
            .with_conn(|conn| {
                Ok(conn
                    .query_row(
                        "SELECT id FROM files WHERE path = ?1",
                        [path.as_str()],
                        |r| r.get::<_, i64>(0),
                    )
                    .ok())
            })
            .unwrap_or(None),
        None => None,
    };

    let layers = crate::ai_runtime::retrieval_broker::RetrievalLayers {
        fts: true,
        vector: true,
        graph: true,
        exact: true,
        template: false,
    };

    let request = crate::ai_runtime::retrieval_broker::RetrievalRequest {
        query,
        max_results: limit.unwrap_or(15),
        layers,
        note_context: note_path,
        file_id_context: file_id,
        scope: crate::ai_runtime::retrieval_scope::RetrievalScope::default(),
    };

    let packets = state
        .db
        .with_conn(|conn| crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request))?;

    let json_packets: Vec<_> = packets
        .into_iter()
        .map(|p| serde_json::to_value(p).unwrap_or_default())
        .collect();

    Ok(json_packets)
}

/// List chat sessions for history UI.
#[tauri::command]
pub async fn session_list(
    state: State<'_, Arc<AppState>>,
    scene: Option<String>,
    note_path: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Vec<SessionSummary>> {
    SessionManager::list_sessions(
        &state.db,
        scene.as_deref(),
        note_path.as_deref(),
        limit.unwrap_or(50),
        offset.unwrap_or(0),
    )
}

/// Delete a session by id.
#[tauri::command]
pub async fn session_delete(state: State<'_, Arc<AppState>>, session_id: i64) -> AppResult<bool> {
    SessionManager::delete_session(&state.db, session_id)
}

/// Rename session title.
#[tauri::command]
pub async fn session_rename(
    state: State<'_, Arc<AppState>>,
    session_id: i64,
    title: String,
) -> AppResult<()> {
    SessionManager::rename_session(&state.db, session_id, title.trim())
}

/// 撤回：删除指定 seq 及之后的所有消息。
/// 返回被删除的消息数量。
#[tauri::command]
pub async fn session_retract(
    state: State<'_, Arc<AppState>>,
    session_id: i64,
    from_seq: i64,
) -> AppResult<u32> {
    SessionManager::retract_messages(&state.db, session_id, from_seq)
}

/// Load recent messages for a session.
#[tauri::command]
pub async fn session_load(
    state: State<'_, Arc<AppState>>,
    session_id: i64,
    limit: Option<u32>,
) -> AppResult<Vec<SessionMessage>> {
    SessionManager::recent_messages(&state.db, session_id, limit.unwrap_or(50))
}

/// List installed skills (global + vault) with validation status.
#[tauri::command]
pub async fn skills_list(
    state: State<'_, Arc<AppState>>,
    scene: Option<String>,
) -> AppResult<Vec<crate::ai_runtime::skills::SkillListEntry>> {
    let vault = state.vault_path()?;
    let scene = scene.as_deref().map(parse_ai_scene).transpose()?;
    crate::ai_runtime::skill_install_service::list_skills(&state.db, &vault, scene)
}

#[derive(Debug, Serialize)]
pub struct SkillsPathsResponse {
    pub global: String,
    pub vault: String,
}

/// Return resolved global and vault skill installation directories.
#[tauri::command]
pub async fn skills_paths(state: State<'_, Arc<AppState>>) -> AppResult<SkillsPathsResponse> {
    use crate::ai_runtime::skills::{global_skills_dir, vault_skills_dir};

    let vault = state.vault_path()?;
    Ok(SkillsPathsResponse {
        global: global_skills_dir().to_string_lossy().into_owned(),
        vault: vault_skills_dir(&vault).to_string_lossy().into_owned(),
    })
}

#[derive(Debug, serde::Deserialize)]
pub struct SkillsInstallRequest {
    pub source: String,
    pub path_or_url: String,
    pub scope: Option<String>,
    pub subpath: Option<String>,
    pub registry: Option<String>,
    pub expected_sha256: Option<String>,
}

/// Install skill from url, git, local, or registry.
#[tauri::command]
pub async fn skills_install(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    request: SkillsInstallRequest,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skill_install_service::{
        install_skill, normalize_skill_scope_arg, SkillInstallRequest,
    };
    use crate::ai_runtime::skill_registry::SkillInstallSource;

    let source = SkillInstallSource::parse(&request.source)
        .ok_or_else(|| AppError::msg(format!("unknown install source: {}", request.source)))?;
    let vault = state.vault_path()?;
    let req = SkillInstallRequest {
        source,
        path_or_url: request.path_or_url,
        scope: normalize_skill_scope_arg(request.scope.as_deref()),
        subpath: request.subpath,
        registry: request.registry,
        expected_sha256: request.expected_sha256,
    };
    let entry = install_skill(&state.db, &vault, Some(&app_handle), req).await?;
    Ok(serde_json::to_value(entry).unwrap_or_default())
}

#[derive(Debug, serde::Deserialize)]
pub struct SkillsPrepareWorkspaceRequest {
    pub name: String,
    pub scope: Option<String>,
}

#[tauri::command]
pub async fn skills_prepare_workspace(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    request: SkillsPrepareWorkspaceRequest,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skill_install_service::{
        normalize_skill_scope_arg, prepare_skill_workspace,
    };

    let vault = state.vault_path()?;
    let result = prepare_skill_workspace(
        &vault,
        Some(&state.db),
        Some(&app_handle),
        &request.name,
        normalize_skill_scope_arg(request.scope.as_deref()),
    )?;
    Ok(serde_json::to_value(result).unwrap_or_default())
}

#[tauri::command]
pub async fn skills_uninstall(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    name: String,
    scope: String,
) -> AppResult<()> {
    use crate::ai_runtime::skill_install_service::{parse_scope, uninstall_skill};
    let vault = state.vault_path()?;
    uninstall_skill(
        &state.db,
        &vault,
        Some(&app_handle),
        &name,
        parse_scope(&scope),
    )
}

#[tauri::command]
pub async fn skills_update(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    name: String,
    scope: String,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skill_install_service::{parse_scope, update_skill};
    let vault = state.vault_path()?;
    let entry = update_skill(
        &state.db,
        &vault,
        Some(&app_handle),
        &name,
        parse_scope(&scope),
    )
    .await?;
    Ok(serde_json::to_value(entry).unwrap_or_default())
}

#[tauri::command]
pub async fn skills_toggle(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    name: String,
    scope: String,
    enabled: bool,
) -> AppResult<()> {
    use crate::ai_runtime::skill_install_service::{parse_scope, toggle_skill};
    let vault = state.vault_path()?;
    toggle_skill(
        &vault,
        Some(&app_handle),
        &name,
        parse_scope(&scope),
        enabled,
    )
}

#[tauri::command]
pub async fn prompt_profile_get(
    state: State<'_, Arc<AppState>>,
) -> AppResult<crate::ai_runtime::prompt_profile::PromptProfile> {
    crate::ai_runtime::prompt_profile::PromptProfile::load(&state.db)
}

#[tauri::command]
pub async fn prompt_profile_set(
    state: State<'_, Arc<AppState>>,
    profile: crate::ai_runtime::prompt_profile::PromptProfile,
) -> AppResult<()> {
    crate::ai_runtime::prompt_profile::PromptProfile::save(&state.db, &profile)
}

/// List built-in prompt profile presets.
#[tauri::command]
pub fn prompt_profile_presets() -> Vec<serde_json::Value> {
    crate::ai_runtime::prompt_profile::preset_templates()
        .into_iter()
        .map(|(label, profile)| serde_json::json!({ "label": label, "profile": profile }))
        .collect()
}

/// Delete all sessions matching scene / note_path filter.
#[tauri::command]
pub async fn session_clear_all(
    state: State<'_, Arc<AppState>>,
    scene: Option<String>,
    note_path: Option<String>,
) -> AppResult<u32> {
    SessionManager::delete_all_filtered(&state.db, scene.as_deref(), note_path.as_deref())
}

/// Clear persisted AI runtime cache (sessions, harness checkpoints, knowledge deposits).
#[tauri::command]
pub async fn ai_cache_clear(state: State<'_, Arc<AppState>>) -> AppResult<serde_json::Value> {
    let aborted_tasks = AgentTaskRuntime::abort_recoverable_tasks(
        &state.db,
        "CACHE_CLEAR",
        "AI cache clear invalidated recoverable task state",
    )?;
    let sessions = SessionManager::delete_all_filtered(&state.db, None, None)?;
    let (checkpoints, deposits, traces) = state.db.with_conn(|conn| {
        let checkpoints = conn.execute(
            "UPDATE ai_traces SET checkpoint = NULL WHERE checkpoint IS NOT NULL",
            [],
        )?;
        let deposits = conn
            .execute("DELETE FROM knowledge_deposits", [])
            .unwrap_or(0);
        let traces = conn.execute("DELETE FROM ai_traces", []).unwrap_or(0);
        Ok::<_, crate::error::AppError>((checkpoints, deposits, traces))
    })?;
    let web_pages = crate::llm::fetch_web_page::clear_web_cache(&state.db).unwrap_or(0);
    let searches = crate::llm::search_web::cleanup_expired_search_cache(&state.db).unwrap_or(0);
    if let Ok(mut cache) = state.ai.context_cache.lock() {
        cache.clear();
    }
    Ok(serde_json::json!({
        "sessions_deleted": sessions,
        "aborted_tasks": aborted_tasks,
        "checkpoints_cleared": checkpoints,
        "deposits_deleted": deposits,
        "traces_deleted": traces,
        "web_pages_cleared": web_pages,
        "searches_cleared": searches,
    }))
}

/// Resume an interrupted harness run from checkpoint.
#[tauri::command]
pub async fn harness_resume(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    request_id: String,
) -> AppResult<AiChatResponse> {
    use crate::ai_runtime::harness_confirm::resume_harness_after_tool_confirm_or_restore;

    let harness_result =
        resume_harness_after_tool_confirm_or_restore(state.inner(), &app_handle, &request_id)
            .await?;

    if harness_has_reliable_final_answer(&harness_result) {
        finalize_chat_harness_run_from_task_policy(state.inner(), &request_id, &harness_result)?;
    }
    sync_agent_task_after_harness(state.inner(), &harness_result)?;

    harness_run_to_chat_response(state.inner(), &harness_result)
}

/// Return a durable Agent Task by id.
#[tauri::command]
pub async fn agent_task_get(
    state: State<'_, Arc<AppState>>,
    task_id: String,
) -> AppResult<Option<AgentTask>> {
    AgentTaskRuntime::get_task(&state.db, &task_id)
}

/// List durable Agent Tasks for the current task UI.
#[tauri::command]
pub async fn agent_task_list(
    state: State<'_, Arc<AppState>>,
    session_id: Option<i64>,
    status: Option<String>,
) -> AppResult<Vec<AgentTask>> {
    let status = match status {
        Some(value) => Some(
            AgentTaskStatus::parse_wire(&value)
                .ok_or_else(|| AppError::msg(format!("invalid task status: {value}")))?,
        ),
        None => None,
    };
    AgentTaskRuntime::list_tasks(&state.db, TaskListFilter { session_id, status })
}

/// List summary-only task steps for the task UI.
#[tauri::command]
pub async fn agent_task_steps(
    state: State<'_, Arc<AppState>>,
    task_id: String,
) -> AppResult<Vec<crate::ai_runtime::agent_task::AgentTaskStep>> {
    AgentTaskRuntime::list_steps(&state.db, &task_id)
}

/// List summary-only task events for the task UI.
#[tauri::command]
pub async fn agent_task_events(
    state: State<'_, Arc<AppState>>,
    task_id: String,
) -> AppResult<Vec<crate::ai_runtime::agent_task::AgentTaskEvent>> {
    AgentTaskRuntime::list_events(&state.db, &task_id)
}

/// Resume a paused Agent Task by durable task id.
#[tauri::command]
pub async fn agent_task_resume(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    task_id: String,
) -> AppResult<AiChatResponse> {
    use crate::ai_runtime::harness_confirm::resume_harness_after_tool_confirm_or_restore;

    let plan = AgentTaskRuntime::prepare_resume_plan(&state.db, &task_id)?;
    if let Err(err) = preflight_agent_task_resume(state.inner(), &plan) {
        let _ = AgentTaskRuntime::pause_recoverable(
            &state.db,
            &plan.task_id,
            "RESUME_PREFLIGHT_FAILED",
        );
        return Err(err);
    }
    AgentTaskRuntime::begin_resume(&state.db, &task_id, &plan)?;
    let harness_result = match resume_harness_after_tool_confirm_or_restore(
        state.inner(),
        &app_handle,
        &plan.request_id,
    )
    .await
    {
        Ok(result) => result,
        Err(err) => {
            let _ = AgentTaskRuntime::pause_recoverable(&state.db, &plan.task_id, "RESUME_FAILED");
            return Err(err);
        }
    };

    if harness_has_reliable_final_answer(&harness_result) {
        let finalize_result = finalize_chat_harness_run_from_task_policy(
            state.inner(),
            &plan.request_id,
            &harness_result,
        );
        if let Err(err) = finalize_result {
            let _ = AgentTaskRuntime::pause_recoverable(
                &state.db,
                &plan.task_id,
                "RESUME_FINALIZE_FAILED",
            );
            return Err(err);
        }
    }
    if let Err(err) = sync_agent_task_after_harness(state.inner(), &harness_result) {
        let _ = AgentTaskRuntime::pause_recoverable(&state.db, &plan.task_id, "RESUME_SYNC_FAILED");
        return Err(err);
    }

    let mut response = harness_run_to_chat_response(state.inner(), &harness_result)?;
    response.resumed = Some(true);
    Ok(response)
}

/// Abort a durable Agent Task and any active model request associated with it.
#[tauri::command]
pub async fn agent_task_abort(state: State<'_, Arc<AppState>>, task_id: String) -> AppResult<()> {
    let task = AgentTaskRuntime::get_task(&state.db, &task_id)?
        .ok_or_else(|| AppError::msg("agent task not found"))?;
    crate::ai_runtime::model_gateway::request_abort(&task.request_id);
    let _ = TraceRecorder::update_status(&state.db, &task.request_id, TraceStatus::Aborted);
    AgentTaskRuntime::abort_task(&state.db, &task_id)
}

/// Abort an active harness/model request.
#[tauri::command]
pub async fn harness_abort(state: State<'_, Arc<AppState>>, request_id: String) -> AppResult<()> {
    crate::ai_runtime::model_gateway::request_abort(&request_id);
    let _ = TraceRecorder::update_status(&state.db, &request_id, TraceStatus::Aborted);
    if let Some(task_id) = AgentTaskRuntime::task_id_for_request(&state.db, &request_id)? {
        AgentTaskRuntime::abort_task(&state.db, &task_id)?;
    }
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
pub struct SkillsReadResourceRequest {
    pub name: String,
    pub scope: Option<String>,
    pub relative_path: String,
}

/// Read a file under a skill's references/scripts/assets directory.
#[tauri::command]
pub async fn skills_read_resource(
    state: State<'_, Arc<AppState>>,
    request: SkillsReadResourceRequest,
) -> AppResult<String> {
    use crate::ai_runtime::skill_install_service::normalize_skill_scope_arg;
    use crate::ai_runtime::skills::read_skill_resource;
    let vault = state.vault_path()?;
    read_skill_resource(
        &vault,
        &request.name,
        normalize_skill_scope_arg(request.scope.as_deref()),
        &request.relative_path,
    )
}

#[derive(Debug, serde::Deserialize)]
pub struct SkillsReadRequest {
    pub file_path: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct SkillsWriteRequest {
    pub file_path: String,
    pub scope: String,
    pub content: String,
}

/// Read SKILL.md content for in-app editing.
#[tauri::command]
pub async fn skills_read(
    state: State<'_, Arc<AppState>>,
    request: SkillsReadRequest,
) -> AppResult<String> {
    let path = std::path::PathBuf::from(&request.file_path);
    let vault = state.vault_path()?;
    crate::ai_runtime::skills::validate_skill_path(&path, &vault)?;
    crate::ai_runtime::skills::read_skill_content(&path)
}

/// Write SKILL.md content after editing.
#[tauri::command]
pub async fn skills_write(
    state: State<'_, Arc<AppState>>,
    request: SkillsWriteRequest,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skills::{write_skill_content, SkillScope};
    let path = std::path::PathBuf::from(&request.file_path);
    let vault = state.vault_path()?;
    crate::ai_runtime::skills::validate_skill_path(&path, &vault)?;
    let scope = if request.scope == "vault" {
        SkillScope::Vault
    } else {
        SkillScope::Global
    };
    let entry = write_skill_content(&path, scope, &request.content)?;
    Ok(serde_json::to_value(entry).unwrap_or_default())
}

/// Migrate a legacy trigger-based skill to new format.
/// Creates a .bak backup before overwriting.
#[tauri::command]
pub async fn skills_migrate_legacy(
    state: State<'_, Arc<AppState>>,
    file_path: String,
    scope: String,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skills::{migrate_legacy_skill, SkillScope};
    let path = std::path::PathBuf::from(&file_path);
    let vault = state.vault_path()?;
    crate::ai_runtime::skills::validate_skill_path(&path, &vault)?;
    let sc = if scope == "vault" {
        SkillScope::Vault
    } else {
        SkillScope::Global
    };
    let entry = migrate_legacy_skill(&path, sc)?;
    Ok(serde_json::to_value(entry).unwrap_or_default())
}

/// Query tool audit entries by request_id.
#[tauri::command]
pub async fn tool_audit_query(
    state: State<'_, Arc<AppState>>,
    request_id: String,
) -> AppResult<Vec<crate::ai_runtime::tool_audit::ToolAuditEntry>> {
    crate::ai_runtime::tool_audit::query_by_request(&state.db, &request_id)
}

#[cfg(test)]
mod tests {
    use super::validate_ai_note_path;

    #[test]
    fn ai_note_path_rejects_classified_notes() {
        let err = validate_ai_note_path(Some(".classified/secret.md")).unwrap_err();
        assert!(err.to_string().contains("涉密笔记不能进入 AI"));
    }

    #[test]
    fn ai_note_path_allows_user_notes_and_empty_context() {
        assert!(validate_ai_note_path(Some("notes/open.md")).is_ok());
        assert!(validate_ai_note_path(None).is_ok());
    }

    mod image_attachment_dto {
        use crate::ai_types::ContentPart;
        use crate::commands::ai_commands::ImageAttachmentDto;

        fn test_image() -> ImageAttachmentDto {
            ImageAttachmentDto {
                id: "img-001".into(),
                data_base64: "iVBORw0KGgo=".into(),
                mime_type: "image/png".into(),
                file_name: Some("screenshot.png".into()),
                size_bytes: 42_000,
            }
        }

        #[test]
        fn data_url_includes_mime_and_base64() {
            let img = test_image();
            let url = img.data_url();
            assert_eq!(url, "data:image/png;base64,iVBORw0KGgo=");
        }

        #[test]
        fn to_content_part_produces_image_url() {
            let img = test_image();
            let part = img.to_content_part();
            match part {
                ContentPart::ImageUrl { image_url } => {
                    assert_eq!(image_url.url, "data:image/png;base64,iVBORw0KGgo=");
                    assert_eq!(image_url.detail.as_deref(), Some("auto"));
                }
                _ => panic!("expected ImageUrl"),
            }
        }

        #[test]
        fn serialization_round_trip() {
            let img = test_image();
            let json = serde_json::to_string(&img).unwrap();
            assert!(json.contains("img-001"));
            assert!(json.contains("screenshot.png"));
            assert!(json.contains("dataBase64")); // camelCase

            let restored: ImageAttachmentDto = serde_json::from_str(&json).unwrap();
            assert_eq!(restored.id, "img-001");
            assert_eq!(restored.data_base64, "iVBORw0KGgo=");
            assert_eq!(restored.mime_type, "image/png");
            assert_eq!(restored.size_bytes, 42_000);
        }

        #[test]
        fn serialized_json_has_snake_case_fields_for_rust_side() {
            // Verify camelCase for frontend IPC, but internal Rust can use snake_case
            let img = test_image();
            let json_str = serde_json::to_string(&img).unwrap();
            // The #[serde(rename_all = "camelCase")] should produce camelCase keys
            assert!(json_str.contains("\"dataBase64\""));
            assert!(json_str.contains("\"mimeType\""));
            assert!(json_str.contains("\"fileName\""));
            assert!(json_str.contains("\"sizeBytes\""));
        }
    }
}
