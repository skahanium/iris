//! Controlled MCP host runtime.
//!
//! This module owns MCP protocol execution. Registry modules store metadata;
//! this runtime performs bounded stdio handshakes and discovery.

use std::future::Future;
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
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

#[derive(Debug, Deserialize)]
struct JsonRpcEnvelope {
    id: Option<serde_json::Value>,
    result: Option<serde_json::Value>,
    error: Option<serde_json::Value>,
}

fn runtime_error(kind: McpRuntimeFailureKind, message: impl Into<String>) -> AppError {
    AppError::msg(format!("{}: {}", kind.as_str(), message.into()))
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

fn json_rpc_envelope_from_value(value: serde_json::Value) -> AppResult<JsonRpcEnvelope> {
    serde_json::from_value(value).map_err(|err| {
        runtime_error(
            McpRuntimeFailureKind::InvalidResponse,
            format!("MCP HTTP response was not a JSON-RPC envelope: {err}"),
        )
    })
}

async fn write_json_line<W>(writer: &mut W, value: serde_json::Value) -> AppResult<()>
where
    W: AsyncWriteExt + Unpin,
{
    let mut line = serde_json::to_vec(&value)?;
    line.push(b'\n');
    writer.write_all(&line).await?;
    writer.flush().await?;
    Ok(())
}

async fn read_capped_json_line<R>(
    reader: &mut R,
    max_line_bytes: usize,
) -> AppResult<JsonRpcEnvelope>
where
    R: AsyncBufReadExt + Unpin,
{
    let mut line = String::new();
    let read = reader.read_line(&mut line).await?;
    if read == 0 {
        return Err(runtime_error(
            McpRuntimeFailureKind::Unavailable,
            "MCP server closed stdout",
        ));
    }
    if line.len() > max_line_bytes {
        return Err(runtime_error(
            McpRuntimeFailureKind::OutputTooLarge,
            "MCP stdout line exceeded configured cap",
        ));
    }
    serde_json::from_str(&line).map_err(|err| {
        runtime_error(
            McpRuntimeFailureKind::InvalidResponse,
            format!("MCP server returned invalid JSON: {err}"),
        )
    })
}

fn result_or_error(envelope: JsonRpcEnvelope, expected_id: i64) -> AppResult<serde_json::Value> {
    if envelope.id.as_ref().and_then(|id| id.as_i64()) != Some(expected_id) {
        return Err(runtime_error(
            McpRuntimeFailureKind::InvalidResponse,
            "MCP response id did not match request",
        ));
    }
    if envelope.error.is_some() {
        return Err(runtime_error(
            McpRuntimeFailureKind::InvalidResponse,
            "MCP server returned a JSON-RPC error",
        ));
    }
    envelope.result.ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::InvalidResponse,
            "MCP response omitted result",
        )
    })
}

fn string_field(value: &serde_json::Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(|item| item.as_str())
        .filter(|item| !item.trim().is_empty())
        .map(str::to_string)
}

fn parse_initialize_result(
    result: serde_json::Value,
) -> AppResult<(String, String, Option<String>)> {
    let protocol_version = string_field(&result, "protocolVersion").ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::InvalidResponse,
            "initialize result omitted protocolVersion",
        )
    })?;
    let server_info = result.get("serverInfo").ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::InvalidResponse,
            "initialize result omitted serverInfo",
        )
    })?;
    let server_name = string_field(server_info, "name").ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::InvalidResponse,
            "initialize result omitted serverInfo.name",
        )
    })?;
    let server_version = string_field(server_info, "version");
    Ok((protocol_version, server_name, server_version))
}

fn parse_tools(result: serde_json::Value) -> AppResult<Vec<McpToolDefinition>> {
    let tools = result
        .get("tools")
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            runtime_error(
                McpRuntimeFailureKind::InvalidResponse,
                "tools/list result omitted tools array",
            )
        })?;
    tools
        .iter()
        .map(|tool| {
            let name = string_field(tool, "name").ok_or_else(|| {
                runtime_error(
                    McpRuntimeFailureKind::InvalidResponse,
                    "tool entry omitted name",
                )
            })?;
            let input_schema = tool
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({}));
            Ok(McpToolDefinition {
                name,
                title: string_field(tool, "title"),
                description: string_field(tool, "description"),
                input_schema,
                output_schema: tool.get("outputSchema").cloned(),
            })
        })
        .collect()
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

