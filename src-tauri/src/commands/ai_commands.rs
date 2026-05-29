//! AI Runtime IPC commands.
//!
//! These commands expose the ai_runtime pipeline to the React frontend
//! through typed Tauri IPC. Phase C: full LLM pipeline with streaming.

use crate::ai_runtime::{
    context_planner::plan_context,
    guardrails::{self, GuardResult},
    model_gateway::{GatewayRequest, LlmMessage, MessageRole, ModelGateway},
    packet_builder::{build_context_packets, max_results_from_budget, ContextBuildOptions},
    retrieval_scope::ContextScopeDto,
    scene_router::resolve_scene,
    session::SessionManager,
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
) -> AppResult<serde_json::Value> {
    let web_search = web_search.unwrap_or(false);
    let request_id = uuid::Uuid::new_v4().to_string();
    let scene: AiScene = serde_json::from_str(&format!("\"{scene}\""))
        .map_err(|e| AppError::msg(format!("invalid scene: {e}")))?;

    let _profile = resolve_scene(scene);

    // Start trace
    TraceRecorder::start(&state.db, &request_id, scene)?;

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

    // Ensure session
    let sid = if let Some(id) = session_id {
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

    // Cache-friendly stable prefix + session history (variable content in history tail)
    let mut messages = ModelGateway::build_stable_prefix(scene, &filtered_packets, &[], web_search);

    for msg in &history {
        let role = match msg.role.as_str() {
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "tool" => MessageRole::Tool,
            _ => MessageRole::User,
        };
        messages.push(LlmMessage {
            role,
            content: msg.content.clone(),
            tool_call_id: None,
            tool_calls: None,
        });
    }

    let mut web_search_meta = None;
    if web_search {
        for msg in messages.iter_mut().rev() {
            if matches!(msg.role, MessageRole::User) {
                let (prefixed, meta) = crate::llm::search_web::prepend_web_search_context_for_db(
                    &state.db,
                    &msg.content,
                )
                .await?;
                msg.content = prefixed;
                web_search_meta = Some(meta);
                break;
            }
        }
    }

    let provider_config = resolved.to_provider_config(scene);

    // Build tool definitions
    let registry = ToolRegistry::new();
    let scene_tools: Vec<_> = registry.for_scene(scene).into_iter().cloned().collect();
    let llm_tools = ModelGateway::tools_to_llm_format(&scene_tools);

    // Build gateway request
    let gateway_request = GatewayRequest {
        provider: provider_config,
        messages,
        tools: llm_tools,
        max_tokens: Some(resolved.output_budget),
        temperature: Some(0.7),
        stream: true,
    };

    // Update trace status
    TraceRecorder::update_status(&state.db, &request_id, TraceStatus::ContextAssembled)?;

    // Create model gateway and send request
    let provider_name = gateway_request.provider.name.clone();
    let gateway =
        ModelGateway::with_defaults(app_handle.clone(), vec![gateway_request.provider.clone()])?;

    let response = gateway
        .send_streaming_request(&request_id, gateway_request)
        .await?;

    // Update trace with model info
    TraceRecorder::update_status(&state.db, &request_id, TraceStatus::ModelCalled)?;

    // Process tool calls if any
    let mut tool_results = Vec::new();
    if !response.tool_calls.is_empty() {
        TraceRecorder::update_status(&state.db, &request_id, TraceStatus::Streaming)?;

        for tool_call in &response.tool_calls {
            // Check if tool requires confirmation
            if registry.requires_confirmation(&tool_call.function.name) {
                // Store pending tool call for later confirmation
                state.pending_tool_calls.lock().unwrap().insert(
                    tool_call.id.clone(),
                    crate::app::PendingToolCall {
                        tool_name: tool_call.function.name.clone(),
                        arguments: tool_call.function.arguments.clone(),
                        request_id: request_id.clone(),
                    },
                );

                // Emit tool confirmation request to frontend
                let confirm_request = serde_json::json!({
                    "request_id": request_id,
                    "tool_call_id": tool_call.id,
                    "tool_name": tool_call.function.name,
                    "arguments": serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments).unwrap_or_default(),
                });

                app_handle
                    .emit("ai:tool_confirm_request", &confirm_request)
                    .map_err(|e| AppError::msg(format!("failed to emit tool confirm: {}", e)))?;

                tool_results.push(serde_json::json!({
                    "tool_call_id": tool_call.id,
                    "status": "pending_confirmation",
                }));
            } else {
                // Auto-execute read-only tools
                let result = execute_tool_auto(
                    state,
                    scene,
                    &tool_call.function.name,
                    &tool_call.function.arguments,
                )
                .await?;

                tool_results.push(serde_json::json!({
                    "tool_call_id": tool_call.id,
                    "status": "completed",
                    "result": result,
                }));
            }
        }
    }

    // Save assistant message
    let assistant_content = response.content.clone().unwrap_or_default();
    let tool_calls_value: Option<serde_json::Value> = if response.tool_calls.is_empty() {
        None
    } else {
        Some(serde_json::to_value(&response.tool_calls).unwrap_or_default())
    };
    SessionManager::append_message(
        &state.db,
        sid,
        "assistant",
        &assistant_content,
        tool_calls_value.as_ref(),
    )?;

    // Verify citations in response
    let citation_result = guardrails::verify_citations(&assistant_content, &filtered_packets);
    let citation_valid = matches!(citation_result, GuardResult::Pass);

    if response.usage.prompt_cache_hit_tokens > 0 || response.usage.prompt_cache_miss_tokens > 0 {
        let _ = crate::llm::config::save_usage_last(
            &state.db,
            response.usage.prompt_cache_hit_tokens,
            response.usage.prompt_cache_miss_tokens,
        );
    }

    // Complete trace
    TraceRecorder::complete(
        &state.db,
        &request_id,
        TraceStatus::Completed,
        Some(&format!("{:?}", ModelGateway::slot_for_scene(scene))),
        Some(&provider_name),
        Some(
            &response
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
        Some(response.usage.prompt_tokens),
        Some(response.usage.completion_tokens),
        None,
    )?;

    info!(
        scene = ?scene,
        provider = %provider_name,
        tokens_input = %response.usage.prompt_tokens,
        tokens_output = %response.usage.completion_tokens,
        "AI request completed"
    );

    Ok(serde_json::json!({
        "request_id": request_id,
        "session_id": sid,
        "status": "completed",
        "content": assistant_content,
        "tool_calls": response.tool_calls,
        "tool_results": tool_results,
        "usage": response.usage,
        "citation_valid": citation_valid,
        "web_search_meta": web_search_meta,
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
        state
            .pending_tool_calls
            .lock()
            .unwrap()
            .remove(&tool_call_id);
        return Ok(serde_json::json!({
            "request_id": request_id,
            "tool_call_id": tool_call_id,
            "status": "rejected",
        }));
    }

    // Retrieve and remove the pending tool call
    let pending = state
        .pending_tool_calls
        .lock()
        .unwrap()
        .remove(&tool_call_id);
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

    // Execute the tool
    let result = execute_tool_auto(
        state.inner(),
        AiScene::KnowledgeLookup,
        &pending.tool_name,
        &args_str,
    )
    .await;

    let output = match result {
        Ok(val) => serde_json::json!({
            "request_id": request_id,
            "tool_call_id": tool_call_id,
            "status": "executed",
            "output": val,
        }),
        Err(e) => serde_json::json!({
            "request_id": request_id,
            "tool_call_id": tool_call_id,
            "status": "error",
            "error": e.to_string(),
        }),
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

// ─── Helper Functions ────────────────────────────────────

/// Auto-execute a read-only tool.
async fn execute_tool_auto(
    state: &AppState,
    _scene: AiScene,
    tool_name: &str,
    args_str: &str,
) -> AppResult<serde_json::Value> {
    let args: serde_json::Value =
        serde_json::from_str(args_str).unwrap_or_else(|_| serde_json::json!({}));

    match tool_name {
        "search_hybrid" | "search_semantic" | "search_keyword" => {
            let query = args["query"]
                .as_str()
                .ok_or_else(|| AppError::msg("missing query parameter"))?;
            let limit = args["limit"].as_u64().unwrap_or(10) as usize;

            let packets = state.db.with_conn(|conn| {
                let request = crate::ai_runtime::retrieval_broker::RetrievalRequest {
                    query: query.to_string(),
                    max_results: limit,
                    layers: crate::ai_runtime::retrieval_broker::RetrievalLayers {
                        fts: true,
                        vector: true,
                        graph: false,
                        exact: false,
                        template: false,
                    },
                    note_context: None,
                    file_id_context: None,
                    scope: crate::ai_runtime::retrieval_scope::RetrievalScope::default(),
                };
                crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request)
            })?;

            Ok(serde_json::json!({
                "results": packets,
                "count": packets.len(),
            }))
        }
        "get_regulation" => {
            let regulation_name = args["regulation_name"]
                .as_str()
                .ok_or_else(|| AppError::msg("missing regulation_name"))?;
            let article = args["article"]
                .as_str()
                .ok_or_else(|| AppError::msg("missing article"))?;

            let packets = state.db.with_conn(|conn| {
                let request = crate::ai_runtime::retrieval_broker::RetrievalRequest {
                    query: format!("{} {}", regulation_name, article),
                    max_results: 1,
                    layers: crate::ai_runtime::retrieval_broker::RetrievalLayers {
                        fts: false,
                        vector: false,
                        graph: false,
                        exact: true,
                        template: false,
                    },
                    note_context: None,
                    file_id_context: None,
                    scope: crate::ai_runtime::retrieval_scope::RetrievalScope::default(),
                };
                crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request)
            })?;

            Ok(serde_json::json!({
                "regulation": packets.first(),
                "found": !packets.is_empty(),
            }))
        }
        _ => Err(AppError::msg(format!(
            "unknown or unsupported auto-tool: {}",
            tool_name
        ))),
    }
}
