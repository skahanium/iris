//! AI Runtime IPC commands.
//!
//! These commands expose the ai_runtime pipeline to the React frontend
//! through typed Tauri IPC. Phase C: full LLM pipeline with streaming.

use crate::ai_runtime::{
    context_planner::plan_context,
    guardrails::{self, GuardResult},
    harness::{run_harness, HarnessRunInput},
    model_gateway::ModelGateway,
    packet_builder::{build_context_packets, max_results_from_budget, ContextBuildOptions},
    retrieval_scope::ContextScopeDto,
    scene_router::resolve_scene,
    session::{SessionManager, SessionMessage, SessionSummary},
    tool_executor::ToolRegistry,
    trace::{TraceRecorder, TraceStatus},
    AiScene, AssembledContext,
};
use std::sync::Arc;

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use tauri::{Emitter, State};
use tracing::info;

/// Assemble context with intent detection and retrieval planning.
#[tauri::command]
pub async fn context_assemble(
    state: State<'_, Arc<AppState>>,
    scene: String,
    note_path: Option<String>,
    _note_content_hash: Option<String>,
    query: String,
    session_id: Option<i64>,
    context_scope: Option<ContextScopeDto>,
) -> AppResult<AssembledContext> {
    let scene: AiScene = serde_json::from_str(&format!("\"{scene}\""))
        .map_err(|e| AppError::msg(format!("invalid scene: {e}")))?;

    let _profile = resolve_scene(scene);
    let registry = ToolRegistry::new();
    let tools: Vec<_> = registry.for_scene(scene).into_iter().cloned().collect();

    // Run intent detection and context planning
    let plan = plan_context(&query, scene, note_path.as_deref())?;

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
    let resolved = crate::llm::config::resolve_for_scene(&state.db, scene)?;
    let build_opts = ContextBuildOptions {
        max_results: max_results_from_budget(
            resolved.input_budget,
            scene,
            resolved.context_strategy,
        ),
        strategy: resolved.context_strategy,
        input_budget: resolved.input_budget,
    };
    let (packets, context_status) = state.db.with_conn(|conn| {
        build_context_packets(
            conn,
            &vault,
            scene,
            note_path.as_deref(),
            file_id,
            primary_query,
            &user_scope,
            build_opts,
        )
    })?;

    // Ensure session exists
    let _sid = if let Some(id) = session_id {
        id
    } else {
        SessionManager::ensure(&state.db, scene, note_path.as_deref())?
    };

    Ok(AssembledContext {
        packets,
        tools,
        context_status,
        execution_plan,
    })
}

