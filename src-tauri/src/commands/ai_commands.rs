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
    session_evidence::{
        enrich_session_evidence_details, list_session_evidence,
        register_packets_from_context_packets, register_session_evidence,
        SessionEvidenceDetailItem, SessionEvidenceItem, SessionEvidenceRegisterPacket,
    },
    tool_executor::ToolRegistry,
    trace::{TraceRecorder, TraceStatus},
    AgentIntent, AiScene, AssembledContext, ContextPacket, TokenUsage, ToolAccessLevel,
};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

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

fn prompt_profile_fingerprint(
    profile: &crate::ai_runtime::prompt_profile::PromptProfile,
) -> String {
    let raw = serde_json::to_string(profile).unwrap_or_default();
    let digest = Sha256::digest(raw.as_bytes());
    hex::encode(&digest[..12])
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
    _note_path: Option<&str>,
    has_attachments: bool,
) -> AgentIntent {
    if has_attachments {
        return AgentIntent::VisionChat;
    }

    let text = message.to_lowercase();
    let contains_any = |needles: &[&str]| needles.iter().any(|needle| text.contains(needle));
    if contains_any(&["技能", "安装 skill", "install skill"]) {
        AgentIntent::SkillManagement
    } else if contains_any(&[
        "联网调研",
        "联网研究",
        "研究综述",
        "文献综述",
        "多来源",
        "对比来源",
        "证据矩阵",
        "研究一下",
        "调研一下",
        "深挖一下",
    ]) {
        AgentIntent::Research
    } else if contains_any(&["引用", "引证", "出处", "核查", "citation"]) {
        AgentIntent::CitationCheck
    } else if contains_any(&["全文检查", "文档检查", "大纲检查", "风格一致"]) {
        AgentIntent::DocumentCheck
    } else if contains_any(&["章节", "这一章", "本章", "chapter"]) {
        AgentIntent::Chapter
    } else if contains_any(&["整理", "归档", "标签", "分类", "知识库"]) {
        AgentIntent::Organize
    } else if contains_any(&["改写", "重写", "润色", "续写", "扩写", "写一段"]) {
        AgentIntent::Write
    } else if contains_any(&[
        "查一下",
        "查阅",
        "搜索",
        "找一下",
        "库里",
        "笔记里",
        "当前笔记",
        "本文",
    ]) {
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

#[derive(Debug, Clone, Copy, Serialize)]
struct ContextBudgetDiagnostics {
    input_budget: usize,
    estimated_total: usize,
    history_tokens: usize,
    evidence_tokens: usize,
    tool_tokens: usize,
    environment_tokens: usize,
}

fn cold_start_evidence_budget(input_budget: usize) -> usize {
    (input_budget / 3).clamp(1, 18_000)
}

fn compact_cold_start_packets_for_budget(
    packets: &mut Vec<ContextPacket>,
    input_budget: usize,
) -> ContextBudgetDiagnostics {
    let evidence_budget = cold_start_evidence_budget(input_budget);
    crate::ai_runtime::harness_support::compact_evidence(packets, evidence_budget);
    let evidence_tokens = packets
        .iter()
        .map(|packet| crate::ai_runtime::harness_support::estimate_tokens(&packet.excerpt))
        .sum();
    ContextBudgetDiagnostics {
        input_budget,
        estimated_total: evidence_tokens,
        history_tokens: 0,
        evidence_tokens,
        tool_tokens: 0,
        environment_tokens: 0,
    }
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
        current_session_id: Some(plan.session_id),
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
    let profile =
        crate::ai_runtime::prompt_profile::PromptProfile::load(&state.db).unwrap_or_default();
    let profile_fingerprint = prompt_profile_fingerprint(&profile);
    let cache_key = ContextAssemblyCacheKey::new(
        scene,
        note_path,
        query,
        &scope_json,
        &format!("{:?}", build_opts.strategy),
        build_opts.input_budget as u32,
        &profile_fingerprint,
    );
    if let Some(cached) = crate::llm::safe_lock(&state.ai.context_cache).get(&cache_key) {
        return Ok(cached);
    }

    let built = state.db.with_conn(|conn| {
        build_context_packets(
            conn, vault, scene, note_path, file_id, query, user_scope, build_opts,
        )
    })?;

    crate::llm::safe_lock(&state.ai.context_cache).insert(
        cache_key,
        built.0.clone(),
        built.1.clone(),
    );
    Ok(built)
}

const WEB_PREFETCH_TIMEOUT: Duration = Duration::from_secs(10);
const WEB_PREFETCH_MAX_SEARCH_RESULTS: usize = 8;
const WEB_PREFETCH_MAX_FETCHES: usize = 3;

fn should_prefetch_web(message: &str) -> bool {
    let lower = message.to_lowercase();
    if lower.contains("http://") || lower.contains("https://") {
        return true;
    }

    let strong_signals = [
        "联网", "搜索", "网页", "最新", "近期", "新闻", "时事", "天气", "web", "search", "online",
        "latest", "recent", "news", "weather", "2025", "2026",
    ];
    strong_signals.iter().any(|signal| lower.contains(signal))
}

fn web_prefetch_allowed(
    task_policy: &AgentTaskPolicy,
    scene: AiScene,
    web_search_enabled: bool,
) -> bool {
    if !web_search_enabled {
        return false;
    }
    let policy_ctx = crate::ai_runtime::tool_policy::ToolPolicyContext {
        task_policy: Some(task_policy.clone()),
        scene,
        autonomy_level: task_policy.autonomy_level,
        web_search_enabled,
        depth: 0,
    };
    matches!(
        crate::ai_runtime::tool_policy::evaluate_tool("web_search", &policy_ctx),
        crate::ai_runtime::tool_policy::ToolPolicyVerdict::AutoAllowed
    )
}

fn extract_prefetch_https_urls(message: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for token in message.split_whitespace() {
        let trimmed = token.trim_matches(|ch: char| {
            ch.is_ascii_punctuation()
                || matches!(
                    ch,
                    '，' | '。' | '；' | '：' | '？' | '！' | '、' | '）' | '（'
                )
        });
        if trimmed.to_ascii_lowercase().starts_with("https://")
            && !urls.iter().any(|existing| existing == trimmed)
        {
            urls.push(trimmed.to_string());
        }
        if urls.len() >= WEB_PREFETCH_MAX_FETCHES {
            break;
        }
    }
    urls
}

fn should_start_web_prefetch(
    message: &str,
    selected_packet_ids: Option<&[String]>,
    task_policy: &AgentTaskPolicy,
    scene: AiScene,
    web_search_enabled: bool,
) -> bool {
    let has_selected_packets = selected_packet_ids.is_some_and(|ids| !ids.is_empty());
    !has_selected_packets
        && should_prefetch_web(message)
        && web_prefetch_allowed(task_policy, scene, web_search_enabled)
}

async fn prefetch_web_context_packets(
    state: &Arc<AppState>,
    message: &str,
    task_policy: &AgentTaskPolicy,
) -> Vec<crate::ai_runtime::ContextPacket> {
    let max_fetches = (task_policy.max_fetch_per_round as usize).min(WEB_PREFETCH_MAX_FETCHES);
    let input = crate::ai_runtime::web_evidence_broker::WebEvidenceBrokerInput {
        query: message.to_string(),
        urls: extract_prefetch_https_urls(message),
        enabled: true,
        max_search_results: WEB_PREFETCH_MAX_SEARCH_RESULTS,
        max_fetches,
    };
    match tokio::time::timeout(
        WEB_PREFETCH_TIMEOUT,
        crate::ai_runtime::web_evidence_broker::collect_web_evidence_with_usage(&state.db, input),
    )
    .await
    {
        Ok(Ok(output)) => crate::ai_runtime::web_evidence_broker::web_evidence_items_to_packets(
            message,
            &output.items,
        ),
        Ok(Err(error)) => {
            tracing::debug!("cold web prefetch failed: {error}");
            Vec::new()
        }
        Err(_) => {
            tracing::debug!("cold web prefetch timed out");
            Vec::new()
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn build_context_packets_with_optional_web_prefetch(
    state: &Arc<AppState>,
    vault: &Path,
    scene: AiScene,
    note_path: Option<&str>,
    file_id: Option<i64>,
    query: &str,
    user_scope: &ContextScopeDto,
    build_opts: ContextBuildOptions,
    task_policy: &AgentTaskPolicy,
    web_search_enabled: bool,
    selected_packet_ids: Option<&[String]>,
) -> AppResult<(
    Vec<crate::ai_runtime::ContextPacket>,
    crate::ai_runtime::ContextStatus,
)> {
    if !should_start_web_prefetch(
        query,
        selected_packet_ids,
        task_policy,
        scene,
        web_search_enabled,
    ) {
        return build_context_packets_cached(
            state, vault, scene, note_path, file_id, query, user_scope, build_opts,
        );
    }

    let state_for_local = Arc::clone(state);
    let vault_for_local = vault.to_path_buf();
    let note_path_for_local = note_path.map(str::to_string);
    let query_for_local = query.to_string();
    let user_scope_for_local = user_scope.clone();
    let local_context = tokio::task::spawn_blocking(move || {
        build_context_packets_cached(
            &state_for_local,
            &vault_for_local,
            scene,
            note_path_for_local.as_deref(),
            file_id,
            &query_for_local,
            &user_scope_for_local,
            build_opts,
        )
    });
    let web_packets = prefetch_web_context_packets(state, query, task_policy);
    let (local_context, web_packets) = tokio::join!(local_context, web_packets);
    let (mut packets, status) = local_context
        .map_err(|err| AppError::msg(format!("context build task failed: {err}")))??;
    packets.extend(web_packets);
    Ok((packets, status))
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
    let policy_ctx = crate::ai_runtime::tool_policy::ToolPolicyContext {
        task_policy: Some(task_policy.clone()),
        scene,
        autonomy_level: task_policy.autonomy_level,
        web_search_enabled,
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
#[serde(rename_all = "camelCase")]
pub struct ToolExecutionOutcomeWire {
    pub status: String,
    pub side_effect_committed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantResumeOutcomeWire {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_message: Option<String>,
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
    pub deliberation_state: Option<crate::ai_runtime::deliberation::DeliberationState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_summary: Option<crate::ai_runtime::deliberation::VerificationSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_refresh_notice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resumed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_confirmation_partial: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_error_message: Option<String>,
    #[serde(
        rename = "toolExecutionOutcome",
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_execution_outcome: Option<ToolExecutionOutcomeWire>,
    #[serde(
        rename = "assistantResumeOutcome",
        skip_serializing_if = "Option::is_none"
    )]
    pub assistant_resume_outcome: Option<AssistantResumeOutcomeWire>,
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
            deliberation_state: harness_result.deliberation_state.clone(),
            verification_summary: harness_result.verification_summary.clone(),
            evidence_refresh_notice,
            tool_call_id: None,
            decision: None,
            resumed: None,
            tool_confirmation_partial: None,
            resume_error_code: None,
            resume_error_message: None,
            tool_execution_outcome: None,
            assistant_resume_outcome: None,
        }
    }

    fn with_tool_confirmation(mut self, tool_call_id: String, decision: impl Into<String>) -> Self {
        self.tool_call_id = Some(tool_call_id);
        self.decision = Some(decision.into());
        let decision = self.decision.as_deref().unwrap_or_default();
        self.resumed = Some(true);
        self.tool_execution_outcome = Some(ToolExecutionOutcomeWire {
            status: if decision == "reject" {
                "rejected".into()
            } else {
                "succeeded".into()
            },
            side_effect_committed: decision != "reject",
            tool_name: None,
            result_summary: None,
        });
        self.assistant_resume_outcome = Some(AssistantResumeOutcomeWire {
            status: "resumed".into(),
            failure_class: None,
            user_message: None,
        });
        self
    }

    fn with_partial_tool_confirmation(
        mut self,
        tool_call_id: String,
        decision: impl Into<String>,
        resume_error_code: String,
        resume_error_message: String,
    ) -> Self {
        self.tool_call_id = Some(tool_call_id);
        self.decision = Some(decision.into());
        self.resumed = Some(false);
        self.tool_confirmation_partial = Some(true);
        self.resume_error_code = Some(resume_error_code.clone());
        self.resume_error_message = Some(resume_error_message);
        self.tool_execution_outcome = Some(ToolExecutionOutcomeWire {
            status: "succeeded".into(),
            side_effect_committed: true,
            tool_name: Some("confirmed_tool".into()),
            result_summary: None,
        });
        self.assistant_resume_outcome = Some(AssistantResumeOutcomeWire {
            status: "failed".into(),
            failure_class: Some(resume_error_code),
            user_message: Some("工具已执行，但继续生成回复失败。".into()),
        });
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
    state: &Arc<AppState>,
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
    state: &Arc<AppState>,
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
    app_handle
        .emit(
            "ai:request_started",
            &serde_json::json!({
                "request_id": request_id.clone(),
                "session_id": sid,
                "task_id": task_id.clone(),
                "intent": task_policy.intent,
                "scene": scene.profile(),
                "domain": "normal",
            }),
        )
        .map_err(|e| AppError::msg(format!("emit request_started: {e}")))?;

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
    let (packets, _context_status) = match build_context_packets_with_optional_web_prefetch(
        state,
        &vault,
        scene,
        note_path.as_deref(),
        file_id,
        &message,
        &user_scope,
        build_opts,
        &task_policy,
        web_search,
        selected_packet_ids.as_deref(),
    )
    .await
    {
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
    let mut filtered_packets: Vec<_> = if resolved_ids.is_empty() {
        ledger.packets().to_vec()
    } else {
        ledger
            .packets()
            .iter()
            .filter(|p| resolved_ids.contains(&p.id))
            .cloned()
            .collect()
    };
    let mut context_budget_diagnostics =
        compact_cold_start_packets_for_budget(&mut filtered_packets, resolved.input_budget);
    context_budget_diagnostics.history_tokens = history
        .iter()
        .map(|message| crate::ai_runtime::harness_support::estimate_tokens(&message.content))
        .sum();
    context_budget_diagnostics.estimated_total = context_budget_diagnostics
        .history_tokens
        .saturating_add(context_budget_diagnostics.evidence_tokens)
        .saturating_add(context_budget_diagnostics.tool_tokens)
        .saturating_add(context_budget_diagnostics.environment_tokens);
    AgentTaskRuntime::record_event(
        &state.db,
        &task_id,
        "context_budget",
        "assembled",
        serde_json::to_value(context_budget_diagnostics).unwrap_or_default(),
    )?;

    if !filtered_packets.is_empty() {
        let register_packets = register_packets_from_context_packets(&filtered_packets);
        let evidence_message_seq = state.db.with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM session_messages WHERE session_id = ?1",
                [sid],
                |row| row.get::<_, i64>(0),
            )?)
        })?;
        let registered = state.db.with_conn(|conn| {
            register_session_evidence(conn, sid, evidence_message_seq, &register_packets)
        })?;
        for (packet, evidence) in filtered_packets.iter_mut().zip(registered.iter()) {
            packet.citation_label = evidence.citation_label.clone();
        }
    }

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
            input_budget: Some(resolved.input_budget.min(u32::MAX as usize) as u32),
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
    let budget_finish = matches!(
        harness_result.finish_reason,
        crate::ai_runtime::harness::HarnessFinishReason::BudgetExhausted
            | crate::ai_runtime::harness::HarnessFinishReason::RoundLimit
    );
    let should_pause_for_budget = !matches!(task_policy.intent, AgentIntent::Chat) && budget_finish;
    let task_status = if harness_result.pending_confirmation {
        AgentTaskStatus::AwaitingConfirmation
    } else if should_pause_for_budget {
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
        state.inner(),
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
    let evidence_packets_value: Option<serde_json::Value> =
        if harness_result.evidence_packets.is_empty() {
            None
        } else {
            Some(serde_json::to_value(&harness_result.evidence_packets).unwrap_or_default())
        };
    let assistant_message_row_id = SessionManager::append_message_with_evidence_packets(
        &state.db,
        session_id,
        "assistant",
        &harness_result.content,
        None,
        tool_calls_value.as_ref(),
        evidence_packets_value.as_ref(),
    )?;

    if !harness_result.evidence_packets.is_empty() {
        let assistant_message_seq = state.db.with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT seq FROM session_messages WHERE id = ?1 AND session_id = ?2",
                rusqlite::params![assistant_message_row_id, session_id],
                |row| row.get::<_, i64>(0),
            )?)
        })?;
        let register_packets =
            register_packets_from_context_packets(&harness_result.evidence_packets);
        state.db.with_conn(|conn| {
            register_session_evidence(conn, session_id, assistant_message_seq, &register_packets)
        })?;
    }

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
    if let Some(summary) = &harness_result.verification_summary {
        if !summary.passed {
            let failed_items = summary
                .items
                .iter()
                .filter(|item| {
                    item.status == crate::ai_runtime::deliberation::VerificationStatus::Failed
                })
                .map(|item| item.description.clone())
                .collect::<Vec<_>>();
            let _ = AgentTaskRuntime::record_event(
                &state.db,
                &task_id,
                "verification_attention",
                "部分验证项未通过，已在回答中提示",
                serde_json::json!({ "failed_items": failed_items }),
            );
        }
    }
    AgentTaskRuntime::complete_task(&state.db, &task_id)
}

