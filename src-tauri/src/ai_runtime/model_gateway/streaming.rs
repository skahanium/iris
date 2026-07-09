use reqwest::{
    header::{ACCEPT, ACCEPT_ENCODING, CONTENT_ENCODING, TRANSFER_ENCODING},
    Client,
};
use serde::{Deserialize, Serialize};
use std::error::Error as StdError;
use std::time::Instant;
use tauri::{AppHandle, Emitter};

use crate::ai_types::{EndpointFamily, FunctionCall, TokenUsage, ToolCall};
use crate::error::{AppError, AppResult};

use super::{
    abort_impl::{clear_abort, is_abort_requested},
    body_impl::{build_llm_api_body, GatewayRequest},
    http_backend_impl::format_llm_http_error,
    usage_impl::parse_usage,
    GatewayResponse,
};

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

    fn sanitizes_visible_output(self) -> bool {
        matches!(self, StreamSurface::VisibleAnswerSanitized)
    }
}

/// Stream event data payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StreamEventData {
    Token { token: String },
    ToolCall { tool_call: ToolCall },
    Done { usage: Option<TokenUsage> },
    Error { message: String, final_error: bool },
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
            builder.header("Authorization", format!("Bearer {api_key}"))
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

impl VisibleStreamSanitizer {
    fn new() -> Self {
        Self::default()
    }

    fn sanitize_delta(&mut self, delta: &str, done: bool) -> String {
        self.raw.push_str(delta);
        let next_visible = sanitize_visible_stream_prefix(&self.raw, done);
        if !next_visible.starts_with(&self.emitted) {
            tracing::warn!(
                emitted_len = self.emitted.len(),
                next_len = next_visible.len(),
                "visible stream sanitizer refused to retract emitted text"
            );
            return String::new();
        }
        let delta = next_visible[self.emitted.len()..].to_string();
        self.emitted = next_visible;
        delta
    }

    fn finish(&mut self) -> String {
        let next_visible = sanitize_visible_stream_prefix(&self.raw, true);
        if !next_visible.starts_with(&self.emitted) {
            tracing::warn!(
                emitted_len = self.emitted.len(),
                next_len = next_visible.len(),
                "visible stream sanitizer refused to retract emitted text on finish"
            );
            return String::new();
        }
        let delta = next_visible[self.emitted.len()..].to_string();
        self.emitted = next_visible;
        delta
    }
}

fn sanitize_visible_stream_prefix(raw: &str, done: bool) -> String {
    let without_reasoning = strip_reasoning_tags_for_stream(raw);
    let without_meta = sanitize_meta_analysis_prefix_for_stream(&without_reasoning, done);
    if done {
        without_meta
    } else {
        withhold_partial_reasoning_open_suffix(&without_meta)
    }
}

fn sanitize_meta_analysis_prefix_for_stream(text: &str, done: bool) -> String {
    let trimmed = text.trim_start();
    if !done && looks_like_partial_meta_analysis_start(trimmed) {
        return String::new();
    }
    crate::ai_runtime::harness_support::sanitize_meta_analysis_prefix(text)
}

fn looks_like_partial_meta_analysis_start(text: &str) -> bool {
    if text.trim().is_empty() {
        return false;
    }
    let lower = text.to_ascii_lowercase();
    const PREFIXES: [&str; 7] = [
        "the user ",
        "the user is ",
        "the current task ",
        "this is a ",
        "i should ",
        "i'll ",
        "the persona ",
    ];
    PREFIXES
        .iter()
        .any(|prefix| prefix.starts_with(lower.as_str()))
}

fn strip_reasoning_tags_for_stream(content: &str) -> String {
    let mut visible = String::new();
    let mut cursor = 0usize;
    while let Some(open) = find_next_reasoning_open_for_stream(content, cursor) {
        visible.push_str(&content[cursor..open.start]);
        let body_start = open.start + open.open_len;
        if let Some(close_start) =
            find_ascii_case_insensitive_for_stream(content, open.close_tag, body_start)
        {
            cursor = close_start + open.close_tag.len();
        } else {
            cursor = content.len();
            break;
        }
    }
    visible.push_str(&content[cursor..]);
    visible
}

#[derive(Debug, Clone, Copy)]
struct ReasoningOpenForStream {
    start: usize,
    open_len: usize,
    close_tag: &'static str,
}

