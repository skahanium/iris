//! Harness UI event emission.

use tauri::{AppHandle, Emitter};

use crate::error::{AppError, AppResult};

use super::types::{HarnessPhase, HarnessTraceEvent};

fn thinking_event_payload(request_id: &str, round: u32, content: &str) -> serde_json::Value {
    serde_json::json!({
        "request_id": request_id,
        "round": round,
        "has_internal_thinking": !content.trim().is_empty(),
        "content_chars": content.chars().count(),
    })
}

pub(crate) fn emit_thinking(
    app_handle: &AppHandle,
    request_id: &str,
    round: u32,
    content: &str,
) -> AppResult<()> {
    app_handle
        .emit(
            "ai:thinking",
            &thinking_event_payload(request_id, round, content),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_event_payload_does_not_expose_raw_internal_content() {
        let payload = thinking_event_payload(
            "req-1",
            2,
            "internal chain-of-thought with <think>hidden</think>",
        );
        let serialized = payload.to_string();

        assert!(!serialized.contains("chain-of-thought"));
        assert!(!serialized.contains("<think>hidden</think>"));
        assert_eq!(payload["content"], serde_json::Value::Null);
        assert_eq!(payload["has_internal_thinking"], true);
    }
}