fn parse_confirmed_tool_args(
    pending_arguments: &str,
    modified_args: Option<serde_json::Value>,
) -> AppResult<serde_json::Value> {
    if let Some(args) = modified_args {
        return if args.is_object() {
            Ok(args)
        } else {
            Err(AppError::msg(
                "tool_arguments_parse_error: tool arguments must be a valid JSON object",
            ))
        };
    }
    crate::ai_runtime::tool_fallback::parse_tool_call_arguments(pending_arguments)
        .map_err(|err| AppError::msg(format!("tool_arguments_parse_error: {err}")))
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
            harness_run_to_chat_response(state.inner(), &harness_result)?
                .with_tool_confirmation(tool_call_id, "reject"),
        );
    }

    let pending = crate::llm::safe_lock(&state.ai.pending_tool_calls).remove(&tool_call_id);
    let Some(pending) = pending else {
        return Err(AppError::msg(format!(
            "no pending tool call for id: {tool_call_id}"
        )));
    };

    let args = parse_confirmed_tool_args(&pending.arguments, modified_args)?;

    dispatch_approved_tool_to_checkpoint(
        state.inner(),
        &app_handle,
        &pending,
        &tool_call_id,
        &args,
    )
    .await?;

    let harness_result =
        match resume_harness_after_tool_confirm_or_restore(state.inner(), &app_handle, &request_id)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                return partial_tool_confirm_response(
                    state.inner(),
                    &request_id,
                    tool_call_id,
                    if decision == "modify" {
                        "modify"
                    } else {
                        "approve"
                    },
                    &error,
                );
            }
        };

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
    );

    // Migration-period event; frontend should use resumed harness payload.
    let _ = app_handle.emit("ai:tool_result", &out);

    Ok(out)
}

