//! Research workflow IPC commands.
//!
//! Exposes the L3 research pipeline (sub-proposition decomposition,
//! evidence matrix, argument chain, summary) to the React frontend.

use crate::ai_runtime::{
    research_workflow::{execute_research, ResearchConfig},
    trace::{TraceRecorder, TraceStatus},
    AiScene,
};
use crate::app::AppState;
use crate::error::AppResult;
use tauri::{Emitter, State};

/// Execute a full research workflow on a topic.
///
/// Returns `ResearchResult` with sub-propositions, evidence matrix,
/// argument chain, and synthesized summary.
#[tauri::command]
pub async fn research_execute(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    topic: String,
    web_authorized: Option<bool>,
) -> AppResult<serde_json::Value> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let scene = AiScene::ResearchSynthesis;

    // Start trace
    TraceRecorder::start(&state.db, &request_id, scene)?;

    let resolved = crate::llm::config::resolve_for_scene(&state.db, scene)?;
    let provider_config = resolved.to_provider_config(scene);

    let config = ResearchConfig {
        web_research_authorized: web_authorized.unwrap_or(false),
        ..Default::default()
    };

    // Execute research workflow
    let result = execute_research(
        &state.db,
        &app_handle,
        &request_id,
        &topic,
        config,
        provider_config,
        web_authorized.unwrap_or(false),
    )
    .await?;

    // Complete trace
    TraceRecorder::complete(
        &state.db,
        &request_id,
        TraceStatus::Completed,
        Some("reasoner"),
        Some("research_workflow"),
        None,
        None,
        None,
        Some(result.total_tokens.prompt_tokens),
        Some(result.total_tokens.completion_tokens),
        None,
    )?;

    // Emit completion event
    app_handle
        .emit(
            "ai:research_complete",
            &serde_json::json!({
                "request_id": request_id,
                "propositions": result.evidence_matrix.propositions.len(),
                "evidence_count": result.evidence_matrix.total_evidence_count,
                "coverage_score": result.evidence_matrix.coverage_score,
            }),
        )
        .ok();

    Ok(serde_json::json!({
        "request_id": result.request_id,
        "topic": result.topic,
        "rounds": result.rounds.len(),
        "evidence_matrix": result.evidence_matrix,
        "argument_chain": result.argument_chain,
        "summary": result.summary,
        "total_tokens": result.total_tokens,
    }))
}

/// Get research workflow status for a session.
#[tauri::command]
pub fn research_status(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    let traces = crate::ai_runtime::trace::TraceRecorder::recent(&state.db, 10)?;

    let research_traces: Vec<_> = traces
        .iter()
        .filter(|t| matches!(t.scene, AiScene::ResearchSynthesis))
        .map(|t| {
            serde_json::json!({
                "request_id": t.request_id,
                "status": format!("{:?}", t.status),
                "latency_ms": t.latency_ms,
                "created_at": t.created_at,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "recent_research": research_traces,
    }))
}