fn find_next_reasoning_open_for_stream(
    content: &str,
    from: usize,
) -> Option<ReasoningOpenForStream> {
    const TAGS: [(&str, &str); 3] = [
        ("<thinking>", "</thinking>"),
        ("<think>", "</think>"),
        ("<reasoning>", "</reasoning>"),
    ];
    let mut best: Option<ReasoningOpenForStream> = None;
    for (open, close) in TAGS {
        if let Some(start) = find_ascii_case_insensitive_for_stream(content, open, from) {
            if best.map_or(true, |candidate| start < candidate.start) {
                best = Some(ReasoningOpenForStream {
                    start,
                    open_len: open.len(),
                    close_tag: close,
                });
            }
        }
    }
    best
}

fn find_ascii_case_insensitive_for_stream(
    haystack: &str,
    needle: &str,
    from: usize,
) -> Option<usize> {
    let bytes = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || bytes.len() < needle.len() || from > bytes.len() - needle.len() {
        return None;
    }
    (from..=bytes.len() - needle.len())
        .find(|&idx| bytes[idx..idx + needle.len()].eq_ignore_ascii_case(needle))
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
    app_handle: &AppHandle,
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
        if let Err(err) = emit_stream_event(app_handle, &event, token_index) {
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
        AppError::msg(sanitized)
    }
}

fn visible_token_delta(
    visible_sanitizer: &mut Option<VisibleStreamSanitizer>,
    delta: &str,
) -> String {
    if let Some(sanitizer) = visible_sanitizer.as_mut() {
        sanitizer.sanitize_delta(delta, false)
    } else {
        delta.to_string()
    }
}

fn visible_token_finish(visible_sanitizer: &mut Option<VisibleStreamSanitizer>) -> String {
    if let Some(sanitizer) = visible_sanitizer.as_mut() {
        sanitizer.finish()
    } else {
        String::new()
    }
}

fn emit_visible_token_delta(
    app_handle: &AppHandle,
    request_id: &str,
    token: String,
    surface: StreamSurface,
    classified: bool,
    token_index: &mut u32,
) -> AppResult<()> {
    if token.is_empty() {
        return Ok(());
    }
    let event = StreamEvent {
        request_id: request_id.to_string(),
        event_type: StreamEventType::Token,
        data: StreamEventData::Token { token },
        surface,
        classified,
    };
    emit_stream_event(app_handle, &event, *token_index)?;
    *token_index += 1;
    Ok(())
}

/// Send a streaming request and emit events to frontend.
pub async fn send_streaming_request(
    app_handle: &AppHandle,
    _client: &Client,
    request_id: &str,
    request: GatewayRequest,
) -> AppResult<GatewayResponse> {
    send_streaming_request_with_meta(
        app_handle,
        _client,
        request_id,
        request,
        false,
        StreamSurface::VisibleAnswer,
    )
    .await
}

/// Send a streaming request with an explicit surface for lifecycle-safe UI routing.
pub async fn send_streaming_request_with_surface(
    app_handle: &AppHandle,
    _client: &Client,
    request_id: &str,
    request: GatewayRequest,
    surface: StreamSurface,
    emit_error_event: bool,
) -> AppResult<GatewayResponse> {
    send_streaming_request_with_meta_error_mode(
        app_handle,
        _client,
        request_id,
        request,
        false,
        surface,
        emit_error_event,
    )
    .await
}

/// Send a streaming request and attach domain metadata to emitted events.
pub async fn send_streaming_request_with_meta(
    app_handle: &AppHandle,
    _client: &Client,
    request_id: &str,
    request: GatewayRequest,
    classified: bool,
    surface: StreamSurface,
) -> AppResult<GatewayResponse> {
    send_streaming_request_with_meta_error_mode(
        app_handle, _client, request_id, request, classified, surface, true,
    )
    .await
}

