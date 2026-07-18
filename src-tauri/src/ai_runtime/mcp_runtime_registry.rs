//! Persistent registry for web evidence providers.
//!
//! MCP is represented here only as one provider kind for broker-owned
//! `web.search` / `web.fetch` calls. This module never starts external
//! processes and never handles raw secrets.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

pub const WEB_SEARCH_PROVIDER_ID_SETTING: &str = "web_search_provider_id";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebEvidenceProviderInput {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub enabled: bool,
    pub transport_kind: String,
    pub transport_config_json: String,
    pub credential_refs_json: String,
    pub web_search_mapping_json: Option<String>,
    pub web_fetch_mapping_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebEvidenceProviderSummary {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub enabled: bool,
    pub transport_kind: String,
    pub transport_config_json: String,
    pub credential_refs_json: String,
    pub web_search_mapping_json: Option<String>,
    pub web_fetch_mapping_json: Option<String>,
    pub provider_config_hash: String,
    pub has_search_mapping: bool,
    pub has_fetch_mapping: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebEvidenceProviderMappingSummary {
    pub id: String,
    pub kind: String,
    pub transport_kind: String,
    pub provider_config_hash: String,
    pub web_search_mapping_json: Option<String>,
    pub web_fetch_mapping_json: Option<String>,
}

/// Persisted, non-sensitive state obtained during an MCP initialization and
/// `tools/list` discovery.  It deliberately contains no tool descriptions or
/// server output: those can contain untrusted or user-controlled text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebEvidenceProviderRuntimeSummary {
    pub provider_id: String,
    pub protocol_version: String,
    pub server_name: String,
    pub server_version: Option<String>,
    pub capabilities_hash: String,
    pub mapping_hash: String,
    pub status: String,
    pub reason_code: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebEvidenceProviderHealthSummary {
    pub provider_id: String,
    pub success_count: i64,
    pub failure_count: i64,
    pub consecutive_failures: i64,
    pub latency_ewma_ms: Option<f64>,
    pub success_ewma: Option<f64>,
    pub last_failure_code: Option<String>,
    pub updated_at: String,
}

fn provider_config_hash(input: &WebEvidenceProviderInput) -> String {
    let raw = serde_json::json!({
        "id": input.id,
        "kind": input.kind,
        "transport_kind": input.transport_kind,
        "transport_config_json": input.transport_config_json,
        "credential_refs_json": input.credential_refs_json,
        "web_search_mapping_json": input.web_search_mapping_json,
        "web_fetch_mapping_json": input.web_fetch_mapping_json,
    });
    let digest = Sha256::digest(raw.to_string().as_bytes());
    hex::encode(&digest[..12])
}

fn short_hash(value: &serde_json::Value) -> String {
    let digest = Sha256::digest(value.to_string().as_bytes());
    hex::encode(&digest[..12])
}

fn validate_provider_identifier(label: &str, value: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::msg(format!("{label} is required")));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(AppError::msg(format!(
            "{label} contains unsupported characters"
        )));
    }
    Ok(value.to_string())
}

fn is_secretish_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    [
        "api_key",
        "apikey",
        "access_token",
        "authorization",
        "password",
        "secret",
        "token",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn is_credential_ref_string(value: &str) -> bool {
    let value = value.trim();
    let service = value.strip_prefix("credential://").unwrap_or(value);
    service.starts_with("iris.")
}

fn string_looks_like_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("bearer ")
        || lower.contains(" as_sk")
        || lower.starts_with("as_sk")
        || lower.contains("sk-")
        || lower.contains("api_key=")
        || lower.contains("access_token=")
        || lower.contains("token=")
        || lower.contains("password=")
}

fn reject_raw_secret(label: &str) -> AppError {
    AppError::msg(format!(
        "{label} must contain credential references, not raw secrets"
    ))
}

