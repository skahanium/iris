//! Persistent registry for MCP runtime profiles and skill runtime requirements.
//!
//! This module stores configuration and health summaries only. It never starts
//! external processes and never handles raw secrets.

use std::net::IpAddr;

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::ai_runtime::skills::SkillScope;
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpRuntimeStatus {
    Unknown,
    Ready,
    Degraded,
    Unavailable,
    Blocked,
}

impl McpRuntimeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Ready => "ready",
            Self::Degraded => "degraded",
            Self::Unavailable => "unavailable",
            Self::Blocked => "blocked",
        }
    }

    fn from_db(value: &str) -> Self {
        match value {
            "ready" => Self::Ready,
            "degraded" => Self::Degraded,
            "unavailable" => Self::Unavailable,
            "blocked" => Self::Blocked,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerCatalogInput {
    pub id: String,
    pub display_name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args_json: String,
    pub url: Option<String>,
    pub env_schema_json: String,
    pub capability_tags_json: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpRuntimeProfileInput {
    pub id: String,
    pub server_id: String,
    pub vault_scope_hash: Option<String>,
    pub display_name: String,
    pub enabled: bool,
    pub transport_config_json: String,
    pub env_bindings_json: String,
    pub status: McpRuntimeStatus,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpRuntimeProfileSummary {
    pub id: String,
    pub server_id: String,
    pub display_name: String,
    pub enabled: bool,
    pub status: McpRuntimeStatus,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpToolInventoryInput {
    pub profile_id: String,
    pub tool_name: String,
    pub schema_hash: String,
    pub capability_mapping_json: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpToolInventorySummary {
    pub profile_id: String,
    pub tool_name: String,
    pub schema_hash: String,
    pub capability_mapping_json: String,
    pub description: Option<String>,
    pub last_discovered_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpHealthEventInput {
    pub profile_id: String,
    pub status: McpRuntimeStatus,
    pub reason_code: String,
    pub message: Option<String>,
    pub metadata_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpHealthEventSummary {
    pub id: i64,
    pub profile_id: String,
    pub status: McpRuntimeStatus,
    pub reason_code: String,
    pub message: Option<String>,
    pub metadata_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillRuntimeRequirementInput {
    pub skill_name: String,
    pub scope: SkillScope,
    pub manifest_hash: Option<String>,
    pub kind: String,
    pub runtime_kind: String,
    pub required_profiles_json: String,
    pub required_capabilities_json: String,
    pub workspace_contract_json: String,
    pub degradation_policy_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillRuntimeReadiness {
    pub runtime_kind: String,
    pub ready: bool,
    pub status: McpRuntimeStatus,
    pub required_profiles: Vec<String>,
    pub missing_profiles: Vec<String>,
    pub degraded_reasons: Vec<String>,
}

fn scope_db(scope: SkillScope) -> &'static str {
    match scope {
        SkillScope::Global => "Global",
        SkillScope::Vault => "Vault",
    }
}

pub fn upsert_server_catalog(db: &Database, input: &McpServerCatalogInput) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO mcp_server_catalog
             (id, display_name, transport, command, args_json, url, env_schema_json,
              capability_tags_json, source, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
               display_name = excluded.display_name,
               transport = excluded.transport,
               command = excluded.command,
               args_json = excluded.args_json,
               url = excluded.url,
               env_schema_json = excluded.env_schema_json,
               capability_tags_json = excluded.capability_tags_json,
               source = excluded.source,
               updated_at = datetime('now')",
            params![
                input.id,
                input.display_name,
                input.transport,
                input.command,
                input.args_json,
                input.url,
                input.env_schema_json,
                input.capability_tags_json,
                input.source
            ],
        )?;
        Ok(())
    })
}

#[derive(Debug, Clone)]
struct McpServerRuntimeConfig {
    transport: String,
    command: Option<String>,
    args_json: String,
    url: Option<String>,
}

fn config_json(value: &str) -> AppResult<serde_json::Value> {
    serde_json::from_str(value)
        .map_err(|err| AppError::msg(format!("invalid MCP transport config JSON: {err}")))
}

fn optional_config_string(config: &serde_json::Value, key: &str) -> Option<String> {
    config
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn validate_no_sensitive_json(label: &str, value: &serde_json::Value) -> AppResult<()> {
    let raw = value.to_string().to_lowercase();
    for marker in [
        "api_key",
        "apikey",
        "access_token",
        "bearer",
        "password",
        "secret",
        "token=",
        "sk-",
    ] {
        if raw.contains(marker) {
            return Err(AppError::msg(format!(
                "MCP {label} must not contain raw secret material"
            )));
        }
    }
    Ok(())
}

fn validate_stdio_command(command: &str) -> AppResult<()> {
    let command = command.trim();
    if command.is_empty() {
        return Err(AppError::msg("MCP stdio command is required"));
    }
    let lower = command.to_lowercase();
    let banned_commands = [
        "cmd",
        "cmd.exe",
        "powershell",
        "powershell.exe",
        "pwsh",
        "pwsh.exe",
        "sh",
        "bash",
        "zsh",
        "fish",
        "npm",
        "npm.cmd",
        "npx",
        "npx.cmd",
        "pnpm",
        "pnpm.cmd",
        "yarn",
        "yarn.cmd",
        "bun",
        "bunx",
    ];
    if banned_commands.contains(&lower.as_str()) {
        let reason = if matches!(
            lower.as_str(),
            "npm"
                | "npm.cmd"
                | "npx"
                | "npx.cmd"
                | "pnpm"
                | "pnpm.cmd"
                | "yarn"
                | "yarn.cmd"
                | "bun"
                | "bunx"
        ) {
            "MCP stdio command may not invoke a package manager"
        } else {
            "MCP stdio command may not invoke a shell wrapper"
        };
        return Err(AppError::msg(reason));
    }
    if command
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '|' | '&' | ';' | '<' | '>' | '`' | '$'))
    {
        return Err(AppError::msg(
            "MCP stdio command must be a structured executable path, not a shell string",
        ));
    }
    Ok(())
}

fn validate_stdio_args(args_json: &str) -> AppResult<()> {
    let args: serde_json::Value = serde_json::from_str(args_json)
        .map_err(|err| AppError::msg(format!("invalid MCP stdio args JSON: {err}")))?;
    let Some(items) = args.as_array() else {
        return Err(AppError::msg("MCP stdio args must be a JSON array"));
    };
    for item in items {
        let Some(arg) = item.as_str() else {
            return Err(AppError::msg("MCP stdio args must contain strings only"));
        };
        if arg.contains("\0") || arg.contains("<<") {
            return Err(AppError::msg("MCP stdio args contain unsafe shell syntax"));
        }
    }
    Ok(())
}

fn host_is_localhost_or_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

fn host_is_private_or_metadata(host: &str) -> bool {
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

fn url_contains_secret(parsed: &reqwest::Url) -> bool {
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return true;
    }
    parsed.query_pairs().any(|(key, value)| {
        let key = key.to_lowercase();
        let value = value.to_lowercase();
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

fn validate_http_url(url: &str, allow_localhost_dev: bool) -> AppResult<()> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|err| AppError::msg(format!("invalid MCP HTTP URL: {err}")))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::msg("MCP HTTP URL must include a host"))?;
    if url_contains_secret(&parsed) {
        return Err(AppError::msg(
            "MCP HTTP URL must not contain secret material",
        ));
    }
    if parsed.scheme() == "https" {
        if host_is_private_or_metadata(host)
            && !(allow_localhost_dev && host_is_localhost_or_loopback(host))
        {
            return Err(AppError::msg(
                "MCP HTTPS URL may not target private, loopback, or metadata hosts outside dev mode",
            ));
        }
        return Ok(());
    }
    if parsed.scheme() == "http" && allow_localhost_dev && host_is_localhost_or_loopback(host) {
        return Ok(());
    }
    Err(AppError::msg(
        "MCP HTTP transport requires HTTPS unless localhost dev mode is explicitly enabled",
    ))
}

fn validate_runtime_profile_transport(
    server: &McpServerRuntimeConfig,
    input: &McpRuntimeProfileInput,
) -> AppResult<()> {
    let config = config_json(&input.transport_config_json)?;
    let env_bindings = config_json(&input.env_bindings_json)?;
    validate_no_sensitive_json("env bindings", &env_bindings)?;
    let transport = server.transport.trim().to_lowercase();
    match transport.as_str() {
        "stdio" => {
            let command = optional_config_string(&config, "command")
                .or_else(|| server.command.clone())
                .ok_or_else(|| AppError::msg("MCP stdio command is required"))?;
            validate_stdio_command(&command)?;
            let args_json = config
                .get("args")
                .map(serde_json::Value::to_string)
                .unwrap_or_else(|| server.args_json.clone());
            validate_stdio_args(&args_json)
        }
        "http" | "https" | "sse" => {
            let url = optional_config_string(&config, "url")
                .or_else(|| server.url.clone())
                .ok_or_else(|| AppError::msg("MCP HTTP URL is required"))?;
            let allow_localhost_dev = config
                .get("allow_localhost_dev")
                .and_then(|value| value.as_bool())
                == Some(true);
            validate_http_url(&url, allow_localhost_dev)
        }
        _ => Err(AppError::msg(format!(
            "unsupported MCP transport: {}",
            server.transport
        ))),
    }
}

pub fn upsert_runtime_profile(db: &Database, input: &McpRuntimeProfileInput) -> AppResult<()> {
    db.with_conn(|conn| {
        let server = conn.query_row(
            "SELECT transport, command, args_json, url FROM mcp_server_catalog WHERE id = ?1",
            [&input.server_id],
            |row| {
                Ok(McpServerRuntimeConfig {
                    transport: row.get(0)?,
                    command: row.get(1)?,
                    args_json: row.get(2)?,
                    url: row.get(3)?,
                })
            },
        )?;
        validate_runtime_profile_transport(&server, input)?;
        conn.execute(
            "INSERT INTO mcp_runtime_profiles
             (id, server_id, vault_scope_hash, display_name, enabled, transport_config_json,
              env_bindings_json, status, last_checked_at, last_error, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'), ?9, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
               server_id = excluded.server_id,
               vault_scope_hash = excluded.vault_scope_hash,
               display_name = excluded.display_name,
               enabled = excluded.enabled,
               transport_config_json = excluded.transport_config_json,
               env_bindings_json = excluded.env_bindings_json,
               status = excluded.status,
               last_checked_at = excluded.last_checked_at,
               last_error = excluded.last_error,
               updated_at = datetime('now')",
            params![
                input.id,
                input.server_id,
                input.vault_scope_hash,
                input.display_name,
                input.enabled as i64,
                input.transport_config_json,
                input.env_bindings_json,
                input.status.as_str(),
                input.last_error
            ],
        )?;
        Ok(())
    })
}

pub fn set_runtime_profile_enabled(
    db: &Database,
    profile_id: &str,
    enabled: bool,
) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE mcp_runtime_profiles
             SET enabled = ?2, updated_at = datetime('now')
             WHERE id = ?1",
            params![profile_id, enabled as i64],
        )?;
        Ok(())
    })
}

pub fn delete_runtime_profile(db: &Database, profile_id: &str) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "DELETE FROM mcp_runtime_profiles WHERE id = ?1",
            [profile_id],
        )?;
        Ok(())
    })
}