/// Send an AI message with full LLM pipeline (shared by IPC and assistant facade).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_ai_send_message(
    state: &AppState,
    app_handle: &tauri::AppHandle,
    scene: String,
    session_id: Option<i64>,
    message: String,
    selected_packet_ids: Option<Vec<String>>,
    note_path: Option<String>,
    context_scope: Option<ContextScopeDto>,
    web_search: Option<bool>,
    new_session: Option<bool>,
) -> AppResult<serde_json::Value> {
    let web_search = web_search.unwrap_or(false);
    let new_session = new_session.unwrap_or(false);
    let request_id = uuid::Uuid::new_v4().to_string();
    let scene: AiScene = serde_json::from_str(&format!("\"{scene}\""))
        .map_err(|e| AppError::msg(format!("invalid scene: {e}")))?;

    let _profile = resolve_scene(scene);

    // Start trace
    TraceRecorder::start(&state.db, &request_id, scene)?;

    app_handle
        .emit(
            "ai:request_started",
            &serde_json::json!({ "request_id": request_id }),
        )
        .map_err(|e| AppError::msg(format!("emit request_started: {e}")))?;

    // Sanitize query for injection attempts
    match guardrails::sanitize_query(&message) {
        GuardResult::Block { reason } => {
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
    let sid = if new_session {
        SessionManager::create_fresh(&state.db, scene, note_path.as_deref())?
    } else if let Some(id) = session_id {
        id
    } else {
        SessionManager::ensure(&state.db, scene, note_path.as_deref())?
    };

    // Save user message
    SessionManager::append_message(&state.db, sid, "user", &message, None)?;

    // Get session history for context
    let history = SessionManager::recent_messages(&state.db, sid, 20)?;

    // Build context packets
    let vault = state.vault_path()?;
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
    let resolved = crate::llm::config::resolve_for_scene(&state.db, scene)?;
    let build_opts = ContextBuildOptions {
        max_results: max_results_from_budget(
            resolved.input_budget,
            scene,
            resolved.context_strategy,
        ),
        strategy: resolved.context_strategy,
        input_budget: resolved.input_budget,
    };
    let (packets, _context_status) = state.db.with_conn(|conn| {
        build_context_packets(
            conn,
            &vault,
            scene,
            note_path.as_deref(),
            file_id,
            &message,
            &user_scope,
            build_opts,
        )
    })?;

    // Filter packets by selected IDs if provided
    let filtered_packets = if let Some(ids) = &selected_packet_ids {
        packets
            .into_iter()
            .filter(|p| ids.contains(&p.id))
            .collect()
    } else {
        packets
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

    let provider_config = resolved.to_provider_config(scene);
    let provider_name = provider_config.name.clone();

    TraceRecorder::update_status(&state.db, &request_id, TraceStatus::ContextAssembled)?;

    let harness_result = run_harness(
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
            history_messages,
            depth: 0,
            resume_from_checkpoint: false,
        },
        provider_config,
        Some(resolved.output_budget),
    )
    .await?;

    TraceRecorder::update_status(&state.db, &request_id, TraceStatus::ModelCalled)?;

    let tool_calls_value: Option<serde_json::Value> = if harness_result.tool_calls.is_empty() {
        None
    } else {
        Some(serde_json::to_value(&harness_result.tool_calls).unwrap_or_default())
    };
    SessionManager::append_message(
        &state.db,
        sid,
        "assistant",
        &harness_result.content,
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
        &request_id,
        TraceStatus::Completed,
        Some(&format!("{:?}", ModelGateway::slot_for_scene(scene))),
        Some(&provider_name),
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

    info!(
        scene = ?scene,
        provider = %provider_name,
        harness_rounds = harness_result.harness_rounds,
        tokens_input = %harness_result.usage.prompt_tokens,
        tokens_output = %harness_result.usage.completion_tokens,
        "AI harness request completed"
    );

    Ok(serde_json::json!({
        "request_id": request_id,
        "session_id": sid,
        "status": if harness_result.pending_confirmation { "pending_tools" } else { "completed" },
        "content": harness_result.content,
        "tool_calls": harness_result.tool_calls,
        "tool_results": harness_result.tool_results,
        "usage": harness_result.usage,
        "citation_valid": harness_result.citation_valid,
        "harness_rounds": harness_result.harness_rounds,
        "evidence_packets": harness_result.evidence_packets,
    }))
}

/// Send an AI message with full LLM pipeline.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn ai_send_message(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    scene: String,
    session_id: Option<i64>,
    message: String,
    selected_packet_ids: Option<Vec<String>>,
    note_path: Option<String>,
    context_scope: Option<ContextScopeDto>,
    web_search: Option<bool>,
    new_session: Option<bool>,
) -> AppResult<serde_json::Value> {
    execute_ai_send_message(
        state.inner().as_ref(),
        &app_handle,
        scene,
        session_id,
        message,
        selected_packet_ids,
        note_path,
        context_scope,
        web_search,
        new_session,
    )
    .await
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
) -> AppResult<serde_json::Value> {
    if decision == "reject" {
        crate::llm::safe_lock(&state.pending_tool_calls).remove(&tool_call_id);
        return Ok(serde_json::json!({
            "request_id": request_id,
            "tool_call_id": tool_call_id,
            "status": "rejected",
        }));
    }

    let pending = crate::llm::safe_lock(&state.pending_tool_calls).remove(&tool_call_id);
    let Some(pending) = pending else {
        return Err(AppError::msg(format!(
            "no pending tool call for id: {tool_call_id}"
        )));
    };

    // Use modified args if provided, otherwise use original
    let args_str = if let Some(args) = modified_args {
        serde_json::to_string(&args).unwrap_or_default()
    } else {
        pending.arguments
    };

    let result = crate::ai_runtime::tool_dispatch::dispatch_tool(
        state.inner(),
        &crate::ai_runtime::tool_dispatch::ToolDispatchContext {
            scene: AiScene::KnowledgeLookup,
            note_path: None,
            file_id: None,
            web_search_enabled: true,
            cold_start_packets: &[],
        },
        &pending.tool_name,
        &serde_json::from_str(&args_str).unwrap_or_default(),
    )
    .await;

    let output = if result.success {
        serde_json::json!({
            "request_id": request_id,
            "tool_call_id": tool_call_id,
            "status": "executed",
            "output": result.output,
        })
    } else {
        serde_json::json!({
            "request_id": request_id,
            "tool_call_id": tool_call_id,
            "status": "error",
            "error": result.error.unwrap_or_else(|| "unknown".into()),
        })
    };

    app_handle
        .emit("ai:tool_result", &output)
        .map_err(|e| AppError::msg(format!("failed to emit tool result: {}", e)))?;

    Ok(output)
}

/// Get available tools for a scene (for frontend display).
#[tauri::command]
pub fn ai_list_tools(scene: String) -> AppResult<Vec<serde_json::Value>> {
    let scene: AiScene = serde_json::from_str(&format!("\"{scene}\""))
        .map_err(|e| AppError::msg(format!("invalid scene: {e}")))?;
    let registry = ToolRegistry::new();
    let tools: Vec<_> = registry
        .for_scene(scene)
        .into_iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "requires_confirmation": t.requires_confirmation,
                "access_level": serde_json::to_string(&t.access_level).unwrap_or_default(),
            })
        })
        .collect();
    Ok(tools)
}