fn legacy_env_bindings(
    value: &serde_json::Value,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    let object = value.as_object()?;
    if object.contains_key("headers") || object.contains_key("env") {
        None
    } else {
        Some(object)
    }
}

fn resolve_env_bindings(
    db: &Database,
    env_bindings_json: &str,
) -> AppResult<Vec<(String, String)>> {
    let value = parse_json_object(env_bindings_json, McpRuntimeFailureKind::AuthMissing)?;
    let Some(bindings) = object_section(&value, "env").or_else(|| legacy_env_bindings(&value))
    else {
        return Ok(Vec::new());
    };
    let mut env = Vec::new();
    for (env_name, binding) in bindings {
        let service = credential_service_from_binding(binding)?.ok_or_else(|| {
            runtime_error(
                McpRuntimeFailureKind::AuthMissing,
                "MCP env binding omitted named credential service",
            )
        })?;
        let value = crate::credentials::get_api_key(db, &service).map_err(|_| {
            runtime_error(
                McpRuntimeFailureKind::AuthMissing,
                format!("MCP credential binding is missing: {service}"),
            )
        })?;
        env.push((env_name.clone(), value));
    }
    Ok(env)
}

fn resolve_plain_env_bindings(transport_config_json: &str) -> AppResult<Vec<(String, String)>> {
    let value = parse_json_object(
        transport_config_json,
        McpRuntimeFailureKind::InvalidResponse,
    )?;
    let Some(bindings) = object_section(&value, "env") else {
        return Ok(Vec::new());
    };
    let mut env = Vec::new();
    for (env_name, value) in bindings {
        let Some(value) = value
            .as_str()
            .map(str::trim)
            .filter(|item| !item.is_empty())
        else {
            continue;
        };
        env.push((env_name.clone(), value.to_string()));
    }
    Ok(env)
}

fn resolve_http_header_bindings(
    db: &Database,
    credential_refs_json: &str,
) -> AppResult<Vec<(String, String)>> {
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
        let mut value = crate::credentials::get_api_key(db, &service).map_err(|_| {
            runtime_error(
                McpRuntimeFailureKind::AuthMissing,
                format!("MCP credential binding is missing: {service}"),
            )
        })?;
        let scheme = binding
            .as_object()
            .and_then(|object| object.get("scheme"))
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|item| !item.is_empty());
        if let Some(scheme) = scheme {
            if scheme.eq_ignore_ascii_case("bearer") {
                value = format!("Bearer {value}");
            } else {
                value = format!("{scheme} {value}");
            }
        }
        headers.push((header_name.clone(), value));
    }
    Ok(headers)
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
        Ok(StoredRemoteProvider {
            url,
            headers: resolve_http_header_bindings(db, &credential_refs_json)?,
            allow_localhost_dev,
        })
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
        let mut env = resolve_plain_env_bindings(&transport_config_json)?;
        env.extend(resolve_env_bindings(db, &credential_refs_json)?);
        Ok(StoredStdioProvider {
            command: PathBuf::from(command),
            args: parse_stdio_args(&args_json)?,
            env,
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

async fn kill_child(child: &mut Child) {
    match child.try_wait() {
        Ok(Some(_)) => {}
        Ok(None) => {
            let _ = child.kill().await;
        }
        Err(_) => {
            let _ = child.kill().await;
        }
    }
}

pub async fn discover_http_tools_with_sender<F, Fut>(
    launch: McpHttpLaunch,
    mut sender: F,
) -> AppResult<McpStdioDiscovery>
where
    F: FnMut(serde_json::Value) -> Fut,
    Fut: Future<Output = AppResult<serde_json::Value>>,
{
    validate_mcp_http_runtime_url(&launch.url, launch.allow_localhost_dev)?;
    if launch.max_response_bytes == 0 {
        return Err(runtime_error(
            McpRuntimeFailureKind::OutputTooLarge,
            "MCP HTTP response cap must be greater than zero",
        ));
    }

    let run = async {
        let init = sender(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "iris",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            },
        }))
        .await?;
        ensure_json_value_under_cap(&init, launch.max_response_bytes)?;
        let init = result_or_error(json_rpc_envelope_from_value(init)?, 1)?;
        let (protocol_version, server_name, server_version) = parse_initialize_result(init)?;

        let initialized = sender(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {},
        }))
        .await?;
        ensure_json_value_under_cap(&initialized, launch.max_response_bytes)?;

        let tools = sender(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {},
        }))
        .await?;
        ensure_json_value_under_cap(&tools, launch.max_response_bytes)?;
        let tools = parse_tools(result_or_error(json_rpc_envelope_from_value(tools)?, 2)?)?;

        Ok::<_, AppError>((protocol_version, server_name, server_version, tools))
    };

    match timeout(launch.request_timeout, run).await {
        Ok(Ok((protocol_version, server_name, server_version, tools))) => Ok(McpStdioDiscovery {
            protocol_version,
            server_name,
            server_version,
            tools,
            stderr_summary: None,
        }),
        Ok(Err(err)) => Err(err),
        Err(_) => Err(runtime_error(
            McpRuntimeFailureKind::Timeout,
            "MCP HTTP request timed out",
        )),
    }
}

