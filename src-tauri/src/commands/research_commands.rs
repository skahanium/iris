//! Research workflow IPC commands.
//!
//! Exposes the L3 research pipeline (sub-proposition decomposition,
//! evidence matrix, argument chain, summary) to the React frontend.

use crate::ai_runtime::{
    model_gateway::{CapabilitySlot, ProviderConfig},
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

    // Get provider config
    let provider_config = get_research_provider(&state).await?;

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

// ─── Helpers ─────────────────────────────────────────────

async fn get_research_provider(state: &State<'_, AppState>) -> AppResult<ProviderConfig> {
    // Research uses the reasoner slot
    let base_url: String = state.db.with_conn(|conn| {
        let result: Result<String, _> = conn.query_row(
            "SELECT value FROM settings WHERE key = 'llm_base_url'",
            [],
            |row| row.get(0),
        );
        Ok(result.unwrap_or_else(|_| "https://api.deepseek.com".to_string()))
    })?;

    let model: String = state.db.with_conn(|conn| {
        let result: Result<String, _> = conn.query_row(
            "SELECT value FROM user_profile WHERE key = 'model_preferences'",
            [],
            |row| row.get(0),
        );
        match result {
            Ok(json_str) => {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    if let Some(m) = v.get("reasoner").and_then(|v| v.as_str()) {
                        return Ok(m.to_string());
                    }
                }
                Ok("deepseek-reasoner".to_string())
            }
            Err(_) => Ok("deepseek-reasoner".to_string()),
        }
    })?;

    let api_key = crate::credentials::get_api_key("llm_api_key").ok();

    Ok(ProviderConfig {
        name: "research_reasoner".into(),
        base_url,
        api_key,
        model,
        slot: CapabilitySlot::Reasoner,
    })
}