pub fn list_runtime_profiles(db: &Database) -> AppResult<Vec<McpRuntimeProfileSummary>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, server_id, display_name, enabled, status, last_error
             FROM mcp_runtime_profiles
             ORDER BY display_name, id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(McpRuntimeProfileSummary {
                id: row.get(0)?,
                server_id: row.get(1)?,
                display_name: row.get(2)?,
                enabled: row.get::<_, i64>(3)? != 0,
                status: McpRuntimeStatus::from_db(&row.get::<_, String>(4)?),
                last_error: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
}

pub fn record_tool_inventory(db: &Database, input: &McpToolInventoryInput) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO mcp_tool_inventory
             (profile_id, tool_name, schema_hash, capability_mapping_json, description, last_discovered_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
             ON CONFLICT(profile_id, tool_name) DO UPDATE SET
               schema_hash = excluded.schema_hash,
               capability_mapping_json = excluded.capability_mapping_json,
               description = excluded.description,
               last_discovered_at = datetime('now')",
            params![
                input.profile_id,
                input.tool_name,
                input.schema_hash,
                input.capability_mapping_json,
                input.description
            ],
        )?;
        Ok(())
    })
}

pub fn list_tool_inventory(
    db: &Database,
    profile_id: &str,
) -> AppResult<Vec<McpToolInventorySummary>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT profile_id, tool_name, schema_hash, capability_mapping_json,
                    description, last_discovered_at
             FROM mcp_tool_inventory
             WHERE profile_id = ?1
             ORDER BY tool_name",
        )?;
        let rows = stmt.query_map([profile_id], |row| {
            Ok(McpToolInventorySummary {
                profile_id: row.get(0)?,
                tool_name: row.get(1)?,
                schema_hash: row.get(2)?,
                capability_mapping_json: row.get(3)?,
                description: row.get(4)?,
                last_discovered_at: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
}

pub fn record_health_event(db: &Database, input: &McpHealthEventInput) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO mcp_health_events
             (profile_id, status, reason_code, message, metadata_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
            params![
                input.profile_id,
                input.status.as_str(),
                input.reason_code,
                input.message,
                input.metadata_json
            ],
        )?;
        conn.execute(
            "UPDATE mcp_runtime_profiles
             SET status = ?2, last_checked_at = datetime('now'), last_error = ?3, updated_at = datetime('now')
             WHERE id = ?1",
            params![input.profile_id, input.status.as_str(), input.message],
        )?;
        Ok(())
    })
}