fn parse_http_json_rpc_response(
    raw: &str,
    max_response_bytes: usize,
) -> AppResult<serde_json::Value> {
    if raw.len() > max_response_bytes {
        return Err(runtime_error(
            McpRuntimeFailureKind::OutputTooLarge,
            "MCP HTTP response exceeded configured cap",
        ));
    }
    if raw.trim().is_empty() {
        return Ok(json!({}));
    }
    if let Ok(value) = serde_json::from_str(raw) {
        return Ok(value);
    }
    let mut data = String::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("data:") {
            let rest = rest.trim();
            if rest != "[DONE]" {
                data.push_str(rest);
            }
        }
    }
    if !data.trim().is_empty() {
        return serde_json::from_str(&data).map_err(|err| {
            runtime_error(
                McpRuntimeFailureKind::InvalidResponse,
                format!("MCP HTTP SSE data was not valid JSON: {err}"),
            )
        });
    }
    Err(runtime_error(
        McpRuntimeFailureKind::InvalidResponse,
        "MCP HTTP server returned invalid JSON",
    ))
}

async fn discover_http_tools(launch: McpHttpLaunch) -> AppResult<McpStdioDiscovery> {
    let parsed = validate_mcp_http_runtime_url(&launch.url, launch.allow_localhost_dev)?;
    let url = parsed.to_string();
    let headers = launch.headers.clone();
    let max_response_bytes = launch.max_response_bytes;
    let mut builder = reqwest::Client::builder()
        .use_rustls_tls()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(launch.request_timeout);
    if !launch.allow_localhost_dev {
        builder = builder.https_only(true);
    }
    let client = builder.build()?;
    discover_http_tools_with_sender(launch, move |request| {
        let client = client.clone();
        let url = url.clone();
        let headers = headers.clone();
        async move {
            let mut request_builder = client
                .post(&url)
                .header(reqwest::header::CONTENT_TYPE, "application/json");
            for (name, value) in &headers {
                request_builder = request_builder.header(name.as_str(), value.as_str());
            }
            let response = request_builder.json(&request).send().await?;
            let status = response.status();
            if status.is_redirection() {
                return Err(runtime_error(
                    McpRuntimeFailureKind::NetworkDenied,
                    "MCP HTTP redirect denied",
                ));
            }
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                return Err(runtime_error(
                    McpRuntimeFailureKind::AuthFailed,
                    "MCP HTTP authentication failed",
                ));
            }
            if !status.is_success() {
                return Err(runtime_error(
                    McpRuntimeFailureKind::Unavailable,
                    format!("MCP HTTP server returned status {status}"),
                ));
            }
            let text = response.text().await?;
            parse_http_json_rpc_response(&text, max_response_bytes)
        }
    })
    .await
}

async fn call_http_tool_with_sender<F, Fut>(
    launch: McpHttpLaunch,
    tool_name: String,
    arguments: serde_json::Value,
    mut sender: F,
) -> AppResult<serde_json::Value>
where
    F: FnMut(serde_json::Value) -> Fut,
    Fut: Future<Output = AppResult<serde_json::Value>>,
{
    validate_mcp_http_runtime_url(&launch.url, launch.allow_localhost_dev)?;
    if launch.max_response_bytes == 0 {
        return Err(runtime_error(
            McpRuntimeFailureKind::OutputTooLarge,
            "MCP HTTP response cap must be greater than zero",
        ));
    }

    let run = async {
        let init = sender(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "iris",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            },
        }))
        .await?;
        ensure_json_value_under_cap(&init, launch.max_response_bytes)?;
        let init = result_or_error(json_rpc_envelope_from_value(init)?, 1)?;
        let _ = parse_initialize_result(init)?;

        let initialized = sender(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {},
        }))
        .await?;
        ensure_json_value_under_cap(&initialized, launch.max_response_bytes)?;

        let call = sender(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments,
            },
        }))
        .await?;
        ensure_json_value_under_cap(&call, launch.max_response_bytes)?;
        result_or_error(json_rpc_envelope_from_value(call)?, 2)
    };

    match timeout(launch.request_timeout, run).await {
        Ok(result) => result,
        Err(_) => Err(runtime_error(
            McpRuntimeFailureKind::Timeout,
            "MCP HTTP tool call timed out",
        )),
    }
}

