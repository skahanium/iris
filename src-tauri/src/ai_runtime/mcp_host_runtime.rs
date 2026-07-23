//! Controlled MCP host runtime.
//!
//! This module owns MCP protocol execution. Registry modules store metadata;
//! this runtime performs bounded stdio handshakes and discovery.

use std::net::IpAddr;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use rmcp::{
    model::{CallToolRequestParams, ClientInfo, Tool},
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig, StreamableHttpClientTransport,
        TokioChildProcess,
    },
    ServiceExt,
};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

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
    proof: McpStdioTransportProof,
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
        self.discovery
            .map(|discovery| (discovery, self.proof))
            .ok_or_else(|| {
                self.failure
                    .unwrap_or(McpRuntimeFailureKind::InvalidResponse)
            })
    }

    pub(crate) fn into_failure(
        self,
    ) -> Result<(McpRuntimeFailureKind, McpStdioTransportProof), McpStdioDiscovery> {
        self.failure
            .map(|failure| (failure, self.proof))
            .ok_or_else(|| self.discovery.expect("probe always has one outcome"))
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

fn rmcp_client_error() -> AppError {
    // Do not surface SDK transport strings: a remote error may echo credentials
    // or provider content. The typed runtime boundary records only this safe code.
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
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

fn http_host_is_private_or_metadata(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if host == "169.254.169.254" || host.eq_ignore_ascii_case("metadata.google.internal") {
        return true;
    }
    let Ok(ip) = host.parse::<IpAddr>() else {
        return false;
    };
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified()
        }
        IpAddr::V6(ip) => {
            let first_segment = ip.segments()[0];
            ip.is_loopback() || ip.is_unspecified() || (first_segment & 0xfe00) == 0xfc00
        }
    }
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
    validate_mcp_http_runtime_url(&launch.url, launch.allow_localhost_dev)?;
    if launch.max_response_bytes == 0 {
        return Err(runtime_error(
            McpRuntimeFailureKind::OutputTooLarge,
            "MCP HTTP response cap must be greater than zero",
        ));
    }

    let config = StreamableHttpClientTransportConfig::with_uri(launch.url.clone())
        .custom_headers(rmcp_headers(&launch.headers)?);
    let transport = StreamableHttpClientTransport::from_config(config);
    let run = async move {
        let client = rmcp_client_info()
            .serve(transport)
            .await
            .map_err(|_| rmcp_client_error())?;
        let peer_info = client.peer_info();
        let tools = client
            .list_all_tools()
            .await
            .map_err(|_| rmcp_client_error())?;
        let _ = client.cancel().await;
        let peer_info = peer_info.ok_or_else(|| {
            runtime_error(
                McpRuntimeFailureKind::InvalidResponse,
                "MCP server did not return initialize metadata",
            )
        })?;
        let tools = tools
            .into_iter()
            .map(mcp_tool_definition_from_rmcp)
            .collect::<Vec<_>>();
        ensure_json_value_under_cap(&serde_json::to_value(&tools)?, launch.max_response_bytes)?;
        Ok::<_, AppError>(McpStdioDiscovery {
            protocol_version: peer_info.protocol_version.to_string(),
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
    validate_mcp_http_runtime_url(&launch.url, launch.allow_localhost_dev)?;
    if launch.max_response_bytes == 0 {
        return Err(runtime_error(
            McpRuntimeFailureKind::OutputTooLarge,
            "MCP HTTP response cap must be greater than zero",
        ));
    }

    let config = StreamableHttpClientTransportConfig::with_uri(launch.url.clone())
        .custom_headers(rmcp_headers(&launch.headers)?);
    let transport = StreamableHttpClientTransport::from_config(config);
    let arguments = rmcp_tool_call_arguments(arguments)?;
    let run = async move {
        let client = rmcp_client_info()
            .serve(transport)
            .await
            .map_err(|_| rmcp_client_error())?;
        let result = client
            .call_tool(CallToolRequestParams::new(tool_name).with_arguments(arguments))
            .await
            .map_err(|_| rmcp_client_error())?;
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
) -> AppResult<(TokioChildProcess, Option<tokio::task::JoinHandle<String>>)> {
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
    let (transport, stderr) = TokioChildProcess::builder(command)
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| {
            runtime_error(
                McpRuntimeFailureKind::Unavailable,
                "failed to start official MCP stdio process",
            )
        })?;
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
        env,
        launch.cwd,
        launch.max_stderr_bytes,
    )?;
    let run = async move {
        let client = rmcp_client_info()
            .serve(transport)
            .await
            .map_err(|_| rmcp_client_error())?;
        let peer_info = client.peer_info();
        let tools = client
            .list_all_tools()
            .await
            .map_err(|_| rmcp_client_error())?;
        let _ = client.cancel().await;
        let peer_info = peer_info.ok_or_else(|| {
            runtime_error(
                McpRuntimeFailureKind::InvalidResponse,
                "MCP server did not return initialize metadata",
            )
        })?;
        let tools = tools
            .into_iter()
            .map(mcp_tool_definition_from_rmcp)
            .collect::<Vec<_>>();
        ensure_json_value_under_cap(&serde_json::to_value(&tools)?, max_response_bytes)?;
        Ok::<_, AppError>(McpStdioDiscovery {
            protocol_version: peer_info.protocol_version.to_string(),
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
    result.map(|mut discovery| {
        discovery.stderr_summary = stderr_summary;
        discovery
    })
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
    )?;
    let tool_name = launch.tool_name;
    let arguments = rmcp_tool_call_arguments(launch.arguments)?;
    let run = async move {
        let client = rmcp_client_info()
            .serve(transport)
            .await
            .map_err(|_| rmcp_client_error())?;
        let result = client
            .call_tool(CallToolRequestParams::new(tool_name).with_arguments(arguments))
            .await
            .map_err(|_| rmcp_client_error())?;
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
    let (result, stderr_summary) = call_stdio_tool_with_rmcp(McpStdioToolCallLaunch {
        command: loaded_provider.command,
        args: loaded_provider.args,
        env: loaded_provider.env,
        cwd: options.cwd,
        request_timeout: options.request_timeout,
        max_stdout_line_bytes: options.max_stdout_line_bytes,
        max_stderr_bytes: options.max_stderr_bytes,
        tool_name: provider.tool_name.clone(),
        arguments,
    })
    .await?;
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
    match discover_provider_stdio_tools(db, provider_id, options).await {
        Ok(discovery) => McpStdioTransportProbe {
            discovery: Some(discovery),
            failure: None,
            proof: McpStdioTransportProof(()),
        },
        Err(error) => McpStdioTransportProbe {
            discovery: None,
            failure: Some(classify_runtime_failure(&error)),
            proof: McpStdioTransportProof(()),
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
}