pub fn list_recent_health_events(
    db: &Database,
    profile_id: &str,
    limit: usize,
) -> AppResult<Vec<McpHealthEventSummary>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, profile_id, status, reason_code, message, metadata_json, created_at
             FROM mcp_health_events
             WHERE profile_id = ?1
             ORDER BY id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![profile_id, limit as i64], |row| {
            Ok(McpHealthEventSummary {
                id: row.get(0)?,
                profile_id: row.get(1)?,
                status: McpRuntimeStatus::from_db(&row.get::<_, String>(2)?),
                reason_code: row.get(3)?,
                message: row.get(4)?,
                metadata_json: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
}

pub fn upsert_skill_runtime_requirement(
    db: &Database,
    input: &SkillRuntimeRequirementInput,
) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO skill_runtime_requirements
             (skill_name, scope, manifest_hash, kind, runtime_kind, required_profiles_json,
              required_capabilities_json, workspace_contract_json, degradation_policy_json, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'))
             ON CONFLICT(skill_name, scope) DO UPDATE SET
               manifest_hash = excluded.manifest_hash,
               kind = excluded.kind,
               runtime_kind = excluded.runtime_kind,
               required_profiles_json = excluded.required_profiles_json,
               required_capabilities_json = excluded.required_capabilities_json,
               workspace_contract_json = excluded.workspace_contract_json,
               degradation_policy_json = excluded.degradation_policy_json,
               updated_at = datetime('now')",
            params![
                input.skill_name,
                scope_db(input.scope),
                input.manifest_hash,
                input.kind,
                input.runtime_kind,
                input.required_profiles_json,
                input.required_capabilities_json,
                input.workspace_contract_json,
                input.degradation_policy_json
            ],
        )?;
        Ok(())
    })
}