async fn call_http_tool(
    launch: McpHttpLaunch,
    tool_name: String,
    arguments: serde_json::Value,
) -> AppResult<serde_json::Value> {
    let parsed = validate_mcp_http_runtime_url(&launch.url, launch.allow_localhost_dev)?;
    let url = parsed.to_string();
    let headers = launch.headers.clone();
    let max_response_bytes = launch.max_response_bytes;
    let mut builder = reqwest::Client::builder()
        .use_rustls_tls()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(launch.request_timeout);
    if !launch.allow_localhost_dev {
        builder = builder.https_only(true);
    }
    let client = builder.build()?;
    call_http_tool_with_sender(launch, tool_name, arguments, move |request| {
        let client = client.clone();
        let url = url.clone();
        let headers = headers.clone();
        async move {
            let mut request_builder = client
                .post(&url)
                .header(reqwest::header::CONTENT_TYPE, "application/json");
            for (name, value) in &headers {
                request_builder = request_builder.header(name.as_str(), value.as_str());
            }
            let response = request_builder.json(&request).send().await?;
            let status = response.status();
            if status.is_redirection() {
                return Err(runtime_error(
                    McpRuntimeFailureKind::NetworkDenied,
                    "MCP HTTP redirect denied",
                ));
            }
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                return Err(runtime_error(
                    McpRuntimeFailureKind::AuthFailed,
                    "MCP HTTP authentication failed",
                ));
            }
            if !status.is_success() {
                return Err(runtime_error(
                    McpRuntimeFailureKind::Unavailable,
                    format!("MCP HTTP server returned status {status}"),
                ));
            }
            let text = response.text().await?;
            parse_http_json_rpc_response(&text, max_response_bytes)
        }
    })
    .await
}
async fn discover_stdio_tools_inner(
    launch: McpStdioLaunch,
    env: &[(String, String)],
) -> AppResult<McpStdioDiscovery> {
    if launch.max_stdout_line_bytes == 0 {
        return Err(runtime_error(
            McpRuntimeFailureKind::OutputTooLarge,
            "MCP stdout cap must be greater than zero",
        ));
    }

    let mut command = Command::new(&launch.command);
    command.args(&launch.args);
    if let Some(cwd) = &launch.cwd {
        command.current_dir(cwd);
    }
    command.env_clear();
    command.envs(env.iter().map(|(key, value)| (key, value)));
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.kill_on_drop(true);

    let mut child = command.spawn().map_err(|err| {
        runtime_error(
            McpRuntimeFailureKind::Unavailable,
            format!("failed to start MCP stdio process: {err}"),
        )
    })?;
    let mut stdin = child.stdin.take().ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::Unavailable,
            "MCP stdio stdin unavailable",
        )
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::Unavailable,
            "MCP stdio stdout unavailable",
        )
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::Unavailable,
            "MCP stdio stderr unavailable",
        )
    })?;
    let stderr_task = tokio::spawn(drain_stderr(stderr, launch.max_stderr_bytes));
    let mut stdout = BufReader::new(stdout);

    let run = async {
        write_json_line(
            &mut stdin,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {
                        "name": "iris",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                },
            }),
        )
        .await?;
        let init = read_capped_json_line(&mut stdout, launch.max_stdout_line_bytes).await?;
        let init = result_or_error(init, 1)?;
        let (protocol_version, server_name, server_version) = parse_initialize_result(init)?;

        write_json_line(
            &mut stdin,
            json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {},
            }),
        )
        .await?;
        write_json_line(
            &mut stdin,
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/list",
                "params": {},
            }),
        )
        .await?;
        let tools = read_capped_json_line(&mut stdout, launch.max_stdout_line_bytes).await?;
        let tools = parse_tools(result_or_error(tools, 2)?)?;

        Ok::<_, AppError>((protocol_version, server_name, server_version, tools))
    };

    let result = timeout(launch.request_timeout, run).await;
    let discovery = match result {
        Ok(Ok((protocol_version, server_name, server_version, tools))) => {
            let _ = stdin.shutdown().await;
            kill_child(&mut child).await;
            let stderr_summary = stderr_task.await.unwrap_or_default();
            McpStdioDiscovery {
                protocol_version,
                server_name,
                server_version,
                tools,
                stderr_summary: (!stderr_summary.is_empty()).then_some(stderr_summary),
            }
        }
        Ok(Err(err)) => {
            kill_child(&mut child).await;
            let _ = stderr_task.await;
            return Err(err);
        }
        Err(_) => {
            kill_child(&mut child).await;
            let _ = stderr_task.await;
            return Err(runtime_error(
                McpRuntimeFailureKind::Timeout,
                "MCP stdio request timed out",
            ));
        }
    };

    Ok(discovery)
}