fn partial_tool_confirm_response(
    state: &AppState,
    request_id: &str,
    tool_call_id: String,
    decision: &str,
    error: &AppError,
) -> AppResult<AiChatResponse> {
    use crate::ai_runtime::harness_support::load_harness_checkpoint;

    let cp = load_harness_checkpoint(&state.db, request_id)?
        .ok_or_else(|| AppError::msg("checkpoint missing"))?;
    let error_message = error.to_string();
    let error_code = crate::ai_runtime::harness_confirm::classify_resume_error(&error_message);
    let content = format!("工具已执行，但继续生成回复失败：{error_code}");
    Ok(AiChatResponse {
        request_id: request_id.to_string(),
        task_id: None,
        session_id: cp.meta.session_id,
        status: "tool_executed_resume_failed".into(),
        content,
        tool_calls: Vec::new(),
        tool_results: cp.tool_results.clone(),
        usage: TokenUsage::default(),
        usage_source: crate::ai_runtime::harness::UsageSource::Estimated,
        citation_valid: false,
        harness_rounds: 0,
        evidence_packets: cp.evidence_packets.clone(),
        pending_confirmation: false,
        deliberation_state: None,
        verification_summary: None,
        evidence_refresh_notice: None,
        tool_call_id: None,
        decision: None,
        resumed: None,
        tool_confirmation_partial: None,
        resume_error_code: None,
        resume_error_message: None,
        tool_execution_outcome: None,
        assistant_resume_outcome: None,
    })
    .map(|response| {
        response.with_partial_tool_confirmation(
            tool_call_id,
            decision.to_string(),
            error_code.to_string(),
            error_message,
        )
    })
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
pub fn ai_list_tools(
    _state: State<'_, Arc<AppState>>,
    scene: String,
) -> AppResult<Vec<AiToolInfo>> {
    let scene = parse_ai_scene(&scene)?;
    let registry = ToolRegistry::new();
    let task_policy = legacy_task_policy_from_scene(scene, None, true, false);
    let ctx = crate::ai_runtime::tool_policy::ToolPolicyContext {
        task_policy: Some(task_policy.clone()),
        scene,
        autonomy_level: task_policy.autonomy_level,
        web_search_enabled: true,
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
    crate::llm::safe_lock(&state.ai.context_cache).clear();

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

/// List active evidence ledger rows for a session.
#[tauri::command]
pub async fn session_evidence_list(
    state: State<'_, Arc<AppState>>,
    session_id: i64,
) -> AppResult<Vec<SessionEvidenceItem>> {
    state
        .db
        .with_conn(|conn| list_session_evidence(conn, session_id))
}

/// Return evidence metadata for the read-only detail view.
#[tauri::command]
pub async fn session_evidence_detail(
    state: State<'_, Arc<AppState>>,
    session_id: i64,
) -> AppResult<Vec<SessionEvidenceDetailItem>> {
    let vault = state.vault_path()?;
    let items = state
        .db
        .with_conn(|conn| list_session_evidence(conn, session_id))?;
    Ok(enrich_session_evidence_details(items, &vault)
        .into_iter()
        .map(SessionEvidenceDetailItem::from)
        .collect())
}

/// Register evidence metadata against a session and return stable citation labels.
#[tauri::command]
pub async fn session_evidence_register(
    state: State<'_, Arc<AppState>>,
    session_id: i64,
    message_seq: i64,
    packets: Vec<SessionEvidenceRegisterPacket>,
) -> AppResult<Vec<SessionEvidenceItem>> {
    state
        .db
        .with_conn(|conn| register_session_evidence(conn, session_id, message_seq, &packets))
}

/// List installed skills (global + vault) with validation status.
#[tauri::command]
pub async fn skills_list(
    state: State<'_, Arc<AppState>>,
    scene: Option<String>,
) -> AppResult<Vec<crate::ai_runtime::skills::SkillListEntry>> {
    let vault = state.vault_path()?;
    let scene = scene.as_deref().map(parse_ai_scene).transpose()?;
    crate::ai_runtime::skills::list_skills(&state.db, &vault, scene)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebEvidenceProviderInput {
    pub id: String,
    pub name: String,
    pub provider_kind: String,
    pub enabled: bool,
    pub transport_kind: Option<String>,
    #[serde(default = "default_provider_config_json")]
    pub transport_config_json: String,
    #[serde(default = "default_provider_config_json")]
    pub credential_refs_json: String,
    pub search_mapping: Option<String>,
    pub fetch_mapping: Option<String>,
}

fn default_provider_config_json() -> String {
    "{}".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebEvidenceProviderSummary {
    pub id: String,
    pub name: String,
    pub provider_kind: String,
    pub enabled: bool,
    pub transport_kind: String,
    pub transport_config_json: String,
    pub credential_refs_json: String,
    pub search_mapping: Option<String>,
    pub fetch_mapping: Option<String>,
    pub mapping_status: String,
    pub diagnostic_status: String,
    pub is_native: bool,
    pub editable: bool,
    pub has_search_mapping: bool,
    pub has_fetch_mapping: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebEvidenceProviderDiagnosticCheck {
    pub label: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebEvidenceProviderDiagnostics {
    pub provider_id: Option<String>,
    pub status: String,
    pub failures: Vec<String>,
    pub checks: Vec<WebEvidenceProviderDiagnosticCheck>,
    pub can_use_for_search: bool,
    pub can_use_for_fetch: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDraftScopeRuleDto {
    pub kind: String,
    pub pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCreateDraftRequest {
    pub name: String,
    pub description: Option<String>,
    pub body: Option<String>,
    pub scope_rules: Vec<SkillDraftScopeRuleDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDraftDto {
    pub name: String,
    pub markdown: String,
    pub scope_rules: Vec<SkillDraftScopeRuleDto>,
    pub content_hash: String,
    pub target_path: String,
}

#[tauri::command]
pub async fn web_evidence_provider_upsert(
    state: State<'_, Arc<AppState>>,
    input: WebEvidenceProviderInput,
) -> AppResult<()> {
    crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
        &state.db,
        &provider_input_to_registry(input)?,
    )
}

#[tauri::command]
pub async fn web_evidence_providers_list(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<WebEvidenceProviderSummary>> {
    crate::ai_runtime::mcp_runtime_registry::list_web_evidence_providers(&state.db).map(|items| {
        items
            .into_iter()
            .map(|item| {
                let mapping_status =
                    provider_mapping_status(item.has_search_mapping, item.has_fetch_mapping);
                WebEvidenceProviderSummary {
                    id: item.id,
                    name: item.name,
                    provider_kind: item.kind.clone(),
                    enabled: item.enabled,
                    transport_kind: item.transport_kind,
                    transport_config_json: item.transport_config_json,
                    credential_refs_json: item.credential_refs_json,
                    search_mapping: item.web_search_mapping_json,
                    fetch_mapping: item.web_fetch_mapping_json,
                    diagnostic_status: provider_diagnostic_status(item.enabled, &mapping_status),
                    mapping_status,
                    is_native: item.kind == "native",
                    editable: item.kind == "mcp",
                    has_search_mapping: item.has_search_mapping,
                    has_fetch_mapping: item.has_fetch_mapping,
                }
            })
            .collect()
    })
}

#[tauri::command]
pub async fn web_evidence_provider_toggle(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
    enabled: bool,
) -> AppResult<()> {
    crate::ai_runtime::mcp_runtime_registry::toggle_web_evidence_provider(
        &state.db,
        &provider_id,
        enabled,
    )
}

#[tauri::command]
pub async fn web_evidence_provider_delete(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
) -> AppResult<()> {
    crate::ai_runtime::mcp_runtime_registry::delete_web_evidence_provider(&state.db, &provider_id)
}

#[tauri::command]
pub async fn web_evidence_provider_diagnostics(
    state: State<'_, Arc<AppState>>,
    provider_id: Option<String>,
    live_check: Option<bool>,
) -> AppResult<WebEvidenceProviderDiagnostics> {
    let live_check = live_check.unwrap_or(false);
    let providers =
        crate::ai_runtime::mcp_runtime_registry::list_web_evidence_providers(&state.db)?;

    if let Some(provider_id) = provider_id.as_deref() {
        if let Some(provider) = providers.iter().find(|item| item.id == provider_id) {
            return provider_diagnostics_for_summary(&state.db, provider, live_check).await;
        }
        return Ok(WebEvidenceProviderDiagnostics {
            provider_id: Some(provider_id.to_string()),
            status: "missing".into(),
            failures: vec!["未找到提供方记录".into()],
            checks: vec![provider_diagnostic_check(
                "configured",
                false,
                "提供方记录缺失",
            )],
            can_use_for_search: false,
            can_use_for_fetch: false,
        });
    }

    let can_use_for_search = providers
        .iter()
        .any(|item| item.enabled && item.has_search_mapping);
    let can_use_for_fetch = providers
        .iter()
        .any(|item| item.enabled && item.has_fetch_mapping);
    let status = if providers.is_empty() {
        "not_configured"
    } else if can_use_for_search || can_use_for_fetch {
        "configured"
    } else {
        "needs_mapping"
    };
    let checks = if providers.is_empty() {
        vec![provider_diagnostic_check(
            "registry",
            false,
            "尚未配置 MCP 联网证据提供方",
        )]
    } else {
        vec![provider_diagnostic_check(
            "registry",
            true,
            "MCP 联网证据提供方注册表可用",
        )]
    };
    let failures = checks
        .iter()
        .filter(|check| check.status != "pass")
        .map(|check| check.message.clone())
        .collect();
    Ok(WebEvidenceProviderDiagnostics {
        provider_id: None,
        status: status.into(),
        failures,
        checks,
        can_use_for_search,
        can_use_for_fetch,
    })
}

fn provider_mapping_status(has_search_mapping: bool, has_fetch_mapping: bool) -> String {
    match (has_search_mapping, has_fetch_mapping) {
        (true, true) => "complete".into(),
        (true, false) | (false, true) => "partial".into(),
        (false, false) => "missing".into(),
    }
}

fn provider_diagnostic_status(enabled: bool, mapping_status: &str) -> String {
    if !enabled {
        "disabled".into()
    } else if mapping_status == "complete" {
        "ready".into()
    } else {
        "needs_mapping".into()
    }
}

fn provider_diagnostic_check(
    label: &str,
    passed: bool,
    message: &str,
) -> WebEvidenceProviderDiagnosticCheck {
    WebEvidenceProviderDiagnosticCheck {
        label: label.into(),
        status: if passed { "pass" } else { "fail" }.into(),
        message: message.into(),
    }
}

fn provider_credential_service_from_binding(value: &serde_json::Value) -> Option<String> {
    let raw = if let Some(raw) = value.as_str() {
        raw.trim()
    } else if let Some(object) = value.as_object() {
        object
            .get("credential")
            .or_else(|| object.get("service"))
            .or_else(|| object.get("ref"))
            .and_then(|item| item.as_str())
            .map(str::trim)
            .unwrap_or_default()
    } else {
        ""
    };
    let service = raw.strip_prefix("credential://").unwrap_or(raw).trim();
    (!service.is_empty()).then(|| service.to_string())
}

fn provider_credential_binding_optional(value: &serde_json::Value, service: &str) -> bool {
    value
        .as_object()
        .and_then(|object| object.get("optional"))
        .and_then(|item| item.as_bool())
        .unwrap_or(matches!(service, "iris.mcp.anysearch" | "iris.mcp.jina"))
}

fn provider_credential_bindings(
    credential_refs_json: &str,
) -> AppResult<Vec<(String, serde_json::Value)>> {
    let value: serde_json::Value = serde_json::from_str(credential_refs_json)
        .map_err(|err| AppError::msg(format!("invalid credential refs JSON: {err}")))?;
    let mut bindings = Vec::new();
    if let Some(headers) = value.get("headers").and_then(|item| item.as_object()) {
        bindings.extend(
            headers
                .iter()
                .map(|(name, binding)| (format!("请求头 {name}"), binding.clone())),
        );
    }
    if let Some(env) = value.get("env").and_then(|item| item.as_object()) {
        bindings.extend(
            env.iter()
                .map(|(name, binding)| (format!("环境变量 {name}"), binding.clone())),
        );
    }
    Ok(bindings)
}

fn provider_credential_diagnostic_checks(
    db: &crate::storage::db::Database,
    provider: &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderSummary,
) -> AppResult<(Vec<WebEvidenceProviderDiagnosticCheck>, bool)> {
    let bindings = provider_credential_bindings(&provider.credential_refs_json)?;
    if bindings.is_empty() {
        return Ok((
            vec![provider_diagnostic_check("credential", true, "不需要凭据")],
            true,
        ));
    }

    let mut checks = Vec::new();
    let mut all_required_credentials_available = true;
    for (label, binding) in bindings {
        let Some(service) = provider_credential_service_from_binding(&binding) else {
            checks.push(provider_diagnostic_check(
                "credential",
                false,
                &format!("{label} 缺少凭据引用"),
            ));
            all_required_credentials_available = false;
            continue;
        };
        let optional = provider_credential_binding_optional(&binding, &service);
        let configured = crate::credentials::api_key_configured(db, &service)?;
        if configured {
            checks.push(provider_diagnostic_check(
                "credential",
                true,
                &format!("{label} Key 已绑定：{service}"),
            ));
        } else if optional {
            checks.push(provider_diagnostic_check(
                "credential",
                true,
                &format!("{label} 可选凭据未绑定，使用匿名模式：{service}"),
            ));
        } else {
            checks.push(provider_diagnostic_check(
                "credential",
                false,
                &format!("{label} 必填凭据缺失：{service}"),
            ));
            all_required_credentials_available = false;
        }
    }
    Ok((checks, all_required_credentials_available))
}

fn provider_mapping_tool_name(mapping_json: Option<&str>) -> Option<String> {
    let value = mapping_json?.trim();
    if value.is_empty() {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(value)
        .ok()
        .and_then(|parsed| {
            parsed
                .get("tool")
                .or_else(|| parsed.get("tool_name"))
                .and_then(|tool| tool.as_str())
                .map(str::trim)
                .filter(|tool| !tool.is_empty())
                .map(str::to_string)
        })
        .or_else(|| Some(value.to_string()))
}

fn diagnostic_error_message(error: &AppError) -> String {
    let redacted = crate::ai_runtime::trace::redact_classified_leaks(&error.to_string());
    const MAX_LEN: usize = 240;
    if redacted.chars().count() > MAX_LEN {
        format!("{}...", redacted.chars().take(MAX_LEN).collect::<String>())
    } else {
        redacted
    }
}

fn provider_transport_url(
    provider: &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderSummary,
) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(&provider.transport_config_json)
        .ok()
        .and_then(|value| {
            value
                .get("url")
                .and_then(|url| url.as_str())
                .map(str::to_string)
        })
}

fn provider_search_smoke_error_message(
    provider: &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderSummary,
    error: &AppError,
) -> String {
    let raw = error.to_string();
    let lower = raw.to_ascii_lowercase();
    let url = provider_transport_url(provider).unwrap_or_default();
    if lower.contains("auth_failed") && url.contains("mcp.tavily.com") {
        return "MCP 服务要求 OAuth 鉴权流程，当前预设不兼容".into();
    }
    diagnostic_error_message(error)
}

async fn run_mcp_search_smoke_test(
    db: &crate::storage::db::Database,
    provider: &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderSummary,
) -> AppResult<(usize, String)> {
    let probe = crate::ai_runtime::web_evidence_broker::probe_mcp_search_provider(
        db,
        &provider.id,
        "Iris note app",
        1,
        Duration::from_secs(20),
    )
    .await?;
    Ok((probe.diagnostic.parsed_row_count, probe.summary()))
}

async fn provider_diagnostics_for_summary(
    db: &crate::storage::db::Database,
    provider: &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderSummary,
    live_check: bool,
) -> AppResult<WebEvidenceProviderDiagnostics> {
    let transport_ok = provider.transport_kind == "https" || provider.transport_kind == "stdio";
    let mut checks = vec![
        provider_diagnostic_check("configured", true, "提供方记录存在"),
        provider_diagnostic_check(
            "enabled",
            provider.enabled,
            if provider.enabled {
                "提供方已启用"
            } else {
                "提供方未启用"
            },
        ),
        provider_diagnostic_check(
            "transport",
            transport_ok,
            if transport_ok {
                "连接方式支持 MCP 联网证据"
            } else {
                "连接方式不支持 MCP 联网证据"
            },
        ),
        provider_diagnostic_check(
            "searchMapping",
            provider.has_search_mapping,
            if provider.has_search_mapping {
                "已配置搜索映射"
            } else {
                "未配置搜索映射"
            },
        ),
        provider_diagnostic_check(
            "fetchMapping",
            provider.has_fetch_mapping,
            if provider.has_fetch_mapping {
                "已配置网页读取映射"
            } else {
                "未配置网页读取映射"
            },
        ),
    ];
    if provider.kind != "mcp" {
        checks.push(provider_diagnostic_check(
            "providerKind",
            false,
            "只有 MCP 提供方可作为可编辑联网证据提供方",
        ));
    }

    let (credential_checks, credentials_ok) = provider_credential_diagnostic_checks(db, provider)?;
    checks.extend(credential_checks);

    let mut can_use_for_search = provider.enabled && provider.has_search_mapping && credentials_ok;
    let mut can_use_for_fetch = provider.enabled && provider.has_fetch_mapping && credentials_ok;

    if live_check && provider.kind == "mcp" && provider.enabled {
        let options = crate::ai_runtime::mcp_host_runtime::McpHostRuntimeOptions {
            request_timeout: Duration::from_secs(20),
            max_stdout_line_bytes: 64 * 1024,
            max_stderr_bytes: 8 * 1024,
            cwd: None,
        };
        match crate::ai_runtime::mcp_host_runtime::discover_provider_tools(
            db,
            &provider.id,
            options,
        )
        .await
        {
            Ok(discovery) => {
                let tool_names = discovery
                    .tools
                    .iter()
                    .map(|tool| tool.name.as_str())
                    .collect::<HashSet<_>>();
                checks.push(provider_diagnostic_check(
                    "liveConnection",
                    true,
                    "MCP 服务已响应 tools/list",
                ));
                if let Some(tool) =
                    provider_mapping_tool_name(provider.web_search_mapping_json.as_deref())
                {
                    let exists = tool_names.contains(tool.as_str());
                    can_use_for_search = can_use_for_search && exists;
                    checks.push(provider_diagnostic_check(
                        "searchToolLive",
                        exists,
                        &if exists {
                            format!("已找到搜索工具 '{tool}'")
                        } else {
                            format!("MCP 服务未报告搜索工具 '{tool}'")
                        },
                    ));
                    if exists && provider.web_search_mapping_json.is_some() {
                        match run_mcp_search_smoke_test(db, provider).await {
                            Ok((parsed_row_count, diagnostic)) => {
                                let parseable = parsed_row_count > 0;
                                let auth_header_reported =
                                    diagnostic.contains("auth header present");
                                checks.push(provider_diagnostic_check(
                                    "searchSmokeAuthHeader",
                                    auth_header_reported,
                                    &format!(
                                        "MCP search probe reported auth header present state: {auth_header_reported}"
                                    ),
                                ));
                                checks.push(provider_diagnostic_check(
                                    "searchSmokeLive",
                                    true,
                                    &format!(
                                        "搜索调用正常，解析出 {parsed_row_count} 条网页证据；{diagnostic}"
                                    ),
                                ));
                                can_use_for_search = can_use_for_search && parseable;
                                checks.push(provider_diagnostic_check(
                                    "searchResultParseLive",
                                    parseable,
                                    if parseable {
                                        "MCP 搜索结果已归一化为联网证据"
                                    } else {
                                        "MCP 搜索结果无法归一化为联网证据"
                                    },
                                ));
                            }
                            Err(error) => {
                                can_use_for_search = false;
                                checks.push(provider_diagnostic_check(
                                    "searchSmokeLive",
                                    false,
                                    &format!(
                                        "MCP 搜索 smoke test 失败：{}",
                                        provider_search_smoke_error_message(provider, &error)
                                    ),
                                ));
                            }
                        }
                    }
                }
                if let Some(tool) =
                    provider_mapping_tool_name(provider.web_fetch_mapping_json.as_deref())
                {
                    let exists = tool_names.contains(tool.as_str());
                    can_use_for_fetch = can_use_for_fetch && exists;
                    checks.push(provider_diagnostic_check(
                        "fetchToolLive",
                        exists,
                        &if exists {
                            format!("已找到网页读取工具 '{tool}'")
                        } else {
                            format!("MCP 服务未报告网页读取工具 '{tool}'")
                        },
                    ));
                }
            }
            Err(error) => {
                can_use_for_search = false;
                can_use_for_fetch = false;
                checks.push(provider_diagnostic_check(
                    "liveConnection",
                    false,
                    &format!(
                        "MCP 实时探测失败：{}",
                        provider_search_smoke_error_message(provider, &error)
                    ),
                ));
            }
        }
    }

    let failures = checks
        .iter()
        .filter(|check| check.status != "pass")
        .map(|check| check.message.clone())
        .collect::<Vec<_>>();
    let mapping_status =
        provider_mapping_status(provider.has_search_mapping, provider.has_fetch_mapping);
    Ok(WebEvidenceProviderDiagnostics {
        provider_id: Some(provider.id.clone()),
        status: if failures.is_empty() && provider.enabled {
            "ready".into()
        } else {
            provider_diagnostic_status(provider.enabled, &mapping_status)
        },
        failures,
        checks,
        can_use_for_search,
        can_use_for_fetch,
    })
}

fn mapping_json_from_tool_name(value: Option<String>) -> AppResult<Option<String>> {
    let Some(value) = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if value.starts_with('{') || value.starts_with('[') {
        serde_json::from_str::<serde_json::Value>(&value)
            .map_err(|err| AppError::msg(format!("invalid provider mapping JSON: {err}")))?;
        return Ok(Some(value));
    }
    Ok(Some(
        serde_json::json!({
            "tool": value,
        })
        .to_string(),
    ))
}

fn provider_input_to_registry(
    input: WebEvidenceProviderInput,
) -> AppResult<crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput> {
    let provider_kind = input.provider_kind.trim().to_lowercase();
    let transport_kind = input
        .transport_kind
        .unwrap_or_else(|| {
            if provider_kind == "native" {
                "native".into()
            } else {
                "stdio".into()
            }
        })
        .trim()
        .to_lowercase();
    Ok(
        crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
            id: input.id,
            name: input.name,
            kind: provider_kind,
            enabled: input.enabled,
            transport_kind,
            transport_config_json: input.transport_config_json,
            credential_refs_json: input.credential_refs_json,
            web_search_mapping_json: mapping_json_from_tool_name(input.search_mapping)?,
            web_fetch_mapping_json: mapping_json_from_tool_name(input.fetch_mapping)?,
        },
    )
}

#[tauri::command]
pub async fn skills_create_draft(
    state: State<'_, Arc<AppState>>,
    request: SkillCreateDraftRequest,
) -> AppResult<SkillDraftDto> {
    let description = request
        .description
        .as_deref()
        .unwrap_or("Iris prompt-only skill");
    let body = request
        .body
        .as_deref()
        .unwrap_or("Use this skill for the confirmed scope.");
    let scope_yaml = request
        .scope_rules
        .iter()
        .map(|rule| format!("  - kind: {}\n    pattern: {:?}", rule.kind, rule.pattern))
        .collect::<Vec<_>>()
        .join("\n");
    let markdown = format!(
        "---\nname: {}\ndescription: {}\nscope:\n{}\n---\n\n{}\n",
        request.name, description, scope_yaml, body
    );
    let digest = Sha256::digest(markdown.as_bytes());
    let vault = state.vault_path()?;
    let slug = skill_draft_slug(&request.name);
    Ok(SkillDraftDto {
        target_path: vault
            .join(".iris")
            .join("skills")
            .join(slug)
            .join("SKILL.md")
            .to_string_lossy()
            .to_string(),
        name: request.name,
        markdown,
        scope_rules: request.scope_rules,
        content_hash: hex::encode(digest),
    })
}

#[tauri::command]
pub async fn skills_confirm(
    state: State<'_, Arc<AppState>>,
    draft: SkillDraftDto,
) -> AppResult<()> {
    let digest = Sha256::digest(draft.markdown.as_bytes());
    let actual_hash = hex::encode(digest);
    if actual_hash != draft.content_hash {
        return Err(AppError::msg(
            "Skill draft hash does not match confirmed content",
        ));
    }
    let vault = state.vault_path()?;
    crate::ai_runtime::skills::write_confirmed_skill_content(
        &vault,
        Path::new(&draft.target_path),
        crate::ai_runtime::skills::SkillScope::Vault,
        &draft.markdown,
    )?;
    Ok(())
}

fn skill_draft_slug(name: &str) -> String {
    let slug = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if slug.is_empty() {
        "skill".into()
    } else {
        slug
    }
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
    crate::ai_runtime::prompt_profile::PromptProfile::save(&state.db, &profile)?;
    crate::llm::safe_lock(&state.ai.context_cache).clear();
    Ok(())
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
    crate::llm::safe_lock(&state.ai.context_cache).clear();
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

/// Query tool audit entries by request_id.
#[tauri::command]
pub async fn tool_audit_query(
    state: State<'_, Arc<AppState>>,
    request_id: String,
) -> AppResult<Vec<crate::ai_runtime::tool_audit::ToolAuditEntry>> {
    crate::ai_runtime::tool_audit::query_by_request(&state.db, &request_id)
}

// ── Classified AI Thread IPC Commands ────────────────────────────────────────

/// List classified AI threads, optionally filtered by document path.
#[tauri::command]
pub async fn classified_ai_thread_list(
    state: State<'_, Arc<AppState>>,
    document_path: Option<String>,
) -> AppResult<Vec<crate::ai_runtime::classified_session::ClassifiedAiThreadSummary>> {
    let vault = state.vault_path()?;
    crate::ai_runtime::classified_session::classified_ai_thread_list(&vault, document_path)
}

/// Load a classified AI thread by id.
#[tauri::command]
pub async fn classified_ai_thread_load(
    state: State<'_, Arc<AppState>>,
    thread_id: String,
) -> AppResult<crate::ai_runtime::classified_session::ClassifiedAiThread> {
    let vault = state.vault_path()?;
    crate::ai_runtime::classified_session::classified_ai_thread_load(&vault, thread_id)
}

/// Save a classified AI thread.
#[tauri::command]
pub async fn classified_ai_thread_save(
    state: State<'_, Arc<AppState>>,
    thread: crate::ai_runtime::classified_session::ClassifiedAiThread,
) -> AppResult<()> {
    let vault = state.vault_path()?;
    crate::ai_runtime::classified_session::classified_ai_thread_save(&vault, thread)
}

/// Delete a classified AI thread.
#[tauri::command]
pub async fn classified_ai_thread_delete(
    state: State<'_, Arc<AppState>>,
    thread_id: String,
) -> AppResult<()> {
    let vault = state.vault_path()?;
    crate::ai_runtime::classified_session::classified_ai_thread_delete(&vault, thread_id)
}

/// Clear the in-memory classified AI thread index cache.
#[tauri::command]
pub async fn classified_ai_cache_clear() -> AppResult<()> {
    crate::ai_runtime::classified_session::classified_ai_cache_clear()
}

/// Search classified documents using the in-memory retrieval index.
///
/// Builds (or reuses) a heading-aware chunk index of `.classified/` Markdown
/// files and returns ranked results by term frequency, heading match,
/// current-document boost, path similarity, and recency.
#[tauri::command]
pub async fn classified_ai_context_search(
    state: State<'_, Arc<AppState>>,
    query: String,
    current_document: Option<String>,
    scope_paths: Option<Vec<String>>,
    limit: Option<usize>,
) -> AppResult<Vec<crate::ai_runtime::classified_retrieval::ClassifiedSearchHit>> {
    let vault = state.vault_path()?;
    let chunks = crate::ai_runtime::classified_retrieval::build_classified_index(&vault)?;

    let filtered: Vec<_> = if let Some(ref paths) = scope_paths {
        chunks
            .into_iter()
            .filter(|c| paths.iter().any(|p| c.document_path.starts_with(p)))
            .collect()
    } else if let Some(ref current) = current_document {
        chunks
            .into_iter()
            .filter(|c| c.document_path == *current)
            .collect()
    } else {
        Vec::new()
    };

    Ok(crate::ai_runtime::classified_retrieval::search_chunks(
        &filtered,
        &query,
        current_document.as_deref(),
        limit.unwrap_or(10),
    ))
}

/// Clear the in-memory classified retrieval chunk index.
#[tauri::command]
pub async fn classified_ai_retrieval_clear() -> AppResult<()> {
    crate::ai_runtime::classified_retrieval::clear_classified_index();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        compact_cold_start_packets_for_budget, finalize_chat_harness_run,
        parse_confirmed_tool_args, partial_tool_confirm_response, should_prefetch_web,
        validate_ai_note_path,
    };

    #[test]
    fn finalize_chat_harness_run_registers_evidence_packets_in_ledger() {
        use crate::ai_runtime::harness::{HarnessFinishReason, HarnessRunResult, UsageSource};
        use crate::ai_runtime::model_gateway::TokenUsage;
        use crate::ai_runtime::session::SessionManager;
        use crate::ai_runtime::session_evidence::list_session_evidence;
        use crate::ai_runtime::trace::TraceRecorder;
        use crate::ai_runtime::{
            AiScene, ContextPacket, SourceSpan, SourceType, TrustLevel, WebEvidenceMeta,
            WebSearchBackend, WebSourceRank,
        };
        use crate::app::AppState;

        let dir = tempfile::tempdir().unwrap();
        let state = AppState::new(dir.path().to_path_buf()).unwrap();
        let session_id =
            SessionManager::create_fresh(&state.db, AiScene::KnowledgeLookup, None).unwrap();
        let request_id = "finalize-evidence-ledger";
        TraceRecorder::start(&state.db, request_id, AiScene::KnowledgeLookup).unwrap();

        let packet = ContextPacket {
            id: "web-packet-1".into(),
            source_type: SourceType::Web,
            source_path: Some("https://example.com/source".into()),
            title: "Example source".into(),
            heading_path: Some("Section".into()),
            source_span: Some(SourceSpan { start: 7, end: 42 }),
            content_hash: "hash-web-1".into(),
            excerpt: "body must stay out of the evidence ledger".into(),
            retrieval_reason: "web search".into(),
            score: 0.91,
            trust_level: TrustLevel::ExternalWeb,
            citation_label: "[C?]".into(),
            stale: false,
            web: Some(WebEvidenceMeta {
                url: Some("https://example.com/source".into()),
                domain: Some("example.com".into()),
                published_at: None,
                fetched_at: "2026-07-03T00:00:00Z".into(),
                search_backend: WebSearchBackend::Provider,
                source_rank: WebSourceRank::Official,
                provider_id: Some("provider-a".into()),
                provider_kind: Some("search".into()),
                raw_result_hash: Some("raw-hash".into()),
                extraction_method: Some("html".into()),
                conflict_group: None,
                conflict_note: None,
                failure_reason: None,
                fallback_from: None,
            }),
            corpus: None,
        };

        let result = HarnessRunResult {
            request_id: request_id.into(),
            session_id,
            content: "answer with evidence".into(),
            tool_calls: Vec::new(),
            tool_results: Vec::new(),
            usage: TokenUsage::default(),
            citation_valid: true,
            harness_rounds: 1,
            pending_confirmation: false,
            evidence_packets: vec![packet.clone()],
            usage_source: UsageSource::Estimated,
            finish_reason: HarnessFinishReason::Completed,
            deliberation_state: None,
            verification_summary: None,
        };

        finalize_chat_harness_run(
            &state,
            request_id,
            session_id,
            crate::ai_types::CapabilitySlot::Fast,
            "test-provider",
            &result,
            &[],
        )
        .unwrap();

        let messages = SessionManager::recent_messages(&state.db, session_id, 10).unwrap();
        let assistant_seq = messages
            .iter()
            .find(|message| message.role == "assistant")
            .expect("assistant message should be persisted")
            .seq;
        let evidence = state
            .db
            .with_conn(|conn| list_session_evidence(conn, session_id))
            .unwrap();

        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].message_seq_first, assistant_seq);
        assert_eq!(evidence[0].title, "Example source");
        assert_eq!(evidence[0].domain.as_deref(), Some("example.com"));
    }

    #[test]
    fn web_prefetch_requires_strong_external_signal() {
        assert!(should_prefetch_web(
            "请联网搜索 2026 年最新 sqlite-vec 变化"
        ));
        assert!(should_prefetch_web(
            "Summarize the latest news about Tauri today"
        ));
        assert!(should_prefetch_web(
            "核对 https://example.com/release 的内容"
        ));
        assert!(!should_prefetch_web("什么是 SQLite 向量索引"));
        assert!(!should_prefetch_web("今天星期几"));
        assert!(!should_prefetch_web("what is today's date"));
        assert!(!should_prefetch_web(
            "compare Rust and TypeScript for desktop apps"
        ));
    }

    #[test]
    fn cold_start_packets_are_compacted_before_harness() {
        use crate::ai_runtime::{ContextPacket, SourceType, TrustLevel};

        fn packet(id: &str, source_type: SourceType, score: f64, excerpt: String) -> ContextPacket {
            ContextPacket {
                id: id.into(),
                source_type,
                source_path: None,
                title: id.into(),
                heading_path: None,
                source_span: None,
                content_hash: String::new(),
                excerpt,
                retrieval_reason: String::new(),
                score,
                trust_level: TrustLevel::UserNote,
                citation_label: String::new(),
                stale: false,
                web: None,
                corpus: None,
            }
        }

        let mut packets = vec![
            packet("web", SourceType::Web, 0.99, "网".repeat(30_000)),
            packet("note", SourceType::Note, 0.5, "本地笔记".repeat(100)),
        ];

        let diagnostics = compact_cold_start_packets_for_budget(&mut packets, 9_000);

        assert!(diagnostics.evidence_tokens <= 3_000);
        assert_eq!(packets[0].id, "note");
        assert!(packets[1].excerpt.contains("已压缩") || packets[1].excerpt.chars().count() < 500);
    }

    #[test]
    fn ai_note_path_rejects_classified_notes() {
        let err = validate_ai_note_path(Some(".classified/secret.md")).unwrap_err();
        assert!(err.to_string().contains("AI"));
    }

    #[test]
    fn ai_note_path_allows_user_notes_and_empty_context() {
        assert!(validate_ai_note_path(Some("notes/open.md")).is_ok());
        assert!(validate_ai_note_path(None).is_ok());
    }
    #[test]
    fn parse_confirmed_tool_args_rejects_invalid_pending_json() {
        let err = parse_confirmed_tool_args(r#"{"query":"x""#, None).unwrap_err();
        assert!(err.to_string().contains("tool_arguments_parse_error"));
        assert!(!err.to_string().contains(r#"{"query"#));
    }

    #[test]
    fn parse_confirmed_tool_args_requires_modified_args_object() {
        let err = parse_confirmed_tool_args("{}", Some(serde_json::json!(["bad"]))).unwrap_err();
        assert!(err.to_string().contains("tool_arguments_parse_error"));
    }

    #[test]
    fn partial_tool_confirm_response_has_structured_outcomes() {
        use crate::ai_harness::harness_support::{
            save_harness_checkpoint, HarnessCheckpoint, HarnessCheckpointMeta,
        };
        use crate::ai_runtime::harness::UsageSource;
        use crate::ai_runtime::model_gateway::TokenUsage;
        use crate::ai_runtime::trace::{TraceRecorder, TraceStatus};
        use crate::ai_runtime::AiScene;
        use crate::app::AppState;

        let dir = tempfile::tempdir().unwrap();
        let state = AppState::new(dir.path().to_path_buf()).unwrap();
        let request_id = "partial-outcome-1";
        TraceRecorder::start(&state.db, request_id, AiScene::KnowledgeLookup).unwrap();
        TraceRecorder::update_status(&state.db, request_id, TraceStatus::AwaitingToolConfirmation)
            .unwrap();
        save_harness_checkpoint(
            &state.db,
            request_id,
            &HarnessCheckpoint {
                meta: HarnessCheckpointMeta {
                    scene: "knowledge_lookup".into(),
                    session_id: 42,
                    note_path: None,
                    note_title: None,
                    selection_excerpt: None,
                    cold_start_packets: Vec::new(),
                    web_search_enabled: false,
                    depth: 0,
                    capability_slot: None,
                    provider_id: None,
                    model: None,
                    endpoint_family: None,
                    thinking: None,
                    output_budget: None,
                    input_budget: None,
                    skill_activation_plan: None,
                    task_policy: None,
                },
                round: 1,
                messages: Vec::new(),
                tool_calls: Vec::new(),
                tool_results: vec![serde_json::json!({
                    "tool_call_id": "tc-write",
                    "status": "completed",
                    "result": { "status": "ok" },
                })],
                evidence_packets: Vec::new(),
                usage: TokenUsage::default(),
                usage_source: UsageSource::Estimated,
                bonus_round_used: false,
            },
        )
        .unwrap();

        let response = partial_tool_confirm_response(
            &state,
            request_id,
            "tc-write".into(),
            "approve",
            &crate::error::AppError::msg("provider_bad_request: 400 Bad Request"),
        )
        .unwrap();

        assert_eq!(
            response.content,
            "工具已执行，但继续生成回复失败：provider_bad_request"
        );
        assert_eq!(
            response.tool_execution_outcome.as_ref().unwrap().status,
            "succeeded"
        );
        assert!(
            response
                .tool_execution_outcome
                .as_ref()
                .unwrap()
                .side_effect_committed
        );
        assert_eq!(
            response.assistant_resume_outcome.as_ref().unwrap().status,
            "failed"
        );
        assert_eq!(
            response
                .assistant_resume_outcome
                .as_ref()
                .unwrap()
                .failure_class
                .as_deref(),
            Some("provider_bad_request")
        );

        let serialized = serde_json::to_value(&response).unwrap();
        assert_eq!(
            serialized["toolExecutionOutcome"]["sideEffectCommitted"],
            true
        );
        assert_eq!(
            serialized["assistantResumeOutcome"]["userMessage"],
            "工具已执行，但继续生成回复失败。"
        );
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
            assert!(json.contains("dataBase64"));

            let restored: ImageAttachmentDto = serde_json::from_str(&json).unwrap();
            assert_eq!(restored.id, "img-001");
            assert_eq!(restored.data_base64, "iVBORw0KGgo=");
            assert_eq!(restored.mime_type, "image/png");
            assert_eq!(restored.size_bytes, 42_000);
        }

        #[test]
        fn serialized_json_has_snake_case_fields_for_rust_side() {
            let img = test_image();
            let json_str = serde_json::to_string(&img).unwrap();
            assert!(json_str.contains("\"dataBase64\""));
            assert!(json_str.contains("\"mimeType\""));
            assert!(json_str.contains("\"fileName\""));
            assert!(json_str.contains("\"sizeBytes\""));
        }
    }
}
