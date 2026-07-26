//! Controlled MCP host runtime.
//!
//! This module owns MCP protocol execution. Registry modules store metadata;
//! this runtime performs bounded stdio handshakes and discovery.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, LazyLock, Mutex as StdMutex, Weak};
use std::time::{Duration, Instant};

use futures_util::{stream::BoxStream, StreamExt};
use rmcp::{
    model::{CallToolRequestParams, ClientInfo, ClientJsonRpcMessage, ServerJsonRpcMessage, Tool},
    service::RunningService,
    transport::{
        async_rw::AsyncRwTransport,
        streamable_http_client::{
            StreamableHttpClient, StreamableHttpClientTransportConfig, StreamableHttpError,
            StreamableHttpPostResponse,
        },
        StreamableHttpClientTransport, Transport,
    },
    RoleClient, ServiceExt,
};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::time::timeout;

use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

/// MCP protocol releases that Iris has explicitly exercised at the host
/// boundary. A peer's initialize response is accepted only when it negotiated
/// one of these releases; this is deliberately not a "latest" comparison.
pub const SUPPORTED_MCP_PROTOCOL_VERSIONS: [&str; 2] = ["2025-06-18", "2025-11-25"];

/// Return whether the protocol version negotiated during MCP initialization is
/// one of the releases supported by this host runtime.
pub fn is_supported_mcp_protocol_version(version: &str) -> bool {
    SUPPORTED_MCP_PROTOCOL_VERSIONS.contains(&version)
}

fn validate_negotiated_mcp_protocol_version(version: &str) -> AppResult<()> {
    if is_supported_mcp_protocol_version(version) {
        return Ok(());
    }

    Err(runtime_error(
        McpRuntimeFailureKind::InvalidResponse,
        "MCP server negotiated an unsupported protocol version",
    ))
}

fn validate_connected_mcp_protocol(
    client: &RunningService<RoleClient, ClientInfo>,
) -> AppResult<()> {
    let peer_info = client.peer_info().ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::InvalidResponse,
            "MCP server did not return initialize metadata",
        )
    })?;
    validate_negotiated_mcp_protocol_version(&peer_info.protocol_version.to_string())
}

#[derive(Debug, Clone)]
pub struct McpStdioLaunch {
    pub command: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub request_timeout: Duration,
    pub max_stdout_line_bytes: usize,
    pub max_stderr_bytes: usize,
}
#[derive(Clone)]
pub struct McpHttpLaunch {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub request_timeout: Duration,
    pub max_response_bytes: usize,
    pub allow_localhost_dev: bool,
}

impl std::fmt::Debug for McpHttpLaunch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let header_names = self
            .headers
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>();
        f.debug_struct("McpHttpLaunch")
            .field("url", &self.url)
            .field("headers", &header_names)
            .field("request_timeout", &self.request_timeout)
            .field("max_response_bytes", &self.max_response_bytes)
            .field("allow_localhost_dev", &self.allow_localhost_dev)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct McpHostRuntimeOptions {
    pub request_timeout: Duration,
    pub max_stdout_line_bytes: usize,
    pub max_stderr_bytes: usize,
    pub cwd: Option<PathBuf>,
    pub stdio_session_pool: bool,
    pub stdio_session_idle_timeout: Duration,
}

pub const DEFAULT_STDIO_SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(300);
/// Bound retained stdio children across providers. Active calls temporarily
/// remove their entry from this cache, so the cap only governs idle reuse.
pub const MAX_STDIO_SESSION_POOL_SIZE: usize = 8;

/// Reject an over-limit newline-delimited JSON-RPC frame before the downstream
/// transport can retain or deserialize it.
struct CappedFrameReader<R> {
    inner: R,
    max_frame_bytes: usize,
    frame_bytes: usize,
}

impl<R> CappedFrameReader<R> {
    fn new(inner: R, max_frame_bytes: usize) -> Self {
        Self {
            inner,
            max_frame_bytes,
            frame_bytes: 0,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for CappedFrameReader<R> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let remaining = buf.remaining();
        if remaining == 0 {
            return std::task::Poll::Ready(Ok(()));
        }
        // Read at most one byte beyond the current frame limit. The byte is
        // staged in `limited` and committed to `buf` only after validation;
        // therefore an over-limit frame is never observable by the JSON-RPC
        // decoder that sits behind this transport.
        let read_limit = remaining.min(
            self.max_frame_bytes
                .saturating_sub(self.frame_bytes)
                .saturating_add(1),
        );
        let initialized = buf.initialize_unfilled_to(read_limit);
        let mut limited = ReadBuf::new(&mut initialized[..read_limit]);
        match std::pin::Pin::new(&mut self.inner).poll_read(cx, &mut limited) {
            std::task::Poll::Ready(Ok(())) => {
                let bytes_read = limited.filled().len();
                for byte in limited.filled() {
                    if *byte == b'\n' {
                        self.frame_bytes = 0;
                    } else {
                        self.frame_bytes = self.frame_bytes.saturating_add(1);
                        if self.frame_bytes > self.max_frame_bytes {
                            return std::task::Poll::Ready(Err(std::io::Error::other(
                                "MCP stdout frame exceeds configured cap",
                            )));
                        }
                    }
                }
                buf.advance(bytes_read);
                std::task::Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

struct BoundedStdioTransport {
    child: Child,
    transport: AsyncRwTransport<RoleClient, CappedFrameReader<ChildStdout>, ChildStdin>,
}

impl Transport<RoleClient> for BoundedStdioTransport {
    type Error = std::io::Error;
    fn send(
        &mut self,
        item: rmcp::service::TxJsonRpcMessage<RoleClient>,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send + 'static {
        self.transport.send(item)
    }
    fn receive(
        &mut self,
    ) -> impl std::future::Future<Output = Option<rmcp::service::RxJsonRpcMessage<RoleClient>>> + Send
    {
        self.transport.receive()
    }
    fn close(&mut self) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        let result = self.child.start_kill();
        async move { result }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpRuntimeFailureKind {
    Unavailable,
    ToolNotFound,
    SchemaMismatch,
    Timeout,
    OutputTooLarge,
    AuthMissing,
    AuthFailed,
    NetworkDenied,
    PolicyDenied,
    InvalidResponse,
}

impl McpRuntimeFailureKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::ToolNotFound => "tool_not_found",
            Self::SchemaMismatch => "schema_mismatch",
            Self::Timeout => "timeout",
            Self::OutputTooLarge => "output_too_large",
            Self::AuthMissing => "auth_missing",
            Self::AuthFailed => "auth_failed",
            Self::NetworkDenied => "network_denied",
            Self::PolicyDenied => "policy_denied",
            Self::InvalidResponse => "invalid_response",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolDefinition {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
    pub output_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpStdioDiscovery {
    pub protocol_version: String,
    pub server_name: String,
    pub server_version: Option<String>,
    pub tools: Vec<McpToolDefinition>,
    pub stderr_summary: Option<String>,
}

/// One result produced by this module's bounded stdio discovery boundary.
///
/// The proof is intentionally private to this module: callers may inspect the
/// discovery data, but cannot manufacture an attested transport result from a
/// deserialized `McpStdioDiscovery`.
#[cfg(test)]
pub(crate) struct McpStdioTransportProbe {
    discovery: Option<McpStdioDiscovery>,
    failure: Option<McpRuntimeFailureKind>,
    proof: Option<McpStdioTransportProof>,
}

#[derive(Debug)]
#[cfg(test)]
pub(crate) struct McpStdioTransportProof(());

#[cfg(test)]
impl McpStdioTransportProbe {
    pub(crate) fn discovery(&self) -> Option<&McpStdioDiscovery> {
        self.discovery.as_ref()
    }

    pub(crate) fn into_discovery(
        self,
    ) -> Result<(McpStdioDiscovery, McpStdioTransportProof), McpRuntimeFailureKind> {
        match (self.discovery, self.failure, self.proof) {
            (Some(discovery), _, Some(proof)) => Ok((discovery, proof)),
            (_, Some(failure), _) => Err(failure),
            _ => Err(McpRuntimeFailureKind::InvalidResponse),
        }
    }

    pub(crate) fn into_failure(
        self,
    ) -> Result<(McpRuntimeFailureKind, Option<McpStdioTransportProof>), McpStdioDiscovery> {
        match (self.discovery, self.failure, self.proof) {
            (_, Some(failure), proof) => Ok((failure, proof)),
            (Some(discovery), _, _) => Err(discovery),
            _ => unreachable!("probe always has one outcome"),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolCallResult {
    pub provider_id: String,
    pub tool_name: String,
    pub result: serde_json::Value,
    pub stderr_summary: Option<String>,
}

#[derive(Clone)]
struct McpStdioToolCallLaunch {
    command: PathBuf,
    args: Vec<String>,
    env: Vec<(String, String)>,
    cwd: Option<PathBuf>,
    request_timeout: Duration,
    max_stdout_line_bytes: usize,
    max_stderr_bytes: usize,
    tool_name: String,
    arguments: serde_json::Value,
}

fn runtime_error(kind: McpRuntimeFailureKind, message: impl Into<String>) -> AppError {
    AppError::msg(format!("{}: {}", kind.as_str(), message.into()))
}

fn rmcp_client_info() -> ClientInfo {
    let mut client_info = ClientInfo::default();
    client_info.client_info.name = "iris".to_string();
    client_info.client_info.version = env!("CARGO_PKG_VERSION").to_string();
    client_info
}

fn mcp_tool_definition_from_rmcp(tool: Tool) -> McpToolDefinition {
    let input_schema = tool.schema_as_json_value();
    let output_schema = tool
        .output_schema
        .as_ref()
        .map(|schema| serde_json::Value::Object((**schema).clone()));
    McpToolDefinition {
        name: tool.name.to_string(),
        title: tool.title,
        description: tool.description.map(|value| value.to_string()),
        input_schema,
        output_schema,
    }
}

fn rmcp_headers(
    headers: &[(String, String)],
) -> AppResult<std::collections::HashMap<http::HeaderName, http::HeaderValue>> {
    headers
        .iter()
        .map(|(name, value)| {
            let name = http::HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                runtime_error(
                    McpRuntimeFailureKind::PolicyDenied,
                    "MCP provider configured an invalid HTTP header name",
                )
            })?;
            if matches!(
                name.as_str(),
                "accept"
                    | "content-type"
                    | "mcp-session-id"
                    | "mcp-protocol-version"
                    | "last-event-id"
            ) {
                return Err(runtime_error(
                    McpRuntimeFailureKind::PolicyDenied,
                    "MCP provider may not override protocol-managed HTTP headers",
                ));
            }
            let value = http::HeaderValue::from_str(value).map_err(|_| {
                runtime_error(
                    McpRuntimeFailureKind::PolicyDenied,
                    "MCP provider configured an invalid HTTP header value",
                )
            })?;
            Ok((name, value))
        })
        .collect()
}

fn rmcp_client_error(error: impl std::fmt::Display) -> AppError {
    // Do not surface SDK transport strings: a remote error may echo credentials
    // or provider content. The typed runtime boundary records only safe codes.
    // The two locally generated cap markers are the sole exception: retaining
    // their category is needed so the broker can apply deterministic overload
    // handling without exposing a remote payload.
    let message = error.to_string();
    if message.contains("MCP stdout frame exceeds configured cap")
        || message.contains("MCP HTTP response exceeds configured cap")
    {
        return runtime_error(
            McpRuntimeFailureKind::OutputTooLarge,
            "MCP response exceeded configured cap",
        );
    }
    runtime_error(
        McpRuntimeFailureKind::Unavailable,
        "official MCP client request failed",
    )
}

fn rmcp_tool_call_arguments(
    arguments: serde_json::Value,
) -> AppResult<serde_json::Map<String, serde_json::Value>> {
    arguments.as_object().cloned().ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::SchemaMismatch,
            "MCP tool arguments must be a JSON object",
        )
    })
}
fn http_host_is_localhost_or_loopback(host: &str) -> bool {
    let host = http_host_without_ipv6_brackets(host);
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

/// `Url::host_str()` preserves brackets around IPv6 literals. Normalize that
/// presentation detail before applying the IP safety policy.
fn http_host_without_ipv6_brackets(host: &str) -> &str {
    host.strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host)
}

fn ip_is_private_or_metadata(ip: IpAddr) -> bool {
    let ipv4_is_private_or_metadata = |ip: std::net::Ipv4Addr| {
        ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified()
    };
    match ip {
        IpAddr::V4(ip) => ipv4_is_private_or_metadata(ip),
        IpAddr::V6(ip) => {
            // `::ffff:127.0.0.1` and friends are IPv4 endpoints encoded as
            // IPv6 literals. Treat their embedded IPv4 address under exactly
            // the same deny rules; otherwise they bypass the loopback/private
            // host guard before the MCP client opens a connection.
            if let Some(ipv4) = ip.to_ipv4() {
                return ipv4_is_private_or_metadata(ipv4);
            }
            let first_segment = ip.segments()[0];
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unicast_link_local()
                || (first_segment & 0xfe00) == 0xfc00
        }
    }
}

fn http_host_is_private_or_metadata(host: &str) -> bool {
    let host = http_host_without_ipv6_brackets(host);
    host.eq_ignore_ascii_case("localhost")
        || host == "169.254.169.254"
        || host.eq_ignore_ascii_case("metadata.google.internal")
        || host
            .parse::<IpAddr>()
            .map(ip_is_private_or_metadata)
            .unwrap_or(false)
}

fn http_url_contains_secret(parsed: &reqwest::Url) -> bool {
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return true;
    }
    parsed.query_pairs().any(|(key, value)| {
        let key = key.to_ascii_lowercase();
        let value = value.to_ascii_lowercase();
        [
            "api_key",
            "apikey",
            "access_token",
            "token",
            "secret",
            "password",
            "bearer",
        ]
        .iter()
        .any(|marker| key.contains(marker) || value.contains(marker))
    })
}

fn validate_mcp_http_runtime_url(url: &str, allow_localhost_dev: bool) -> AppResult<reqwest::Url> {
    let parsed = reqwest::Url::parse(url).map_err(|err| {
        runtime_error(
            McpRuntimeFailureKind::NetworkDenied,
            format!("invalid MCP HTTP URL: {err}"),
        )
    })?;
    let host = parsed.host_str().ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::NetworkDenied,
            "MCP HTTP URL must include a host",
        )
    })?;
    if http_url_contains_secret(&parsed) {
        return Err(runtime_error(
            McpRuntimeFailureKind::NetworkDenied,
            "MCP HTTP URL must not contain secret material",
        ));
    }
    if parsed.scheme() == "https" {
        if http_host_is_private_or_metadata(host)
            && !(allow_localhost_dev && http_host_is_localhost_or_loopback(host))
        {
            return Err(runtime_error(
                McpRuntimeFailureKind::NetworkDenied,
                "MCP HTTPS URL may not target private, loopback, or metadata hosts outside dev mode",
            ));
        }
        return Ok(parsed);
    }
    if parsed.scheme() == "http" && allow_localhost_dev && http_host_is_localhost_or_loopback(host)
    {
        return Ok(parsed);
    }
    Err(runtime_error(
        McpRuntimeFailureKind::NetworkDenied,
        "MCP HTTP transport requires HTTPS unless localhost dev mode is explicitly enabled",
    ))
}

/// The public RMCP HTTP-client seam lets Iris apply one response-size policy
/// to both JSON and SSE replies without forking the MCP protocol transport.
/// A server-declared oversized response is rejected before its body is read;
/// chunked bodies are counted before every chunk reaches the SSE or JSON
/// decoder.
#[derive(Debug, thiserror::Error)]
enum BoundedMcpHttpClientError {
    #[error("MCP HTTP request failed")]
    Request(#[from] reqwest::Error),
    #[error("MCP HTTP response exceeds configured cap")]
    ResponseTooLarge,
}

#[derive(Clone)]
struct BoundedMcpHttpClient {
    client: reqwest::Client,
    max_response_bytes: usize,
}

impl BoundedMcpHttpClient {
    fn reject_declared_oversize(
        &self,
        response: &reqwest::Response,
    ) -> Result<(), BoundedMcpHttpClientError> {
        if response
            .content_length()
            .is_some_and(|length| length > self.max_response_bytes as u64)
        {
            return Err(BoundedMcpHttpClientError::ResponseTooLarge);
        }
        Ok(())
    }

