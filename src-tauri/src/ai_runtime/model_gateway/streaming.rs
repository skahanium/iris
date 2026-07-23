use reqwest::{
    header::{ACCEPT, ACCEPT_ENCODING, CONTENT_ENCODING, TRANSFER_ENCODING},
    Client,
};
use serde::{Deserialize, Serialize};
use std::error::Error as StdError;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use crate::ai_types::{EndpointFamily, FunctionCall, TokenUsage, ToolCall};
use crate::error::{AppError, AppResult, ProviderErrorKind};

use super::{
    abort_impl::{clear_abort, is_abort_requested},
    body_impl::{build_llm_api_body, uses_openai_responses, GatewayRequest},
    http_backend_impl::format_llm_http_error,
    responses_impl::{ResponsesStreamDelta, ResponsesStreamState},
    usage_impl::parse_usage,
    GatewayResponse,
};

/// A provider must acknowledge a streaming request promptly. This deadline covers
/// waiting for response headers, which reqwest's per-read timeout does not bound.
const STREAM_FIRST_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
/// Cancellation remains responsive even while a provider is still opening a stream.
const ABORT_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Streaming event emitted to frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    pub request_id: String,
    pub event_type: StreamEventType,
    pub data: StreamEventData,
    pub surface: StreamSurface,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub classified: bool,
}

/// Stream event types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamEventType {
    Token,
    ReasoningSummary,
    ToolCall,
    Done,
    Error,
}

/// Stream surface controls whether provider tokens are allowed to reach the visible answer slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamSurface {
    InternalCandidate,
    VisibleAnswer,
    VisibleAnswerSanitized,
}

impl StreamSurface {
    fn wire(self) -> &'static str {
        match self {
            StreamSurface::InternalCandidate => "internal_candidate",
            StreamSurface::VisibleAnswer | StreamSurface::VisibleAnswerSanitized => {
                "visible_answer"
            }
        }
    }

    fn candidate_kind(self) -> &'static str {
        match self {
            StreamSurface::InternalCandidate => "internal_candidate",
            StreamSurface::VisibleAnswer | StreamSurface::VisibleAnswerSanitized => {
                "visible_answer_candidate"
            }
        }
    }

    fn is_visible(self) -> bool {
        matches!(
            self,
            StreamSurface::VisibleAnswer | StreamSurface::VisibleAnswerSanitized
        )
    }

    pub(crate) fn sanitizes_visible_output(self) -> bool {
        matches!(self, StreamSurface::VisibleAnswerSanitized)
    }
}

/// Stream event data payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StreamEventData {
    Token {
        token: String,
        /// When true, the observer must discard any previously emitted visible text.
        #[serde(default)]
        replace_visible: bool,
    },
    ReasoningSummary {
        summary_id: String,
        text: String,
    },
    ToolCall {
        tool_call: ToolCall,
    },
    Done {
        usage: Option<TokenUsage>,
    },
    Error {
        message: String,
        final_error: bool,
    },
}

/// Receives normalized streaming lifecycle events without depending on Tauri.
pub trait StreamEventObserver: Send {
    /// Handle a stream event together with its emitted token index.
    fn observe(&mut self, event: &StreamEvent, token_index: u32) -> AppResult<()>;

    /// A tool batch finished; the next model turn may be the visible final answer.
    fn on_tools_finished(&mut self) -> AppResult<()> {
        Ok(())
    }

    /// Another tool batch is about to run; hide any provisional visible answer.
    fn on_tools_starting(&mut self) -> AppResult<()> {
        Ok(())
    }
}

fn lifecycle_content_hash(value: &str) -> String {
    let mut hash = 0x811c9dc5u32;
    for byte in value.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    format!("{hash:08x}")
}

fn streaming_endpoint_url(base_url: &str, endpoint_family: EndpointFamily) -> String {
    let base = base_url.trim_end_matches('/');
    match endpoint_family {
        EndpointFamily::AnthropicMessages => {
            if base.ends_with("/v1") {
                format!("{base}/messages")
            } else {
                format!("{base}/v1/messages")
            }
        }
        EndpointFamily::OpenAiCompatibleChatCompletions | EndpointFamily::ResponsesReserved => {
            crate::llm::providers::chat_completions_url(base_url)
        }
    }
}

fn apply_streaming_auth_headers(
    builder: reqwest::RequestBuilder,
    endpoint_family: EndpointFamily,
    api_key: &str,
) -> reqwest::RequestBuilder {
    match endpoint_family {
        EndpointFamily::AnthropicMessages => builder.header("x-api-key", api_key).header(
            "anthropic-version",
            crate::llm::providers::ANTHROPIC_API_VERSION,
        ),
        EndpointFamily::OpenAiCompatibleChatCompletions | EndpointFamily::ResponsesReserved => {
            builder.header("Authorization", format!("Bearer {}", api_key))
        }
    }
}

#[derive(Default)]
struct AnthropicToolUseBlock {
    id: Option<String>,
    name: Option<String>,
    input_json: String,
}

#[derive(Default)]
struct AnthropicStreamState {
    content: String,
    tool_blocks: std::collections::BTreeMap<usize, AnthropicToolUseBlock>,
    usage: TokenUsage,
    finish_reason: Option<String>,
}