/// Re-index all knowledge: anchors, regulations, block links.
#[tauri::command]
pub async fn knowledge_reindex(state: State<'_, Arc<AppState>>) -> AppResult<serde_json::Value> {
    let vault = state.vault_path()?;
    let mut stats = serde_json::json!({
        "anchors": 0,
        "regulations": 0,
    });

    state.db.with_conn(|conn| {
        // Re-index regulations
        match crate::knowledge::regulations::reindex_all_regulations(conn, &vault) {
            Ok(count) => {
                stats["regulations"] = serde_json::json!(count);
            }
            Err(e) => tracing::warn!("regulation reindex error: {e}"),
        }
        Ok::<_, crate::error::AppError>(())
    })?;

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
    let _scene: AiScene = scene
        .as_deref()
        .map(|s| serde_json::from_str(&format!("\"{s}\"")))
        .transpose()
        .map_err(|e| AppError::msg(format!("invalid scene: {e}")))?
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

/// Load recent messages for a session.
#[tauri::command]
pub async fn session_load(
    state: State<'_, Arc<AppState>>,
    session_id: i64,
    limit: Option<u32>,
) -> AppResult<Vec<SessionMessage>> {
    SessionManager::recent_messages(&state.db, session_id, limit.unwrap_or(50))
}

/// List installed skills (global + vault).
#[tauri::command]
pub async fn skills_list(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<crate::ai_runtime::skills::SkillEntry>> {
    let vault = state.vault_path()?;
    crate::ai_runtime::skills::scan_all(&vault)
}

#[derive(Debug, serde::Deserialize)]
pub struct SkillsInstallRequest {
    pub source: String,
    pub path_or_url: String,
    pub scope: String,
    pub subpath: Option<String>,
}

/// Install skill from url, git, or local path.
#[tauri::command]
pub async fn skills_install(
    state: State<'_, Arc<AppState>>,
    request: SkillsInstallRequest,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skills::{install_from_git, install_from_url, SkillScope};
    let vault = state.vault_path()?;
    let scope = match request.scope.as_str() {
        "vault" => SkillScope::Vault,
        _ => SkillScope::Global,
    };
    match request.source.as_str() {
        "url" => {
            let entry = install_from_url(&request.path_or_url, scope, &vault).await?;
            Ok(serde_json::to_value(entry).unwrap_or_default())
        }
        "git" => {
            let entries = install_from_git(
                &request.path_or_url,
                request.subpath.as_deref(),
                scope,
                &vault,
            )
            .await?;
            Ok(serde_json::to_value(entries).unwrap_or_default())
        }
        "local" => {
            let path = std::path::PathBuf::from(&request.path_or_url);
            let entry = crate::ai_runtime::skills::install_from_local(&path, scope, &vault)?;
            Ok(serde_json::to_value(entry).unwrap_or_default())
        }
        other => Err(AppError::msg(format!("unknown install source: {other}"))),
    }
}

#[tauri::command]
pub async fn skills_uninstall(
    state: State<'_, Arc<AppState>>,
    name: String,
    scope: String,
) -> AppResult<()> {
    use crate::ai_runtime::skills::{uninstall, SkillScope};
    let vault = state.vault_path()?;
    let sc = if scope == "vault" {
        SkillScope::Vault
    } else {
        SkillScope::Global
    };
    uninstall(&name, sc, &vault)
}

#[tauri::command]
pub async fn skills_toggle(
    state: State<'_, Arc<AppState>>,
    name: String,
    scope: String,
    enabled: bool,
) -> AppResult<()> {
    use crate::ai_runtime::skills::{set_enabled, SkillScope};
    let vault = state.vault_path()?;
    let sc = if scope == "vault" {
        SkillScope::Vault
    } else {
        SkillScope::Global
    };
    set_enabled(&name, sc, &vault, enabled)
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
    let sessions = SessionManager::delete_all_filtered(&state.db, None, None)?;
    let (checkpoints, deposits) = state.db.with_conn(|conn| {
        let checkpoints = conn.execute(
            "UPDATE ai_traces SET checkpoint = NULL WHERE checkpoint IS NOT NULL",
            [],
        )?;
        let deposits = conn
            .execute("DELETE FROM knowledge_deposits", [])
            .unwrap_or(0);
        Ok::<_, crate::error::AppError>((checkpoints, deposits))
    })?;
    Ok(serde_json::json!({
        "sessions_deleted": sessions,
        "checkpoints_cleared": checkpoints,
        "deposits_deleted": deposits,
    }))
}

/// Resume an interrupted harness run from checkpoint.
#[tauri::command]
pub async fn harness_resume(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    request_id: String,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::harness_support::load_harness_checkpoint;

    let cp = load_harness_checkpoint(&state.db, &request_id)?
        .ok_or_else(|| AppError::msg("未找到可恢复的 checkpoint"))?;
    let scene: AiScene = serde_json::from_str(&format!("\"{}\"", cp.meta.scene))
        .map_err(|e| AppError::msg(format!("invalid scene in checkpoint: {e}")))?;
    let resolved = crate::llm::config::resolve_for_scene(&state.db, scene)?;
    let provider_config = resolved.to_provider_config(scene);

    let harness_result = run_harness(
        &state,
        &app_handle,
        HarnessRunInput {
            request_id: request_id.clone(),
            scene,
            session_id: cp.meta.session_id,
            note_path: cp.meta.note_path.clone(),
            note_title: cp.meta.note_title.clone(),
            selection_excerpt: cp.meta.selection_excerpt.clone(),
            cold_start_packets: cp.meta.cold_start_packets.clone(),
            web_search_enabled: cp.meta.web_search_enabled,
            history_messages: vec![],
            depth: cp.meta.depth,
            resume_from_checkpoint: true,
        },
        provider_config,
        Some(resolved.output_budget),
    )
    .await?;

    Ok(serde_json::to_value(harness_result).unwrap_or_default())
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