pub fn clear_skill_runtime_requirement(
    db: &Database,
    skill_name: &str,
    scope: SkillScope,
) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "DELETE FROM skill_runtime_requirements WHERE skill_name = ?1 AND scope = ?2",
            params![skill_name, scope_db(scope)],
        )?;
        Ok(())
    })
}

pub fn resolve_skill_runtime(
    db: &Database,
    skill_name: &str,
    scope: SkillScope,
) -> AppResult<SkillRuntimeReadiness> {
    db.with_conn(|conn| {
        let row: Option<(String, String)> = conn
            .query_row(
                "SELECT runtime_kind, required_profiles_json
                 FROM skill_runtime_requirements
                 WHERE skill_name = ?1 AND scope = ?2",
                params![skill_name, scope_db(scope)],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        let Some((runtime_kind, profiles_json)) = row else {
            return Ok(SkillRuntimeReadiness {
                runtime_kind: "not_applicable".to_string(),
                ready: true,
                status: McpRuntimeStatus::Ready,
                required_profiles: Vec::new(),
                missing_profiles: Vec::new(),
                degraded_reasons: Vec::new(),
            });
        };

        let required_profiles: Vec<String> =
            serde_json::from_str(&profiles_json).unwrap_or_default();
        if runtime_kind != "mcp" || required_profiles.is_empty() {
            return Ok(SkillRuntimeReadiness {
                runtime_kind,
                ready: true,
                status: McpRuntimeStatus::Ready,
                required_profiles,
                missing_profiles: Vec::new(),
                degraded_reasons: Vec::new(),
            });
        }

        let mut missing_profiles = Vec::new();
        let mut degraded_reasons = Vec::new();
        let mut worst_status = McpRuntimeStatus::Ready;
        for profile_id in &required_profiles {
            let profile: Option<(i64, String, Option<String>)> = conn
                .query_row(
                    "SELECT enabled, status, last_error FROM mcp_runtime_profiles WHERE id = ?1",
                    [profile_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            match profile {
                Some((enabled, status, _last_error)) if enabled != 0 && status == "ready" => {}
                Some((_enabled, status, last_error)) => {
                    worst_status = McpRuntimeStatus::from_db(&status);
                    degraded_reasons.push(
                        last_error
                            .unwrap_or_else(|| format!("MCP profile {profile_id} is {status}")),
                    );
                    missing_profiles.push(profile_id.clone());
                }
                None => {
                    worst_status = McpRuntimeStatus::Unavailable;
                    degraded_reasons.push(format!("MCP profile {profile_id} is not configured"));
                    missing_profiles.push(profile_id.clone());
                }
            }
        }

        Ok(SkillRuntimeReadiness {
            runtime_kind,
            ready: missing_profiles.is_empty(),
            status: if missing_profiles.is_empty() {
                McpRuntimeStatus::Ready
            } else {
                worst_status
            },
            required_profiles,
            missing_profiles,
            degraded_reasons,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> McpServerCatalogInput {
        McpServerCatalogInput {
            id: "anysearch".into(),
            display_name: "AnySearch".into(),
            transport: "stdio".into(),
            command: Some("anysearch-mcp".into()),
            args_json: "[]".into(),
            url: None,
            env_schema_json: "{}".into(),
            capability_tags_json: "[\"web.search\"]".into(),
            source: "test".into(),
        }
    }

    fn profile(status: McpRuntimeStatus, enabled: bool) -> McpRuntimeProfileInput {
        McpRuntimeProfileInput {
            id: "anysearch-default".into(),
            server_id: "anysearch".into(),
            vault_scope_hash: Some("vault".into()),
            display_name: "AnySearch default".into(),
            enabled,
            transport_config_json: "{}".into(),
            env_bindings_json: "{}".into(),
            status,
            last_error: None,
        }
    }

    fn requirement() -> SkillRuntimeRequirementInput {
        SkillRuntimeRequirementInput {
            skill_name: "anysearch".into(),
            scope: SkillScope::Vault,
            manifest_hash: Some("hash".into()),
            kind: "mcp_dependent".into(),
            runtime_kind: "mcp".into(),
            required_profiles_json: "[\"anysearch-default\"]".into(),
            required_capabilities_json: "[\"web.search\"]".into(),
            workspace_contract_json: "{}".into(),
            degradation_policy_json: "{}".into(),
        }
    }

    #[test]
    fn missing_requirement_is_ready_prompt_only_path() {
        let db = Database::open_in_memory().unwrap();
        let readiness = resolve_skill_runtime(&db, "plain", SkillScope::Vault).unwrap();

        assert_eq!(readiness.runtime_kind, "not_applicable");
        assert!(readiness.ready);
        assert!(readiness.required_profiles.is_empty());
    }

    #[test]
    fn mcp_requirement_is_ready_only_when_profile_is_ready() {
        let db = Database::open_in_memory().unwrap();
        upsert_server_catalog(&db, &server()).unwrap();
        upsert_runtime_profile(&db, &profile(McpRuntimeStatus::Ready, true)).unwrap();
        upsert_skill_runtime_requirement(&db, &requirement()).unwrap();

        let readiness = resolve_skill_runtime(&db, "anysearch", SkillScope::Vault).unwrap();
        assert!(readiness.ready);
        assert_eq!(readiness.status, McpRuntimeStatus::Ready);
        assert_eq!(readiness.required_profiles, vec!["anysearch-default"]);
    }

    #[test]
    fn disabled_mcp_profile_blocks_runtime_readiness() {
        let db = Database::open_in_memory().unwrap();
        upsert_server_catalog(&db, &server()).unwrap();
        upsert_runtime_profile(&db, &profile(McpRuntimeStatus::Ready, false)).unwrap();
        upsert_skill_runtime_requirement(&db, &requirement()).unwrap();

        let readiness = resolve_skill_runtime(&db, "anysearch", SkillScope::Vault).unwrap();
        assert!(!readiness.ready);
        assert_eq!(readiness.missing_profiles, vec!["anysearch-default"]);
    }

    #[test]
    fn profile_toggle_and_delete_updates_registry_and_readiness() {
        let db = Database::open_in_memory().unwrap();
        upsert_server_catalog(&db, &server()).unwrap();
        upsert_runtime_profile(&db, &profile(McpRuntimeStatus::Ready, true)).unwrap();
        upsert_skill_runtime_requirement(&db, &requirement()).unwrap();

        set_runtime_profile_enabled(&db, "anysearch-default", false).unwrap();
        let disabled = resolve_skill_runtime(&db, "anysearch", SkillScope::Vault).unwrap();
        assert!(!disabled.ready);
        assert_eq!(disabled.missing_profiles, vec!["anysearch-default"]);

        set_runtime_profile_enabled(&db, "anysearch-default", true).unwrap();
        let enabled = resolve_skill_runtime(&db, "anysearch", SkillScope::Vault).unwrap();
        assert!(enabled.ready);

        delete_runtime_profile(&db, "anysearch-default").unwrap();
        let deleted = resolve_skill_runtime(&db, "anysearch", SkillScope::Vault).unwrap();
        assert!(!deleted.ready);
        assert_eq!(deleted.status, McpRuntimeStatus::Unavailable);
    }

    #[test]
    fn runtime_profile_upsert_rejects_unsafe_stdio_transport() {
        let db = Database::open_in_memory().unwrap();
        let mut unsafe_server = server();
        unsafe_server.command = Some("cmd.exe /C anysearch".into());
        upsert_server_catalog(&db, &unsafe_server).unwrap();

        let err = upsert_runtime_profile(&db, &profile(McpRuntimeStatus::Ready, true)).unwrap_err();
        assert!(err.to_string().contains("stdio command"));

        let mut package_server = server();
        package_server.id = "npx-search".into();
        package_server.command = Some("npx".into());
        upsert_server_catalog(&db, &package_server).unwrap();
        let mut package_profile = profile(McpRuntimeStatus::Ready, true);
        package_profile.id = "npx-search-default".into();
        package_profile.server_id = "npx-search".into();
        let err = upsert_runtime_profile(&db, &package_profile).unwrap_err();
        assert!(err.to_string().contains("package manager"));
    }

    #[test]
    fn runtime_profile_upsert_rejects_unsafe_http_transport() {
        let db = Database::open_in_memory().unwrap();
        let mut http_server = server();
        http_server.id = "remote".into();
        http_server.transport = "http".into();
        http_server.command = None;
        http_server.url = Some("http://example.com/mcp".into());
        upsert_server_catalog(&db, &http_server).unwrap();
        let mut remote_profile = profile(McpRuntimeStatus::Ready, true);
        remote_profile.id = "remote-default".into();
        remote_profile.server_id = "remote".into();
        let err = upsert_runtime_profile(&db, &remote_profile).unwrap_err();
        assert!(err.to_string().contains("HTTPS"));

        let mut token_server = http_server;
        token_server.id = "token-remote".into();
        token_server.url = Some("https://example.com/mcp?api_key=secret".into());
        upsert_server_catalog(&db, &token_server).unwrap();
        let mut token_profile = remote_profile;
        token_profile.id = "token-remote-default".into();
        token_profile.server_id = "token-remote".into();
        let err = upsert_runtime_profile(&db, &token_profile).unwrap_err();
        assert!(err.to_string().contains("secret"));
    }

    #[test]
    fn runtime_profile_upsert_accepts_structured_stdio_and_https() {
        let db = Database::open_in_memory().unwrap();
        upsert_server_catalog(&db, &server()).unwrap();
        upsert_runtime_profile(&db, &profile(McpRuntimeStatus::Ready, true)).unwrap();

        let mut https_server = server();
        https_server.id = "remote".into();
        https_server.transport = "http".into();
        https_server.command = None;
        https_server.url = Some("https://example.com/mcp".into());
        upsert_server_catalog(&db, &https_server).unwrap();
        let mut remote_profile = profile(McpRuntimeStatus::Ready, true);
        remote_profile.id = "remote-default".into();
        remote_profile.server_id = "remote".into();
        upsert_runtime_profile(&db, &remote_profile).unwrap();

        let profiles = list_runtime_profiles(&db).unwrap();
        assert_eq!(profiles.len(), 2);
    }
    #[test]
    fn tool_inventory_and_health_events_are_metadata_only() {
        let db = Database::open_in_memory().unwrap();
        upsert_server_catalog(&db, &server()).unwrap();
        upsert_runtime_profile(&db, &profile(McpRuntimeStatus::Ready, true)).unwrap();

        record_tool_inventory(
            &db,
            &McpToolInventoryInput {
                profile_id: "anysearch-default".into(),
                tool_name: "search".into(),
                schema_hash: "sha256:abc".into(),
                capability_mapping_json: "[\"web.search\"]".into(),
                description: Some("Search the web".into()),
            },
        )
        .unwrap();
        let tools = list_tool_inventory(&db, "anysearch-default").unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].tool_name, "search");
        assert_eq!(tools[0].schema_hash, "sha256:abc");

        record_health_event(
            &db,
            &McpHealthEventInput {
                profile_id: "anysearch-default".into(),
                status: McpRuntimeStatus::Degraded,
                reason_code: "timeout".into(),
                message: Some("health check timed out".into()),
                metadata_json: "{\"duration_ms\":5000}".into(),
            },
        )
        .unwrap();
        let events = list_recent_health_events(&db, "anysearch-default", 5).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].reason_code, "timeout");
        assert!(events[0].metadata_json.contains("duration_ms"));
        assert!(!events[0].metadata_json.contains("raw_output"));
    }
}