/// Send a streaming request and optionally suppress terminal error events.
pub async fn send_streaming_request_with_meta_error_mode(
    app_handle: &AppHandle,
    _client: &Client,
    request_id: &str,
    request: GatewayRequest,
    classified: bool,
    surface: StreamSurface,
    emit_error_event: bool,
) -> AppResult<GatewayResponse> {
    if is_abort_requested(request_id) {
        return Err(finish_stream_with_error(
            app_handle,
            request_id,
            "request aborted",
            classified,
            surface,
            0,
            true,
        ));
    }

    let endpoint_family = request.provider.endpoint_family;
    let url = streaming_endpoint_url(request.provider.base_url.as_str(), endpoint_family);
    let request_started_at = Instant::now();

    let mut body = build_llm_api_body(&request).map_err(|e| {
        finish_stream_with_error(
            app_handle,
            request_id,
            e.to_string(),
            classified,
            surface,
            0,
            emit_error_event,
        )
    })?;
    body["stream"] = serde_json::json!(true);

    let streaming_client =
        crate::network::cert_pinning::create_streaming_https_client().map_err(|e| {
            finish_stream_with_error(
                app_handle,
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

    let response = req_builder.json(&body).send().await.map_err(|e| {
        finish_stream_with_error(
            app_handle,
            request_id,
            format!("LLM streaming request failed: {e}"),
            classified,
            surface,
            0,
            emit_error_event,
        )
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(finish_stream_with_error(
            app_handle,
            request_id,
            format_llm_http_error(status, &text),
            classified,
            surface,
            0,
            emit_error_event,
        ));
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
    const ABORT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

    'stream: loop {
        // Race the next stream chunk against a periodic abort poll so a stalled
        // socket (no incoming chunks) can still be interrupted by the user
        // within ~500ms, rather than waiting for reqwest's read_timeout.
        let chunk_opt = match tokio::time::timeout(ABORT_POLL_INTERVAL, stream.next()).await {
            Ok(chunk) => chunk,
            Err(_) => {
                if is_abort_requested(request_id) {
                    return Err(finish_stream_with_error(
                        app_handle,
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
                app_handle,
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
                app_handle,
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
                let token = visible_token_finish(&mut visible_sanitizer);
                emit_visible_token_delta(
                    app_handle,
                    request_id,
                    token,
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
                emit_stream_event(app_handle, &event, token_index)?;
                // The stream is finished; stop reading from the socket. Some
                // providers/proxies keep the connection open after [DONE];
                // `continue` here would wait for the server to close (or the
                // 60s timeout), surfacing as a hang.
                break 'stream;
            }

            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                if endpoint_family == EndpointFamily::AnthropicMessages {
                    if let Some(delta) = anthropic_state.apply_event_json(&json).map_err(|e| {
                        finish_stream_with_error(
                            app_handle,
                            request_id,
                            e.to_string(),
                            classified,
                            surface,
                            token_index,
                            emit_error_event,
                        )
                    })? {
                        let token = visible_token_delta(&mut visible_sanitizer, &delta);
                        emit_visible_token_delta(
                            app_handle,
                            request_id,
                            token,
                            surface,
                            classified,
                            &mut token_index,
                        )?;
                    }
                    if json["type"].as_str() == Some("message_stop") {
                        let token = visible_token_finish(&mut visible_sanitizer);
                        emit_visible_token_delta(
                            app_handle,
                            request_id,
                            token,
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
                        emit_stream_event(app_handle, &event, token_index)?;
                        // Anthropic's terminal event; stop reading the socket
                        // so a half-open connection cannot hang the loop.
                        break 'stream;
                    }
                    continue;
                }

                // Process content delta
                if let Some(delta) = json["choices"][0]["delta"]["content"].as_str() {
                    full_content.push_str(delta);
                    let token = visible_token_delta(&mut visible_sanitizer, delta);
                    emit_visible_token_delta(
                        app_handle,
                        request_id,
                        token,
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

    // Flush remaining carry buffer
    if !carry.trim().is_empty() {
        if let Some(pos) = carry.find("data: ") {
            let remainder = &carry[pos..];
            if let Some(data) = remainder.strip_prefix("data: ") {
                let data = data.trim();
                if data != "[DONE]" {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                        if endpoint_family == EndpointFamily::AnthropicMessages {
                            if let Some(delta) =
                                anthropic_state.apply_event_json(&json).map_err(|e| {
                                    finish_stream_with_error(
                                        app_handle,
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
                                let token = visible_token_delta(&mut visible_sanitizer, &delta);
                                emit_visible_token_delta(
                                    app_handle,
                                    request_id,
                                    token,
                                    surface,
                                    classified,
                                    &mut token_index,
                                )?;
                            }
                            if json["type"].as_str() == Some("message_stop") {
                                let token = visible_token_finish(&mut visible_sanitizer);
                                emit_visible_token_delta(
                                    app_handle,
                                    request_id,
                                    token,
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
                                emit_stream_event(app_handle, &event, token_index)?;
                            }
                            clear_abort(request_id);
                            return Ok(anthropic_state.into_gateway_response());
                        }

                        if let Some(delta) = json["choices"][0]["delta"]["content"].as_str() {
                            full_content.push_str(delta);
                            let token = visible_token_delta(&mut visible_sanitizer, delta);
                            emit_visible_token_delta(
                                app_handle,
                                request_id,
                                token,
                                surface,
                                classified,
                                &mut token_index,
                            )?;
                        }
                        if let Some(tc_deltas) =
                            json["choices"][0]["delta"]["tool_calls"].as_array()
                        {
                            for tc_delta in tc_deltas {
                                let idx = tc_delta["index"].as_u64().unwrap_or(0) as usize;
                                let entry = tool_call_deltas.entry(idx).or_insert((
                                    None,
                                    None,
                                    String::new(),
                                ));
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
                        if json.get("usage").is_some()
                            && json["usage"].get("prompt_tokens").is_some()
                        {
                            usage = parse_usage(&json);
                        }
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
            emit_stream_event(app_handle, &event, token_index)?;
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
        emit_stream_event(app_handle, &event, token_index)?;
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
    })
}

/// Emit a stream event to the frontend (`llm:*` 与 `engine.rs` / 侧栏监听一致).
pub(super) fn emit_stream_event(
    app_handle: &AppHandle,
    event: &StreamEvent,
    token_index: u32,
) -> AppResult<()> {
    let emit_err = |e: tauri::Error| AppError::msg(format!("Failed to emit stream event: {e}"));
    match event.event_type {
        StreamEventType::Token => {
            if let StreamEventData::Token { token } = &event.data {
                tracing::debug!(
                    request_id = %event.request_id,
                    event = "stream_token_emitted",
                    token_index,
                    content_len = token.len(),
                    content_hash = %lifecycle_content_hash(token),
                    surface = event.surface.wire(),
                    candidate_kind = event.surface.candidate_kind(),
                    classified = event.classified,
                    "AI lifecycle stream token emitted"
                );
                if !event.surface.is_visible() {
                    return Ok(());
                }
                let mut payload = serde_json::json!({
                    "request_id": event.request_id,
                    "token": token,
                    "index": token_index,
                    "surface": event.surface.wire(),
                    "candidate_kind": event.surface.candidate_kind(),
                });
                if event.classified {
                    payload["classified"] = serde_json::json!(true);
                }
                app_handle.emit("llm:token", payload).map_err(emit_err)?;
            }
        }
        StreamEventType::Done => {
            tracing::debug!(
                request_id = %event.request_id,
                event = "stream_done_emitted",
                token_index,
                surface = event.surface.wire(),
                candidate_kind = event.surface.candidate_kind(),
                classified = event.classified,
                "AI lifecycle stream done emitted"
            );
            if !event.surface.is_visible() {
                return Ok(());
            }
            let mut payload = serde_json::json!({
                "request_id": event.request_id,
                "surface": event.surface.wire(),
                "candidate_kind": event.surface.candidate_kind(),
            });
            if event.classified {
                payload["classified"] = serde_json::json!(true);
            }
            app_handle.emit("llm:done", payload).map_err(emit_err)?;
        }
        StreamEventType::Error => {
            let (message, final_error) = if let StreamEventData::Error {
                message,
                final_error,
            } = &event.data
            {
                (message.clone(), *final_error)
            } else {
                ("stream error".to_string(), true)
            };
            if !event.surface.is_visible() {
                return Ok(());
            }
            let mut payload = serde_json::json!({
                "request_id": event.request_id,
                "error": message,
                "final": final_error,
                "surface": event.surface.wire(),
                "candidate_kind": event.surface.candidate_kind(),
            });
            if event.classified {
                payload["classified"] = serde_json::json!(true);
            }
            app_handle.emit("llm:error", payload).map_err(emit_err)?;
        }
        StreamEventType::ToolCall => {
            app_handle.emit("ai:tool_call", event).map_err(emit_err)?;
        }
    }
    Ok(())
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

        assert_eq!(sanitizer.sanitize_delta("答复<thi", false), "答复");
        assert_eq!(sanitizer.sanitize_delta("nk>hidden", false), "");
        assert_eq!(
            sanitizer.sanitize_delta("</think>正文开始", false),
            "正文开始"
        );
        assert_eq!(sanitizer.finish(), "");
    }

    #[test]
    fn visible_stream_sanitizer_suppresses_unclosed_reasoning_tail() {
        let mut sanitizer = VisibleStreamSanitizer::new();

        assert_eq!(
            sanitizer.sanitize_delta("可以先看结论。", false),
            "可以先看结论。"
        );
        assert_eq!(sanitizer.sanitize_delta("<reasoning>internal", false), "");
        assert_eq!(sanitizer.finish(), "");
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