impl AnthropicStreamState {
    fn apply_event_json(&mut self, json: &serde_json::Value) -> AppResult<Option<String>> {
        match json["type"].as_str() {
            Some("content_block_start") => {
                let index = json["index"].as_u64().unwrap_or(0) as usize;
                let block = &json["content_block"];
                match block["type"].as_str() {
                    Some("text") => {
                        if let Some(text) = block["text"].as_str() {
                            self.content.push_str(text);
                            return Ok(Some(text.to_string()));
                        }
                    }
                    Some("tool_use") => {
                        let entry = self.tool_blocks.entry(index).or_default();
                        entry.id = block["id"].as_str().map(str::to_string);
                        entry.name = block["name"].as_str().map(str::to_string);
                        if let Some(input) = block.get("input") {
                            if input != &serde_json::json!({}) {
                                entry.input_json = serde_json::to_string(input)
                                    .unwrap_or_else(|_| "{}".to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
            Some("content_block_delta") => {
                let index = json["index"].as_u64().unwrap_or(0) as usize;
                let delta = &json["delta"];
                match delta["type"].as_str() {
                    Some("text_delta") => {
                        if let Some(text) = delta["text"].as_str() {
                            self.content.push_str(text);
                            return Ok(Some(text.to_string()));
                        }
                    }
                    Some("input_json_delta") => {
                        if let Some(partial) = delta["partial_json"].as_str() {
                            self.tool_blocks
                                .entry(index)
                                .or_default()
                                .input_json
                                .push_str(partial);
                        }
                    }
                    _ => {}
                }
            }
            Some("message_start") | Some("message_delta") => {
                if let Some(stop_reason) = json["delta"]["stop_reason"].as_str() {
                    self.finish_reason = Some(stop_reason.to_string());
                }
                if let Some(input_tokens) = json["message"]["usage"]["input_tokens"].as_u64() {
                    self.usage.prompt_tokens = input_tokens as u32;
                }
                if let Some(input_tokens) = json["usage"]["input_tokens"].as_u64() {
                    self.usage.prompt_tokens = input_tokens as u32;
                }
                if let Some(output_tokens) = json["usage"]["output_tokens"].as_u64() {
                    self.usage.completion_tokens = output_tokens as u32;
                }
                self.usage.total_tokens = self.usage.prompt_tokens + self.usage.completion_tokens;
            }
            Some("error") => {
                let message = json["error"]["message"]
                    .as_str()
                    .or_else(|| json["message"].as_str())
                    .unwrap_or("Anthropic stream error");
                return Err(AppError::msg(message.to_string()));
            }
            _ => {}
        }
        Ok(None)
    }

    fn into_gateway_response(self) -> GatewayResponse {
        let tool_calls = self
            .tool_blocks
            .into_values()
            .filter_map(|block| {
                let id = block.id?;
                let name = block.name?;
                let arguments = normalize_tool_arguments(block.input_json);
                Some(ToolCall {
                    id,
                    call_type: "function".to_string(),
                    function: FunctionCall { name, arguments },
                })
            })
            .collect();

        GatewayResponse {
            content: if self.content.is_empty() {
                None
            } else {
                Some(self.content)
            },
            tool_calls,
            usage: self.usage,
            finish_reason: self.finish_reason.unwrap_or_else(|| "stop".to_string()),
            reasoning_content: None,
            continuation: None,
        }
    }
}

fn normalize_tool_arguments(input_json: String) -> String {
    let trimmed = input_json.trim();
    if trimmed.is_empty() {
        return "{}".to_string();
    }
    serde_json::from_str::<serde_json::Value>(trimmed)
        .and_then(|value| serde_json::to_string(&value))
        .unwrap_or(input_json)
}

fn sanitize_stream_error_message(message: &str) -> String {
    let redacted = crate::ai_runtime::trace::redact_classified_leaks(message);
    if redacted.starts_with("Stream read error:") {
        return "模型流式连接中断，请稍后重试或切换模型。".to_string();
    }
    if let Some((prefix, _)) = redacted.split_once("）：") {
        return format!("{prefix}）");
    }
    const MAX_ERROR_CHARS: usize = 200;
    if redacted.chars().count() <= MAX_ERROR_CHARS {
        return redacted;
    }
    redacted.chars().take(MAX_ERROR_CHARS).collect::<String>() + "…"
}

fn endpoint_family_label(endpoint_family: EndpointFamily) -> &'static str {
    match endpoint_family {
        EndpointFamily::OpenAiCompatibleChatCompletions => "openai_compatible_chat_completions",
        EndpointFamily::AnthropicMessages => "anthropic_messages",
        EndpointFamily::ResponsesReserved => "responses_reserved",
    }
}

fn redacted_error_detail(value: &str) -> serde_json::Value {
    let redacted = crate::ai_runtime::trace::redact_classified_leaks(value);
    let lower = value.to_ascii_lowercase();
    let has_sensitive_marker = [
        "sk-", "bearer ", "api_key", "apikey", "key=", "token=", "secret=",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    let detail = if has_sensitive_marker || redacted.contains("[REDACTED") {
        None
    } else if redacted.chars().count() > 240 {
        Some(redacted.chars().take(240).collect::<String>() + "...")
    } else {
        Some(redacted.clone())
    };

    serde_json::json!({
        "detail": detail,
        "len": redacted.len(),
        "hash": lifecycle_content_hash(&redacted),
    })
}

#[derive(Debug, Clone)]
struct StreamReadErrorDiagnostic {
    is_timeout: bool,
    is_body: bool,
    is_connect: bool,
    is_decode: bool,
    source_chain: Vec<String>,
}

impl StreamReadErrorDiagnostic {
    fn from_reqwest(error: &reqwest::Error) -> Self {
        let mut source_chain = vec![error.to_string()];
        let mut source = error.source();
        while let Some(error) = source {
            source_chain.push(error.to_string());
            if source_chain.len() >= 6 {
                break;
            }
            source = error.source();
        }

        Self {
            is_timeout: error.is_timeout(),
            is_body: error.is_body(),
            is_connect: error.is_connect(),
            is_decode: error.is_decode(),
            source_chain,
        }
    }

    fn to_safe_json(&self) -> serde_json::Value {
        serde_json::json!({
            "is_timeout": self.is_timeout,
            "is_body": self.is_body,
            "is_connect": self.is_connect,
            "is_decode": self.is_decode,
            "source_chain": self.source_chain.iter().map(|item| redacted_error_detail(item)).collect::<Vec<_>>(),
        })
    }
}

#[derive(Debug, Clone)]
struct StreamReadFailureDiagnostic {
    provider_id: String,
    model: String,
    endpoint_family: EndpointFamily,
    http_version: String,
    status: u16,
    content_encoding: Option<String>,
    transfer_encoding: Option<String>,
    elapsed_ms: u128,
    chunk_count: u64,
    byte_count: u64,
    sse_line_count: u64,
    saw_done: bool,
    visible_partial: bool,
    error: StreamReadErrorDiagnostic,
}

impl StreamReadFailureDiagnostic {
    fn to_safe_json(&self) -> serde_json::Value {
        serde_json::json!({
            "event": "stream_body_read_failed",
            "provider": self.provider_id,
            "model": self.model,
            "endpoint_family": endpoint_family_label(self.endpoint_family),
            "http_version": self.http_version,
            "status": self.status,
            "content_encoding": self.content_encoding,
            "transfer_encoding": self.transfer_encoding,
            "elapsed_ms": self.elapsed_ms,
            "chunk_count": self.chunk_count,
            "byte_count": self.byte_count,
            "sse_line_count": self.sse_line_count,
            "saw_done": self.saw_done,
            "visible_partial": self.visible_partial,
            "error": self.error.to_safe_json(),
        })
    }
}

#[derive(Default)]
struct VisibleStreamSanitizer {
    raw: String,
    emitted: String,
}

enum VisibleSanitizeOutcome {
    None,
    Append(String),
    Replace(String),
}

impl VisibleSanitizeOutcome {
    #[cfg(test)]
    fn as_test_delta(&self) -> &str {
        match self {
            Self::None => "",
            Self::Append(delta) | Self::Replace(delta) => delta.as_str(),
        }
    }
}

impl VisibleStreamSanitizer {
    fn new() -> Self {
        Self::default()
    }

    fn sanitize_delta(&mut self, delta: &str, done: bool) -> VisibleSanitizeOutcome {
        self.raw.push_str(delta);
        let next_visible = self.sanitize_visible_stream_prefix(&self.raw, done);
        if next_visible == self.emitted {
            return VisibleSanitizeOutcome::None;
        }
        if !next_visible.starts_with(&self.emitted) {
            tracing::warn!(
                emitted_len = self.emitted.len(),
                next_len = next_visible.len(),
                "visible stream sanitizer replaced previously emitted text"
            );
            self.emitted = next_visible.clone();
            return VisibleSanitizeOutcome::Replace(next_visible);
        }
        let delta = next_visible[self.emitted.len()..].to_string();
        self.emitted = next_visible;
        if delta.is_empty() {
            VisibleSanitizeOutcome::None
        } else {
            VisibleSanitizeOutcome::Append(delta)
        }
    }

    fn finish(&mut self) -> VisibleSanitizeOutcome {
        self.sanitize_delta("", true)
    }

    /// Normalize the visible stream with the same answer sanitizer used by terminal persistence.
    /// A leading planning prefix stays private until a non-planning paragraph appears; it is never
    /// released merely because it is long.
    fn sanitize_visible_stream_prefix(&self, raw: &str, done: bool) -> String {
        let without_meta = sanitize_meta_analysis_prefix_for_stream(raw, done);

        if done {
            without_meta
        } else {
            withhold_partial_reasoning_open_suffix(&without_meta)
        }
    }
}

fn sanitize_meta_analysis_prefix_for_stream(text: &str, done: bool) -> String {
    let normalized = crate::ai_runtime::text_support::sanitize_meta_analysis_prefix(text);
    if !done
        && crate::ai_runtime::text_support::starts_with_meta_analysis_or_partial_prefix(text)
        && (normalized.is_empty() || normalized == text.trim())
    {
        return String::new();
    }
    normalized
}

fn withhold_partial_reasoning_open_suffix(visible: &str) -> String {
    const OPEN_TAGS: [&str; 3] = ["<thinking>", "<think>", "<reasoning>"];
    let lower = visible.to_ascii_lowercase();
    let mut keep_len = visible.len();
    for tag in OPEN_TAGS {
        for prefix_len in 1..tag.len() {
            if lower.ends_with(&tag[..prefix_len]) {
                keep_len = keep_len.min(visible.len().saturating_sub(prefix_len));
            }
        }
    }
    visible[..keep_len].to_string()
}

fn stream_error_event(
    request_id: &str,
    message: &str,
    classified: bool,
    surface: StreamSurface,
    final_error: bool,
) -> StreamEvent {
    StreamEvent {
        request_id: request_id.to_string(),
        event_type: StreamEventType::Error,
        data: StreamEventData::Error {
            message: sanitize_stream_error_message(message),
            final_error,
        },
        surface,
        classified,
    }
}

const SSE_JSON_PARSE_FAILURE_THRESHOLD: u32 = 3;

#[derive(Debug, Default)]
struct SseJsonFailureTracker {
    consecutive_failures: u32,
}

impl SseJsonFailureTracker {
    fn record_success(&mut self) {
        self.consecutive_failures = 0;
    }

    fn record_parse_result(&mut self, request_id: &str, data: &str) -> AppResult<()> {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        tracing::warn!(
            request_id = %request_id,
            consecutive_failures = self.consecutive_failures,
            data_preview = %sanitize_stream_error_message(data),
            "SSE data line contained invalid JSON"
        );
        if self.consecutive_failures >= SSE_JSON_PARSE_FAILURE_THRESHOLD {
            return Err(AppError::msg(format!(
                "stream_invalid_json: consecutive invalid SSE JSON data lines reached {}",
                self.consecutive_failures
            )));
        }
        Ok(())
    }

    fn parse_data(&mut self, request_id: &str, data: &str) -> AppResult<Option<serde_json::Value>> {
        if data.trim().is_empty() {
            return Ok(None);
        }
        match serde_json::from_str::<serde_json::Value>(data) {
            Ok(json) => {
                self.record_success();
                Ok(Some(json))
            }
            Err(_) => {
                self.record_parse_result(request_id, data)?;
                Ok(None)
            }
        }
    }
}
const PARTIAL_VISIBLE_STREAM_ERROR: &str = "partial_visible_stream_error";

fn has_visible_partial(surface: StreamSurface, token_index: u32) -> bool {
    surface.is_visible() && token_index > 0
}

fn should_emit_stream_error(
    emit_error_event: bool,
    surface: StreamSurface,
    token_index: u32,
) -> bool {
    emit_error_event || has_visible_partial(surface, token_index)
}

fn finish_stream_with_error(
    observer: &mut dyn StreamEventObserver,
    request_id: &str,
    message: impl Into<String>,
    classified: bool,
    surface: StreamSurface,
    token_index: u32,
    emit_error_event: bool,
) -> AppError {
    let message = message.into();
    let sanitized = sanitize_stream_error_message(&message);
    let visible_partial = has_visible_partial(surface, token_index);
    if should_emit_stream_error(emit_error_event, surface, token_index) {
        let event = stream_error_event(request_id, &message, classified, surface, true);
        if let Err(err) = observer.observe(&event, token_index) {
            tracing::warn!(
                request_id = %request_id,
                error = %err,
                "failed to emit llm:error for streaming failure"
            );
        }
    }
    clear_abort(request_id);
    if visible_partial && !emit_error_event {
        AppError::msg(format!("{PARTIAL_VISIBLE_STREAM_ERROR}: {sanitized}"))
    } else {
        classify_known_stream_failure(sanitized)
    }
}

fn classify_known_stream_failure(message: String) -> AppError {
    let lower = message.to_ascii_lowercase();
    if lower.contains("request aborted") {
        AppError::provider(ProviderErrorKind::Cancelled, message)
    } else if lower.contains("llm_stream_first_response_timeout") {
        AppError::provider(ProviderErrorKind::Timeout, message)
    } else {
        AppError::msg(message)
    }
}

fn visible_token_delta(
    visible_sanitizer: &mut Option<VisibleStreamSanitizer>,
    delta: &str,
) -> VisibleSanitizeOutcome {
    if let Some(sanitizer) = visible_sanitizer.as_mut() {
        sanitizer.sanitize_delta(delta, false)
    } else if delta.is_empty() {
        VisibleSanitizeOutcome::None
    } else {
        VisibleSanitizeOutcome::Append(delta.to_string())
    }
}

fn visible_token_finish(
    visible_sanitizer: &mut Option<VisibleStreamSanitizer>,
) -> VisibleSanitizeOutcome {
    if let Some(sanitizer) = visible_sanitizer.as_mut() {
        sanitizer.finish()
    } else {
        VisibleSanitizeOutcome::None
    }
}

fn emit_visible_token_outcome(
    observer: &mut dyn StreamEventObserver,
    request_id: &str,
    outcome: VisibleSanitizeOutcome,
    surface: StreamSurface,
    classified: bool,
    token_index: &mut u32,
) -> AppResult<()> {
    let (token, replace_visible) = match outcome {
        VisibleSanitizeOutcome::None => return Ok(()),
        VisibleSanitizeOutcome::Append(token) => (token, false),
        VisibleSanitizeOutcome::Replace(token) => (token, true),
    };
    if token.is_empty() && !replace_visible {
        return Ok(());
    }
    let event = StreamEvent {
        request_id: request_id.to_string(),
        event_type: StreamEventType::Token,
        data: StreamEventData::Token {
            token,
            replace_visible,
        },
        surface,
        classified,
    };
    observer.observe(&event, *token_index)?;
    *token_index += 1;
    Ok(())
}

#[cfg(test)]
fn emit_visible_token_delta(
    observer: &mut dyn StreamEventObserver,
    request_id: &str,
    token: String,
    surface: StreamSurface,
    classified: bool,
    token_index: &mut u32,
) -> AppResult<()> {
    emit_visible_token_outcome(
        observer,
        request_id,
        if token.is_empty() {
            VisibleSanitizeOutcome::None
        } else {
            VisibleSanitizeOutcome::Append(token)
        },
        surface,
        classified,
        token_index,
    )
}

/// Send a streaming request and deliver each lifecycle event to an observer.
pub async fn send_streaming_request_to_observer(
    _client: &Client,
    request_id: &str,
    request: GatewayRequest,
    observer: &mut dyn StreamEventObserver,
    classified: bool,
    surface: StreamSurface,
    emit_error_event: bool,
) -> AppResult<GatewayResponse> {
    if is_abort_requested(request_id) {
        return Err(finish_stream_with_error(
            observer,
            request_id,
            "request aborted",
            classified,
            surface,
            0,
            true,
        ));
    }

    if uses_openai_responses(&request) {
        return send_openai_responses_stream(
            request_id,
            request,
            observer,
            classified,
            surface,
            emit_error_event,
        )
        .await;
    }

    let endpoint_family = request.provider.endpoint_family;
    let url = streaming_endpoint_url(request.provider.base_url.as_str(), endpoint_family);
    let request_started_at = Instant::now();

    let mut body = build_llm_api_body(&request).map_err(|e| {
        finish_stream_with_error(
            observer,
            request_id,
            e.to_string(),
            classified,
            surface,
            0,
            emit_error_event,
        )
    })?;
    body["stream"] = serde_json::json!(true);

    // Production always builds the dedicated HTTPS-only streaming client. The
    // injected client is used only by deterministic local protocol tests.
    #[cfg(test)]
    let streaming_client = _client.clone();
    #[cfg(not(test))]
    let streaming_client =
        crate::network::cert_pinning::create_streaming_https_client().map_err(|e| {
            finish_stream_with_error(
                observer,
                request_id,
                e.to_string(),
                classified,
                surface,
                0,
                emit_error_event,
            )
        })?;
    let mut req_builder = streaming_client
        .post(&url)
        .header(ACCEPT, "text/event-stream")
        .header(ACCEPT_ENCODING, "identity")
        .header("Content-Type", "application/json");

    if let Some(api_key) = &request.provider.api_key {
        req_builder = apply_streaming_auth_headers(req_builder, endpoint_family, api_key);
    }

    let send = req_builder.json(&body).send();
    tokio::pin!(send);
    let first_response_deadline = tokio::time::sleep(STREAM_FIRST_RESPONSE_TIMEOUT);
    tokio::pin!(first_response_deadline);
    let abort_wait = wait_for_abort_signal(request_id);
    tokio::pin!(abort_wait);
    let response = tokio::select! {
        result = &mut send => result.map_err(|e| {
            finish_stream_with_error(
                observer,
                request_id,
                format!("LLM streaming request failed: {e}"),
                classified,
                surface,
                0,
                emit_error_event,
            )
        }),
        _ = &mut first_response_deadline => Err(finish_stream_with_error(
            observer,
            request_id,
            "llm_stream_first_response_timeout",
            classified,
            surface,
            0,
            emit_error_event,
        )),
        _ = &mut abort_wait => Err(finish_stream_with_error(
            observer,
            request_id,
            "request aborted",
            classified,
            surface,
            0,
            true,
        )),
    }?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let message = format_llm_http_error(status, &text);
        let _ = finish_stream_with_error(
            observer,
            request_id,
            message.clone(),
            classified,
            surface,
            0,
            emit_error_event,
        );
        return Err(AppError::from_llm_http_status(status, message));
    }

    let http_version = format!("{:?}", response.version());
    let status = response.status().as_u16();
    let content_encoding = response
        .headers()
        .get(CONTENT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let transfer_encoding = response
        .headers()
        .get(TRANSFER_ENCODING)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let provider_id = request.provider.name.clone();
    let model = request.provider.model.clone();

    let mut full_content = String::new();
    let mut full_reasoning = String::new();
    let mut usage = TokenUsage::default();
    let mut token_index: u32 = 0;
    let mut anthropic_state = AnthropicStreamState::default();
    let mut json_failure_tracker = SseJsonFailureTracker::default();
    let mut visible_sanitizer = if surface.sanitizes_visible_output() {
        Some(VisibleStreamSanitizer::new())
    } else {
        None
    };

    // Incremental tool call accumulator: index -> (id, name, args_buf).
    // OpenAI streams tool calls as deltas: id+name arrive first, then
    // argument fragments across multiple subsequent deltas.
    let mut tool_call_deltas: std::collections::HashMap<
        usize,
        (Option<String>, Option<String>, String),
    > = std::collections::HashMap::new();

    // Process SSE stream with carry buffer to handle chunks split across TCP boundaries.
    // The outer loop is labeled so the [DONE] / message_stop terminators can break out
    // of the stream entirely instead of looping back to `stream.next().await` (which
    // would block on keep-alive sockets until the read_timeout, surfacing as a
    // spurious "重试中" cycle to the user).
    //
    // The loop races `stream.next()` against a periodic abort poll via
    // `tokio::time::timeout`. Without this, a stalled/half-open socket (no chunks
    // arriving) would block `stream.next().await` and the per-chunk abort check
    // would never run — so clicking "中止" could not interrupt a hung stream until
    // reqwest's read_timeout killed it. The timeout race lets abort fire within
    // ~500ms regardless of whether chunks are flowing.
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    let mut carry = String::new();
    let mut carry_truncated = false;
    let mut chunk_count: u64 = 0;
    let mut byte_count: u64 = 0;
    let mut sse_line_count: u64 = 0;
    let saw_done = false;
    const MAX_CARRY_BYTES: usize = 1_048_576;
    'stream: loop {
        // Race the next stream chunk against a periodic abort poll so a stalled
        // socket (no incoming chunks) can still be interrupted by the user
        // within ~500ms, rather than waiting for reqwest's read_timeout.
        let chunk_opt = match tokio::time::timeout(ABORT_POLL_INTERVAL, stream.next()).await {
            Ok(chunk) => chunk,
            Err(_) => {
                if is_abort_requested(request_id) {
                    return Err(finish_stream_with_error(
                        observer,
                        request_id,
                        "request aborted",
                        classified,
                        surface,
                        token_index,
                        true,
                    ));
                }
                continue 'stream;
            }
        };

        let chunk_result = match chunk_opt {
            Some(r) => r,
            None => break 'stream,
        };

        if is_abort_requested(request_id) {
            return Err(finish_stream_with_error(
                observer,
                request_id,
                "request aborted",
                classified,
                surface,
                token_index,
                true,
            ));
        }

        let chunk = chunk_result.map_err(|e| {
            let diagnostic = StreamReadFailureDiagnostic {
                provider_id: provider_id.clone(),
                model: model.clone(),
                endpoint_family,
                http_version: http_version.clone(),
                status,
                content_encoding: content_encoding.clone(),
                transfer_encoding: transfer_encoding.clone(),
                elapsed_ms: request_started_at.elapsed().as_millis(),
                chunk_count,
                byte_count,
                sse_line_count,
                saw_done,
                visible_partial: has_visible_partial(surface, token_index),
                error: StreamReadErrorDiagnostic::from_reqwest(&e),
            };
            tracing::warn!(
                request_id = %request_id,
                diagnostic = %diagnostic.to_safe_json(),
                "stream body read failed"
            );
            finish_stream_with_error(
                observer,
                request_id,
                format!("Stream read error: {e}"),
                classified,
                surface,
                token_index,
                emit_error_event,
            )
        })?;
        chunk_count = chunk_count.saturating_add(1);
        byte_count = byte_count.saturating_add(chunk.len() as u64);

        let chunk_text = String::from_utf8_lossy(&chunk);
        if carry.len() + chunk_text.len() > MAX_CARRY_BYTES {
            if !carry_truncated {
                tracing::warn!(
                    request_id = %request_id,
                    carry_len = carry.len(),
                    "SSE 缓冲区超过 1 MiB 上限，截断缓冲数据"
                );
                carry_truncated = true;
            }
        } else {
            carry.push_str(&chunk_text);
        }

        while let Some(pos) = carry.find('\n') {
            let line: String = carry.drain(..=pos).collect();
            let line = line.trim_end_matches('\n').trim_end_matches('\r');
            sse_line_count = sse_line_count.saturating_add(1);

            if !line.starts_with("data: ") {
                continue;
            }

            let data = &line[6..];
            if data == "[DONE]" {
                emit_visible_token_outcome(
                    observer,
                    request_id,
                    visible_token_finish(&mut visible_sanitizer),
                    surface,
                    classified,
                    &mut token_index,
                )?;
                let event = StreamEvent {
                    request_id: request_id.to_string(),
                    event_type: StreamEventType::Done,
                    data: StreamEventData::Done {
                        usage: Some(usage.clone()),
                    },
                    surface,
                    classified,
                };
                observer.observe(&event, token_index)?;
                // The stream is finished; stop reading from the socket. Some
                // providers/proxies keep the connection open after [DONE];
                // `continue` here would wait for the server to close (or the
                // 60s timeout), surfacing as a hang.
                break 'stream;
            }

            let Some(json) = (match json_failure_tracker.parse_data(request_id, data) {
                Ok(json) => json,
                Err(err) => {
                    return Err(finish_stream_with_error(
                        observer,
                        request_id,
                        err.to_string(),
                        classified,
                        surface,
                        token_index,
                        emit_error_event,
                    ));
                }
            }) else {
                continue;
            };
            if endpoint_family == EndpointFamily::AnthropicMessages {
                if let Some(delta) = anthropic_state.apply_event_json(&json).map_err(|e| {
                    finish_stream_with_error(
                        observer,
                        request_id,
                        e.to_string(),
                        classified,
                        surface,
                        token_index,
                        emit_error_event,
                    )
                })? {
                    emit_visible_token_outcome(
                        observer,
                        request_id,
                        visible_token_delta(&mut visible_sanitizer, &delta),
                        surface,
                        classified,
                        &mut token_index,
                    )?;
                }
                if json["type"].as_str() == Some("message_stop") {
                    emit_visible_token_outcome(
                        observer,
                        request_id,
                        visible_token_finish(&mut visible_sanitizer),
                        surface,
                        classified,
                        &mut token_index,
                    )?;
                    let event = StreamEvent {
                        request_id: request_id.to_string(),
                        event_type: StreamEventType::Done,
                        data: StreamEventData::Done {
                            usage: Some(anthropic_state.usage.clone()),
                        },
                        surface,
                        classified,
                    };
                    observer.observe(&event, token_index)?;
                    // Anthropic's terminal event; stop reading the socket
                    // so a half-open connection cannot hang the loop.
                    break 'stream;
                }
                continue;
            }

            // Process content delta
            if let Some(delta) = json["choices"][0]["delta"]["content"].as_str() {
                full_content.push_str(delta);
                emit_visible_token_outcome(
                    observer,
                    request_id,
                    visible_token_delta(&mut visible_sanitizer, delta),
                    surface,
                    classified,
                    &mut token_index,
                )?;
            }

            if let Some(reasoning) = json["choices"][0]["delta"]["reasoning_content"].as_str() {
                full_reasoning.push_str(reasoning);
            }

            // Accumulate tool call deltas by index
            if let Some(tc_deltas) = json["choices"][0]["delta"]["tool_calls"].as_array() {
                for tc_delta in tc_deltas {
                    let idx = tc_delta["index"].as_u64().unwrap_or(0) as usize;
                    let entry = tool_call_deltas
                        .entry(idx)
                        .or_insert((None, None, String::new()));

                    if let Some(id) = tc_delta["id"].as_str() {
                        entry.0 = Some(id.to_string());
                    }
                    if let Some(name) = tc_delta["function"]["name"].as_str() {
                        entry.1 = Some(name.to_string());
                    }
                    if let Some(args) = tc_delta["function"]["arguments"].as_str() {
                        entry.2.push_str(args);
                    }
                }
            }

            if json.get("usage").is_some() && json["usage"].get("prompt_tokens").is_some() {
                usage = parse_usage(&json);
            }
        }
    }

    // Flush remaining carry buffer
    if !carry.trim().is_empty() {
        if let Some(pos) = carry.find("data: ") {
            let remainder = &carry[pos..];
            if let Some(data) = remainder.strip_prefix("data: ") {
                let data = data.trim();
                if data != "[DONE]" {
                    let Some(json) = (match json_failure_tracker.parse_data(request_id, data) {
                        Ok(json) => json,
                        Err(err) => {
                            return Err(finish_stream_with_error(
                                observer,
                                request_id,
                                err.to_string(),
                                classified,
                                surface,
                                token_index,
                                emit_error_event,
                            ));
                        }
                    }) else {
                        return Ok(GatewayResponse {
                            content: Some(full_content),
                            tool_calls: vec![],
                            usage,
                            finish_reason: "stop".into(),
                            reasoning_content: if full_reasoning.is_empty() {
                                None
                            } else {
                                Some(full_reasoning)
                            },
                            continuation: None,
                        });
                    };
                    if endpoint_family == EndpointFamily::AnthropicMessages {
                        if let Some(delta) =
                            anthropic_state.apply_event_json(&json).map_err(|e| {
                                finish_stream_with_error(
                                    observer,
                                    request_id,
                                    e.to_string(),
                                    classified,
                                    surface,
                                    token_index,
                                    emit_error_event,
                                )
                            })?
                        {
                            full_content.push_str(delta.as_str());
                            emit_visible_token_outcome(
                                observer,
                                request_id,
                                visible_token_delta(&mut visible_sanitizer, &delta),
                                surface,
                                classified,
                                &mut token_index,
                            )?;
                        }
                        if json["type"].as_str() == Some("message_stop") {
                            emit_visible_token_outcome(
                                observer,
                                request_id,
                                visible_token_finish(&mut visible_sanitizer),
                                surface,
                                classified,
                                &mut token_index,
                            )?;
                            let event = StreamEvent {
                                request_id: request_id.to_string(),
                                event_type: StreamEventType::Done,
                                data: StreamEventData::Done {
                                    usage: Some(anthropic_state.usage.clone()),
                                },
                                surface,
                                classified,
                            };
                            observer.observe(&event, token_index)?;
                        }
                        clear_abort(request_id);
                        return Ok(anthropic_state.into_gateway_response());
                    }

                    if let Some(delta) = json["choices"][0]["delta"]["content"].as_str() {
                        full_content.push_str(delta);
                        emit_visible_token_outcome(
                            observer,
                            request_id,
                            visible_token_delta(&mut visible_sanitizer, delta),
                            surface,
                            classified,
                            &mut token_index,
                        )?;
                    }
                    if let Some(tc_deltas) = json["choices"][0]["delta"]["tool_calls"].as_array() {
                        for tc_delta in tc_deltas {
                            let idx = tc_delta["index"].as_u64().unwrap_or(0) as usize;
                            let entry =
                                tool_call_deltas
                                    .entry(idx)
                                    .or_insert((None, None, String::new()));
                            if let Some(id) = tc_delta["id"].as_str() {
                                entry.0 = Some(id.to_string());
                            }
                            if let Some(name) = tc_delta["function"]["name"].as_str() {
                                entry.1 = Some(name.to_string());
                            }
                            if let Some(args) = tc_delta["function"]["arguments"].as_str() {
                                entry.2.push_str(args);
                            }
                        }
                    }
                    if json.get("usage").is_some() && json["usage"].get("prompt_tokens").is_some() {
                        usage = parse_usage(&json);
                    }
                }
            }
        }
    }

    if endpoint_family == EndpointFamily::AnthropicMessages {
        let response = anthropic_state.into_gateway_response();
        for tc in &response.tool_calls {
            let event = StreamEvent {
                request_id: request_id.to_string(),
                event_type: StreamEventType::ToolCall,
                data: StreamEventData::ToolCall {
                    tool_call: tc.clone(),
                },
                surface,
                classified,
            };
            observer.observe(&event, token_index)?;
        }
        clear_abort(request_id);
        return Ok(response);
    }

    // Assemble tool calls from accumulated deltas (deduplicated by index)
    let tool_calls: Vec<ToolCall> = tool_call_deltas
        .into_iter()
        .filter_map(|(_, (id, name, args))| {
            Some(ToolCall {
                id: id?,
                call_type: "function".into(),
                function: FunctionCall {
                    name: name?,
                    arguments: args,
                },
            })
        })
        .collect();

    // Emit tool call events for each assembled call
    for tc in &tool_calls {
        let event = StreamEvent {
            request_id: request_id.to_string(),
            event_type: StreamEventType::ToolCall,
            data: StreamEventData::ToolCall {
                tool_call: tc.clone(),
            },
            surface,
            classified,
        };
        observer.observe(&event, token_index)?;
    }

    clear_abort(request_id);
    Ok(GatewayResponse {
        content: if full_content.is_empty() {
            None
        } else {
            Some(full_content)
        },
        tool_calls,
        usage,
        finish_reason: "stop".to_string(),
        reasoning_content: if full_reasoning.is_empty() {
            None
        } else {
            Some(full_reasoning)
        },
        continuation: None,
    })
}

fn responses_endpoint_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/responses")
    } else {
        format!("{base}/v1/responses")
    }
}

/// Stream one documented OpenAI Responses exchange. This intentionally has a
/// separate parser instead of pretending Responses events are Chat Completions
/// chunks: tool call IDs and `previous_response_id` must remain exact.
async fn send_openai_responses_stream(
    request_id: &str,
    request: GatewayRequest,
    observer: &mut dyn StreamEventObserver,
    classified: bool,
    surface: StreamSurface,
    emit_error_event: bool,
) -> AppResult<GatewayResponse> {
    let url = responses_endpoint_url(request.provider.base_url.as_str());
    let mut body = build_llm_api_body(&request).map_err(|error| {
        finish_stream_with_error(
            observer,
            request_id,
            error.to_string(),
            classified,
            surface,
            0,
            emit_error_event,
        )
    })?;
    body["stream"] = serde_json::json!(true);

    let streaming_client =
        crate::network::cert_pinning::create_streaming_https_client().map_err(|error| {
            finish_stream_with_error(
                observer,
                request_id,
                error.to_string(),
                classified,
                surface,
                0,
                emit_error_event,
            )
        })?;
    let mut request_builder = streaming_client
        .post(url)
        .header(ACCEPT, "text/event-stream")
        .header(ACCEPT_ENCODING, "identity")
        .header("Content-Type", "application/json");
    if let Some(api_key) = &request.provider.api_key {
        request_builder = apply_streaming_auth_headers(
            request_builder,
            request.provider.endpoint_family,
            api_key,
        );
    }

    let send = request_builder.json(&body).send();
    tokio::pin!(send);
    let first_response_deadline = tokio::time::sleep(STREAM_FIRST_RESPONSE_TIMEOUT);
    tokio::pin!(first_response_deadline);
    let abort_wait = wait_for_abort_signal(request_id);
    tokio::pin!(abort_wait);
    let response = tokio::select! {
        result = &mut send => result.map_err(|error| finish_stream_with_error(
            observer, request_id, format!("LLM streaming request failed: {error}"),
            classified, surface, 0, emit_error_event,
        )),
        _ = &mut first_response_deadline => Err(finish_stream_with_error(
            observer, request_id, "llm_stream_first_response_timeout",
            classified, surface, 0, emit_error_event,
        )),
        _ = &mut abort_wait => Err(finish_stream_with_error(
            observer, request_id, "request aborted", classified, surface, 0, true,
        )),
    }?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let message = format_llm_http_error(status, &text);
        let _ = finish_stream_with_error(
            observer,
            request_id,
            message.clone(),
            classified,
            surface,
            0,
            emit_error_event,
        );
        return Err(AppError::from_llm_http_status(status, message));
    }

    use futures_util::StreamExt;
    let mut stream = response.bytes_stream();
    let mut carry = String::new();
    let mut state = ResponsesStreamState::default();
    let mut tracker = SseJsonFailureTracker::default();
    let mut visible_sanitizer = surface
        .sanitizes_visible_output()
        .then(VisibleStreamSanitizer::new);
    let mut token_index = 0;
    let mut completed = false;

    'stream: loop {
        let chunk = match tokio::time::timeout(ABORT_POLL_INTERVAL, stream.next()).await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(_) => {
                if is_abort_requested(request_id) {
                    return Err(finish_stream_with_error(
                        observer,
                        request_id,
                        "request aborted",
                        classified,
                        surface,
                        token_index,
                        true,
                    ));
                }
                continue;
            }
        };
        if is_abort_requested(request_id) {
            return Err(finish_stream_with_error(
                observer,
                request_id,
                "request aborted",
                classified,
                surface,
                token_index,
                true,
            ));
        }
        let chunk = chunk.map_err(|error| {
            finish_stream_with_error(
                observer,
                request_id,
                format!("Stream read error: {error}"),
                classified,
                surface,
                token_index,
                emit_error_event,
            )
        })?;
        carry.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(line_end) = carry.find('\n') {
            let line: String = carry.drain(..=line_end).collect();
            let line = line.trim_end_matches('\n').trim_end_matches('\r');
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if data == "[DONE]" {
                // Unlike Chat Completions, a Responses stream is successful only
                // after its documented `response.completed` event. A proxy may
                // still append [DONE]; stop reading it, then reject the stream
                // below if the semantic terminal event never arrived.
                break 'stream;
            }
            let Some(json) = tracker.parse_data(request_id, data).map_err(|error| {
                finish_stream_with_error(
                    observer,
                    request_id,
                    error.to_string(),
                    classified,
                    surface,
                    token_index,
                    emit_error_event,
                )
            })?
            else {
                continue;
            };
            let terminal = matches!(json["type"].as_str(), Some("response.completed"));
            for delta in state.apply_event_json(&json).map_err(|error| {
                finish_stream_with_error(
                    observer,
                    request_id,
                    error.to_string(),
                    classified,
                    surface,
                    token_index,
                    emit_error_event,
                )
            })? {
                match delta {
                    ResponsesStreamDelta::Text(text) => {
                        emit_visible_token_outcome(
                            observer,
                            request_id,
                            visible_token_delta(&mut visible_sanitizer, &text),
                            surface,
                            classified,
                            &mut token_index,
                        )?;
                    }
                    ResponsesStreamDelta::ReasoningSummary { summary_id, text } => {
                        observer.observe(
                            &StreamEvent {
                                request_id: request_id.to_string(),
                                event_type: StreamEventType::ReasoningSummary,
                                data: StreamEventData::ReasoningSummary { summary_id, text },
                                surface: StreamSurface::InternalCandidate,
                                classified,
                            },
                            token_index,
                        )?;
                    }
                }
            }
            if terminal {
                completed = true;
                break 'stream;
            }
        }
    }

