//! Harness UI event emission.

use tauri::{AppHandle, Emitter};

use crate::error::{AppError, AppResult};

use super::types::{HarnessPhase, HarnessTraceEvent};

pub(crate) fn emit_thinking(
    app_handle: &AppHandle,
    request_id: &str,
    round: u32,
    content: &str,
) -> AppResult<()> {
    app_handle
        .emit(
            "ai:thinking",
            &serde_json::json!({
                "request_id": request_id,
                "round": round,
                "content": content,
            }),
        )
        .map_err(|e| AppError::msg(format!("emit thinking: {e}")))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_trace_phase(
    app_handle: &AppHandle,
    request_id: &str,
    round: u32,
    phase: HarnessPhase,
    tool_name: &str,
    status: &str,
    duration_ms: Option<u64>,
    message: Option<String>,
    output_preview: Option<String>,
) -> AppResult<()> {
    app_handle
        .emit(
            "ai:harness_trace",
            &HarnessTraceEvent {
                request_id: request_id.to_string(),
                round,
                phase,
                tool_name: tool_name.to_string(),
                status: status.to_string(),
                duration_ms,
                message,
                output_preview,
            },
        )
        .map_err(|e| AppError::msg(format!("emit harness trace: {e}")))
}
