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
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn context_assemble(
    state: State<'_, Arc<AppState>>,
    scene: String,
    note_path: Option<String>,
    _note_content_hash: Option<String>,
    query: String,
    session_id: Option<i64>,
    context_scope: Option<ContextScopeDto>,
    web_search: Option<bool>,
) -> AppResult<AssembledContext> {
    let scene: AiScene = serde_json::from_str(&format!("\"{scene}\""))
        .map_err(|e| AppError::msg(format!("invalid scene: {e}")))?;

    let profile = resolve_scene(scene);
    let registry = ToolRegistry::new();
    let skill_allowed_tools = state
        .vault_path()
        .ok()
        .and_then(|vault| {
            crate::ai_runtime::skills::active_skill_allowed_tools(&vault, scene, Some(&state.db))
                .ok()
        })
        .unwrap_or_default();
    let policy_ctx = crate::ai_runtime::tool_policy::ToolPolicyContext {
        scene,
        autonomy_level: profile.autonomy_level,
        web_search_enabled: web_search.unwrap_or(false),
        skill_allowed_tools,
        depth: 0,
    };
    let tools: Vec<_> = registry.tools_for_policy_surface(&policy_ctx, false);

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
            token_budget: None,
            max_rounds_override: None,
        },
        provider_config,
        Some(resolved.output_budget),
        resolved.thinking,
    )
    .await?;

    TraceRecorder::update_status(&state.db, &request_id, TraceStatus::ModelCalled)?;

    if !harness_result.pending_confirmation {
        finalize_chat_harness_run(
            state,
            &request_id,
            sid,
            scene,
            &provider_name,
            &harness_result,
            &filtered_packets,
        )?;
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

    Ok(serde_json::json!({
        "request_id": request_id,
        "session_id": sid,
        "status": if harness_result.pending_confirmation { "pending_tools" } else { "completed" },
        "content": harness_result.content,
        "tool_calls": harness_result.tool_calls,
        "tool_results": harness_result.tool_results,
        "usage": harness_result.usage,
        "usage_source": harness_result.usage_source,
        "citation_valid": harness_result.citation_valid,
        "harness_rounds": harness_result.harness_rounds,
        "evidence_packets": harness_result.evidence_packets,
        "pending_confirmation": harness_result.pending_confirmation,
        "evidence_refresh_notice": evidence_refresh_notice,
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

/// Persist session + trace after a completed harness run (not pending confirmation).
fn finalize_chat_harness_run(
    state: &AppState,
    request_id: &str,
    session_id: i64,
    scene: AiScene,
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
        Some(&format!("{:?}", ModelGateway::slot_for_scene(scene))),
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

fn harness_run_to_chat_json(
    harness_result: &crate::ai_runtime::harness::HarnessRunResult,
) -> serde_json::Value {
    serde_json::json!({
        "request_id": harness_result.request_id,
        "session_id": harness_result.session_id,
        "status": if harness_result.pending_confirmation { "pending_tools" } else { "completed" },
        "content": harness_result.content,
        "tool_calls": harness_result.tool_calls,
        "tool_results": harness_result.tool_results,
        "usage": harness_result.usage,
        "citation_valid": harness_result.citation_valid,
        "harness_rounds": harness_result.harness_rounds,
        "evidence_packets": harness_result.evidence_packets,
        "pending_confirmation": harness_result.pending_confirmation,
    })
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
    use crate::ai_runtime::harness_confirm::{
        append_rejected_tool_to_checkpoint, dispatch_approved_tool_to_checkpoint,
        resume_harness_after_tool_confirm_or_restore,
    };

    if decision == "reject" {
        crate::llm::safe_lock(&state.pending_tool_calls).remove(&tool_call_id);
        append_rejected_tool_to_checkpoint(state.inner(), &request_id, &tool_call_id)?;
        let harness_result = resume_harness_after_tool_confirm_or_restore(
            state.inner(),
            &app_handle,
            &request_id,
        )
        .await?;
        if !harness_result.pending_confirmation {
            let scene: AiScene = serde_json::from_str(&format!(
                "\"{}\"",
                load_scene_from_checkpoint(state.inner(), &request_id)?
            ))
            .map_err(|e| AppError::msg(format!("scene: {e}")))?;
            let resolved = crate::llm::config::resolve_for_scene(&state.db, scene)?;
            finalize_chat_harness_run(
                state.inner(),
                &request_id,
                harness_result.session_id,
                scene,
                &resolved.to_provider_config(scene).name,
                &harness_result,
                &harness_result.evidence_packets,
            )?;
        }
        let mut out = harness_run_to_chat_json(&harness_result);
        if let Some(obj) = out.as_object_mut() {
            obj.insert("tool_call_id".into(), serde_json::json!(tool_call_id));
            obj.insert("decision".into(), serde_json::json!("reject"));
            obj.insert("resumed".into(), serde_json::json!(true));
        }
        return Ok(out);
    }

    let pending = crate::llm::safe_lock(&state.pending_tool_calls).remove(&tool_call_id);
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
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&msg.content) {
                    installed_skill = json.get("name").and_then(|n| n.as_str()).map(String::from);
                }
            }
        }
    }

    let harness_result = resume_harness_after_tool_confirm_or_restore(
        state.inner(),
        &app_handle,
        &request_id,
    )
    .await?;

    if !harness_result.pending_confirmation {
        let scene = pending.scene;
        let resolved = crate::llm::config::resolve_for_scene(&state.db, scene)?;
        finalize_chat_harness_run(
            state.inner(),
            &request_id,
            harness_result.session_id,
            scene,
            &resolved.to_provider_config(scene).name,
            &harness_result,
            &harness_result.evidence_packets,
        )?;
    }

    let mut out = harness_run_to_chat_json(&harness_result);
    if let Some(obj) = out.as_object_mut() {
        obj.insert("tool_call_id".into(), serde_json::json!(tool_call_id));
        obj.insert(
            "decision".into(),
            serde_json::json!(if decision == "modify" {
                "modify"
            } else {
                "approve"
            }),
        );
        obj.insert("resumed".into(), serde_json::json!(true));
        if let Some(name) = installed_skill {
            obj.insert("installed_skill".into(), serde_json::json!(name));
        }
    }

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

/// Get available tools for a scene (for frontend display).
#[tauri::command]
pub fn ai_list_tools(
    state: State<'_, Arc<AppState>>,
    scene: String,
) -> AppResult<Vec<serde_json::Value>> {
    let scene: AiScene = serde_json::from_str(&format!("\"{scene}\""))
        .map_err(|e| AppError::msg(format!("invalid scene: {e}")))?;
    let registry = ToolRegistry::new();
    let profile = crate::ai_runtime::resolve_scene(scene);
    let skill_allowed_tools = state
        .vault_path()
        .ok()
        .and_then(|vault| {
            crate::ai_runtime::skills::active_skill_allowed_tools(&vault, scene, Some(&state.db))
                .ok()
        })
        .unwrap_or_default();
    let ctx = crate::ai_runtime::tool_policy::ToolPolicyContext {
        scene,
        autonomy_level: profile.autonomy_level,
        web_search_enabled: true,
        skill_allowed_tools,
        depth: 0,
    };
    let tools: Vec<_> = registry
        .tools_for_policy_surface(&ctx, false)
        .iter()
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
    let scene = scene
        .as_deref()
        .map(|s| serde_json::from_str::<AiScene>(&format!("\"{s}\"")))
        .transpose()
        .map_err(|e| AppError::msg(format!("invalid scene: {e}")))?;
    crate::ai_runtime::skill_install_service::list_skills(&state.db, &vault, scene)
}

#[derive(Debug, serde::Deserialize)]
pub struct SkillsInstallRequest {
    pub source: String,
    pub path_or_url: String,
    pub scope: String,
    pub subpath: Option<String>,
    pub registry: Option<String>,
}

/// Install skill from url, git, local, or registry.
#[tauri::command]
pub async fn skills_install(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    request: SkillsInstallRequest,
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::skill_install_service::{
        install_skill, parse_scope, SkillInstallRequest,
    };
    use crate::ai_runtime::skill_registry::SkillInstallSource;

    let source = SkillInstallSource::parse(&request.source)
        .ok_or_else(|| AppError::msg(format!("unknown install source: {}", request.source)))?;
    let vault = state.vault_path()?;
    let req = SkillInstallRequest {
        source,
        path_or_url: request.path_or_url,
        scope: parse_scope(&request.scope),
        subpath: request.subpath,
        registry: request.registry,
    };
    let entry = install_skill(&state.db, &vault, Some(&app_handle), req).await?;
    Ok(serde_json::to_value(entry).unwrap_or_default())
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
    let web_pages = crate::llm::fetch_web_page::clear_web_cache(&state.db).unwrap_or(0);
    let searches = crate::llm::search_web::cleanup_expired_search_cache(&state.db).unwrap_or(0);
    Ok(serde_json::json!({
        "sessions_deleted": sessions,
        "checkpoints_cleared": checkpoints,
        "deposits_deleted": deposits,
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
) -> AppResult<serde_json::Value> {
    use crate::ai_runtime::harness_confirm::resume_harness_after_tool_confirm_or_restore;

    let harness_result =
        resume_harness_after_tool_confirm_or_restore(state.inner(), &app_handle, &request_id)
            .await?;

    if !harness_result.pending_confirmation {
        let scene_str = load_scene_from_checkpoint(state.inner(), &request_id)?;
        let scene: AiScene = serde_json::from_str(&format!("\"{scene_str}\""))
            .map_err(|e| AppError::msg(format!("invalid scene: {e}")))?;
        let resolved = crate::llm::config::resolve_for_scene(&state.db, scene)?;
        finalize_chat_harness_run(
            state.inner(),
            &request_id,
            harness_result.session_id,
            scene,
            &resolved.to_provider_config(scene).name,
            &harness_result,
            &harness_result.evidence_packets,
        )?;
    }

    Ok(harness_run_to_chat_json(&harness_result))
}

/// Abort an active harness/model request.
#[tauri::command]
pub async fn harness_abort(state: State<'_, Arc<AppState>>, request_id: String) -> AppResult<()> {
    crate::ai_runtime::model_gateway::request_abort(&request_id);
    let _ = TraceRecorder::update_status(&state.db, &request_id, TraceStatus::Aborted);
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
pub struct SkillsReadResourceRequest {
    pub name: String,
    pub scope: String,
    pub relative_path: String,
}

/// Read a file under a skill's references/scripts/assets directory.
#[tauri::command]
pub async fn skills_read_resource(
    state: State<'_, Arc<AppState>>,
    request: SkillsReadResourceRequest,
) -> AppResult<String> {
    use crate::ai_runtime::skill_install_service::parse_scope;
    use crate::ai_runtime::skills::read_skill_resource;
    let vault = state.vault_path()?;
    read_skill_resource(
        &vault,
        &request.name,
        parse_scope(&request.scope),
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