    if !completed {
        return Err(finish_stream_with_error(
            observer,
            request_id,
            "responses_stream_incomplete",
            classified,
            surface,
            token_index,
            emit_error_event,
        ));
    }

    emit_visible_token_outcome(
        observer,
        request_id,
        visible_token_finish(&mut visible_sanitizer),
        surface,
        classified,
        &mut token_index,
    )?;
    let gateway_response = state.into_gateway_response();
    for tool_call in &gateway_response.tool_calls {
        observer.observe(
            &StreamEvent {
                request_id: request_id.to_string(),
                event_type: StreamEventType::ToolCall,
                data: StreamEventData::ToolCall {
                    tool_call: tool_call.clone(),
                },
                surface,
                classified,
            },
            token_index,
        )?;
    }
    observer.observe(
        &StreamEvent {
            request_id: request_id.to_string(),
            event_type: StreamEventType::Done,
            data: StreamEventData::Done {
                usage: Some(gateway_response.usage.clone()),
            },
            surface,
            classified,
        },
        token_index,
    )?;
    clear_abort(request_id);
    Ok(gateway_response)
}

async fn wait_for_abort_signal(request_id: &str) {
    loop {
        tokio::time::sleep(ABORT_POLL_INTERVAL).await;
        if is_abort_requested(request_id) {
            return;
        }
    }
}