fn validate_provider_json_value(
    label: &str,
    value: &serde_json::Value,
    current_key: Option<&str>,
    secret_context: bool,
) -> AppResult<()> {
    match value {
        serde_json::Value::String(raw) => {
            if current_key
                .map(|key| key.eq_ignore_ascii_case("scheme"))
                .unwrap_or(false)
            {
                return Ok(());
            }
            if is_credential_ref_string(raw) {
                return Ok(());
            }
            if secret_context || string_looks_like_secret(raw) {
                return Err(reject_raw_secret(label));
            }
            Ok(())
        }
        serde_json::Value::Array(items) => {
            for item in items {
                validate_provider_json_value(label, item, current_key, secret_context)?;
            }
            Ok(())
        }
        serde_json::Value::Object(object) => {
            for (key, child) in object {
                let next_secret_context = secret_context || is_secretish_key(key);
                validate_provider_json_value(label, child, Some(key), next_secret_context)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_provider_json(label: &str, raw: &str) -> AppResult<()> {
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|err| AppError::msg(format!("invalid {label} JSON: {err}")))?;
    validate_provider_json_value(label, &value, None, false)
}

fn validate_optional_mapping(label: &str, raw: &Option<String>) -> AppResult<()> {
    let Some(raw) = raw
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    validate_provider_json(label, raw)
}

fn object_from_json(
    label: &str,
    raw: &str,
) -> AppResult<serde_json::Map<String, serde_json::Value>> {
    serde_json::from_str::<serde_json::Value>(raw)
        .map_err(|err| AppError::msg(format!("invalid {label} JSON: {err}")))?
        .as_object()
        .cloned()
        .ok_or_else(|| AppError::msg(format!("{label} must be a JSON object")))
}

/// Reject transport configuration that could make an MCP process inherit or
/// receive credentials before examining any values within that configuration.
///
/// Structural transport restrictions deliberately take precedence over the
/// generic raw-secret scan so an invalid `env` section is rejected as an
/// environment binding even when a value happens to resemble a secret.
fn validate_mcp_transport_config_security(
    transport_kind: &str,
    transport_config_json: &str,
) -> AppResult<()> {
    let transport = object_from_json("transport config", transport_config_json)?;
    match transport_kind {
        "stdio" if transport.contains_key("env") => Err(AppError::msg(
            "MCP stdio transport config must not define environment variables",
        )),
        "https" if transport.contains_key("env") || transport.contains_key("headers") => {
            Err(AppError::msg(
                "MCP HTTPS transport config must not contain env or headers; use credential refs.headers",
            ))
        }
        _ => Ok(()),
    }
}

pub(crate) fn validate_mcp_runtime_transport_security(
    transport_kind: &str,
    transport_config_json: &str,
    credential_refs_json: &str,
) -> AppResult<()> {
    validate_mcp_transport_config_security(transport_kind, transport_config_json)?;
    let credentials = object_from_json("credential refs", credential_refs_json)?;
    match transport_kind {
        "stdio" => {
            if !credentials.is_empty() {
                return Err(AppError::msg(
                    "MCP stdio providers cannot use credential or environment bindings; use HTTPS header credentials instead",
                ));
            }
        }
        "https" => {
            if credentials.contains_key("env") {
                return Err(AppError::msg(
                    "MCP HTTPS providers only support credential refs.headers, not environment bindings",
                ));
            }
            if credentials.keys().any(|key| key != "headers") {
                return Err(AppError::msg(
                    "MCP HTTPS credential refs may only contain headers",
                ));
            }
            if let Some(headers) = credentials.get("headers") {
                let headers = headers.as_object().ok_or_else(|| {
                    AppError::msg("MCP HTTPS credential refs.headers must be an object")
                })?;
                for (name, binding) in headers {
                    let header =
                        reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                            AppError::msg("MCP HTTPS credential header name is invalid")
                        })?;
                    if matches!(
                        header.as_str(),
                        "host"
                            | "content-length"
                            | "connection"
                            | "transfer-encoding"
                            | "mcp-protocol-version"
                    ) {
                        return Err(AppError::msg("MCP HTTPS credential header is reserved"));
                    }
                    let service = match binding {
                        serde_json::Value::String(value) => value,
                        serde_json::Value::Object(object) => object
                            .get("credential")
                            .or_else(|| object.get("service"))
                            .or_else(|| object.get("ref"))
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default(),
                        _ => "",
                    };
                    if !is_credential_ref_string(service) {
                        return Err(AppError::msg(
                            "MCP HTTPS header credentials must use credential://iris.* references",
                        ));
                    }
                    if let Some(scheme) = binding
                        .as_object()
                        .and_then(|object| object.get("scheme"))
                        .and_then(serde_json::Value::as_str)
                    {
                        if scheme.contains(['\r', '\n']) {
                            return Err(AppError::msg("MCP HTTPS credential scheme is invalid"));
                        }
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn ensure_search_mapping_result_limit(mapping_json: &str, max_results_arg: &str) -> Option<String> {
    let mut mapping = serde_json::from_str::<serde_json::Value>(mapping_json).ok()?;
    let object = mapping.as_object_mut()?;
    let has_tool = object
        .get("tool")
        .or_else(|| object.get("tool_name"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|tool| !tool.trim().is_empty());
    if !has_tool || object.contains_key("maxResultsArg") {
        return None;
    }
    object.insert(
        "maxResultsArg".into(),
        serde_json::Value::String(max_results_arg.to_string()),
    );
    serde_json::to_string(&mapping).ok()
}

fn heal_legacy_search_result_limit_mappings(conn: &rusqlite::Connection) -> AppResult<()> {
    let mut stmt = conn.prepare(
        "SELECT id, name, kind, enabled, transport_kind, transport_config_json,
                credential_refs_json, web_search_mapping_json, web_fetch_mapping_json
         FROM web_evidence_providers
         WHERE kind = 'mcp' AND web_search_mapping_json IS NOT NULL",
    )?;
    let candidates = stmt
        .query_map([], |row| {
            Ok(WebEvidenceProviderInput {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                enabled: row.get::<_, i64>(3)? != 0,
                transport_kind: row.get(4)?,
                transport_config_json: row.get(5)?,
                credential_refs_json: row.get(6)?,
                web_search_mapping_json: row.get(7)?,
                web_fetch_mapping_json: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    for mut input in candidates {
        let Some(max_results_arg) = crate::config_manifest::resolve_mcp_search_result_limit_arg(
            &input.transport_config_json,
            &input.name,
        ) else {
            continue;
        };
        let Some(current) = input.web_search_mapping_json.as_deref() else {
            continue;
        };
        let Some(healed) = ensure_search_mapping_result_limit(current, &max_results_arg) else {
            continue;
        };
        input.web_search_mapping_json = Some(healed.clone());
        let config_hash = provider_config_hash(&input);
        conn.execute(
            "UPDATE web_evidence_providers
             SET web_search_mapping_json = ?2,
                 provider_config_hash = ?3,
                 updated_at = datetime('now')
             WHERE id = ?1",
            params![input.id, healed, config_hash],
        )?;
    }
    Ok(())
}

fn normalize_provider_input(
    input: &WebEvidenceProviderInput,
) -> AppResult<WebEvidenceProviderInput> {
    let id = validate_provider_identifier("provider id", &input.id)?;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::msg("provider name is required"));
    }
    let kind = input.kind.trim().to_lowercase();
    if !matches!(kind.as_str(), "native" | "mcp") {
        return Err(AppError::msg("provider kind must be native or mcp"));
    }
    let transport_kind = input.transport_kind.trim().to_lowercase();
    if !matches!(transport_kind.as_str(), "native" | "stdio" | "https") {
        return Err(AppError::msg(
            "provider transport must be native, stdio, or https",
        ));
    }
    if kind == "native" && transport_kind != "native" {
        return Err(AppError::msg("native provider must use native transport"));
    }
    if kind == "mcp" && transport_kind == "native" {
        return Err(AppError::msg(
            "MCP provider must use stdio or https transport",
        ));
    }
    if kind == "mcp" {
        validate_mcp_transport_config_security(
            &transport_kind,
            input.transport_config_json.trim(),
        )?;
    }
    validate_provider_json("transport config", &input.transport_config_json)?;
    validate_provider_json("credential refs", &input.credential_refs_json)?;
    validate_optional_mapping("web.search mapping", &input.web_search_mapping_json)?;
    validate_optional_mapping("web.fetch mapping", &input.web_fetch_mapping_json)?;

    if kind == "mcp" {
        validate_mcp_runtime_transport_security(
            &transport_kind,
            input.transport_config_json.trim(),
            input.credential_refs_json.trim(),
        )?;
    }

    Ok(WebEvidenceProviderInput {
        id,
        name: name.to_string(),
        kind,
        enabled: input.enabled,
        transport_kind,
        transport_config_json: input.transport_config_json.trim().to_string(),
        credential_refs_json: input.credential_refs_json.trim().to_string(),
        web_search_mapping_json: {
            let mapping = input
                .web_search_mapping_json
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let max_results_arg = crate::config_manifest::resolve_mcp_search_result_limit_arg(
                input.transport_config_json.trim(),
                name,
            );
            match (mapping, max_results_arg) {
                (Some(value), Some(max_results_arg)) => Some(
                    ensure_search_mapping_result_limit(&value, &max_results_arg).unwrap_or(value),
                ),
                (mapping, _) => mapping,
            }
        },
        web_fetch_mapping_json: input
            .web_fetch_mapping_json
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    })
}

pub fn upsert_web_evidence_provider(
    db: &Database,
    input: &WebEvidenceProviderInput,
) -> AppResult<()> {
    let input = normalize_provider_input(input)?;
    let config_hash = provider_config_hash(&input);
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO web_evidence_providers
             (id, name, kind, enabled, transport_kind, transport_config_json,
              credential_refs_json, web_search_mapping_json, web_fetch_mapping_json,
              provider_config_hash, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               kind = excluded.kind,
               enabled = excluded.enabled,
               transport_kind = excluded.transport_kind,
               transport_config_json = excluded.transport_config_json,
               credential_refs_json = excluded.credential_refs_json,
               web_search_mapping_json = excluded.web_search_mapping_json,
               web_fetch_mapping_json = excluded.web_fetch_mapping_json,
               provider_config_hash = excluded.provider_config_hash,
               updated_at = datetime('now')",
            params![
                input.id,
                input.name,
                input.kind,
                if input.enabled { 1 } else { 0 },
                input.transport_kind,
                input.transport_config_json,
                input.credential_refs_json,
                input.web_search_mapping_json,
                input.web_fetch_mapping_json,
                config_hash
            ],
        )?;
        conn.execute(
            "DELETE FROM web_evidence_provider_runtime WHERE provider_id = ?1",
            [input.id.as_str()],
        )?;
        conn.execute(
            "DELETE FROM web_evidence_provider_health WHERE provider_id = ?1",
            [input.id.as_str()],
        )?;
        Ok(())
    })
}

pub fn record_web_evidence_provider_discovery(
    db: &Database,
    provider_id: &str,
    protocol_version: &str,
    server_name: &str,
    server_version: Option<&str>,
    tool_schema_hash: &str,
) -> AppResult<()> {
    let provider_id = validate_provider_identifier("provider id", provider_id)?;
    let mapping_hash = db.with_read_conn(|conn| {
        let (kind, transport_kind, web_search, web_fetch): (
            String,
            String,
            Option<String>,
            Option<String>,
        ) = conn.query_row(
            "SELECT kind, transport_kind, web_search_mapping_json, web_fetch_mapping_json
             FROM web_evidence_providers WHERE id = ?1",
            [provider_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        Ok(short_hash(&serde_json::json!({
            "providerId": provider_id,
            "kind": kind,
            "transport": transport_kind,
            "webSearch": web_search,
            "webFetch": web_fetch,
        })))
    })?;
    let capabilities_hash = short_hash(&serde_json::json!({"tools": tool_schema_hash}));
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO web_evidence_provider_runtime
             (provider_id, protocol_version, server_name, server_version, capabilities_hash, mapping_hash, status, reason_code, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'ready', 'discovered', datetime('now'))
             ON CONFLICT(provider_id) DO UPDATE SET protocol_version=excluded.protocol_version, server_name=excluded.server_name,
               server_version=excluded.server_version, capabilities_hash=excluded.capabilities_hash, mapping_hash=excluded.mapping_hash,
               status='ready', reason_code='discovered', updated_at=datetime('now')",
            params![provider_id, protocol_version, server_name, server_version, capabilities_hash, mapping_hash],
        )?;
        Ok(())
    })
}

pub fn record_web_evidence_provider_call(
    db: &Database,
    provider_id: &str,
    success: bool,
    latency_ms: u64,
    failure_code: Option<&str>,
) -> AppResult<()> {
    let provider_id = validate_provider_identifier("provider id", provider_id)?;
    let failure_code = failure_code
        .map(str::trim)
        .filter(|code| {
            !code.is_empty()
                && code.len() <= 64
                && code
                    .chars()
                    .all(|character| character.is_ascii_lowercase() || character == '_')
        })
        .map(str::to_string)
        .or_else(|| (!success).then(|| "unavailable".to_string()));
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO web_evidence_provider_health
             (provider_id, success_count, failure_count, consecutive_failures, latency_ewma_ms, success_ewma, last_failure_code, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))
             ON CONFLICT(provider_id) DO UPDATE SET
               success_count = success_count + excluded.success_count,
               failure_count = failure_count + excluded.failure_count,
               consecutive_failures = CASE WHEN excluded.success_count = 1 THEN 0 ELSE consecutive_failures + 1 END,
               latency_ewma_ms = CASE WHEN latency_ewma_ms IS NULL THEN excluded.latency_ewma_ms ELSE latency_ewma_ms * 0.8 + excluded.latency_ewma_ms * 0.2 END,
               success_ewma = CASE WHEN success_ewma IS NULL THEN excluded.success_ewma ELSE success_ewma * 0.8 + excluded.success_ewma * 0.2 END,
               last_failure_code = excluded.last_failure_code,
               updated_at = datetime('now')",
            params![provider_id, if success { 1 } else { 0 }, if success { 0 } else { 1 }, if success { 0 } else { 1 }, latency_ms as f64, if success { 1.0 } else { 0.0 }, failure_code.as_deref()],
        )?;
        conn.execute(
            "UPDATE web_evidence_provider_runtime
             SET status = CASE WHEN ?2 = 1 THEN 'ready' ELSE 'degraded' END,
                 reason_code = CASE WHEN ?2 = 1 THEN 'call_succeeded' ELSE COALESCE(?3, 'call_failed') END,
                 updated_at = datetime('now')
             WHERE provider_id = ?1",
            params![provider_id, if success { 1 } else { 0 }, failure_code.as_deref()],
        )?;
        Ok(())
    })
}

pub fn web_evidence_provider_runtime(
    db: &Database,
    provider_id: &str,
) -> AppResult<Option<WebEvidenceProviderRuntimeSummary>> {
    let provider_id = validate_provider_identifier("provider id", provider_id)?;
    db.with_read_conn(|conn| conn.query_row(
        "SELECT provider_id, protocol_version, server_name, server_version, capabilities_hash, mapping_hash, status, reason_code, updated_at FROM web_evidence_provider_runtime WHERE provider_id=?1",
        [provider_id],
        |row| Ok(WebEvidenceProviderRuntimeSummary { provider_id: row.get(0)?, protocol_version: row.get(1)?, server_name: row.get(2)?, server_version: row.get(3)?, capabilities_hash: row.get(4)?, mapping_hash: row.get(5)?, status: row.get(6)?, reason_code: row.get(7)?, updated_at: row.get(8)? }),
    ).optional().map_err(Into::into))
}

pub fn web_evidence_provider_health(
    db: &Database,
    provider_id: &str,
) -> AppResult<Option<WebEvidenceProviderHealthSummary>> {
    let provider_id = validate_provider_identifier("provider id", provider_id)?;
    db.with_read_conn(|conn| conn.query_row(
        "SELECT provider_id, success_count, failure_count, consecutive_failures, latency_ewma_ms, success_ewma, last_failure_code, updated_at FROM web_evidence_provider_health WHERE provider_id=?1",
        [provider_id],
        |row| Ok(WebEvidenceProviderHealthSummary { provider_id: row.get(0)?, success_count: row.get(1)?, failure_count: row.get(2)?, consecutive_failures: row.get(3)?, latency_ewma_ms: row.get(4)?, success_ewma: row.get(5)?, last_failure_code: row.get(6)?, updated_at: row.get(7)? }),
    ).optional().map_err(Into::into))
}

pub fn list_web_evidence_providers(db: &Database) -> AppResult<Vec<WebEvidenceProviderSummary>> {
    db.with_conn(|conn| {
        heal_legacy_search_result_limit_mappings(conn)?;
        let mut stmt = conn.prepare(
            "SELECT id, name, kind, enabled, transport_kind,
                    transport_config_json, credential_refs_json,
                    web_search_mapping_json, web_fetch_mapping_json,
                    provider_config_hash,
                    web_search_mapping_json IS NOT NULL,
                    web_fetch_mapping_json IS NOT NULL
             FROM web_evidence_providers
             ORDER BY kind, name, id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(WebEvidenceProviderSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                enabled: row.get::<_, i64>(3)? != 0,
                transport_kind: row.get(4)?,
                transport_config_json: row.get(5)?,
                credential_refs_json: row.get(6)?,
                web_search_mapping_json: row.get(7)?,
                web_fetch_mapping_json: row.get(8)?,
                provider_config_hash: row.get(9)?,
                has_search_mapping: row.get::<_, i64>(10)? != 0,
                has_fetch_mapping: row.get::<_, i64>(11)? != 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    })
}

pub fn list_enabled_web_provider_mappings(
    db: &Database,
) -> AppResult<Vec<WebEvidenceProviderMappingSummary>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, kind, transport_kind, provider_config_hash,
                    web_search_mapping_json, web_fetch_mapping_json
             FROM web_evidence_providers
             WHERE enabled = 1
             ORDER BY
               CASE kind WHEN 'mcp' THEN 0 WHEN 'native' THEN 1 ELSE 2 END,
               name,
               id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(WebEvidenceProviderMappingSummary {
                id: row.get(0)?,
                kind: row.get(1)?,
                transport_kind: row.get(2)?,
                provider_config_hash: row.get(3)?,
                web_search_mapping_json: row.get(4)?,
                web_fetch_mapping_json: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    })
}

pub fn list_enabled_web_search_provider_mappings(
    db: &Database,
) -> AppResult<Vec<WebEvidenceProviderMappingSummary>> {
    Ok(list_enabled_web_provider_mappings(db)?
        .into_iter()
        .filter(|provider| provider.kind == "mcp" && provider.web_search_mapping_json.is_some())
        .collect())
}

fn read_selected_web_search_provider_id(db: &Database) -> AppResult<Option<String>> {
    db.with_conn(|conn| {
        let raw: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [WEB_SEARCH_PROVIDER_ID_SETTING],
                |row| row.get(0),
            )
            .optional()?;
        Ok(raw.and_then(|value| normalize_settings_string_value(&value)))
    })
}

fn normalize_settings_string_value(raw: &str) -> Option<String> {
    let parsed = serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.to_string());
    let trimmed = parsed.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn save_selected_web_search_provider_id(
    db: &Database,
    provider_id: Option<&str>,
) -> AppResult<()> {
    let provider_id = provider_id.map(str::trim).filter(|value| !value.is_empty());
    db.with_conn(|conn| {
        if let Some(provider_id) = provider_id {
            let provider_id = validate_provider_identifier("provider id", provider_id)?;
            let json = serde_json::to_string(&provider_id)?;
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![WEB_SEARCH_PROVIDER_ID_SETTING, json],
            )?;
        } else {
            conn.execute(
                "DELETE FROM settings WHERE key = ?1",
                [WEB_SEARCH_PROVIDER_ID_SETTING],
            )?;
        }
        Ok(())
    })
}

pub fn resolve_selected_web_search_provider(
    db: &Database,
) -> AppResult<WebEvidenceProviderMappingSummary> {
    let providers = list_enabled_web_search_provider_mappings(db)?;
    if providers.is_empty() {
        return Err(AppError::msg(
            "web_search_provider_missing: no enabled MCP web.search provider is configured",
        ));
    }

    if let Some(selected_id) = read_selected_web_search_provider_id(db)? {
        return providers
            .into_iter()
            .find(|provider| provider.id == selected_id)
            .ok_or_else(|| {
                AppError::msg(
                    "web_search_provider_unavailable: selected MCP web.search provider is disabled, missing, or lacks a search mapping",
                )
            });
    }

    if providers.len() == 1 {
        return Ok(providers
            .into_iter()
            .next()
            .expect("checked provider count"));
    }

    Err(AppError::msg(
        "web_search_provider_unselected: choose one MCP web.search provider before enabling web search",
    ))
}

pub fn toggle_web_evidence_provider(
    db: &Database,
    provider_id: &str,
    enabled: bool,
) -> AppResult<()> {
    let provider_id = validate_provider_identifier("provider id", provider_id)?;
    db.with_conn(|conn| {
        let changed = conn.execute(
            "UPDATE web_evidence_providers
             SET enabled = ?2, updated_at = datetime('now')
             WHERE id = ?1",
            params![provider_id, if enabled { 1 } else { 0 }],
        )?;
        if changed == 0 {
            return Err(AppError::msg("web evidence provider not found"));
        }
        Ok(())
    })
}

pub fn delete_web_evidence_provider(db: &Database, provider_id: &str) -> AppResult<()> {
    let provider_id = validate_provider_identifier("provider id", provider_id)?;
    db.with_conn(|conn| {
        conn.execute(
            "DELETE FROM web_evidence_providers WHERE id = ?1",
            [provider_id],
        )?;
        Ok(())
    })
}

pub fn web_evidence_provider_exists(db: &Database, provider_id: &str) -> AppResult<bool> {
    let provider_id = validate_provider_identifier("provider id", provider_id)?;
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT 1 FROM web_evidence_providers WHERE id = ?1",
            [provider_id],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(Into::into)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> WebEvidenceProviderInput {
        WebEvidenceProviderInput {
            id: "anysearch".into(),
            name: "AnySearch".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json: "{}".into(),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: Some(r#"{"tool":"search"}"#.into()),
            web_fetch_mapping_json: Some(r#"{"tool":"fetch"}"#.into()),
        }
    }

    #[test]
    fn upsert_persists_anysearch_max_results_arg_without_runtime_patch() {
        let db = Database::open_in_memory().unwrap();
        let mut input = provider();
        input.transport_kind = "https".into();
        input.transport_config_json =
            r#"{"url":"https://api.anysearch.com/mcp","allow_localhost_dev":false}"#.into();
        input.credential_refs_json = r#"{"headers":{"Authorization":{"scheme":"bearer","credential":"credential://iris.mcp.anysearch","optional":true}}}"#.into();
        input.web_search_mapping_json = Some(r#"{"tool":"search","queryArg":"query"}"#.into());

        upsert_web_evidence_provider(&db, &input).unwrap();

        let stored = list_web_evidence_providers(&db).unwrap();
        let mapping = stored[0]
            .web_search_mapping_json
            .as_deref()
            .expect("search mapping");
        let parsed: serde_json::Value = serde_json::from_str(mapping).unwrap();
        assert_eq!(
            parsed.get("maxResultsArg").and_then(|v| v.as_str()),
            Some("max_results")
        );
    }

    #[test]
    fn list_silently_heals_legacy_anysearch_mappings() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO web_evidence_providers
                 (id, name, kind, enabled, transport_kind, transport_config_json,
                  credential_refs_json, web_search_mapping_json, web_fetch_mapping_json,
                  provider_config_hash, updated_at)
                 VALUES (?1, ?2, 'mcp', 1, 'https', ?3, '{}', ?4, NULL, 'legacy', datetime('now'))",
                params![
                    "anysearch-legacy",
                    "AnySearch",
                    r#"{"url":"https://api.anysearch.com/mcp"}"#,
                    r#"{"tool":"search"}"#,
                ],
            )?;
            Ok(())
        })
        .unwrap();

        let stored = list_web_evidence_providers(&db).unwrap();
        let mapping = stored[0]
            .web_search_mapping_json
            .as_deref()
            .expect("search mapping");
        assert!(
            mapping.contains("maxResultsArg"),
            "expected healed mapping, got {mapping}"
        );
    }

    #[test]
    fn upsert_persists_firecrawl_limit_arg_without_runtime_patch() {
        let db = Database::open_in_memory().unwrap();
        let input = WebEvidenceProviderInput {
            id: "firecrawl".into(),
            name: "Firecrawl".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "https".into(),
            transport_config_json:
                r#"{"preset_id":"firecrawl","url":"https://mcp.firecrawl.dev/v2/mcp"}"#.into(),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: Some(
                r#"{"tool":"firecrawl_search","queryArg":"query"}"#.into(),
            ),
            web_fetch_mapping_json: Some(r#"{"tool":"firecrawl_scrape"}"#.into()),
        };

        upsert_web_evidence_provider(&db, &input).unwrap();

        let stored = list_web_evidence_providers(&db).unwrap();
        let mapping = stored[0]
            .web_search_mapping_json
            .as_deref()
            .expect("search mapping");
        let parsed: serde_json::Value = serde_json::from_str(mapping).unwrap();
        assert_eq!(
            parsed.get("maxResultsArg").and_then(|v| v.as_str()),
            Some("limit")
        );
    }

    #[test]
    fn list_silently_heals_legacy_firecrawl_mappings() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO web_evidence_providers
                 (id, name, kind, enabled, transport_kind, transport_config_json,
                  credential_refs_json, web_search_mapping_json, web_fetch_mapping_json,
                  provider_config_hash, updated_at)
                 VALUES (?1, ?2, 'mcp', 1, 'https', ?3, '{}', ?4, NULL, 'legacy', datetime('now'))",
                params![
                    "firecrawl-legacy",
                    "Firecrawl",
                    r#"{"preset_id":"firecrawl","url":"https://mcp.firecrawl.dev/v2/mcp"}"#,
                    r#"{"tool":"firecrawl_search","queryArg":"query"}"#,
                ],
            )?;
            Ok(())
        })
        .unwrap();

        let stored = list_web_evidence_providers(&db).unwrap();
        let mapping = stored[0]
            .web_search_mapping_json
            .as_deref()
            .expect("search mapping");
        assert!(
            mapping.contains(r#""maxResultsArg":"limit""#)
                || mapping.contains(r#""maxResultsArg": "limit""#),
            "expected healed mapping, got {mapping}"
        );
    }

    #[test]
    fn web_evidence_provider_registry_round_trips_minimal_mapping() {
        let db = Database::open_in_memory().unwrap();
        upsert_web_evidence_provider(&db, &provider()).unwrap();

        let providers = list_web_evidence_providers(&db).unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, "anysearch");
        assert_eq!(providers[0].kind, "mcp");
        assert!(providers[0].enabled);
        assert!(providers[0].has_search_mapping);
        assert!(providers[0].has_fetch_mapping);

        toggle_web_evidence_provider(&db, "anysearch", false).unwrap();
        assert!(!list_web_evidence_providers(&db).unwrap()[0].enabled);

        delete_web_evidence_provider(&db, "anysearch").unwrap();
        assert!(list_web_evidence_providers(&db).unwrap().is_empty());
    }

    #[test]
    fn web_evidence_provider_rejects_raw_secret_material() {
        let db = Database::open_in_memory().unwrap();
        let mut input = provider();
        input.credential_refs_json = r#"{"api_key":"plain"}"#.into();

        let err = upsert_web_evidence_provider(&db, &input).unwrap_err();
        assert!(err.to_string().contains("credential references"));
    }

    #[test]
    fn web_evidence_provider_allows_https_header_refs_but_rejects_env_refs() {
        let db = Database::open_in_memory().unwrap();
        let mut input = provider();
        input.transport_kind = "https".into();
        input.transport_config_json =
            r#"{"url":"https://api.anysearch.com/mcp","allow_localhost_dev":false}"#.into();
        input.credential_refs_json = r#"{"headers":{"Authorization":{"scheme":"bearer","credential":"credential://iris.mcp.anysearch"}}}"#.into();

        upsert_web_evidence_provider(&db, &input).unwrap();

        let stored = list_web_evidence_providers(&db).unwrap();
        assert_eq!(stored[0].credential_refs_json, input.credential_refs_json);

        input.credential_refs_json =
            r#"{"env":{"ANYSEARCH_API_KEY":"credential://iris.mcp.anysearch"}}"#.into();
        let err = upsert_web_evidence_provider(&db, &input).unwrap_err();
        assert!(err.to_string().contains("only support"), "{err}");
    }

    #[test]
    fn web_evidence_provider_rejects_stdio_environment_bindings() {
        let db = Database::open_in_memory().unwrap();
        let mut input = provider();
        input.credential_refs_json = r#"{"env":{"API_KEY":"credential://iris.mcp.test"}}"#.into();
        let err = upsert_web_evidence_provider(&db, &input).unwrap_err();
        assert!(err.to_string().contains("stdio providers cannot"), "{err}");

        input.credential_refs_json = "{}".into();
        input.transport_config_json = r#"{"command":"test","env":{"API_KEY":"nope"}}"#.into();
        let err = upsert_web_evidence_provider(&db, &input).unwrap_err();
        assert!(
            err.to_string().contains("must not define environment"),
            "{err}"
        );
    }

    #[test]
    fn runtime_discovery_and_health_are_persisted_without_payloads() {
        let db = Database::open_in_memory().unwrap();
        upsert_web_evidence_provider(&db, &provider()).unwrap();
        record_web_evidence_provider_discovery(
            &db,
            "anysearch",
            "2025-11-25",
            "AnySearch",
            Some("1.2.3"),
            "tool-schema-hash",
        )
        .unwrap();
        record_web_evidence_provider_call(&db, "anysearch", true, 120, None).unwrap();
        record_web_evidence_provider_call(&db, "anysearch", false, 240, Some("timeout")).unwrap();

        let runtime = web_evidence_provider_runtime(&db, "anysearch")
            .unwrap()
            .unwrap();
        assert_eq!(runtime.protocol_version, "2025-11-25");
        assert_eq!(runtime.server_name, "AnySearch");
        assert_eq!(runtime.status, "degraded");
        assert_eq!(runtime.reason_code, "timeout");
        let health = web_evidence_provider_health(&db, "anysearch")
            .unwrap()
            .unwrap();
        assert_eq!(health.success_count, 1);
        assert_eq!(health.failure_count, 1);
        assert_eq!(health.consecutive_failures, 1);
        assert_eq!(health.last_failure_code.as_deref(), Some("timeout"));
        assert!(health.latency_ewma_ms.is_some());

        let mut changed = provider();
        changed.web_search_mapping_json = Some(r#"{"tool":"search_v2"}"#.into());
        upsert_web_evidence_provider(&db, &changed).unwrap();
        assert!(web_evidence_provider_runtime(&db, "anysearch")
            .unwrap()
            .is_none());
        assert!(web_evidence_provider_health(&db, "anysearch")
            .unwrap()
            .is_none());
    }

    #[test]
    fn web_evidence_provider_rejects_raw_authorization_header() {
        let db = Database::open_in_memory().unwrap();
        let mut input = provider();
        input.credential_refs_json =
            r#"{"headers":{"Authorization":"Bearer as_sk_plain_secret"}}"#.into();

        let err = upsert_web_evidence_provider(&db, &input).unwrap_err();
        assert!(err.to_string().contains("credential references"));
    }

    #[test]
    fn selected_web_search_provider_requires_mcp_provider() {
        let db = Database::open_in_memory().unwrap();

        let err = resolve_selected_web_search_provider(&db).unwrap_err();

        assert!(err.to_string().contains("web_search_provider_missing"));
    }

    #[test]
    fn selected_web_search_provider_requires_choice_when_multiple_are_available() {
        let db = Database::open_in_memory().unwrap();
        upsert_web_evidence_provider(&db, &provider()).unwrap();
        let mut second = provider();
        second.id = "brave".into();
        second.name = "Brave Search".into();
        second.web_search_mapping_json = Some(r#"{"tool":"brave_web_search"}"#.into());
        upsert_web_evidence_provider(&db, &second).unwrap();

        let err = resolve_selected_web_search_provider(&db).unwrap_err();

        assert!(err.to_string().contains("web_search_provider_unselected"));
    }

    #[test]
    fn selected_web_search_provider_uses_single_provider_without_saved_choice() {
        let db = Database::open_in_memory().unwrap();
        upsert_web_evidence_provider(&db, &provider()).unwrap();

        let selected = resolve_selected_web_search_provider(&db).unwrap();

        assert_eq!(selected.id, "anysearch");
        assert_eq!(selected.kind, "mcp");
    }

    #[test]
    fn selected_web_search_provider_honors_saved_choice() {
        let db = Database::open_in_memory().unwrap();
        upsert_web_evidence_provider(&db, &provider()).unwrap();
        let mut second = provider();
        second.id = "brave".into();
        second.name = "Brave Search".into();
        second.web_search_mapping_json = Some(r#"{"tool":"brave_web_search"}"#.into());
        upsert_web_evidence_provider(&db, &second).unwrap();
        save_selected_web_search_provider_id(&db, Some("brave")).unwrap();

        let selected = resolve_selected_web_search_provider(&db).unwrap();

        assert_eq!(selected.id, "brave");
        assert_eq!(
            selected.web_search_mapping_json.as_deref(),
            Some(r#"{"tool":"brave_web_search"}"#)
        );
    }

    #[test]
    fn selected_web_search_provider_rejects_stale_saved_choice() {
        let db = Database::open_in_memory().unwrap();
        upsert_web_evidence_provider(&db, &provider()).unwrap();
        save_selected_web_search_provider_id(&db, Some("missing-provider")).unwrap();

        let err = resolve_selected_web_search_provider(&db).unwrap_err();

        assert!(err.to_string().contains("web_search_provider_unavailable"));
    }

    #[test]
    fn legacy_mcp_runtime_tables_are_not_target_state() {
        let db = Database::open_in_memory().unwrap();

        for table in [
            "mcp_server_catalog",
            "mcp_runtime_profiles",
            "mcp_tool_inventory",
            "mcp_health_events",
            "skill_runtime_requirements",
        ] {
            let exists = db
                .with_conn(|conn| {
                    conn.query_row(
                        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                        [table],
                        |_| Ok(()),
                    )
                    .optional()
                    .map(|value| value.is_some())
                    .map_err(Into::into)
                })
                .unwrap();
            assert!(!exists, "{table} must not exist after AI reign-in");
        }
    }
}