    async fn read_body_under_cap(
        &self,
        response: reqwest::Response,
    ) -> Result<Vec<u8>, BoundedMcpHttpClientError> {
        self.reject_declared_oversize(&response)?;
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if chunk.len() > self.max_response_bytes.saturating_sub(body.len()) {
                return Err(BoundedMcpHttpClientError::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }

    fn capped_sse_stream(
        &self,
        response: reqwest::Response,
    ) -> Result<
        BoxStream<'static, Result<sse_stream::Sse, sse_stream::Error>>,
        BoundedMcpHttpClientError,
    > {
        self.reject_declared_oversize(&response)?;
        let max_response_bytes = self.max_response_bytes;
        let mut received_bytes = 0_usize;
        let stream = response.bytes_stream().map(move |chunk| {
            let chunk = chunk.map_err(BoundedMcpHttpClientError::from)?;
            if chunk.len() > max_response_bytes.saturating_sub(received_bytes) {
                return Err(BoundedMcpHttpClientError::ResponseTooLarge);
            }
            received_bytes += chunk.len();
            Ok(chunk)
        });
        Ok(sse_stream::SseStream::from_bytes_stream(stream).boxed())
    }

    fn apply_headers(
        mut request: reqwest::RequestBuilder,
        auth_header: Option<String>,
        custom_headers: std::collections::HashMap<http::HeaderName, http::HeaderValue>,
    ) -> reqwest::RequestBuilder {
        if let Some(auth_header) = auth_header {
            request = request.bearer_auth(auth_header);
        }
        for (name, value) in custom_headers {
            request = request.header(name, value);
        }
        request
    }

    fn response_content_type(response: &reqwest::Response) -> Option<String> {
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .map(|value| String::from_utf8_lossy(value.as_bytes()).to_string())
    }
}

impl StreamableHttpClient for BoundedMcpHttpClient {
    type Error = BoundedMcpHttpClientError;

    async fn get_stream(
        &self,
        uri: std::sync::Arc<str>,
        session_id: std::sync::Arc<str>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        custom_headers: std::collections::HashMap<http::HeaderName, http::HeaderValue>,
    ) -> Result<
        BoxStream<'static, Result<sse_stream::Sse, sse_stream::Error>>,
        StreamableHttpError<Self::Error>,
    > {
        let mut request = self
            .client
            .get(uri.as_ref())
            .header(
                reqwest::header::ACCEPT,
                "text/event-stream, application/json",
            )
            .header("Mcp-Session-Id", session_id.as_ref());
        if let Some(last_event_id) = last_event_id {
            request = request.header("Last-Event-Id", last_event_id);
        }
        let response = Self::apply_headers(request, auth_header, custom_headers)
            .send()
            .await
            .map_err(|error| StreamableHttpError::Client(error.into()))?
            .error_for_status()
            .map_err(|error| StreamableHttpError::Client(error.into()))?;
        let content_type = Self::response_content_type(&response);
        match content_type.as_deref() {
            Some(value)
                if value.starts_with("text/event-stream")
                    || value.starts_with("application/json") =>
            {
                self.capped_sse_stream(response)
                    .map_err(StreamableHttpError::Client)
            }
            _ => Err(StreamableHttpError::UnexpectedContentType(content_type)),
        }
    }

    async fn delete_session(
        &self,
        uri: std::sync::Arc<str>,
        session_id: std::sync::Arc<str>,
        auth_header: Option<String>,
        custom_headers: std::collections::HashMap<http::HeaderName, http::HeaderValue>,
    ) -> Result<(), StreamableHttpError<Self::Error>> {
        let response = Self::apply_headers(
            self.client
                .delete(uri.as_ref())
                .header("Mcp-Session-Id", session_id.as_ref()),
            auth_header,
            custom_headers,
        )
        .send()
        .await
        .map_err(|error| StreamableHttpError::Client(error.into()))?;
        if response.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED {
            return Ok(());
        }
        response
            .error_for_status()
            .map_err(|error| StreamableHttpError::Client(error.into()))?;
        Ok(())
    }

    async fn post_message(
        &self,
        uri: std::sync::Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<std::sync::Arc<str>>,
        auth_header: Option<String>,
        custom_headers: std::collections::HashMap<http::HeaderName, http::HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        let session_was_attached = session_id.is_some();
        let mut request = self.client.post(uri.as_ref()).header(
            reqwest::header::ACCEPT,
            "text/event-stream, application/json",
        );
        if let Some(session_id) = session_id {
            request = request.header("Mcp-Session-Id", session_id.as_ref());
        }
        let response = Self::apply_headers(request, auth_header, custom_headers)
            .json(&message)
            .send()
            .await
            .map_err(|error| StreamableHttpError::Client(error.into()))?;
        let status = response.status();
        if matches!(
            status,
            reqwest::StatusCode::ACCEPTED | reqwest::StatusCode::NO_CONTENT
        ) {
            return Ok(StreamableHttpPostResponse::Accepted);
        }
        if status == reqwest::StatusCode::NOT_FOUND && session_was_attached {
            return Err(StreamableHttpError::SessionExpired);
        }
        let content_type = Self::response_content_type(&response);
        let session_id = response
            .headers()
            .get("Mcp-Session-Id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        if !status.is_success() {
            let body = self
                .read_body_under_cap(response)
                .await
                .map_err(StreamableHttpError::Client)?;
            if content_type
                .as_deref()
                .is_some_and(|value| value.starts_with("application/json"))
            {
                if let Ok(message @ ServerJsonRpcMessage::Error(_)) = serde_json::from_slice(&body)
                {
                    return Ok(StreamableHttpPostResponse::Json(message, session_id));
                }
            }
            return Err(StreamableHttpError::UnexpectedServerResponse(
                "MCP HTTP server rejected the request".into(),
            ));
        }
        match content_type.as_deref() {
            Some(value) if value.starts_with("text/event-stream") => self
                .capped_sse_stream(response)
                .map(|stream| StreamableHttpPostResponse::Sse(stream, session_id))
                .map_err(StreamableHttpError::Client),
            Some(value) if value.starts_with("application/json") => {
                let body = self
                    .read_body_under_cap(response)
                    .await
                    .map_err(StreamableHttpError::Client)?;
                match serde_json::from_slice::<ServerJsonRpcMessage>(&body) {
                    Ok(message) => Ok(StreamableHttpPostResponse::Json(message, session_id)),
                    Err(_) => Ok(StreamableHttpPostResponse::Accepted),
                }
            }
            _ => Err(StreamableHttpError::UnexpectedContentType(content_type)),
        }
    }
}

/// Resolve a remote MCP hostname once, reject non-public targets, and pin the
/// resulting addresses into the reqwest client. This prevents a later DNS
/// rebinding lookup from turning an already-approved hostname into a private
/// or metadata endpoint. Redirects are disabled because each redirect target
/// would otherwise need the same validation and pinning treatment.
async fn pinned_mcp_http_client(launch: &McpHttpLaunch) -> AppResult<reqwest::Client> {
    let parsed = validate_mcp_http_runtime_url(&launch.url, launch.allow_localhost_dev)?;
    let host = parsed.host_str().ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::NetworkDenied,
            "MCP HTTP URL has no host",
        )
    })?;
    let port = parsed.port_or_known_default().ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::NetworkDenied,
            "MCP HTTP URL has no port",
        )
    })?;
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(launch.request_timeout)
        .timeout(launch.request_timeout);
    if host.parse::<IpAddr>().is_err() {
        let addresses = tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| {
                runtime_error(
                    McpRuntimeFailureKind::NetworkDenied,
                    "MCP DNS lookup failed",
                )
            })?
            .collect::<Vec<_>>();
        if addresses.is_empty()
            || addresses
                .iter()
                .any(|address| ip_is_private_or_metadata(address.ip()))
        {
            return Err(runtime_error(
                McpRuntimeFailureKind::NetworkDenied,
                "MCP DNS resolved to a denied network address",
            ));
        }
        builder = builder.resolve_to_addrs(host, &addresses);
    }
    builder.build().map_err(|_| {
        runtime_error(
            McpRuntimeFailureKind::Unavailable,
            "MCP HTTP client initialization failed",
        )
    })
}