/// Emit a `llm:reset` event so the frontend drops buffered tokens from a
/// non-terminal round (tool-call round or inconclusive reflection) before the
/// next round begins streaming. This prevents intermediate preamble or
/// `NEED_MORE_EVIDENCE` sentinels from sticking to the final answer surface.
pub fn emit_stream_reset(app_handle: &AppHandle, request_id: &str) -> AppResult<()> {
    emit_stream_reset_with_surface(
        app_handle,
        request_id,
        "unknown",
        StreamSurface::VisibleAnswer,
        None,
    )
}

pub fn emit_stream_reset_with_reason(
    app_handle: &AppHandle,
    request_id: &str,
    reason_kind: &str,
) -> AppResult<()> {
    emit_stream_reset_with_surface(
        app_handle,
        request_id,
        reason_kind,
        StreamSurface::InternalCandidate,
        None,
    )
}

pub fn emit_stream_reset_with_surface(
    app_handle: &AppHandle,
    request_id: &str,
    reason_kind: &str,
    surface: StreamSurface,
    round: Option<u32>,
) -> AppResult<()> {
    tracing::debug!(
        request_id = %request_id,
        event = "stream_reset_emitted",
        reason_kind,
        surface = surface.wire(),
        candidate_kind = surface.candidate_kind(),
        round,
        "AI lifecycle stream reset emitted"
    );
    app_handle
        .emit(
            "llm:reset",
            serde_json::json!({
                "request_id": request_id,
                "reason_kind": reason_kind,
                "surface": surface.wire(),
                "candidate_kind": surface.candidate_kind(),
                "round": round,
            }),
        )
        .map_err(|e| AppError::msg(format!("Failed to emit llm:reset: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingStreamObserver {
        events: Vec<StreamEvent>,
    }

    impl StreamEventObserver for RecordingStreamObserver {
        fn observe(&mut self, event: &StreamEvent, _token_index: u32) -> AppResult<()> {
            self.events.push(event.clone());
            Ok(())
        }
    }

    #[test]
    fn stream_events_are_delivered_to_observer_without_tauri_handle() {
        let mut observer = RecordingStreamObserver::default();
        let mut token_index = 0;

        emit_visible_token_delta(
            &mut observer,
            "agent-run",
            "观察者令牌".to_string(),
            StreamSurface::VisibleAnswer,
            false,
            &mut token_index,
        )
        .unwrap();

        assert_eq!(token_index, 1);
        assert_eq!(observer.events.len(), 1);
        assert!(matches!(
            observer.events[0].data,
            StreamEventData::Token { ref token, .. } if token == "观察者令牌"
        ));
    }

    #[test]
    fn anthropic_stream_state_accumulates_text_and_tool_use_blocks() {
        let mut state = AnthropicStreamState::default();

        state
            .apply_event_json(&serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {
                    "type": "text_delta",
                    "text": "先查一下。"
                }
            }))
            .unwrap();
        state
            .apply_event_json(&serde_json::json!({
                "type": "content_block_start",
                "index": 1,
                "content_block": {
                    "type": "tool_use",
                    "id": "toolu_stream_1",
                    "name": "search_hybrid",
                    "input": {}
                }
            }))
            .unwrap();
        state
            .apply_event_json(&serde_json::json!({
                "type": "content_block_delta",
                "index": 1,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": "{\"query\":\"阶段 1\""
                }
            }))
            .unwrap();
        state
            .apply_event_json(&serde_json::json!({
                "type": "content_block_delta",
                "index": 1,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": ",\"limit\":5}"
                }
            }))
            .unwrap();
        state
            .apply_event_json(&serde_json::json!({
                "type": "message_delta",
                "delta": { "stop_reason": "tool_use" },
                "usage": { "output_tokens": 11 }
            }))
            .unwrap();

        let response = state.into_gateway_response();
        assert_eq!(response.content.as_deref(), Some("先查一下。"));
        assert_eq!(response.finish_reason, "tool_use");
        assert_eq!(response.usage.completion_tokens, 11);
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "toolu_stream_1");
        assert_eq!(response.tool_calls[0].function.name, "search_hybrid");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&response.tool_calls[0].function.arguments)
                .unwrap(),
            serde_json::json!({ "query": "阶段 1", "limit": 5 })
        );
    }

    #[test]
    fn sse_json_failure_tracker_tolerates_single_bad_line_but_fails_after_threshold() {
        let mut tracker = SseJsonFailureTracker::default();
        assert!(tracker.record_parse_result("req-json", "{bad json").is_ok());
        assert_eq!(tracker.consecutive_failures, 1);
        assert!(tracker
            .record_parse_result("req-json", "{still bad")
            .is_ok());
        let err = tracker
            .record_parse_result("req-json", "{third bad")
            .unwrap_err();
        assert!(err.to_string().contains("stream_invalid_json"));
    }

    #[test]
    fn sse_json_failure_tracker_resets_after_valid_json() {
        let mut tracker = SseJsonFailureTracker::default();
        assert!(tracker.record_parse_result("req-json", "{bad json").is_ok());
        tracker.record_success();
        assert_eq!(tracker.consecutive_failures, 0);
        assert!(tracker
            .record_parse_result("req-json", "{bad again")
            .is_ok());
    }
    #[test]
    fn sse_json_failure_tracker_ignores_empty_data_lines() {
        let mut tracker = SseJsonFailureTracker::default();
        assert!(tracker.parse_data("req-json", "   ").unwrap().is_none());
        assert_eq!(tracker.consecutive_failures, 0);
    }
    #[test]
    fn stream_error_event_uses_public_lifecycle_contract() {
        let event = stream_error_event(
            "req-stream-error",
            "模型请求失败（500）：provider echoed prompt text",
            false,
            StreamSurface::VisibleAnswer,
            true,
        );

        assert_eq!(event.request_id, "req-stream-error");
        assert!(matches!(event.event_type, StreamEventType::Error));
        match event.data {
            StreamEventData::Error {
                message,
                final_error,
            } => {
                assert!(message.contains("模型请求失败"));
                assert!(!message.contains("prompt text"));
                assert!(final_error);
            }
            _ => panic!("expected error payload"),
        }
    }

    #[test]
    fn visible_partial_stream_errors_force_terminal_event() {
        assert!(!should_emit_stream_error(
            false,
            StreamSurface::VisibleAnswer,
            0
        ));
        assert!(should_emit_stream_error(
            false,
            StreamSurface::VisibleAnswer,
            1
        ));
        assert!(!should_emit_stream_error(
            false,
            StreamSurface::InternalCandidate,
            1
        ));
        assert!(should_emit_stream_error(
            true,
            StreamSurface::VisibleAnswer,
            0
        ));
    }

    #[test]
    fn stream_body_failure_diagnostic_is_structured_and_redacted() {
        let diagnostic = StreamReadFailureDiagnostic {
            provider_id: "deepseek".into(),
            model: "deepseek-v4-pro".into(),
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
            http_version: "HTTP/2.0".into(),
            status: 200,
            content_encoding: Some("identity".into()),
            transfer_encoding: None,
            elapsed_ms: 12_345,
            chunk_count: 7,
            byte_count: 4096,
            sse_line_count: 12,
            saw_done: false,
            visible_partial: true,
            error: StreamReadErrorDiagnostic {
                is_timeout: false,
                is_body: true,
                is_connect: false,
                is_decode: true,
                source_chain: vec![
                    "body error".into(),
                    "secret sk-test-123456789012 request body should not appear".into(),
                ],
            },
        };

        let value = diagnostic.to_safe_json();
        let rendered = value.to_string();

        assert_eq!(value["provider"], "deepseek");
        assert_eq!(value["model"], "deepseek-v4-pro");
        assert_eq!(
            value["endpoint_family"],
            "openai_compatible_chat_completions"
        );
        assert_eq!(value["http_version"], "HTTP/2.0");
        assert_eq!(value["status"], 200);
        assert_eq!(value["content_encoding"], "identity");
        assert_eq!(value["chunk_count"], 7);
        assert_eq!(value["byte_count"], 4096);
        assert_eq!(value["sse_line_count"], 12);
        assert_eq!(value["saw_done"], false);
        assert_eq!(value["visible_partial"], true);
        assert_eq!(value["error"]["is_body"], true);
        assert_eq!(value["error"]["is_decode"], true);
        assert_eq!(value["error"]["source_chain"][0]["detail"], "body error");
        assert!(rendered.contains("stream_body_read_failed"));
        assert!(!rendered.contains("sk-test"));
        assert!(!rendered.contains("request body"));
    }

    #[test]
    fn visible_stream_sanitizer_holds_split_think_tag_until_safe_text() {
        let mut sanitizer = VisibleStreamSanitizer::new();

        assert_eq!(
            sanitizer.sanitize_delta("答复<thi", false).as_test_delta(),
            "答复"
        );
        assert_eq!(
            sanitizer.sanitize_delta("nk>hidden", false).as_test_delta(),
            ""
        );
        assert_eq!(
            sanitizer
                .sanitize_delta("</think>正文开始", false)
                .as_test_delta(),
            "正文开始"
        );
        assert_eq!(sanitizer.finish().as_test_delta(), "");
    }

    #[test]
    fn visible_stream_sanitizer_suppresses_unclosed_reasoning_tail() {
        let mut sanitizer = VisibleStreamSanitizer::new();

        assert_eq!(
            sanitizer
                .sanitize_delta("可以先看结论。", false)
                .as_test_delta(),
            "可以先看结论。"
        );
        assert_eq!(
            sanitizer
                .sanitize_delta("<reasoning>internal", false)
                .as_test_delta(),
            ""
        );
        assert_eq!(sanitizer.finish().as_test_delta(), "");
    }

    #[test]
    fn visible_stream_sanitizer_never_releases_a_long_meta_analysis_prefix() {
        let mut sanitizer = VisibleStreamSanitizer::new();
        let first_meta_paragraph = format!(
            "The user is asking for current sports information. {}",
            "I should inspect the system instructions before answering. ".repeat(12)
        );
        assert!(first_meta_paragraph.chars().count() > 500);

        assert_eq!(
            sanitizer
                .sanitize_delta(&first_meta_paragraph, false)
                .as_test_delta(),
            ""
        );
        assert_eq!(
            sanitizer
                .sanitize_delta(
                    "\n\nThe system prompt requires verified evidence before a final response.",
                    false,
                )
                .as_test_delta(),
            ""
        );
        assert_eq!(
            sanitizer
                .sanitize_delta("\n\n这是基于联网证据的最终答复。", false)
                .as_test_delta(),
            "这是基于联网证据的最终答复。"
        );
        assert_eq!(sanitizer.finish().as_test_delta(), "");
    }

    #[test]
    fn visible_stream_sanitizer_preserves_normal_answers_with_common_openers() {
        let mut sanitizer = VisibleStreamSanitizer::new();

        assert_eq!(
            sanitizer
                .sanitize_delta(
                    "Given sufficient context, the answer can be concise.",
                    false
                )
                .as_test_delta(),
            "Given sufficient context, the answer can be concise."
        );
        assert_eq!(sanitizer.finish().as_test_delta(), "");
    }

    #[test]
    fn sanitized_surface_is_visible_to_the_frontend() {
        assert!(StreamSurface::VisibleAnswerSanitized.is_visible());
        assert_eq!(
            StreamSurface::VisibleAnswerSanitized.wire(),
            "visible_answer"
        );
        assert_eq!(
            StreamSurface::VisibleAnswerSanitized.candidate_kind(),
            "visible_answer_candidate"
        );
    }
}
