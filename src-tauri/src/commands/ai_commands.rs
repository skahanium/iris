//! AI Runtime IPC commands.
//!
//! These commands expose the ai_runtime pipeline to the React frontend
//! through typed Tauri IPC. They do NOT replace the existing llm_generate
//! path — that continues to work for the current AI panel.

use crate::ai_runtime::{
    packet_builder::build_context_packets,
    scene_router::resolve_scene,
    session::SessionManager,
    tool_executor::ToolRegistry,
    trace::{TraceRecorder, TraceStatus},
    AiScene, AssembledContext,
};
use crate::app::AppState;
use crate::error::{AppError, AppResult};
use tauri::State;

/// Phase A: assemble context without LLM call.
/// Returns evidence packets (empty in Phase A), available tools, and context status.
#[tauri::command]
pub async fn context_assemble(
    state: State<'_, AppState>,
    scene: String,
    note_path: Option<String>,
    note_content_hash: Option<String>,
    query: String,
    session_id: Option<i64>,
) -> AppResult<AssembledContext> {
    let scene: AiScene = serde_json::from_str(&format!("\"{scene}\""))
        .map_err(|e| AppError::msg(format!("invalid scene: {e}")))?;

    let _profile = resolve_scene(scene);
    let registry = ToolRegistry::new();
    let tools: Vec<_> = registry.for_scene(scene).into_iter().cloned().collect();

    // Resolve file_id for graph layer
    let file_id = match &note_path {
        Some(path) => {
            state.db.with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT id FROM files WHERE path = ?1",
                    [path.as_str()],
                    |r| r.get::<_, i64>(0),
                ).ok())
            }).unwrap_or(None)
        }
        None => None,
    };

    let (packets, context_status) = state.db.with_conn(|conn| {
        build_context_packets(
            conn,
            scene,
            note_path.as_deref(),
            file_id,
            &query,
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
    })
}

/// Send an AI message (Phase A: trace-only stub).
/// Phase B+ will wire in model gateway and streaming.
#[tauri::command]
pub async fn ai_send_message(
    state: State<'_, AppState>,
    scene: String,
    session_id: Option<i64>,
    message: String,
    _selected_packet_ids: Option<Vec<String>>,
) -> AppResult<serde_json::Value> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let scene: AiScene = serde_json::from_str(&format!("\"{scene}\""))
        .map_err(|e| AppError::msg(format!("invalid scene: {e}")))?;

    // Start trace
    TraceRecorder::start(&state.db, &request_id, scene)?;

    // Ensure session
    let sid = if let Some(id) = session_id {
        id
    } else {
        SessionManager::ensure(&state.db, scene, None)?
    };

    // Save user message
    SessionManager::append_message(&state.db, sid, "user", &message, None)?;

    // Phase A: return stub response (no actual LLM call yet)
    // Phase B+ will perform retrieval + model call + streaming
    let stub_response = serde_json::json!({
        "request_id": request_id,
        "session_id": sid,
        "status": "stub",
        "message": "AI Runtime Phase A: LLM pipeline not yet wired. Your message has been saved."
    });

    TraceRecorder::complete(
        &state.db,
        &request_id,
        TraceStatus::Completed,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;

    Ok(stub_response)
}

/// Handle tool confirmation from the user.
#[tauri::command]
pub async fn tool_confirm(
    _state: State<'_, AppState>,
    request_id: String,
    tool_call_id: String,
    decision: String,
    modified_args: Option<serde_json::Value>,
) -> AppResult<serde_json::Value> {
    let _decision = match decision.as_str() {
        "approve" => "approved",
        "reject" => "rejected",
        "modify" => "modified",
        other => return Err(AppError::msg(format!("invalid decision: {other}"))),
    };

    // Phase A: acknowledge confirmation.
    // Phase B+ will execute the tool and feed result back to LLM.
    Ok(serde_json::json!({
        "request_id": request_id,
        "tool_call_id": tool_call_id,
        "status": _decision,
        "note": "Phase A: tool execution not yet wired"
    }))
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
pub async fn knowledge_reindex(
    state: State<'_, AppState>,
) -> AppResult<serde_json::Value> {
    let vault = state.vault_path()?;
    let mut stats = serde_json::json!({
        "anchors": 0,
        "regulations": 0,
    });

    state.db.with_conn(|conn| {
        // Re-index regulations
        match crate::knowledge::regulations::reindex_all_regulations(conn, &vault) {
            Ok(count) => { stats["regulations"] = serde_json::json!(count); }
            Err(e) => tracing::warn!("regulation reindex error: {e}"),
        }
        Ok::<_, crate::error::AppError>(())
    })?;

    Ok(stats)
}

/// Hybrid search across all knowledge layers.
#[tauri::command]
pub async fn search_hybrid(
    state: State<'_, AppState>,
    query: String,
    scene: Option<String>,
    note_path: Option<String>,
    limit: Option<usize>,
) -> AppResult<Vec<serde_json::Value>> {
    let scene: AiScene = scene
        .as_deref()
        .map(|s| serde_json::from_str(&format!("\"{s}\"")))
        .transpose()
        .map_err(|e| AppError::msg(format!("invalid scene: {e}")))?
        .unwrap_or(AiScene::KnowledgeLookup);

    let file_id = match &note_path {
        Some(path) => {
            state.db.with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT id FROM files WHERE path = ?1",
                    [path.as_str()],
                    |r| r.get::<_, i64>(0),
                ).ok())
            }).unwrap_or(None)
        }
        None => None,
    };

    let layers = crate::ai_runtime::retrieval_broker::RetrievalLayers {
        fts: true, vector: true, graph: true, exact: true, template: false,
    };

    let request = crate::ai_runtime::retrieval_broker::RetrievalRequest {
        query,
        max_results: limit.unwrap_or(15),
        layers,
        note_context: note_path,
        file_id_context: file_id,
    };

    let packets = state.db.with_conn(|conn| {
        crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request)
    })?;

    let json_packets: Vec<_> = packets
        .into_iter()
        .map(|p| serde_json::to_value(p).unwrap_or_default())
        .collect();

    Ok(json_packets)
}
