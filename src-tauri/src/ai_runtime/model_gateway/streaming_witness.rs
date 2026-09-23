//! Outbound boundary-witness helpers for streaming requests.
//!
//! Split out of `streaming.rs` to keep that file inside its pinned size
//! budget. These helpers only annotate the request's boundary slot; no
//! streaming state lives here.

use super::{body_impl::uses_openai_responses, GatewayRequest, GatewayResponse};
use crate::ai_types::EndpointFamily;

pub(crate) fn note_boundary_serialized(request: &GatewayRequest, body: &serde_json::Value) {
    let Some(slot) = &request.boundary else {
        return;
    };
    slot.note_generated(
        &request.messages,
        u32::try_from(request.tools.len()).unwrap_or(u32::MAX),
        !request.provider.model.is_empty(),
    );
    let family = if uses_openai_responses(request) {
        crate::ai_runtime::boundary_events::ProtocolFamily::OpenAiResponses
    } else {
        match request.provider.endpoint_family {
            EndpointFamily::AnthropicMessages => {
                crate::ai_runtime::boundary_events::ProtocolFamily::AnthropicMessages
            }
            _ => crate::ai_runtime::boundary_events::ProtocolFamily::OpenAiChatCompletions,
        }
    };
    slot.note_serialized(family, body, &request.messages);
}

pub(crate) fn note_boundary_sent(request: &GatewayRequest) {
    if let Some(slot) = &request.boundary {
        slot.note_request_sent();
    }
}

pub(crate) fn finish_reason_class(reason: &str) -> &'static str {
    match reason {
        "stop" | "end_turn" => "stop",
        "tool_calls" | "tool_use" => "tool_calls",
        "length" | "max_tokens" => "length",
        _ => "other",
    }
}

pub(crate) fn http_status_class(status: u16) -> &'static str {
    match status {
        200..=299 => "2xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "other",
    }
}

pub(crate) fn note_boundary_success(request: &GatewayRequest, response: &GatewayResponse) {
    let Some(slot) = &request.boundary else {
        return;
    };
    slot.note_provider_returned(
        crate::ai_runtime::boundary_events::ProviderReturnStructure {
            http_status_class: Some("2xx"),
            has_content: response
                .content
                .as_deref()
                .is_some_and(|content| !content.is_empty()),
            tool_call_count: u32::try_from(response.tool_calls.len()).unwrap_or(u32::MAX),
            finish_reason_class: Some(finish_reason_class(&response.finish_reason)),
        },
    );
    if !slot.has_name_origin() {
        slot.note_name_origin(
            crate::ai_runtime::tool_name_origin::handshake_payload_from_calls(
                slot.correlation().protocol_adapter.as_str(),
                request.tools.iter().map(|tool| tool.function.name.as_str()),
                response
                    .tool_calls
                    .iter()
                    .map(|call| call.function.name.as_str()),
                std::iter::empty::<&str>(),
            ),
        );
    }
    slot.note_handshake_end(crate::ai_runtime::boundary_events::RecordCompleteness::Complete);
}

pub(crate) fn note_boundary_http_failure(request: &GatewayRequest, status: u16) {
    let Some(slot) = &request.boundary else {
        return;
    };
    slot.note_request_sent();
    slot.note_provider_returned(
        crate::ai_runtime::boundary_events::ProviderReturnStructure {
            http_status_class: Some(http_status_class(status)),
            has_content: false,
            tool_call_count: 0,
            finish_reason_class: Some("error"),
        },
    );
    slot.note_handshake_end(crate::ai_runtime::boundary_events::RecordCompleteness::Complete);
}

pub(crate) fn stream_failure_reason_class(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("request aborted") {
        "aborted"
    } else if lower.contains("timeout") {
        "timeout"
    } else if lower.contains("llm streaming request failed") || lower.contains("stream read error")
    {
        "transport"
    } else {
        "error"
    }
}

pub(crate) fn note_boundary_unsuccessful(request: &GatewayRequest, message: &str) {
    let Some(slot) = &request.boundary else {
        return;
    };
    slot.note_known_failure(stream_failure_reason_class(message));
}

pub(crate) fn complete_boundary(
    request: &GatewayRequest,
    response: GatewayResponse,
) -> GatewayResponse {
    note_boundary_success(request, &response);
    response
}