async fn bounded_mcp_http_client(launch: &McpHttpLaunch) -> AppResult<BoundedMcpHttpClient> {
    if launch.max_response_bytes == 0 {
        return Err(runtime_error(
            McpRuntimeFailureKind::OutputTooLarge,
            "MCP HTTP response cap must be greater than zero",
        ));
    }
    Ok(BoundedMcpHttpClient {
        client: pinned_mcp_http_client(launch).await?,
        max_response_bytes: launch.max_response_bytes,
    })
}

fn ensure_json_value_under_cap(value: &serde_json::Value, max_bytes: usize) -> AppResult<()> {
    if max_bytes == 0 {
        return Err(runtime_error(
            McpRuntimeFailureKind::OutputTooLarge,
            "MCP HTTP response cap must be greater than zero",
        ));
    }
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > max_bytes {
        return Err(runtime_error(
            McpRuntimeFailureKind::OutputTooLarge,
            "MCP HTTP response exceeded configured cap",
        ));
    }
    Ok(())
}

fn config_string(config: &serde_json::Value, key: &str) -> Option<String> {
    config
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_stdio_args(args_json: &str) -> AppResult<Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(args_json).map_err(|err| {
        runtime_error(
            McpRuntimeFailureKind::InvalidResponse,
            format!("stored MCP stdio args are invalid JSON: {err}"),
        )
    })?;
    let items = value.as_array().ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::InvalidResponse,
            "stored MCP stdio args are not an array",
        )
    })?;
    items
        .iter()
        .map(|item| {
            item.as_str().map(str::to_string).ok_or_else(|| {
                runtime_error(
                    McpRuntimeFailureKind::InvalidResponse,
                    "stored MCP stdio args contain non-string values",
                )
            })
        })
        .collect()
}

struct StoredStdioProvider {
    command: PathBuf,
    args: Vec<String>,
    env: Vec<(String, String)>,
}

struct StoredRemoteProvider {
    url: String,
    headers: Vec<(String, String)>,
    allow_localhost_dev: bool,
}

fn load_provider_transport(db: &Database, provider_id: &str) -> AppResult<String> {
    db.with_read_conn(|conn| {
        let transport: String = conn.query_row(
            "SELECT transport_kind
             FROM web_evidence_providers
             WHERE id = ?1",
            [provider_id],
            |row| row.get(0),
        )?;
        Ok(transport.trim().to_ascii_lowercase())
    })
}

fn credential_service_from_binding(value: &serde_json::Value) -> AppResult<Option<String>> {
    let raw = if let Some(raw) = value.as_str() {
        raw.trim()
    } else if let Some(object) = value.as_object() {
        object
            .get("credential")
            .or_else(|| object.get("service"))
            .or_else(|| object.get("ref"))
            .and_then(|item| item.as_str())
            .map(str::trim)
            .unwrap_or_default()
    } else {
        ""
    };
    if raw.is_empty() {
        return Ok(None);
    }
    let service = raw.strip_prefix("credential://").unwrap_or(raw).trim();
    crate::security::ipc_policy::validate_credential_service(service)?;
    Ok(Some(service.to_string()))
}

fn credential_binding_optional(value: &serde_json::Value, service: &str) -> bool {
    value
        .as_object()
        .and_then(|object| object.get("optional"))
        .and_then(|item| item.as_bool())
        .unwrap_or_else(|| crate::config_manifest::is_mcp_optional_credential_service(service))
}

fn credential_missing_error(service: &str, configured: bool) -> AppError {
    if configured {
        runtime_error(
            McpRuntimeFailureKind::AuthMissing,
            format!("credential_unreadable: 系统凭据不可读取: {service}"),
        )
    } else {
        runtime_error(
            McpRuntimeFailureKind::AuthMissing,
            format!("MCP credential binding is missing: {service}"),
        )
    }
}

fn credential_available_for_binding(_db: &Database, service: &str) -> AppResult<bool> {
    crate::credentials::credential_available(service)
}

fn parse_json_object(
    raw: &str,
    failure_kind: McpRuntimeFailureKind,
) -> AppResult<serde_json::Value> {
    serde_json::from_str(raw).map_err(|err| {
        runtime_error(
            failure_kind,
            format!("MCP JSON configuration is invalid: {err}"),
        )
    })
}

fn object_section<'a>(
    value: &'a serde_json::Value,
    section: &str,
) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
    value.get(section).and_then(|item| item.as_object())
}

#[cfg(test)]
fn resolve_http_header_bindings_with_lookup<F>(
    credential_refs_json: &str,
    lookup_credential: F,
) -> AppResult<Vec<(String, String)>>
where
    F: FnMut(&str) -> AppResult<String>,
{
    resolve_http_header_bindings_with_lookup_and_config(
        credential_refs_json,
        lookup_credential,
        |_| Ok(false),
    )
}

fn resolve_http_header_bindings_with_lookup_and_config<F, C>(
    credential_refs_json: &str,
    mut lookup_credential: F,
    mut credential_available: C,
) -> AppResult<Vec<(String, String)>>
where
    F: FnMut(&str) -> AppResult<String>,
    C: FnMut(&str) -> AppResult<bool>,
{
    let value = parse_json_object(credential_refs_json, McpRuntimeFailureKind::AuthMissing)?;
    let Some(bindings) = object_section(&value, "headers") else {
        return Ok(Vec::new());
    };
    let mut headers = Vec::new();
    for (header_name, binding) in bindings {
        let service = credential_service_from_binding(binding)?.ok_or_else(|| {
            runtime_error(
                McpRuntimeFailureKind::AuthMissing,
                "MCP HTTP header binding omitted named credential service",
            )
        })?;
        let configured = credential_available(&service)?;
        let mut value = match lookup_credential(&service) {
            Ok(value) => value,
            Err(_) if credential_binding_optional(binding, &service) && !configured => continue,
            Err(_) => return Err(credential_missing_error(&service, configured)),
        };
        let scheme = binding
            .as_object()
            .and_then(|object| object.get("scheme"))
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|item| !item.is_empty());
        if let Some(scheme) = scheme {
            if scheme.eq_ignore_ascii_case("bearer") {
                let raw_key = value.trim();
                if raw_key
                    .get(..7)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bearer "))
                {
                    return Err(runtime_error(
                        McpRuntimeFailureKind::AuthFailed,
                        "MCP Bearer credential must contain the raw key only",
                    ));
                }
                value = format!("Bearer {raw_key}");
            } else {
                value = format!("{scheme} {value}");
            }
        }
        headers.push((header_name.clone(), value));
    }
    Ok(headers)
}

fn resolve_http_header_bindings(
    db: &Database,
    credential_refs_json: &str,
) -> AppResult<Vec<(String, String)>> {
    resolve_http_header_bindings_with_lookup_and_config(
        credential_refs_json,
        |service| Ok(crate::credentials::get_runtime_secret(service)?.to_string()),
        |service| credential_available_for_binding(db, service),
    )
}

