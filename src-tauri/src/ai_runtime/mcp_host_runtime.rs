//! Controlled MCP host runtime.
//!
//! This module owns MCP protocol execution. Registry modules store metadata;
//! this runtime performs bounded stdio handshakes and discovery.

use std::future::Future;
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::timeout;

use crate::ai_runtime::mcp_runtime_registry::{record_tool_inventory, McpToolInventoryInput};
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
#[derive(Debug, Clone)]
pub struct McpHttpLaunch {
    pub url: String,
    pub request_timeout: Duration,
    pub max_response_bytes: usize,
    pub allow_localhost_dev: bool,
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
    pub profile_id: String,
    pub tool_name: String,
    pub result: serde_json::Value,
    pub stderr_summary: Option<String>,
}

#[derive(Debug, Clone)]
struct McpStdioToolCallLaunch {
    command: PathBuf,
    args: Vec<String>,
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

fn schema_hash(schema: &serde_json::Value) -> AppResult<String> {
    let bytes = serde_json::to_vec(schema)?;
    let digest = Sha256::digest(&bytes);
    Ok(format!("sha256:{}", hex::encode(digest)))
}

fn safe_tool_description(description: &Option<String>) -> Option<String> {
    let description = description.as_ref()?.trim();
    if description.is_empty() {
        return None;
    }
    let lower = description.to_lowercase();
    if [
        "api_key",
        "apikey",
        "access_token",
        "bearer",
        "password",
        "secret",
        "token=",
        "sk-",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return None;
    }
    Some(description.chars().take(500).collect())
}

#[derive(Debug)]
struct StoredStdioProfile {
    command: PathBuf,
    args: Vec<String>,
    capability_mapping_json: String,
}

#[derive(Debug)]
struct StoredRemoteProfile {
    transport: String,
    url: String,
    allow_localhost_dev: bool,
    capability_mapping_json: String,
}

fn load_profile_transport(db: &Database, profile_id: &str) -> AppResult<String> {
    db.with_read_conn(|conn| {
        let transport: String = conn.query_row(
            "SELECT s.transport
             FROM mcp_runtime_profiles p
             JOIN mcp_server_catalog s ON s.id = p.server_id
             WHERE p.id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;
        Ok(transport.trim().to_ascii_lowercase())
    })
}

fn record_discovered_tool_inventory(
    db: &Database,
    profile_id: &str,
    capability_mapping_json: &str,
    tools: &[McpToolDefinition],
) -> AppResult<()> {
    for tool in tools {
        record_tool_inventory(
            db,
            &McpToolInventoryInput {
                profile_id: profile_id.to_string(),
                tool_name: tool.name.clone(),
                schema_hash: schema_hash(&tool.input_schema)?,
                capability_mapping_json: capability_mapping_json.to_string(),
                description: safe_tool_description(&tool.description),
            },
        )?;
    }
    Ok(())
}

fn load_remote_profile(db: &Database, profile_id: &str) -> AppResult<StoredRemoteProfile> {
    db.with_read_conn(|conn| {
        let (enabled, transport_config_json, server_transport, server_url, capability_tags_json): (
            i64,
            String,
            String,
            Option<String>,
            String,
        ) = conn.query_row(
            "SELECT p.enabled, p.transport_config_json, s.transport, s.url, s.capability_tags_json
             FROM mcp_runtime_profiles p
             JOIN mcp_server_catalog s ON s.id = p.server_id
             WHERE p.id = ?1",
            params![profile_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )?;
        if enabled == 0 {
            return Err(runtime_error(
                McpRuntimeFailureKind::PolicyDenied,
                "MCP profile is disabled",
            ));
        }
        let transport = server_transport.trim().to_ascii_lowercase();
        if !matches!(transport.as_str(), "http" | "https" | "sse") {
            return Err(runtime_error(
                McpRuntimeFailureKind::PolicyDenied,
                "unsupported_transport: MCP profile is not HTTP/SSE",
            ));
        }
        let config: serde_json::Value = serde_json::from_str(&transport_config_json)?;
        let url = config_string(&config, "url")
            .or(server_url)
            .ok_or_else(|| {
                runtime_error(
                    McpRuntimeFailureKind::InvalidResponse,
                    "MCP HTTP profile has no URL",
                )
            })?;
        let allow_localhost_dev = config
            .get("allow_localhost_dev")
            .and_then(|value| value.as_bool())
            == Some(true);
        validate_mcp_http_runtime_url(&url, allow_localhost_dev)?;
        Ok(StoredRemoteProfile {
            transport,
            url,
            allow_localhost_dev,
            capability_mapping_json: capability_tags_json,
        })
    })
}

fn load_stdio_profile(db: &Database, profile_id: &str) -> AppResult<StoredStdioProfile> {
    db.with_read_conn(|conn| {
        let (enabled, transport_config_json, server_transport, server_command, server_args_json, capability_tags_json): (
            i64,
            String,
            String,
            Option<String>,
            String,
            String,
        ) = conn.query_row(
            "SELECT p.enabled, p.transport_config_json, s.transport, s.command, s.args_json, s.capability_tags_json
             FROM mcp_runtime_profiles p
             JOIN mcp_server_catalog s ON s.id = p.server_id
             WHERE p.id = ?1",
            params![profile_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        )?;
        if enabled == 0 {
            return Err(runtime_error(
                McpRuntimeFailureKind::PolicyDenied,
                "MCP profile is disabled",
            ));
        }
        if server_transport != "stdio" {
            return Err(runtime_error(
                McpRuntimeFailureKind::PolicyDenied,
                "only stdio MCP profile discovery is implemented",
            ));
        }
        let config: serde_json::Value = serde_json::from_str(&transport_config_json)?;
        let command = config_string(&config, "command")
            .or(server_command)
            .ok_or_else(|| runtime_error(McpRuntimeFailureKind::InvalidResponse, "MCP profile has no stdio command"))?;
        let args_json = config
            .get("args")
            .map(serde_json::Value::to_string)
            .unwrap_or(server_args_json);
        Ok(StoredStdioProfile {
            command: PathBuf::from(command),
            args: parse_stdio_args(&args_json)?,
            capability_mapping_json: capability_tags_json,
        })
    })
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
    String::from_utf8_lossy(&collected).trim().to_string()
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

pub async fn discover_http_tools(launch: McpHttpLaunch) -> AppResult<McpStdioDiscovery> {
    let parsed = validate_mcp_http_runtime_url(&launch.url, launch.allow_localhost_dev)?;
    let url = parsed.to_string();
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
        async move {
            let response = client
                .post(&url)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .json(&request)
                .send()
                .await?;
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
            if text.len() > max_response_bytes {
                return Err(runtime_error(
                    McpRuntimeFailureKind::OutputTooLarge,
                    "MCP HTTP response exceeded configured cap",
                ));
            }
            if text.trim().is_empty() {
                return Ok(json!({}));
            }
            serde_json::from_str(&text).map_err(|err| {
                runtime_error(
                    McpRuntimeFailureKind::InvalidResponse,
                    format!("MCP HTTP server returned invalid JSON: {err}"),
                )
            })
        }
    })
    .await
}

pub async fn call_http_tool_with_sender<F, Fut>(
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

pub async fn call_http_tool(
    launch: McpHttpLaunch,
    tool_name: String,
    arguments: serde_json::Value,
) -> AppResult<serde_json::Value> {
    let parsed = validate_mcp_http_runtime_url(&launch.url, launch.allow_localhost_dev)?;
    let url = parsed.to_string();
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
        async move {
            let response = client
                .post(&url)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .json(&request)
                .send()
                .await?;
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
            if text.len() > max_response_bytes {
                return Err(runtime_error(
                    McpRuntimeFailureKind::OutputTooLarge,
                    "MCP HTTP response exceeded configured cap",
                ));
            }
            if text.trim().is_empty() {
                return Ok(json!({}));
            }
            serde_json::from_str(&text).map_err(|err| {
                runtime_error(
                    McpRuntimeFailureKind::InvalidResponse,
                    format!("MCP HTTP server returned invalid JSON: {err}"),
                )
            })
        }
    })
    .await
}
pub async fn discover_stdio_tools(launch: McpStdioLaunch) -> AppResult<McpStdioDiscovery> {
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

pub async fn call_profile_stdio_tool(
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
    let profile = load_stdio_profile(db, &provider.profile_id)?;
    let (result, stderr_summary) = call_stdio_tool(McpStdioToolCallLaunch {
        command: profile.command,
        args: profile.args,
        cwd: options.cwd,
        request_timeout: options.request_timeout,
        max_stdout_line_bytes: options.max_stdout_line_bytes,
        max_stderr_bytes: options.max_stderr_bytes,
        tool_name: provider.tool_name.clone(),
        arguments,
    })
    .await?;
    Ok(McpToolCallResult {
        profile_id: provider.profile_id.clone(),
        tool_name: provider.tool_name.clone(),
        result,
        stderr_summary,
    })
}

pub async fn call_profile_http_tool_with_sender<F, Fut>(
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
    let profile = load_remote_profile(db, &provider.profile_id)?;
    if profile.transport == "sse" {
        return Err(runtime_error(
            McpRuntimeFailureKind::PolicyDenied,
            "unsupported_transport: MCP SSE runtime is not implemented",
        ));
    }
    let result = call_http_tool_with_sender(
        McpHttpLaunch {
            url: profile.url,
            request_timeout: options.request_timeout,
            max_response_bytes: options.max_stdout_line_bytes,
            allow_localhost_dev: profile.allow_localhost_dev,
        },
        provider.tool_name.clone(),
        arguments,
        sender,
    )
    .await?;
    Ok(McpToolCallResult {
        profile_id: provider.profile_id.clone(),
        tool_name: provider.tool_name.clone(),
        result,
        stderr_summary: None,
    })
}

pub async fn call_profile_http_tool(
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
    let profile = load_remote_profile(db, &provider.profile_id)?;
    if profile.transport == "sse" {
        return Err(runtime_error(
            McpRuntimeFailureKind::PolicyDenied,
            "unsupported_transport: MCP SSE runtime is not implemented",
        ));
    }
    let result = call_http_tool(
        McpHttpLaunch {
            url: profile.url,
            request_timeout: options.request_timeout,
            max_response_bytes: options.max_stdout_line_bytes,
            allow_localhost_dev: profile.allow_localhost_dev,
        },
        provider.tool_name.clone(),
        arguments,
    )
    .await?;
    Ok(McpToolCallResult {
        profile_id: provider.profile_id.clone(),
        tool_name: provider.tool_name.clone(),
        result,
        stderr_summary: None,
    })
}

pub async fn call_profile_tool(
    db: &Database,
    provider: &crate::ai_runtime::capability_resolver::ResolvedCapabilityProvider,
    arguments: serde_json::Value,
    options: McpHostRuntimeOptions,
) -> AppResult<McpToolCallResult> {
    match load_profile_transport(db, &provider.profile_id)?.as_str() {
        "stdio" => call_profile_stdio_tool(db, provider, arguments, options).await,
        "http" | "https" => call_profile_http_tool(db, provider, arguments, options).await,
        "sse" => Err(runtime_error(
            McpRuntimeFailureKind::PolicyDenied,
            "unsupported_transport: MCP SSE runtime is not implemented",
        )),
        other => Err(runtime_error(
            McpRuntimeFailureKind::PolicyDenied,
            format!("unsupported_transport: {other}"),
        )),
    }
}

pub async fn call_required_capability_stdio(
    db: &Database,
    capability: &str,
    arguments: serde_json::Value,
    options: McpHostRuntimeOptions,
) -> AppResult<McpToolCallResult> {
    let provider =
        crate::ai_runtime::capability_resolver::resolve_required_capability(db, capability)?;
    call_profile_tool(db, &provider, arguments, options).await
}
pub async fn discover_profile_stdio_tools(
    db: &Database,
    profile_id: &str,
    options: McpHostRuntimeOptions,
) -> AppResult<McpStdioDiscovery> {
    let profile = load_stdio_profile(db, profile_id)?;
    let discovery = discover_stdio_tools(McpStdioLaunch {
        command: profile.command,
        args: profile.args,
        cwd: options.cwd,
        request_timeout: options.request_timeout,
        max_stdout_line_bytes: options.max_stdout_line_bytes,
        max_stderr_bytes: options.max_stderr_bytes,
    })
    .await?;

    record_discovered_tool_inventory(
        db,
        profile_id,
        &profile.capability_mapping_json,
        &discovery.tools,
    )?;

    Ok(discovery)
}

pub async fn discover_profile_http_tools_with_sender<F, Fut>(
    db: &Database,
    profile_id: &str,
    options: McpHostRuntimeOptions,
    sender: F,
) -> AppResult<McpStdioDiscovery>
where
    F: FnMut(serde_json::Value) -> Fut,
    Fut: Future<Output = AppResult<serde_json::Value>>,
{
    let profile = load_remote_profile(db, profile_id)?;
    if profile.transport == "sse" {
        return Err(runtime_error(
            McpRuntimeFailureKind::PolicyDenied,
            "unsupported_transport: MCP SSE runtime is not implemented",
        ));
    }
    let discovery = discover_http_tools_with_sender(
        McpHttpLaunch {
            url: profile.url,
            request_timeout: options.request_timeout,
            max_response_bytes: options.max_stdout_line_bytes,
            allow_localhost_dev: profile.allow_localhost_dev,
        },
        sender,
    )
    .await?;
    record_discovered_tool_inventory(
        db,
        profile_id,
        &profile.capability_mapping_json,
        &discovery.tools,
    )?;
    Ok(discovery)
}

pub async fn discover_profile_tools(
    db: &Database,
    profile_id: &str,
    options: McpHostRuntimeOptions,
) -> AppResult<McpStdioDiscovery> {
    match load_profile_transport(db, profile_id)?.as_str() {
        "stdio" => discover_profile_stdio_tools(db, profile_id, options).await,
        "http" | "https" => {
            let profile = load_remote_profile(db, profile_id)?;
            let discovery = discover_http_tools(McpHttpLaunch {
                url: profile.url,
                request_timeout: options.request_timeout,
                max_response_bytes: options.max_stdout_line_bytes,
                allow_localhost_dev: profile.allow_localhost_dev,
            })
            .await?;
            record_discovered_tool_inventory(
                db,
                profile_id,
                &profile.capability_mapping_json,
                &discovery.tools,
            )?;
            Ok(discovery)
        }
        "sse" => Err(runtime_error(
            McpRuntimeFailureKind::PolicyDenied,
            "unsupported_transport: MCP SSE runtime is not implemented",
        )),
        other => Err(runtime_error(
            McpRuntimeFailureKind::PolicyDenied,
            format!("unsupported_transport: {other}"),
        )),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    fn fake_server_source() -> &'static str {
        r##"
use std::io::{self, BufRead, Write};

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "ok".to_string());
    if mode == "invalid" {
        println!("not-json");
        return;
    }

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    let _initialize = lines.next();
    println!("{}", r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"fake-mcp","version":"0.1.0"}}}"#);
    io::stdout().flush().unwrap();

    let _initialized = lines.next();
    let request = lines.next().and_then(Result::ok).unwrap_or_default();
    eprintln!("fake server log");
    if request.contains("\"tools/call\"") {
        println!("{}", r#"{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"search result for iris"}],"isError":false}}"#);
    } else {
        println!("{}", r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"search","title":"Search","description":"Search the web","inputSchema":{"type":"object","properties":{"q":{"type":"string"}},"required":["q"]}}]}}"#);
    }
    io::stdout().flush().unwrap();
}
"##
    }

    fn compile_fake_server() -> PathBuf {
        let temp_dir = tempfile::tempdir().unwrap();
        let dir = temp_dir.keep();
        let src = dir.join("fake_mcp_server.rs");
        let exe = dir.join(if cfg!(windows) {
            "fake_mcp_server.exe"
        } else {
            "fake_mcp_server"
        });
        fs::write(&src, fake_server_source()).unwrap();
        let output = Command::new("rustc")
            .arg(&src)
            .arg("-o")
            .arg(&exe)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "rustc failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        exe
    }

    fn launch(command: PathBuf) -> McpStdioLaunch {
        McpStdioLaunch {
            command,
            args: Vec::new(),
            cwd: None,
            request_timeout: Duration::from_secs(5),
            max_stdout_line_bytes: 16 * 1024,
            max_stderr_bytes: 1024,
        }
    }

    #[tokio::test]
    async fn stdio_discovery_initializes_and_lists_tools() {
        let exe = compile_fake_server();
        let discovery = discover_stdio_tools(launch(exe)).await.unwrap();

        assert_eq!(discovery.protocol_version, MCP_PROTOCOL_VERSION);
        assert_eq!(discovery.server_name, "fake-mcp");
        assert_eq!(discovery.server_version, Some("0.1.0".into()));
        assert_eq!(discovery.tools.len(), 1);
        assert_eq!(discovery.tools[0].name, "search");
        assert_eq!(discovery.tools[0].title, Some("Search".into()));
        assert_eq!(discovery.tools[0].input_schema["type"], "object");
        assert!(discovery
            .stderr_summary
            .unwrap()
            .contains("fake server log"));
    }

    #[tokio::test]
    async fn stdio_discovery_normalizes_invalid_stdout_as_invalid_response() {
        let exe = compile_fake_server();
        let mut launch = launch(exe);
        launch.args.push("invalid".into());

        let err = discover_stdio_tools(launch).await.unwrap_err();
        assert!(err.to_string().contains("invalid_response"));
    }

    #[tokio::test]
    async fn mcp_tools_call_mapped_capability_invokes_stdio_tools_call() {
        use crate::ai_runtime::mcp_runtime_registry::{
            record_tool_inventory, upsert_runtime_profile, upsert_server_catalog,
            McpRuntimeProfileInput, McpRuntimeStatus, McpServerCatalogInput, McpToolInventoryInput,
        };
        use crate::storage::db::Database;

        let db = Database::open_in_memory().unwrap();
        let exe = compile_fake_server();
        upsert_server_catalog(
            &db,
            &McpServerCatalogInput {
                id: "fake".into(),
                display_name: "Fake MCP".into(),
                transport: "stdio".into(),
                command: Some(exe.to_string_lossy().into_owned()),
                args_json: "[]".into(),
                url: None,
                env_schema_json: "{}".into(),
                capability_tags_json: "{\"capability\":\"web.search\"}".into(),
                source: "test".into(),
            },
        )
        .unwrap();
        upsert_runtime_profile(
            &db,
            &McpRuntimeProfileInput {
                id: "fake-default".into(),
                server_id: "fake".into(),
                vault_scope_hash: None,
                display_name: "Fake default".into(),
                enabled: true,
                transport_config_json: "{}".into(),
                env_bindings_json: "{}".into(),
                status: McpRuntimeStatus::Ready,
                last_error: None,
            },
        )
        .unwrap();
        record_tool_inventory(
            &db,
            &McpToolInventoryInput {
                profile_id: "fake-default".into(),
                tool_name: "search".into(),
                schema_hash: "sha256:test".into(),
                capability_mapping_json: "{\"capability\":\"web.search\"}".into(),
                description: Some("Search the web".into()),
            },
        )
        .unwrap();

        let call = call_required_capability_stdio(
            &db,
            "web.search",
            json!({"q": "iris"}),
            McpHostRuntimeOptions {
                request_timeout: Duration::from_secs(5),
                max_stdout_line_bytes: 16 * 1024,
                max_stderr_bytes: 1024,
                cwd: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(call.profile_id, "fake-default");
        assert_eq!(call.tool_name, "search");
        assert_eq!(call.result["isError"], false);
        assert_eq!(call.result["content"][0]["text"], "search result for iris");
    }

    #[tokio::test]
    async fn http_discovery_initializes_and_lists_tools_with_mock_sender() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let seen_calls = seen.clone();
        let discovery = discover_http_tools_with_sender(
            McpHttpLaunch {
                url: "https://example.com/mcp".into(),
                request_timeout: Duration::from_secs(5),
                max_response_bytes: 16 * 1024,
                allow_localhost_dev: false,
            },
            move |request: serde_json::Value| {
                let seen_calls = seen_calls.clone();
                async move {
                    let method = request
                        .get("method")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string();
                    seen_calls.lock().unwrap().push(method.clone());
                    match method.as_str() {
                        "initialize" => Ok(json!({
                            "jsonrpc": "2.0",
                            "id": 1,
                            "result": {
                                "protocolVersion": MCP_PROTOCOL_VERSION,
                                "serverInfo": {"name": "http-mcp", "version": "0.2.0"}
                            }
                        })),
                        "notifications/initialized" => Ok(json!({"accepted": true})),
                        "tools/list" => Ok(json!({
                            "jsonrpc": "2.0",
                            "id": 2,
                            "result": {
                                "tools": [{
                                    "name": "search",
                                    "title": "Search",
                                    "description": "HTTP search",
                                    "inputSchema": {"type": "object", "properties": {"q": {"type": "string"}}}
                                }]
                            }
                        })),
                        _ => Err(AppError::msg("unexpected MCP method")),
                    }
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(discovery.protocol_version, MCP_PROTOCOL_VERSION);
        assert_eq!(discovery.server_name, "http-mcp");
        assert_eq!(discovery.server_version, Some("0.2.0".into()));
        assert_eq!(discovery.tools.len(), 1);
        assert_eq!(discovery.tools[0].name, "search");
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            ["initialize", "notifications/initialized", "tools/list"]
        );
    }

    #[tokio::test]
    async fn http_discovery_rejects_unsafe_targets_before_send() {
        let sent = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let sent_count = sent.clone();
        let err = discover_http_tools_with_sender(
            McpHttpLaunch {
                url: "http://example.com/mcp".into(),
                request_timeout: Duration::from_secs(5),
                max_response_bytes: 16 * 1024,
                allow_localhost_dev: false,
            },
            move |_request| {
                let sent_count = sent_count.clone();
                async move {
                    sent_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok(json!({}))
                }
            },
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("network_denied"));
        assert_eq!(sent.load(std::sync::atomic::Ordering::SeqCst), 0);

        let err = validate_mcp_http_runtime_url("https://169.254.169.254/mcp", false).unwrap_err();
        assert!(err.to_string().contains("network_denied"));
    }
    #[tokio::test]
    async fn profile_http_discovery_persists_tool_inventory_with_mock_sender() {
        use crate::ai_runtime::mcp_runtime_registry::{
            list_tool_inventory, upsert_runtime_profile, upsert_server_catalog,
            McpRuntimeProfileInput, McpRuntimeStatus, McpServerCatalogInput,
        };
        use crate::storage::db::Database;

        let db = Database::open_in_memory().unwrap();
        upsert_server_catalog(
            &db,
            &McpServerCatalogInput {
                id: "http-fake".into(),
                display_name: "HTTP Fake MCP".into(),
                transport: "http".into(),
                command: None,
                args_json: "[]".into(),
                url: Some("https://example.com/mcp".into()),
                env_schema_json: "{}".into(),
                capability_tags_json: "[\"web.search\"]".into(),
                source: "test".into(),
            },
        )
        .unwrap();
        upsert_runtime_profile(
            &db,
            &McpRuntimeProfileInput {
                id: "http-fake-default".into(),
                server_id: "http-fake".into(),
                vault_scope_hash: None,
                display_name: "HTTP fake default".into(),
                enabled: true,
                transport_config_json: "{}".into(),
                env_bindings_json: "{}".into(),
                status: McpRuntimeStatus::Unknown,
                last_error: None,
            },
        )
        .unwrap();

        let discovery = discover_profile_http_tools_with_sender(
            &db,
            "http-fake-default",
            McpHostRuntimeOptions {
                request_timeout: Duration::from_secs(5),
                max_stdout_line_bytes: 16 * 1024,
                max_stderr_bytes: 1024,
                cwd: None,
            },
            |request| async move {
                match request.get("method").and_then(|value| value.as_str()) {
                    Some("initialize") => Ok(json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "result": {
                            "protocolVersion": MCP_PROTOCOL_VERSION,
                            "serverInfo": {"name": "http-profile-mcp", "version": "1.0.0"}
                        }
                    })),
                    Some("notifications/initialized") => Ok(json!({"accepted": true})),
                    Some("tools/list") => Ok(json!({
                        "jsonrpc": "2.0",
                        "id": 2,
                        "result": {
                            "tools": [{
                                "name": "search",
                                "description": "HTTP profile search",
                                "inputSchema": {"type": "object", "properties": {"q": {"type": "string"}}}
                            }]
                        }
                    })),
                    _ => Err(AppError::msg("unexpected MCP HTTP profile method")),
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(discovery.server_name, "http-profile-mcp");
        assert_eq!(discovery.tools[0].name, "search");
        let inventory = list_tool_inventory(&db, "http-fake-default").unwrap();
        assert_eq!(inventory.len(), 1);
        assert_eq!(inventory[0].tool_name, "search");
        assert_eq!(inventory[0].capability_mapping_json, "[\"web.search\"]");
    }

    #[tokio::test]
    async fn http_mcp_tools_call_invokes_mapped_capability_with_mock_sender() {
        use crate::ai_runtime::capability_resolver::resolve_required_capability;
        use crate::ai_runtime::mcp_runtime_registry::{
            record_tool_inventory, upsert_runtime_profile, upsert_server_catalog,
            McpRuntimeProfileInput, McpRuntimeStatus, McpServerCatalogInput, McpToolInventoryInput,
        };
        use crate::storage::db::Database;

        let db = Database::open_in_memory().unwrap();
        upsert_server_catalog(
            &db,
            &McpServerCatalogInput {
                id: "http-fake".into(),
                display_name: "HTTP Fake MCP".into(),
                transport: "http".into(),
                command: None,
                args_json: "[]".into(),
                url: Some("https://example.com/mcp".into()),
                env_schema_json: "{}".into(),
                capability_tags_json: "[\"web.search\"]".into(),
                source: "test".into(),
            },
        )
        .unwrap();
        upsert_runtime_profile(
            &db,
            &McpRuntimeProfileInput {
                id: "http-fake-default".into(),
                server_id: "http-fake".into(),
                vault_scope_hash: None,
                display_name: "HTTP fake default".into(),
                enabled: true,
                transport_config_json: "{}".into(),
                env_bindings_json: "{}".into(),
                status: McpRuntimeStatus::Ready,
                last_error: None,
            },
        )
        .unwrap();
        record_tool_inventory(
            &db,
            &McpToolInventoryInput {
                profile_id: "http-fake-default".into(),
                tool_name: "search".into(),
                schema_hash: "sha256:test".into(),
                capability_mapping_json: "{\"capability\":\"web.search\"}".into(),
                description: Some("HTTP search".into()),
            },
        )
        .unwrap();
        let provider = resolve_required_capability(&db, "web.search").unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let seen_calls = seen.clone();

        let call = call_profile_http_tool_with_sender(
            &db,
            &provider,
            json!({"q": "iris"}),
            McpHostRuntimeOptions {
                request_timeout: Duration::from_secs(5),
                max_stdout_line_bytes: 16 * 1024,
                max_stderr_bytes: 1024,
                cwd: None,
            },
            move |request| {
                let seen_calls = seen_calls.clone();
                async move {
                    let method = request
                        .get("method")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string();
                    seen_calls.lock().unwrap().push(method.clone());
                    match method.as_str() {
                        "initialize" => Ok(json!({
                            "jsonrpc": "2.0",
                            "id": 1,
                            "result": {
                                "protocolVersion": MCP_PROTOCOL_VERSION,
                                "serverInfo": {"name": "http-profile-mcp", "version": "1.0.0"}
                            }
                        })),
                        "notifications/initialized" => Ok(json!({"accepted": true})),
                        "tools/call" => Ok(json!({
                            "jsonrpc": "2.0",
                            "id": 2,
                            "result": {
                                "content": [{"type": "text", "text": "http search result for iris"}],
                                "isError": false
                            }
                        })),
                        _ => Err(AppError::msg("unexpected MCP HTTP profile method")),
                    }
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(call.profile_id, "http-fake-default");
        assert_eq!(call.tool_name, "search");
        assert_eq!(call.result["isError"], false);
        assert_eq!(
            call.result["content"][0]["text"],
            "http search result for iris"
        );
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            ["initialize", "notifications/initialized", "tools/call"]
        );
    }

    #[tokio::test]
    async fn profile_sse_discovery_returns_stable_unsupported_transport() {
        use crate::ai_runtime::mcp_runtime_registry::{
            upsert_runtime_profile, upsert_server_catalog, McpRuntimeProfileInput,
            McpRuntimeStatus, McpServerCatalogInput,
        };
        use crate::storage::db::Database;

        let db = Database::open_in_memory().unwrap();
        upsert_server_catalog(
            &db,
            &McpServerCatalogInput {
                id: "sse-fake".into(),
                display_name: "SSE Fake MCP".into(),
                transport: "sse".into(),
                command: None,
                args_json: "[]".into(),
                url: Some("https://example.com/sse".into()),
                env_schema_json: "{}".into(),
                capability_tags_json: "[\"web.search\"]".into(),
                source: "test".into(),
            },
        )
        .unwrap();
        upsert_runtime_profile(
            &db,
            &McpRuntimeProfileInput {
                id: "sse-fake-default".into(),
                server_id: "sse-fake".into(),
                vault_scope_hash: None,
                display_name: "SSE fake default".into(),
                enabled: true,
                transport_config_json: "{}".into(),
                env_bindings_json: "{}".into(),
                status: McpRuntimeStatus::Unknown,
                last_error: None,
            },
        )
        .unwrap();

        let err = discover_profile_tools(
            &db,
            "sse-fake-default",
            McpHostRuntimeOptions {
                request_timeout: Duration::from_secs(5),
                max_stdout_line_bytes: 16 * 1024,
                max_stderr_bytes: 1024,
                cwd: None,
            },
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("unsupported_transport"));
    }

    #[tokio::test]
    async fn profile_stdio_discovery_persists_tool_inventory() {
        use crate::ai_runtime::mcp_runtime_registry::{
            list_tool_inventory, upsert_runtime_profile, upsert_server_catalog,
            McpRuntimeProfileInput, McpRuntimeStatus, McpServerCatalogInput,
        };
        use crate::storage::db::Database;

        let db = Database::open_in_memory().unwrap();
        let exe = compile_fake_server();
        upsert_server_catalog(
            &db,
            &McpServerCatalogInput {
                id: "fake".into(),
                display_name: "Fake MCP".into(),
                transport: "stdio".into(),
                command: Some(exe.to_string_lossy().into_owned()),
                args_json: "[]".into(),
                url: None,
                env_schema_json: "{}".into(),
                capability_tags_json: "[\"web.search\"]".into(),
                source: "test".into(),
            },
        )
        .unwrap();
        upsert_runtime_profile(
            &db,
            &McpRuntimeProfileInput {
                id: "fake-default".into(),
                server_id: "fake".into(),
                vault_scope_hash: None,
                display_name: "Fake default".into(),
                enabled: true,
                transport_config_json: "{}".into(),
                env_bindings_json: "{}".into(),
                status: McpRuntimeStatus::Unknown,
                last_error: None,
            },
        )
        .unwrap();

        let discovery = discover_profile_stdio_tools(
            &db,
            "fake-default",
            McpHostRuntimeOptions {
                request_timeout: Duration::from_secs(5),
                max_stdout_line_bytes: 16 * 1024,
                max_stderr_bytes: 1024,
                cwd: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(discovery.tools[0].name, "search");
        let inventory = list_tool_inventory(&db, "fake-default").unwrap();
        assert_eq!(inventory.len(), 1);
        assert_eq!(inventory[0].tool_name, "search");
        assert!(inventory[0].schema_hash.starts_with("sha256:"));
        assert_eq!(inventory[0].capability_mapping_json, "[\"web.search\"]");
    }
}