async fn call_stdio_tool(
    launch: McpStdioToolCallLaunch,
) -> AppResult<(serde_json::Value, Option<String>)> {
    if launch.max_stdout_line_bytes == 0 {
        return Err(runtime_error(
            McpRuntimeFailureKind::OutputTooLarge,
            "MCP stdout cap must be greater than zero",
        ));
    }

    let mut command = Command::new(&launch.command);
    command.args(&launch.args);
    if let Some(cwd) = &launch.cwd {
        command.current_dir(cwd);
    }
    command.env_clear();
    command.envs(launch.env.iter().map(|(key, value)| (key, value)));
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.kill_on_drop(true);

    let mut child = command.spawn().map_err(|err| {
        runtime_error(
            McpRuntimeFailureKind::Unavailable,
            format!("failed to start MCP stdio process: {err}"),
        )
    })?;
    let mut stdin = child.stdin.take().ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::Unavailable,
            "MCP stdio stdin unavailable",
        )
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::Unavailable,
            "MCP stdio stdout unavailable",
        )
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        runtime_error(
            McpRuntimeFailureKind::Unavailable,
            "MCP stdio stderr unavailable",
        )
    })?;
    let stderr_task = tokio::spawn(drain_stderr(stderr, launch.max_stderr_bytes));
    let mut stdout = BufReader::new(stdout);

    let run = async {
        write_json_line(
            &mut stdin,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {
                        "name": "iris",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                },
            }),
        )
        .await?;
        let init = read_capped_json_line(&mut stdout, launch.max_stdout_line_bytes).await?;
        let _ = parse_initialize_result(result_or_error(init, 1)?)?;

        write_json_line(
            &mut stdin,
            json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {},
            }),
        )
        .await?;
        write_json_line(
            &mut stdin,
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": launch.tool_name,
                    "arguments": launch.arguments,
                },
            }),
        )
        .await?;
        let call = read_capped_json_line(&mut stdout, launch.max_stdout_line_bytes).await?;
        result_or_error(call, 2)
    };

    let result = timeout(launch.request_timeout, run).await;
    match result {
        Ok(Ok(result)) => {
            let _ = stdin.shutdown().await;
            kill_child(&mut child).await;
            let stderr_summary = stderr_task.await.unwrap_or_default();
            Ok((
                result,
                (!stderr_summary.is_empty()).then_some(stderr_summary),
            ))
        }
        Ok(Err(err)) => {
            kill_child(&mut child).await;
            let _ = stderr_task.await;
            Err(err)
        }
        Err(_) => {
            kill_child(&mut child).await;
            let _ = stderr_task.await;
            Err(runtime_error(
                McpRuntimeFailureKind::Timeout,
                "MCP stdio tool call timed out",
            ))
        }
    }
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
    let (result, stderr_summary) = call_stdio_tool(McpStdioToolCallLaunch {
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

pub async fn call_provider_http_tool_with_sender<F, Fut>(
    db: &Database,
    provider: &crate::ai_runtime::capability_resolver::ResolvedCapabilityProvider,
    arguments: serde_json::Value,
    options: McpHostRuntimeOptions,
    sender: F,
) -> AppResult<McpToolCallResult>
where
    F: FnMut(serde_json::Value) -> Fut,
    Fut: Future<Output = AppResult<serde_json::Value>>,
{
    if provider.provider_kind != "mcp" {
        return Err(runtime_error(
            McpRuntimeFailureKind::PolicyDenied,
            "resolved provider is not an MCP provider",
        ));
    }
    let loaded_provider = load_remote_provider(db, &provider.profile_id)?;
    let result = call_http_tool_with_sender(
        McpHttpLaunch {
            url: loaded_provider.url,
            headers: loaded_provider.headers,
            request_timeout: options.request_timeout,
            max_response_bytes: options.max_stdout_line_bytes,
            allow_localhost_dev: loaded_provider.allow_localhost_dev,
        },
        provider.tool_name.clone(),
        arguments,
        sender,
    )
    .await?;
    Ok(McpToolCallResult {
        provider_id: provider.profile_id.clone(),
        tool_name: provider.tool_name.clone(),
        result,
        stderr_summary: None,
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
    discover_stdio_tools_inner(
        McpStdioLaunch {
            command: provider.command,
            args: provider.args,
            cwd: options.cwd,
            request_timeout: options.request_timeout,
            max_stdout_line_bytes: options.max_stdout_line_bytes,
            max_stderr_bytes: options.max_stderr_bytes,
        },
        &env,
    )
    .await
}

pub async fn discover_provider_http_tools_with_sender<F, Fut>(
    db: &Database,
    provider_id: &str,
    options: McpHostRuntimeOptions,
    sender: F,
) -> AppResult<McpStdioDiscovery>
where
    F: FnMut(serde_json::Value) -> Fut,
    Fut: Future<Output = AppResult<serde_json::Value>>,
{
    let provider = load_remote_provider(db, provider_id)?;
    discover_http_tools_with_sender(
        McpHttpLaunch {
            url: provider.url,
            headers: provider.headers,
            request_timeout: options.request_timeout,
            max_response_bytes: options.max_stdout_line_bytes,
            allow_localhost_dev: provider.allow_localhost_dev,
        },
        sender,
    )
    .await
}

pub async fn discover_provider_tools(
    db: &Database,
    provider_id: &str,
    options: McpHostRuntimeOptions,
) -> AppResult<McpStdioDiscovery> {
    match load_provider_transport(db, provider_id)?.as_str() {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn resolves_http_authorization_header_from_credential_ref() {
        let db = Database::open_in_memory().unwrap();
        crate::credentials::set_api_key(&db, "iris.mcp.anysearch", "test-anysearch-key").unwrap();

        let headers = resolve_http_header_bindings(
            &db,
            r#"{"headers":{"Authorization":{"scheme":"bearer","credential":"credential://iris.mcp.anysearch"}}}"#,
        )
        .unwrap();

        assert_eq!(
            headers,
            vec![("Authorization".into(), "Bearer test-anysearch-key".into())]
        );
        crate::credentials::delete_api_key(&db, "iris.mcp.anysearch").unwrap();
    }

    #[test]
    fn resolves_plain_and_secret_stdio_env_without_mixing_values() {
        let db = Database::open_in_memory().unwrap();
        crate::credentials::set_api_key(&db, "iris.mcp.brave", "test-brave-key").unwrap();

        let plain =
            resolve_plain_env_bindings(r#"{"env":{"SEARXNG_URL":"https://search.example"}}"#)
                .unwrap();
        let secret = resolve_env_bindings(
            &db,
            r#"{"env":{"BRAVE_API_KEY":"credential://iris.mcp.brave"}}"#,
        )
        .unwrap();

        assert_eq!(
            plain,
            vec![("SEARXNG_URL".into(), "https://search.example".into())]
        );
        assert_eq!(
            secret,
            vec![("BRAVE_API_KEY".into(), "test-brave-key".into())]
        );
        crate::credentials::delete_api_key(&db, "iris.mcp.brave").unwrap();
    }

    #[test]
    fn parses_streamable_http_sse_tool_call_response() {
        let parsed = parse_http_json_rpc_response(
            r#"event: message
data: {"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"ok"}]}}

"#,
            2048,
        )
        .unwrap();

        let result = result_or_error(json_rpc_envelope_from_value(parsed).unwrap(), 2).unwrap();
        assert_eq!(result["content"][0]["text"], "ok");
    }

    #[test]
    fn parses_streamable_http_sse_json_rpc_data() {
        let parsed = parse_http_json_rpc_response(
            r#"event: message
data: {"jsonrpc":"2.0","id":1,"result":{"ok":true}}

"#,
            2048,
        )
        .unwrap();

        assert_eq!(parsed["result"]["ok"], true);
    }
}