fn load_remote_provider(db: &Database, provider_id: &str) -> AppResult<StoredRemoteProvider> {
    db.with_read_conn(|conn| {
        let (enabled, transport, transport_config_json, credential_refs_json): (
            i64,
            String,
            String,
            String,
        ) = conn.query_row(
            "SELECT enabled, transport_kind, transport_config_json, credential_refs_json
             FROM web_evidence_providers
             WHERE id = ?1 AND kind = 'mcp'",
            [provider_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        if enabled == 0 {
            return Err(runtime_error(
                McpRuntimeFailureKind::PolicyDenied,
                "MCP provider is disabled",
            ));
        }
        let transport = transport.trim().to_ascii_lowercase();
        if transport != "https" {
            return Err(runtime_error(
                McpRuntimeFailureKind::PolicyDenied,
                "unsupported_transport: MCP provider is not HTTPS",
            ));
        }
        crate::ai_runtime::mcp_runtime_registry::validate_mcp_runtime_transport_security(
            &transport,
            &transport_config_json,
            &credential_refs_json,
        )?;
        let config: serde_json::Value = serde_json::from_str(&transport_config_json)?;
        let url = config_string(&config, "url").ok_or_else(|| {
            runtime_error(
                McpRuntimeFailureKind::InvalidResponse,
                "MCP HTTPS provider has no URL",
            )
        })?;
        let allow_localhost_dev = config
            .get("allow_localhost_dev")
            .and_then(|value| value.as_bool())
            == Some(true);
        validate_mcp_http_runtime_url(&url, allow_localhost_dev)?;
        let headers = resolve_http_header_bindings(db, &credential_refs_json)?;
        Ok(StoredRemoteProvider {
            url,
            headers,
            allow_localhost_dev,
        })
    })
}

pub(crate) fn provider_http_auth_header_present(
    db: &Database,
    provider_id: &str,
) -> AppResult<bool> {
    let provider = load_remote_provider(db, provider_id)?;
    Ok(provider
        .headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("authorization")))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HttpAuthFingerprint {
    pub host: String,
    pub auth_header_present: bool,
    pub auth_looks_bearer: bool,
    pub token_prefix_as_sk: bool,
    pub token_len: usize,
}

impl HttpAuthFingerprint {
    pub(crate) fn summary(&self) -> String {
        format!(
            "host={}; authHeaderPresent={}; authLooksBearer={}; tokenPrefixAsSk={}; tokenLen={}. 厂商控制台 Last Used 不一定统计 MCP，不能作为未带 Key 的证据",
            self.host,
            self.auth_header_present,
            self.auth_looks_bearer,
            self.token_prefix_as_sk,
            self.token_len
        )
    }
}

/// Non-sensitive Authorization header fingerprint for diagnostics UI.
pub(crate) fn provider_http_auth_fingerprint(
    db: &Database,
    provider_id: &str,
) -> AppResult<HttpAuthFingerprint> {
    let provider = load_remote_provider(db, provider_id)?;
    let host = reqwest::Url::parse(&provider.url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".into());
    let auth_value = provider
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        .map(|(_, value)| value.as_str());
    let Some(value) = auth_value else {
        return Ok(HttpAuthFingerprint {
            host,
            auth_header_present: false,
            auth_looks_bearer: false,
            token_prefix_as_sk: false,
            token_len: 0,
        });
    };
    let trimmed = value.trim();
    let auth_looks_bearer = trimmed
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bearer "));
    let token = if auth_looks_bearer {
        trimmed[7..].trim()
    } else {
        trimmed
    };
    Ok(HttpAuthFingerprint {
        host,
        auth_header_present: true,
        auth_looks_bearer,
        token_prefix_as_sk: token.starts_with("as_sk_"),
        token_len: token.chars().count(),
    })
}

fn load_stdio_provider(db: &Database, provider_id: &str) -> AppResult<StoredStdioProvider> {
    db.with_read_conn(|conn| {
        let (enabled, transport_config_json, credential_refs_json, transport): (
            i64,
            String,
            String,
            String,
        ) = conn.query_row(
            "SELECT enabled, transport_config_json, credential_refs_json, transport_kind
             FROM web_evidence_providers
             WHERE id = ?1 AND kind = 'mcp'",
            [provider_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        if enabled == 0 {
            return Err(runtime_error(
                McpRuntimeFailureKind::PolicyDenied,
                "MCP provider is disabled",
            ));
        }
        if transport != "stdio" {
            return Err(runtime_error(
                McpRuntimeFailureKind::PolicyDenied,
                "MCP provider is not stdio",
            ));
        }
        crate::ai_runtime::mcp_runtime_registry::validate_mcp_runtime_transport_security(
            &transport,
            &transport_config_json,
            &credential_refs_json,
        )?;
        let config: serde_json::Value = serde_json::from_str(&transport_config_json)?;
        let command = config_string(&config, "command").ok_or_else(|| {
            runtime_error(
                McpRuntimeFailureKind::InvalidResponse,
                "MCP provider has no stdio command",
            )
        })?;
        let args_json = config
            .get("args")
            .map(serde_json::Value::to_string)
            .unwrap_or_else(|| "[]".to_string());
        Ok(StoredStdioProvider {
            command: PathBuf::from(command),
            args: parse_stdio_args(&args_json)?,
            env: Vec::new(),
        })
    })
}

fn sanitize_runtime_output(raw: &str) -> String {
    crate::ai_runtime::trace::redact_classified_leaks(raw)
        .trim()
        .to_string()
}

async fn drain_stderr<R>(mut stderr: R, max_bytes: usize) -> String
where
    R: AsyncReadExt + Unpin,
{
    let mut collected = Vec::new();
    let mut buffer = [0_u8; 512];
    loop {
        match stderr.read(&mut buffer).await {
            Ok(0) => break,
            Ok(read) => {
                let remaining = max_bytes.saturating_sub(collected.len());
                if remaining > 0 {
                    collected.extend_from_slice(&buffer[..read.min(remaining)]);
                }
            }
            Err(_) => break,
        }
    }
    sanitize_runtime_output(&String::from_utf8_lossy(&collected))
}

async fn discover_http_tools_with_rmcp(launch: McpHttpLaunch) -> AppResult<McpStdioDiscovery> {
    if launch.max_response_bytes == 0 {
        return Err(runtime_error(
            McpRuntimeFailureKind::OutputTooLarge,
            "MCP HTTP response cap must be greater than zero",
        ));
    }

    let config = StreamableHttpClientTransportConfig::with_uri(launch.url.clone())
        .custom_headers(rmcp_headers(&launch.headers)?);
    let transport =
        StreamableHttpClientTransport::with_client(bounded_mcp_http_client(&launch).await?, config);
    let run = async move {
        let client = rmcp_client_info()
            .serve(transport)
            .await
            .map_err(rmcp_client_error)?;
        let peer_info = client.peer_info().ok_or_else(|| {
            runtime_error(
                McpRuntimeFailureKind::InvalidResponse,
                "MCP server did not return initialize metadata",
            )
        })?;
        let protocol_version = peer_info.protocol_version.to_string();
        if let Err(error) = validate_negotiated_mcp_protocol_version(&protocol_version) {
            let _ = client.cancel().await;
            return Err(error);
        }
        let tools = client.list_all_tools().await.map_err(rmcp_client_error)?;
        let _ = client.cancel().await;
        let tools = tools
            .into_iter()
            .map(mcp_tool_definition_from_rmcp)
            .collect::<Vec<_>>();
        ensure_json_value_under_cap(&serde_json::to_value(&tools)?, launch.max_response_bytes)?;
        Ok::<_, AppError>(McpStdioDiscovery {
            protocol_version,
            server_name: peer_info.server_info.name.clone(),
            server_version: Some(peer_info.server_info.version.clone()),
            tools,
            stderr_summary: None,
        })
    };
    match timeout(launch.request_timeout, run).await {
        Ok(result) => result,
        Err(_) => Err(runtime_error(
            McpRuntimeFailureKind::Timeout,
            "MCP HTTP request timed out",
        )),
    }
}

async fn call_http_tool_with_rmcp(
    launch: McpHttpLaunch,
    tool_name: String,
    arguments: serde_json::Value,
) -> AppResult<serde_json::Value> {
    if launch.max_response_bytes == 0 {
        return Err(runtime_error(
            McpRuntimeFailureKind::OutputTooLarge,
            "MCP HTTP response cap must be greater than zero",
        ));
    }

    let config = StreamableHttpClientTransportConfig::with_uri(launch.url.clone())
        .custom_headers(rmcp_headers(&launch.headers)?);
    let transport =
        StreamableHttpClientTransport::with_client(bounded_mcp_http_client(&launch).await?, config);
    let arguments = rmcp_tool_call_arguments(arguments)?;
    let run = async move {
        let client = rmcp_client_info()
            .serve(transport)
            .await
            .map_err(rmcp_client_error)?;
        validate_connected_mcp_protocol(&client)?;
        let result = client
            .call_tool(CallToolRequestParams::new(tool_name).with_arguments(arguments))
            .await
            .map_err(rmcp_client_error)?;
        let _ = client.cancel().await;
        let result = serde_json::to_value(result)?;
        ensure_json_value_under_cap(&result, launch.max_response_bytes)?;
        Ok::<_, AppError>(result)
    };
    match timeout(launch.request_timeout, run).await {
        Ok(result) => result,
        Err(_) => Err(runtime_error(
            McpRuntimeFailureKind::Timeout,
            "MCP HTTP tool call timed out",
        )),
    }
}

async fn discover_http_tools(launch: McpHttpLaunch) -> AppResult<McpStdioDiscovery> {
    discover_http_tools_with_rmcp(launch).await
}

async fn call_http_tool(
    launch: McpHttpLaunch,
    tool_name: String,
    arguments: serde_json::Value,
) -> AppResult<serde_json::Value> {
    call_http_tool_with_rmcp(launch, tool_name, arguments).await
}

#[cfg(test)]
fn build_stdio_child_env<I>(
    host_env: I,
    provider_env: &[(String, String)],
) -> std::collections::HashMap<String, String>
where
    I: IntoIterator<Item = (String, String)>,
{
    let mut env: std::collections::HashMap<String, String> = host_env.into_iter().collect();
    env.extend(provider_env.iter().cloned());
    env
}

fn spawn_rmcp_stdio_transport(
    command_path: PathBuf,
    args: Vec<String>,
    env: Vec<(String, String)>,
    cwd: Option<PathBuf>,
    max_stderr_bytes: usize,
    max_stdout_line_bytes: usize,
) -> AppResult<(
    BoundedStdioTransport,
    Option<tokio::task::JoinHandle<String>>,
)> {
    if max_stdout_line_bytes == 0 {
        return Err(runtime_error(
            McpRuntimeFailureKind::OutputTooLarge,
            "MCP stdout cap must be greater than zero",
        ));
    }
    let mut command = Command::new(command_path);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    // An MCP process receives only explicitly permitted configuration. In
    // particular it never inherits an LLM provider key from Iris itself.
    command.env_clear();
    command.envs(env);
    command.kill_on_drop(true);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|_| {
        runtime_error(
            McpRuntimeFailureKind::Unavailable,
            "failed to start official MCP stdio process",
        )
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::Unavailable,
            "MCP stdout pipe unavailable",
        )
    })?;
    let stdin = child.stdin.take().ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::Unavailable,
            "MCP stdin pipe unavailable",
        )
    })?;
    let stderr = child.stderr.take();
    let transport = BoundedStdioTransport {
        child,
        transport: AsyncRwTransport::new(
            CappedFrameReader::new(stdout, max_stdout_line_bytes),
            stdin,
        ),
    };
    let stderr_task = stderr.map(|stderr| tokio::spawn(drain_stderr(stderr, max_stderr_bytes)));
    Ok((transport, stderr_task))
}

