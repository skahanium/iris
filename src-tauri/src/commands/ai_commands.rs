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

    let (packets, context_status) = build_context_packets(scene, note_path.as_deref(), &query);

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