async fn finish_rmcp_stdio_stderr(
    stderr_task: Option<tokio::task::JoinHandle<String>>,
) -> Option<String> {
    let summary = match stderr_task {
        Some(task) => task.await.unwrap_or_default(),
        None => String::new(),
    };
    (!summary.is_empty()).then_some(summary)
}

async fn discover_stdio_tools_with_rmcp(
    launch: McpStdioLaunch,
    env: Vec<(String, String)>,
) -> AppResult<McpStdioDiscovery> {
    discover_stdio_tools_with_rmcp_attempt(launch, env).await.0
}

/// Return whether a child transport was successfully spawned along with the
/// discovery result. Configuration and spawn failures are deliberately
/// distinguished from failures observed after a transport attempt.
async fn discover_stdio_tools_with_rmcp_attempt(
    launch: McpStdioLaunch,
    env: Vec<(String, String)>,
) -> (AppResult<McpStdioDiscovery>, bool) {
    if launch.max_stdout_line_bytes == 0 {
        return (
            Err(runtime_error(
                McpRuntimeFailureKind::OutputTooLarge,
                "MCP stdout cap must be greater than zero",
            )),
            false,
        );
    }
    let request_timeout = launch.request_timeout;
    let max_response_bytes = launch.max_stdout_line_bytes;
    let (transport, stderr_task) = match spawn_rmcp_stdio_transport(
        launch.command,
        launch.args,
        env,
        launch.cwd,
        launch.max_stderr_bytes,
        launch.max_stdout_line_bytes,
    ) {
        Ok(transport) => transport,
        Err(error) => return (Err(error), false),
    };
    let run = async move {
        let client = rmcp_client_info()
            .serve(transport)
            .await
            .map_err(rmcp_client_error)?;
        let peer_info = client.peer_info().ok_or_else(|| {
            runtime_error(
                McpRuntimeFailureKind::InvalidResponse,
                "MCP server did not return initialize metadata",
            )
        })?;
        let protocol_version = peer_info.protocol_version.to_string();
        if let Err(error) = validate_negotiated_mcp_protocol_version(&protocol_version) {
            let _ = client.cancel().await;
            return Err(error);
        }
        let tools = client.list_all_tools().await.map_err(rmcp_client_error)?;
        let _ = client.cancel().await;
        let tools = tools
            .into_iter()
            .map(mcp_tool_definition_from_rmcp)
            .collect::<Vec<_>>();
        ensure_json_value_under_cap(&serde_json::to_value(&tools)?, max_response_bytes)?;
        Ok::<_, AppError>(McpStdioDiscovery {
            protocol_version,
            server_name: peer_info.server_info.name.clone(),
            server_version: Some(peer_info.server_info.version.clone()),
            tools,
            stderr_summary: None,
        })
    };
    let result = match timeout(request_timeout, run).await {
        Ok(result) => result,
        Err(_) => Err(runtime_error(
            McpRuntimeFailureKind::Timeout,
            "MCP stdio request timed out",
        )),
    };
    let stderr_summary = finish_rmcp_stdio_stderr(stderr_task).await;
    (
        result.map(|mut discovery| {
            discovery.stderr_summary = stderr_summary;
            discovery
        }),
        true,
    )
}

async fn call_stdio_tool_with_rmcp(
    launch: McpStdioToolCallLaunch,
) -> AppResult<(serde_json::Value, Option<String>)> {
    if launch.max_stdout_line_bytes == 0 {
        return Err(runtime_error(
            McpRuntimeFailureKind::OutputTooLarge,
            "MCP stdout cap must be greater than zero",
        ));
    }
    let request_timeout = launch.request_timeout;
    let max_response_bytes = launch.max_stdout_line_bytes;
    let (transport, stderr_task) = spawn_rmcp_stdio_transport(
        launch.command,
        launch.args,
        launch.env,
        launch.cwd,
        launch.max_stderr_bytes,
        launch.max_stdout_line_bytes,
    )?;
    let tool_name = launch.tool_name;
    let arguments = rmcp_tool_call_arguments(launch.arguments)?;
    let run = async move {
        let client = rmcp_client_info()
            .serve(transport)
            .await
            .map_err(rmcp_client_error)?;
        validate_connected_mcp_protocol(&client)?;
        let result = client
            .call_tool(CallToolRequestParams::new(tool_name).with_arguments(arguments))
            .await
            .map_err(rmcp_client_error)?;
        let _ = client.cancel().await;
        let result = serde_json::to_value(result)?;
        ensure_json_value_under_cap(&result, max_response_bytes)?;
        Ok::<_, AppError>(result)
    };
    let result = match timeout(request_timeout, run).await {
        Ok(result) => result,
        Err(_) => Err(runtime_error(
            McpRuntimeFailureKind::Timeout,
            "MCP stdio tool call timed out",
        )),
    };
    let stderr_summary = finish_rmcp_stdio_stderr(stderr_task).await;
    result.map(|result| (result, stderr_summary))
}

/// A live MCP stdio client retained between calls so subsequent searches skip
/// the 3-5s process spawn + RMCP handshake cost.
///
/// The `RunningService` owns the child transport; dropping it cancels the
/// service loop and, via `kill_on_drop`, terminates the child process. The
/// stderr drain task is detached on spawn (its `JoinHandle` is dropped), so it
/// keeps the stderr pipe clear for the lifetime of the child without needing
/// to be stored here.
struct McpStdioSession {
    client: RunningService<RoleClient, ClientInfo>,
    last_used: Instant,
}

/// Global pool keyed by a launch fingerprint (command + args + cwd). Env is
/// intentionally excluded because stdio providers are launched with a cleared
/// environment and no provider-defined env (`StoredStdioProvider.env` is always
/// empty), so the fingerprint uniquely identifies a reusable process.
static STDIO_SESSION_POOL: LazyLock<Mutex<HashMap<String, McpStdioSession>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Per-launch gates prevent concurrent first calls for the same profile from
/// spawning duplicate children. Weak entries make the registry self-cleaning
/// once the final caller releases its gate.
static STDIO_PROFILE_GATES: LazyLock<StdMutex<HashMap<String, Weak<Mutex<()>>>>> =
    LazyLock::new(|| StdMutex::new(HashMap::new()));

fn stdio_profile_gate(fingerprint: &str) -> Arc<Mutex<()>> {
    let mut gates = STDIO_PROFILE_GATES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    gates.retain(|_, gate| gate.strong_count() > 0);
    gates
        .entry(fingerprint.to_string())
        .or_default()
        .upgrade()
        .unwrap_or_else(|| {
            let gate = Arc::new(Mutex::new(()));
            gates.insert(fingerprint.to_string(), Arc::downgrade(&gate));
            gate
        })
}

fn stdio_session_fingerprint(command: &Path, args: &[String], cwd: Option<&Path>) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(command.to_string_lossy().as_bytes());
    hasher.update(b"\0");
    for arg in args {
        hasher.update(arg.as_bytes());
        hasher.update(b"\0");
    }
    if let Some(cwd) = cwd {
        hasher.update(cwd.to_string_lossy().as_bytes());
    }
    hasher.update(b"\0");
    hex::encode(&hasher.finalize()[..16])
}

fn stdio_session_is_alive(client: &RunningService<RoleClient, ClientInfo>) -> bool {
    !client.is_closed() && !client.is_transport_closed()
}

/// Spawn a fresh stdio child, complete the RMCP initialize handshake, and
/// return the resulting session. The caller is responsible for inserting it
/// into the pool (or dropping it on failure).
async fn spawn_stdio_session(
    command: PathBuf,
    args: Vec<String>,
    env: Vec<(String, String)>,
    cwd: Option<PathBuf>,
    request_timeout: Duration,
    max_stderr_bytes: usize,
    max_stdout_line_bytes: usize,
) -> AppResult<McpStdioSession> {
    let (transport, stderr_task) = spawn_rmcp_stdio_transport(
        command,
        args,
        env,
        cwd,
        max_stderr_bytes,
        max_stdout_line_bytes,
    )?;
    // Detach the stderr drain: the task keeps the pipe clear for the child's
    // lifetime and completes naturally when the process exits. We never need
    // to await its summary for pooled calls (stderr diagnostics are most
    // useful for diagnosing the spawn failures the pool avoids).
    drop(stderr_task);
    let init = async move {
        rmcp_client_info()
            .serve(transport)
            .await
            .map_err(rmcp_client_error)
    };
    let client = match timeout(request_timeout, init).await {
        Ok(Ok(client)) => client,
        Ok(Err(error)) => return Err(error),
        Err(_) => {
            return Err(runtime_error(
                McpRuntimeFailureKind::Timeout,
                "MCP stdio session initialization timed out",
            ));
        }
    };
    validate_connected_mcp_protocol(&client)?;
    Ok(McpStdioSession {
        client,
        last_used: Instant::now(),
    })
}

/// Return the pool keys whose sessions have exceeded `idle_timeout`. Pure
/// (side-effect free) so it can be unit-tested without a live `RunningService`.
fn expired_stdio_session_keys<'a, I>(
    entries: I,
    now: Instant,
    idle_timeout: Duration,
) -> Vec<String>
where
    I: IntoIterator<Item = (&'a String, Instant)>,
{
    let mut expired = Vec::new();
    for (key, last_used) in entries {
        if now.duration_since(last_used) > idle_timeout {
            expired.push(key.clone());
        }
    }
    expired
}

/// Return least-recently-used session keys that must be evicted before the
/// idle pool exceeds its configured capacity. The key tie-breaker makes the
/// result deterministic for tests and avoids retaining arbitrary providers.
fn stdio_session_pool_eviction_keys<'a, I>(entries: I, capacity: usize) -> Vec<String>
where
    I: IntoIterator<Item = (&'a String, Instant)>,
{
    let mut entries = entries
        .into_iter()
        .map(|(key, last_used)| (key.clone(), last_used))
        .collect::<Vec<_>>();
    let excess = entries.len().saturating_sub(capacity);
    if excess == 0 {
        return Vec::new();
    }
    entries.sort_by(|(left_key, left_used), (right_key, right_used)| {
        left_used
            .cmp(right_used)
            .then_with(|| left_key.cmp(right_key))
    });
    entries
        .into_iter()
        .take(excess)
        .map(|(key, _)| key)
        .collect()
}

fn enforce_stdio_session_pool_capacity(pool: &mut HashMap<String, McpStdioSession>) {
    let keys = stdio_session_pool_eviction_keys(
        pool.iter().map(|(key, session)| (key, session.last_used)),
        MAX_STDIO_SESSION_POOL_SIZE,
    );
    for key in keys {
        pool.remove(&key);
    }
}

/// Remove and drop idle sessions from the pool. Dropping a `McpStdioSession`
/// cancels its `RunningService` (the Drop guard fires the cancellation token)
/// and `kill_on_drop` terminates the child process.
async fn sweep_expired_stdio_sessions(
    pool: &mut HashMap<String, McpStdioSession>,
    idle_timeout: Duration,
) {
    let now = Instant::now();
    let last_used_pairs: Vec<(String, Instant)> = pool
        .iter()
        .map(|(key, session)| (key.clone(), session.last_used))
        .collect();
    let expired = expired_stdio_session_keys(
        last_used_pairs.iter().map(|(key, ts)| (key, *ts)),
        now,
        idle_timeout,
    );
    for key in expired {
        pool.remove(&key);
    }
}

/// Lazily spawn the background reaper the first time the pool is used. The
/// reaper drops idle stdio sessions every 60s. Spawning lazily (rather than
/// from Tauri's synchronous `setup` hook) guarantees we are inside the Tokio
/// runtime context that `tokio::spawn` requires.
#[cfg(not(test))]
static STDIO_SESSION_POOL_REAPER_GUARD: std::sync::OnceLock<()> = std::sync::OnceLock::new();

#[cfg(not(test))]
fn ensure_stdio_session_pool_reaper() {
    STDIO_SESSION_POOL_REAPER_GUARD.get_or_init(|| {
        tokio::spawn(async move {
            let interval = Duration::from_secs(60);
            let idle_timeout = DEFAULT_STDIO_SESSION_IDLE_TIMEOUT;
            loop {
                tokio::time::sleep(interval).await;
                let mut pool = STDIO_SESSION_POOL.lock().await;
                sweep_expired_stdio_sessions(&mut pool, idle_timeout).await;
            }
        });
    });
}

// Unit tests exercise expiry and bounded-pool behavior deterministically; a
// forever background task would keep their shared Tokio runtime alive after
// assertions finish and turn a green suite into a hung test process.
#[cfg(test)]
fn ensure_stdio_session_pool_reaper() {}

/// Pooled stdio tool call: reuse a live session if one exists for this launch
/// fingerprint, otherwise spawn a fresh one. On success the session is returned
/// to the pool for the next call; on failure (or if the transport closed during
/// the call) the session is dropped so the next caller spawns a fresh process.
///
/// Pooled calls do not surface per-call stderr: the background drain keeps the
/// stderr pipe clear for the lifetime of the session, and stderr diagnostics
/// are most useful for diagnosing the spawn failures that the pooled path
/// avoids. Spawn failures here still return the underlying error.
async fn call_stdio_tool_pooled(
    launch: McpStdioToolCallLaunch,
    idle_timeout: Duration,
) -> AppResult<(serde_json::Value, Option<String>)> {
    ensure_stdio_session_pool_reaper();
    if launch.max_stdout_line_bytes == 0 {
        return Err(runtime_error(
            McpRuntimeFailureKind::OutputTooLarge,
            "MCP stdout cap must be greater than zero",
        ));
    }
    let request_timeout = launch.request_timeout;
    let max_response_bytes = launch.max_stdout_line_bytes;
    let fingerprint =
        stdio_session_fingerprint(&launch.command, &launch.args, launch.cwd.as_deref());
    let profile_gate = stdio_profile_gate(&fingerprint);
    let _profile_guard = profile_gate.lock().await;
    let tool_name = launch.tool_name.clone();
    let arguments = rmcp_tool_call_arguments(launch.arguments)?;

    // Opportunistic sweep + reuse. Holding the pool lock only for the lookup
    // keeps concurrent callers to different providers unblocked.
    let reused = {
        let mut pool = STDIO_SESSION_POOL.lock().await;
        sweep_expired_stdio_sessions(&mut pool, idle_timeout).await;
        pool.remove(&fingerprint)
            .filter(|session| stdio_session_is_alive(&session.client))
    };
    let mut session = match reused {
        Some(session) => session,
        None => {
            spawn_stdio_session(
                launch.command,
                launch.args,
                launch.env,
                launch.cwd,
                request_timeout,
                launch.max_stderr_bytes,
                launch.max_stdout_line_bytes,
            )
            .await?
        }
    };

    let call_run = async {
        let result = session
            .client
            .call_tool(CallToolRequestParams::new(tool_name).with_arguments(arguments))
            .await
            .map_err(rmcp_client_error)?;
        let result = serde_json::to_value(result)?;
        ensure_json_value_under_cap(&result, max_response_bytes)?;
        Ok::<_, AppError>(result)
    };
    let result = match timeout(request_timeout, call_run).await {
        Ok(result) => result,
        Err(_) => Err(runtime_error(
            McpRuntimeFailureKind::Timeout,
            "MCP stdio tool call timed out",
        )),
    };

    let keep = result.is_ok() && stdio_session_is_alive(&session.client);
    {
        let mut pool = STDIO_SESSION_POOL.lock().await;
        if keep {
            session.last_used = Instant::now();
            pool.insert(fingerprint, session);
            enforce_stdio_session_pool_capacity(&mut pool);
        }
        // else: drop session -> cancel -> kill_on_drop terminates the child
    }
    result.map(|result| (result, None))
}

pub async fn call_provider_stdio_tool(
    db: &Database,
    provider: &crate::ai_runtime::capability_resolver::ResolvedCapabilityProvider,
    arguments: serde_json::Value,
    options: McpHostRuntimeOptions,
) -> AppResult<McpToolCallResult> {
    if provider.provider_kind != "mcp" {
        return Err(runtime_error(
            McpRuntimeFailureKind::PolicyDenied,
            "resolved provider is not an MCP provider",
        ));
    }
    let loaded_provider = load_stdio_provider(db, &provider.profile_id)?;
    let launch = McpStdioToolCallLaunch {
        command: loaded_provider.command,
        args: loaded_provider.args,
        env: loaded_provider.env,
        cwd: options.cwd,
        request_timeout: options.request_timeout,
        max_stdout_line_bytes: options.max_stdout_line_bytes,
        max_stderr_bytes: options.max_stderr_bytes,
        tool_name: provider.tool_name.clone(),
        arguments,
    };
    let (result, stderr_summary) = if options.stdio_session_pool {
        call_stdio_tool_pooled(launch, options.stdio_session_idle_timeout).await?
    } else {
        call_stdio_tool_with_rmcp(launch).await?
    };
    Ok(McpToolCallResult {
        provider_id: provider.profile_id.clone(),
        tool_name: provider.tool_name.clone(),
        result,
        stderr_summary,
    })
}

pub async fn call_provider_http_tool(
    db: &Database,
    provider: &crate::ai_runtime::capability_resolver::ResolvedCapabilityProvider,
    arguments: serde_json::Value,
    options: McpHostRuntimeOptions,
) -> AppResult<McpToolCallResult> {
    if provider.provider_kind != "mcp" {
        return Err(runtime_error(
            McpRuntimeFailureKind::PolicyDenied,
            "resolved provider is not an MCP provider",
        ));
    }
    let loaded_provider = load_remote_provider(db, &provider.profile_id)?;
    let result = call_http_tool(
        McpHttpLaunch {
            url: loaded_provider.url,
            headers: loaded_provider.headers,
            request_timeout: options.request_timeout,
            max_response_bytes: options.max_stdout_line_bytes,
            allow_localhost_dev: loaded_provider.allow_localhost_dev,
        },
        provider.tool_name.clone(),
        arguments,
    )
    .await?;
    Ok(McpToolCallResult {
        provider_id: provider.profile_id.clone(),
        tool_name: provider.tool_name.clone(),
        result,
        stderr_summary: None,
    })
}

pub async fn call_provider_tool(
    db: &Database,
    provider: &crate::ai_runtime::capability_resolver::ResolvedCapabilityProvider,
    arguments: serde_json::Value,
    options: McpHostRuntimeOptions,
) -> AppResult<McpToolCallResult> {
    match load_provider_transport(db, &provider.profile_id)?.as_str() {
        "stdio" => call_provider_stdio_tool(db, provider, arguments, options).await,
        "https" => call_provider_http_tool(db, provider, arguments, options).await,
        other => Err(runtime_error(
            McpRuntimeFailureKind::PolicyDenied,
            format!("unsupported_transport: {other}"),
        )),
    }
}

pub async fn call_required_capability(
    db: &Database,
    capability: &str,
    arguments: serde_json::Value,
    options: McpHostRuntimeOptions,
) -> AppResult<McpToolCallResult> {
    let provider =
        crate::ai_runtime::capability_resolver::resolve_required_capability(db, capability)?;
    call_provider_tool(db, &provider, arguments, options).await
}
pub async fn discover_provider_stdio_tools(
    db: &Database,
    provider_id: &str,
    options: McpHostRuntimeOptions,
) -> AppResult<McpStdioDiscovery> {
    let provider = load_stdio_provider(db, provider_id)?;
    let env = provider.env;
    discover_stdio_tools_with_rmcp(
        McpStdioLaunch {
            command: provider.command,
            args: provider.args,
            cwd: options.cwd,
            request_timeout: options.request_timeout,
            max_stdout_line_bytes: options.max_stdout_line_bytes,
            max_stderr_bytes: options.max_stderr_bytes,
        },
        env,
    )
    .await
}

/// Execute stdio discovery and retain an unforgeable in-process attestation
/// for evaluation contracts. This is intentionally separate from the normal
/// discovery API, whose serializable result is used by product UI.
#[cfg(test)]
pub(crate) async fn probe_provider_stdio_tools(
    db: &Database,
    provider_id: &str,
    options: McpHostRuntimeOptions,
) -> McpStdioTransportProbe {
    let provider = match load_stdio_provider(db, provider_id) {
        Ok(provider) => provider,
        Err(error) => {
            return McpStdioTransportProbe {
                discovery: None,
                failure: Some(classify_runtime_failure(&error)),
                proof: None,
            };
        }
    };
    let env = provider.env;
    let (discovery, transport_spawned) = discover_stdio_tools_with_rmcp_attempt(
        McpStdioLaunch {
            command: provider.command,
            args: provider.args,
            cwd: options.cwd,
            request_timeout: options.request_timeout,
            max_stdout_line_bytes: options.max_stdout_line_bytes,
            max_stderr_bytes: options.max_stderr_bytes,
        },
        env,
    )
    .await;

    match discovery {
        Ok(discovery) => McpStdioTransportProbe {
            discovery: Some(discovery),
            failure: None,
            proof: transport_spawned.then_some(McpStdioTransportProof(())),
        },
        Err(error) => McpStdioTransportProbe {
            discovery: None,
            failure: Some(classify_runtime_failure(&error)),
            proof: transport_spawned.then_some(McpStdioTransportProof(())),
        },
    }
}

#[cfg(test)]
fn classify_runtime_failure(error: &AppError) -> McpRuntimeFailureKind {
    match error.to_string().split_once(':').map(|(kind, _)| kind) {
        Some("unavailable") => McpRuntimeFailureKind::Unavailable,
        Some("tool_not_found") => McpRuntimeFailureKind::ToolNotFound,
        Some("schema_mismatch") => McpRuntimeFailureKind::SchemaMismatch,
        Some("timeout") => McpRuntimeFailureKind::Timeout,
        Some("output_too_large") => McpRuntimeFailureKind::OutputTooLarge,
        Some("auth_missing") => McpRuntimeFailureKind::AuthMissing,
        Some("auth_failed") => McpRuntimeFailureKind::AuthFailed,
        Some("network_denied") => McpRuntimeFailureKind::NetworkDenied,
        Some("policy_denied") => McpRuntimeFailureKind::PolicyDenied,
        _ => McpRuntimeFailureKind::InvalidResponse,
    }
}

pub async fn discover_provider_tools(
    db: &Database,
    provider_id: &str,
    options: McpHostRuntimeOptions,
) -> AppResult<McpStdioDiscovery> {
    discover_provider_tools_with_observation(db, provider_id, options, true).await
}

/// Discover MCP tools for a user-requested diagnostic without affecting Run health data.
pub async fn discover_provider_tools_without_recording(
    db: &Database,
    provider_id: &str,
    options: McpHostRuntimeOptions,
) -> AppResult<McpStdioDiscovery> {
    discover_provider_tools_with_observation(db, provider_id, options, false).await
}

async fn discover_provider_tools_with_observation(
    db: &Database,
    provider_id: &str,
    options: McpHostRuntimeOptions,
    record_observation: bool,
) -> AppResult<McpStdioDiscovery> {
    let started = Instant::now();
    let result = match load_provider_transport(db, provider_id)?.as_str() {
        "stdio" => discover_provider_stdio_tools(db, provider_id, options).await,
        "https" => {
            let provider = load_remote_provider(db, provider_id)?;
            discover_http_tools(McpHttpLaunch {
                url: provider.url,
                headers: provider.headers,
                request_timeout: options.request_timeout,
                max_response_bytes: options.max_stdout_line_bytes,
                allow_localhost_dev: provider.allow_localhost_dev,
            })
            .await
        }
        other => Err(runtime_error(
            McpRuntimeFailureKind::PolicyDenied,
            format!("unsupported_transport: {other}"),
        )),
    };
    observe_provider_discovery_result(
        db,
        provider_id,
        started.elapsed(),
        &result,
        record_observation,
    )?;
    result
}

fn observe_provider_discovery_result(
    db: &Database,
    provider_id: &str,
    elapsed: Duration,
    result: &AppResult<McpStdioDiscovery>,
    record_observation: bool,
) -> AppResult<()> {
    if !record_observation {
        return Ok(());
    }
    match result {
        Ok(discovery) => {
            let tool_schema_hash = {
                let tools = discovery
                    .tools
                    .iter()
                    .map(|tool| {
                        serde_json::json!({
                            "name": tool.name,
                            "inputSchema": tool.input_schema,
                            "outputSchema": tool.output_schema,
                        })
                    })
                    .collect::<Vec<_>>();
                let digest = sha2::Sha256::digest(serde_json::to_string(&tools)?.as_bytes());
                hex::encode(&digest[..12])
            };
            let _ = crate::ai_runtime::mcp_runtime_registry::record_web_evidence_provider_discovery(
                db,
                provider_id,
                &discovery.protocol_version,
                &discovery.server_name,
                discovery.server_version.as_deref(),
                &tool_schema_hash,
            );
            let _ = crate::ai_runtime::mcp_runtime_registry::record_web_evidence_provider_call(
                db,
                provider_id,
                true,
                elapsed.as_millis() as u64,
                None,
            );
        }
        Err(error) => {
            let code = error
                .to_string()
                .split(':')
                .next()
                .unwrap_or("unavailable")
                .to_string();
            let _ = crate::ai_runtime::mcp_runtime_registry::record_web_evidence_provider_call(
                db,
                provider_id,
                false,
                elapsed.as_millis() as u64,
                Some(&code),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{io::AsyncWriteExt, net::TcpListener};

    async fn one_shot_http_response(
        body: &'static str,
        headers: &'static str,
    ) -> reqwest::Response {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local MCP HTTP test peer");
        let address = listener.local_addr().expect("read local test peer address");
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept test HTTP client");
            let response =
                format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{headers}\r\n{body}");
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write test HTTP response");
        });
        reqwest::Client::new()
            .get(format!("http://{address}/mcp"))
            .send()
            .await
            .expect("receive test HTTP response")
    }

    #[tokio::test]
    async fn capped_frame_reader_rejects_an_oversized_frame_before_parsing() {
        let source = std::io::Cursor::new(b"12345\n".to_vec());
        let mut reader = CappedFrameReader::new(source, 4);
        let mut output = Vec::new();

        let error = reader.read_to_end(&mut output).await.unwrap_err();

        assert!(error.to_string().contains("frame exceeds configured cap"));
    }

    #[tokio::test]
    async fn capped_frame_reader_resets_its_limit_after_each_newline() {
        let source = std::io::Cursor::new(b"1234\n5678\n".to_vec());
        let mut reader = CappedFrameReader::new(source, 4);
        let mut output = Vec::new();

        reader.read_to_end(&mut output).await.unwrap();

        assert_eq!(output, b"1234\n5678\n");
    }

    #[tokio::test]
    async fn bounded_http_reader_rejects_declared_oversize_before_body_accumulation() {
        let response = one_shot_http_response("12345", "Content-Length: 5\r\n").await;
        let client = BoundedMcpHttpClient {
            client: reqwest::Client::new(),
            max_response_bytes: 4,
        };

        let error = client.read_body_under_cap(response).await.unwrap_err();

        assert!(matches!(error, BoundedMcpHttpClientError::ResponseTooLarge));
    }

    #[tokio::test]
    async fn bounded_http_reader_rejects_chunked_oversize_before_json_parsing() {
        let response =
            one_shot_http_response("5\r\n12345\r\n0\r\n\r\n", "Transfer-Encoding: chunked\r\n")
                .await;
        let client = BoundedMcpHttpClient {
            client: reqwest::Client::new(),
            max_response_bytes: 4,
        };

        let error = client.read_body_under_cap(response).await.unwrap_err();

        assert!(matches!(error, BoundedMcpHttpClientError::ResponseTooLarge));
    }

    #[test]
    fn local_transport_cap_keeps_the_safe_output_too_large_category() {
        let error = rmcp_client_error("MCP HTTP response exceeds configured cap");

        assert!(error.to_string().starts_with("output_too_large:"));
    }

    fn missing_test_credential(_service: &str) -> AppResult<String> {
        Err(AppError::msg("missing test credential"))
    }

    #[test]
    fn http_auth_fingerprint_summarizes_without_token_material() {
        let fingerprint = HttpAuthFingerprint {
            host: "api.anysearch.com".into(),
            auth_header_present: true,
            auth_looks_bearer: true,
            token_prefix_as_sk: true,
            token_len: 38,
        };
        let summary = fingerprint.summary();
        assert!(summary.contains("host=api.anysearch.com"));
        assert!(summary.contains("authHeaderPresent=true"));
        assert!(summary.contains("tokenPrefixAsSk=true"));
        assert!(summary.contains("tokenLen=38"));
        assert!(!summary.contains("as_sk_"));
        assert!(!summary.to_lowercase().contains("bearer as_sk"));
    }

    #[test]
    fn http_runtime_url_requires_https_for_remote_hosts() {
        let err = validate_mcp_http_runtime_url("http://example.com/mcp", false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("requires HTTPS"), "{err}");
    }

    #[test]
    fn http_runtime_url_rejects_secret_material() {
        let err = validate_mcp_http_runtime_url("https://example.com/mcp?api_key=secret", false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("secret material"), "{err}");
    }

    #[test]
    fn http_runtime_url_blocks_private_hosts_outside_dev_mode() {
        let err = validate_mcp_http_runtime_url("https://127.0.0.1:9000/mcp", false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("private, loopback, or metadata"), "{err}");
    }

    #[test]
    fn http_runtime_url_blocks_ipv4_mapped_loopback_hosts_outside_dev_mode() {
        let err = validate_mcp_http_runtime_url("https://[::ffff:127.0.0.1]:9000/mcp", false)
            .unwrap_err()
            .to_string();

        assert!(err.contains("private, loopback, or metadata"), "{err}");
    }

    #[test]
    fn mcp_dns_pinning_rejects_private_results_before_transport_start() {
        assert!(ip_is_private_or_metadata(
            "10.0.0.8".parse().expect("private IP")
        ));
        assert!(ip_is_private_or_metadata(
            "::ffff:169.254.169.254"
                .parse()
                .expect("mapped metadata IP")
        ));
        assert!(!ip_is_private_or_metadata(
            "1.1.1.1".parse().expect("public IP")
        ));
    }

    #[test]
    fn http_runtime_url_allows_localhost_only_in_dev_mode() {
        assert!(validate_mcp_http_runtime_url("http://localhost:9000/mcp", true).is_ok());
        assert!(validate_mcp_http_runtime_url("https://localhost:9000/mcp", true).is_ok());
    }

    #[test]
    fn http_launch_debug_redacts_header_values() {
        let launch = McpHttpLaunch {
            url: "https://api.anysearch.com/mcp".into(),
            headers: vec![("Authorization".into(), "Bearer as_sk_secret_value".into())],
            request_timeout: Duration::from_secs(5),
            max_response_bytes: 1024,
            allow_localhost_dev: false,
        };

        let debug = format!("{launch:?}");

        assert!(debug.contains("Authorization"));
        assert!(!debug.contains("as_sk_secret_value"), "{debug}");
        assert!(!debug.contains("Bearer"), "{debug}");
    }

    #[test]
    fn rmcp_client_identifies_iris_without_enabling_extra_capabilities() {
        let info = rmcp_client_info();

        assert_eq!(info.client_info.name, "iris");
        assert_eq!(info.client_info.version, env!("CARGO_PKG_VERSION"));
        assert!(info.capabilities.roots.is_none());
    }

    #[test]
    fn rmcp_header_conversion_rejects_protocol_owned_headers() {
        let error = rmcp_headers(&[("Mcp-Session-Id".into(), "forged".into())])
            .unwrap_err()
            .to_string();

        assert!(error.contains("protocol-managed"), "{error}");
    }

    #[test]
    fn rmcp_header_conversion_preserves_authorization_without_logging_value() {
        let headers =
            rmcp_headers(&[("Authorization".into(), "Bearer test-secret".into())]).unwrap();

        assert_eq!(headers.len(), 1);
        assert!(headers
            .keys()
            .any(|name| name.as_str().eq_ignore_ascii_case("authorization")));
    }

    #[test]
    fn rmcp_tool_conversion_preserves_declared_schemas() {
        let input_schema = serde_json::Map::from_iter([(
            "type".into(),
            serde_json::Value::String("object".into()),
        )]);
        let tool = Tool::new("web_search", "Search the web", input_schema);

        let converted = mcp_tool_definition_from_rmcp(tool);

        assert_eq!(converted.name, "web_search");
        assert_eq!(converted.description.as_deref(), Some("Search the web"));
        assert_eq!(converted.input_schema["type"], "object");
    }

    #[test]
    fn resolves_http_authorization_header_from_credential_ref() {
        let headers = resolve_http_header_bindings_with_lookup(
            r#"{"headers":{"Authorization":{"scheme":"bearer","credential":"credential://iris.mcp.codex_header_present"}}}"#,
            |service| match service {
                "iris.mcp.codex_header_present" => Ok("test-header-key".into()),
                _ => missing_test_credential(service),
            },
        )
        .unwrap();

        assert_eq!(
            headers,
            vec![("Authorization".into(), "Bearer test-header-key".into())]
        );
    }

    #[test]
    fn bearer_binding_rejects_a_stored_bearer_prefix_before_network_access() {
        let error = resolve_http_header_bindings_with_lookup(
            r#"{"headers":{"Authorization":{"scheme":"bearer","credential":"credential://iris.mcp.codex_header_present"}}}"#,
            |service| match service {
                "iris.mcp.codex_header_present" => Ok("Bearer test-header-key".into()),
                _ => missing_test_credential(service),
            },
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("auth_failed"), "{error}");
        assert!(error.contains("raw key"), "{error}");
        assert!(!error.contains("test-header-key"), "{error}");
    }

    #[test]
    fn optional_http_authorization_header_is_skipped_when_key_is_missing() {
        let headers = resolve_http_header_bindings_with_lookup(
            r#"{"headers":{"Authorization":{"scheme":"bearer","credential":"credential://iris.mcp.codex_optional_missing","optional":true}}}"#,
            missing_test_credential,
        )
        .unwrap();

        assert!(headers.is_empty(), "{headers:?}");
    }

    #[test]
    fn optional_anysearch_binding_with_unreadable_credential_is_not_anonymous() {
        let err = resolve_http_header_bindings_with_lookup_and_config(
            r#"{"headers":{"Authorization":{"scheme":"bearer","credential":"credential://iris.mcp.anysearch"}}}"#,
            missing_test_credential,
            |service| Ok(service == "iris.mcp.anysearch"),
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("auth_missing"), "{err}");
        assert!(err.contains("credential_unreadable"), "{err}");
        assert!(err.contains("iris.mcp.anysearch"), "{err}");
    }

    #[test]
    fn legacy_anysearch_binding_without_configured_marker_uses_anonymous_mode() {
        let headers = resolve_http_header_bindings_with_lookup_and_config(
            r#"{"headers":{"Authorization":{"scheme":"bearer","credential":"credential://iris.mcp.anysearch"}}}"#,
            missing_test_credential,
            |_| Ok(false),
        )
        .unwrap();

        assert!(headers.is_empty(), "{headers:?}");
    }

    #[test]
    fn optional_http_authorization_header_is_used_when_key_is_configured() {
        let headers = resolve_http_header_bindings_with_lookup(
            r#"{"headers":{"Authorization":{"scheme":"bearer","credential":"credential://iris.mcp.codex_optional_present","optional":true}}}"#,
            |service| match service {
                "iris.mcp.codex_optional_present" => Ok("test-optional-key".into()),
                _ => missing_test_credential(service),
            },
        )
        .unwrap();

        assert_eq!(
            headers,
            vec![("Authorization".into(), "Bearer test-optional-key".into())]
        );
    }

    #[test]
    fn required_http_authorization_header_still_fails_when_key_is_missing() {
        let service = "iris.mcp.codex_required_missing";

        let err = resolve_http_header_bindings_with_lookup(
            r#"{"headers":{"Authorization":{"scheme":"bearer","credential":"credential://iris.mcp.codex_required_missing"}}}"#,
            missing_test_credential,
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("auth_missing"), "{err}");
        assert!(err.contains(service), "{err}");
    }

    #[test]
    fn stdio_security_rejects_credential_and_plain_environment_bindings() {
        let credential_err =
            crate::ai_runtime::mcp_runtime_registry::validate_mcp_runtime_transport_security(
                "stdio",
                r#"{"command":"mcp-server"}"#,
                r#"{"env":{"API_KEY":"credential://iris.mcp.test"}}"#,
            )
            .unwrap_err()
            .to_string();
        assert!(
            credential_err.contains("stdio providers cannot"),
            "{credential_err}"
        );
        let plain_err =
            crate::ai_runtime::mcp_runtime_registry::validate_mcp_runtime_transport_security(
                "stdio",
                r#"{"command":"mcp-server","env":{"MODE":"test"}}"#,
                "{}",
            )
            .unwrap_err()
            .to_string();
        assert!(
            plain_err.contains("must not define environment"),
            "{plain_err}"
        );
    }

    #[test]
    fn stdio_child_environment_contains_only_explicit_values() {
        let host = vec![
            ("PATH".to_string(), "/usr/local/bin:/usr/bin".to_string()),
            ("HOME".to_string(), "/Users/iris".to_string()),
            ("API_KEY".to_string(), "old".to_string()),
        ];
        let provider = vec![
            ("API_KEY".to_string(), "new".to_string()),
            ("CUSTOM_FLAG".to_string(), "1".to_string()),
        ];

        let env = build_stdio_child_env(host, &provider);

        // The process launcher calls env_clear; this helper is retained for
        // deterministic session-key tests only and must not be used to inherit
        // the host process environment.
        let explicit = build_stdio_child_env(Vec::new(), &provider);
        assert!(!explicit.contains_key("PATH"));
        assert!(!explicit.contains_key("HOME"));
        assert_eq!(explicit.get("API_KEY").map(String::as_str), Some("new"));
        assert_eq!(explicit.get("CUSTOM_FLAG").map(String::as_str), Some("1"));
        assert_eq!(env.get("API_KEY").map(String::as_str), Some("new"));
    }

    #[test]
    fn negotiated_protocol_version_accepts_only_the_supported_releases() {
        assert!(super::is_supported_mcp_protocol_version("2025-06-18"));
        assert!(super::is_supported_mcp_protocol_version("2025-11-25"));
        assert!(super::validate_negotiated_mcp_protocol_version("2025-06-18").is_ok());
        assert!(super::validate_negotiated_mcp_protocol_version("2025-11-25").is_ok());

        assert!(!super::is_supported_mcp_protocol_version("2025-03-26"));
        assert!(!super::is_supported_mcp_protocol_version("2026-01-01"));
        assert!(!super::is_supported_mcp_protocol_version("not-a-version"));
        assert!(
            super::validate_negotiated_mcp_protocol_version("2025-03-26")
                .unwrap_err()
                .to_string()
                .starts_with("invalid_response:")
        );
    }

    #[test]
    fn diagnostic_discovery_observation_does_not_persist_runtime_or_health() {
        let db = Database::open_in_memory().unwrap();
        crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
                id: "diagnostic-provider".into(),
                name: "Diagnostic provider".into(),
                kind: "mcp".into(),
                enabled: true,
                transport_kind: "stdio".into(),
                transport_config_json: r#"{"command":"mcp-server"}"#.into(),
                credential_refs_json: "{}".into(),
                web_search_mapping_json: Some(r#"{"tool":"search"}"#.into()),
                web_fetch_mapping_json: None,
            },
        )
        .unwrap();
        let discovery = McpStdioDiscovery {
            protocol_version: "2025-06-18".into(),
            server_name: "Diagnostic MCP".into(),
            server_version: None,
            tools: vec![McpToolDefinition {
                name: "search".into(),
                title: None,
                description: None,
                input_schema: serde_json::json!({"type":"object"}),
                output_schema: None,
            }],
            stderr_summary: None,
        };

        super::observe_provider_discovery_result(
            &db,
            "diagnostic-provider",
            Duration::from_millis(12),
            &Ok(discovery),
            false,
        )
        .unwrap();

        assert!(
            crate::ai_runtime::mcp_runtime_registry::web_evidence_provider_runtime(
                &db,
                "diagnostic-provider"
            )
            .unwrap()
            .is_none()
        );
        assert!(
            crate::ai_runtime::mcp_runtime_registry::web_evidence_provider_health(
                &db,
                "diagnostic-provider"
            )
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn stdio_session_fingerprint_is_deterministic_and_distinguishes_launches() {
        let cmd = PathBuf::from("/bin/sh");
        let args = vec!["fixture.sh".to_string(), "search-only".to_string()];
        let cwd: Option<&Path> = None;

        let a = stdio_session_fingerprint(&cmd, &args, cwd);
        let b = stdio_session_fingerprint(&cmd, &args, cwd);
        assert_eq!(a, b, "same launch must produce the same fingerprint");

        let different_args = vec!["fixture.sh".to_string(), "search-fetch".to_string()];
        let c = stdio_session_fingerprint(&cmd, &different_args, cwd);
        assert_ne!(a, c, "different args must produce a different fingerprint");

        let other_cmd = PathBuf::from("/usr/bin/env");
        let d = stdio_session_fingerprint(&other_cmd, &args, cwd);
        assert_ne!(
            a, d,
            "different command must produce a different fingerprint"
        );

        let with_cwd = stdio_session_fingerprint(&cmd, &args, Some(Path::new("/tmp")));
        assert_ne!(a, with_cwd, "cwd must participate in the fingerprint");
    }

    #[test]
    fn expired_stdio_session_keys_collects_only_idle_entries() {
        let now = Instant::now();
        let idle_timeout = Duration::from_secs(300);

        let fresh = now - Duration::from_secs(10);
        let idle = now - Duration::from_secs(400);
        let boundary = now - idle_timeout; // exactly idle_timeout ago: not expired (strict >)

        let entries = [
            ("alpha".to_string(), fresh),
            ("beta".to_string(), idle),
            ("gamma".to_string(), boundary),
        ];
        let keys =
            expired_stdio_session_keys(entries.iter().map(|(k, t)| (k, *t)), now, idle_timeout);
        assert_eq!(keys, vec!["beta".to_string()]);
    }

    #[test]
    fn stdio_session_pool_capacity_evicts_the_oldest_sessions_deterministically() {
        let now = Instant::now();
        let entries = [
            ("oldest".to_string(), now - Duration::from_secs(30)),
            ("middle".to_string(), now - Duration::from_secs(20)),
            ("newest".to_string(), now - Duration::from_secs(10)),
        ];

        let evicted =
            stdio_session_pool_eviction_keys(entries.iter().map(|(key, used)| (key, *used)), 2);
        assert_eq!(evicted, vec!["oldest".to_string()]);
    }

    #[test]
    fn same_stdio_fingerprint_reuses_one_initialization_gate() {
        let first = stdio_profile_gate("same-profile");
        let second = stdio_profile_gate("same-profile");
        let other = stdio_profile_gate("other-profile");

        assert!(Arc::ptr_eq(&first, &second));
        assert!(!Arc::ptr_eq(&first, &other));
    }
}
