//! Explicit, per-Run read-only MCP tool bindings.
//!
//! Mutable provider discovery is a management-plane concern. Runtime execution
//! consumes only immutable snapshots accepted for one normal-domain Run.

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::ai_runtime::run_contract::ExternalToolGrantRef;
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

pub(crate) const EXTERNAL_READ_CAPABILITY: &str = "external.read";
pub(crate) const MAX_EXTERNAL_MODEL_CHARS: usize = 8_000;
pub(crate) const MAX_EXTERNAL_EVIDENCE_CHARS: usize = 2_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCapabilityBindingInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub provider_id: String,
    pub mcp_tool_name: String,
    pub input_schema: Value,
    #[serde(default)]
    pub argument_mapping: Value,
    pub risk_class: String,
    pub read_only: bool,
    pub user_trusted: bool,
    pub attested_binding_config_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCapabilityBindingSummary {
    pub id: String,
    pub provider_id: String,
    pub exposed_name: String,
    pub mcp_tool_name: String,
    pub input_schema: Value,
    pub argument_mapping: Value,
    pub output_policy: Value,
    pub provider_config_hash: String,
    pub binding_config_hash: String,
    pub provider_enabled: bool,
    pub config_matches: bool,
    pub user_trusted: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FrozenMcpToolSnapshot {
    pub(crate) run_id: String,
    pub(crate) binding_id: String,
    pub(crate) provider_id: String,
    pub(crate) exposed_name: String,
    pub(crate) mcp_tool_name: String,
    pub(crate) input_schema: Value,
    pub(crate) argument_mapping: Value,
    pub(crate) output_policy: Value,
    pub(crate) provider_config_hash: String,
    pub(crate) provider_launch_hash: String,
    pub(crate) transport_kind: String,
    pub(crate) transport_config_json: String,
    pub(crate) credential_refs_json: String,
    pub(crate) binding_config_hash: String,
    pub(crate) capability: String,
    pub(crate) risk_class: String,
    pub(crate) read_only: bool,
    pub(crate) user_trusted: bool,
    pub(crate) frozen_at: String,
    pub(crate) snapshot_integrity_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpReadOnlyToolCandidate {
    pub name: String,
    pub input_schema: Value,
    pub risk_class: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpReadOnlyToolAttestation {
    pub provider_display_name: String,
    pub provider_config_hash: String,
    pub binding_config_hash: String,
    pub name: String,
    pub input_schema: Value,
    pub risk_class: String,
    pub read_only: bool,
}

pub(crate) fn review_discovered_tool(
    tool_name: &str,
    input_schema: &Value,
    read_only_hint: Option<bool>,
) -> AppResult<McpReadOnlyToolCandidate> {
    let tool_name = tool_name.trim();
    if !validate_tool_identifier(tool_name) {
        return Err(safe_error("external_tool_binding_invalid"));
    }
    if read_only_hint != Some(true) || tool_category_is_forbidden(tool_name) {
        return Err(safe_error("external_tool_not_read_only"));
    }
    Ok(McpReadOnlyToolCandidate {
        name: tool_name.to_string(),
        input_schema: normalized_input_schema(input_schema)?,
        risk_class: "read_only".into(),
        read_only: true,
    })
}

fn safe_error(code: &'static str) -> AppError {
    AppError::msg(code)
}

fn hash_json(value: &Value) -> String {
    let digest = Sha256::digest(value.to_string().as_bytes());
    hex::encode(&digest[..12])
}

fn full_hash_json(value: &Value) -> String {
    hex::encode(Sha256::digest(value.to_string().as_bytes()))
}

fn validate_identifier(value: &str, max_chars: usize) -> bool {
    let value = value.trim();
    !value.is_empty() && value.chars().count() <= max_chars && !value.chars().any(char::is_control)
}

fn validate_schema_token(value: &str, max_chars: usize) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.chars().count() <= max_chars
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn validate_tool_identifier(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.chars().count() <= 128
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ':' | '/')
        })
}

fn tool_category_is_forbidden(tool_name: &str) -> bool {
    let lower = tool_name.to_ascii_lowercase();
    let markers = [
        "write",
        "create",
        "update",
        "edit",
        "patch",
        "set",
        "reset",
        "put",
        "mutate",
        "add",
        "append",
        "send",
        "post",
        "publish",
        "delete",
        "remove",
        "rm",
        "move",
        "rename",
        "upload",
        "calendar",
        "schedule",
        "process",
        "execute",
        "exec",
        "shell",
        "command",
        "secret",
        "credential",
        "password",
        "token",
    ];
    let prompt_injection_markers = [
        "ignore",
        "instruction",
        "override",
        "prompt",
        "jailbreak",
        "bypass",
        "reveal",
        "exfiltrat",
    ];
    lower
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .any(|token| markers.iter().any(|marker| token.starts_with(marker)))
        || markers
            .iter()
            .filter(|marker| marker.len() >= 4)
            .any(|marker| lower.starts_with(marker))
        || prompt_injection_markers
            .iter()
            .any(|marker| lower.contains(marker))
}

fn schema_name_is_forbidden(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "operation",
        "action",
        "method",
        "intent",
        "mutation",
        "command",
        "secret",
        "credential",
        "password",
        "api_key",
        "apikey",
    ]
    .iter()
    .any(|marker| lower == *marker || lower.starts_with(&format!("{marker}_")))
        || tool_category_is_forbidden(name)
}

fn schema_enum_value_is_forbidden(value: &Value) -> bool {
    value.as_str().is_some_and(|value| {
        !validate_schema_token(value, 64)
            || tool_category_is_forbidden(value)
            || schema_name_is_forbidden(value)
    })
}

fn sanitize_schema_node(value: &Value, depth: usize) -> AppResult<Value> {
    const ALLOWED_KEYS: &[&str] = &[
        "type",
        "properties",
        "required",
        "additionalProperties",
        "items",
        "enum",
    ];
    const STRIPPED_TEXT_KEYS: &[&str] = &["description", "title", "$comment"];
    if depth > 8 {
        return Err(safe_error("external_tool_schema_unsupported"));
    }
    let object = value
        .as_object()
        .ok_or_else(|| safe_error("external_tool_schema_invalid"))?;
    if object.keys().any(|key| {
        !ALLOWED_KEYS.contains(&key.as_str()) && !STRIPPED_TEXT_KEYS.contains(&key.as_str())
    }) {
        return Err(safe_error("external_tool_schema_unsupported"));
    }
    let schema_type = object
        .get("type")
        .and_then(Value::as_str)
        .filter(|value| {
            matches!(
                *value,
                "object" | "array" | "string" | "integer" | "number" | "boolean" | "null"
            )
        })
        .ok_or_else(|| safe_error("external_tool_schema_invalid"))?;
    let mut sanitized = serde_json::Map::new();
    sanitized.insert("type".into(), Value::String(schema_type.into()));

    if let Some(values) = object.get("enum") {
        let values = values
            .as_array()
            .filter(|values| !values.is_empty() && values.len() <= 64)
            .ok_or_else(|| safe_error("external_tool_schema_invalid"))?;
        if values.iter().any(|value| {
            !value_matches_type(value, schema_type) || schema_enum_value_is_forbidden(value)
        }) {
            return Err(safe_error("external_tool_not_read_only"));
        }
        sanitized.insert("enum".into(), Value::Array(values.clone()));
    }

    match schema_type {
        "object" => {
            let properties = match object.get("properties") {
                Some(properties) => properties
                    .as_object()
                    .cloned()
                    .ok_or_else(|| safe_error("external_tool_schema_invalid"))?,
                None => serde_json::Map::new(),
            };
            if properties.len() > 64 {
                return Err(safe_error("external_tool_schema_invalid"));
            }
            let mut sanitized_properties = serde_json::Map::new();
            for (name, child) in properties {
                if !validate_schema_token(&name, 64) || schema_name_is_forbidden(&name) {
                    return Err(safe_error("external_tool_not_read_only"));
                }
                sanitized_properties.insert(name, sanitize_schema_node(&child, depth + 1)?);
            }
            sanitized.insert(
                "properties".into(),
                Value::Object(sanitized_properties.clone()),
            );
            let required = match object.get("required") {
                Some(required) => required
                    .as_array()
                    .cloned()
                    .ok_or_else(|| safe_error("external_tool_schema_invalid"))?,
                None => Vec::new(),
            };
            let mut required_names = HashSet::new();
            if required.len() > 64
                || required.iter().any(|value| {
                    value.as_str().is_none_or(|name| {
                        !sanitized_properties.contains_key(name)
                            || !required_names.insert(name.to_string())
                    })
                })
            {
                return Err(safe_error("external_tool_schema_invalid"));
            }
            if !required.is_empty() {
                sanitized.insert("required".into(), Value::Array(required));
            }
            if object
                .get("additionalProperties")
                .is_some_and(|value| value != &Value::Bool(false))
            {
                return Err(safe_error("external_tool_schema_unsupported"));
            }
            sanitized.insert("additionalProperties".into(), Value::Bool(false));
            if object.contains_key("items") {
                return Err(safe_error("external_tool_schema_invalid"));
            }
        }
        "array" => {
            let items = object
                .get("items")
                .ok_or_else(|| safe_error("external_tool_schema_invalid"))?;
            sanitized.insert("items".into(), sanitize_schema_node(items, depth + 1)?);
            if object.contains_key("properties")
                || object.contains_key("required")
                || object.contains_key("additionalProperties")
            {
                return Err(safe_error("external_tool_schema_invalid"));
            }
        }
        _ => {
            if object.contains_key("properties")
                || object.contains_key("required")
                || object.contains_key("additionalProperties")
                || object.contains_key("items")
            {
                return Err(safe_error("external_tool_schema_invalid"));
            }
        }
    }
    Ok(Value::Object(sanitized))
}

fn normalized_input_schema(schema: &Value) -> AppResult<Value> {
    if schema.to_string().len() > 32_000 {
        return Err(safe_error("external_tool_schema_too_large"));
    }
    let schema = sanitize_schema_node(schema, 0)?;
    let object = schema
        .as_object()
        .ok_or_else(|| safe_error("external_tool_schema_invalid"))?;
    if object.get("type").and_then(Value::as_str) != Some("object") {
        return Err(safe_error("external_tool_schema_invalid"));
    }
    Ok(schema)
}

fn mapping_node_is_safe(value: &Value, depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    match value {
        Value::Object(object) => {
            object.len() <= 64
                && object.iter().all(|(key, value)| {
                    validate_schema_token(key, 64)
                        && !schema_name_is_forbidden(key)
                        && mapping_node_is_safe(value, depth + 1)
                })
        }
        Value::String(value) => {
            validate_schema_token(value, 64) && !schema_name_is_forbidden(value)
        }
        _ => false,
    }
}

fn normalized_argument_mapping(mapping: &Value) -> AppResult<Value> {
    if !mapping_node_is_safe(mapping, 0) {
        return Err(safe_error("external_tool_mapping_invalid"));
    }
    let object = mapping
        .as_object()
        .ok_or_else(|| safe_error("external_tool_mapping_invalid"))?;
    if object.len() > 64 {
        return Err(safe_error("external_tool_mapping_invalid"));
    }
    let mut targets = HashSet::new();
    for target in object.values() {
        let Some(target) = target.as_str() else {
            return Err(safe_error("external_tool_mapping_invalid"));
        };
        if !targets.insert(target) {
            return Err(safe_error("external_tool_mapping_invalid"));
        }
    }
    Ok(mapping.clone())
}

fn output_policy() -> Value {
    serde_json::json!({
        "mode": "text_or_json",
        "maxModelChars": MAX_EXTERNAL_MODEL_CHARS,
        "maxEvidenceChars": MAX_EXTERNAL_EVIDENCE_CHARS
    })
}

fn binding_hash(provider: (&str, &str, &str), contract: (&str, &Value, &Value, &Value)) -> String {
    let (provider_id, provider_config_hash, provider_launch_hash) = provider;
    let (mcp_tool_name, input_schema, argument_mapping, output_policy) = contract;
    hash_json(&serde_json::json!({
        "capability": EXTERNAL_READ_CAPABILITY,
        "riskClass": "read_only",
        "readOnly": true,
        "userTrusted": true,
        "providerId": provider_id,
        "providerConfigHash": provider_config_hash,
        "providerLaunchHash": provider_launch_hash,
        "mcpToolName": mcp_tool_name,
        "inputSchema": input_schema,
        "argumentMapping": argument_mapping,
        "outputPolicy": output_policy
    }))
}

pub(crate) fn attest_reviewed_tool(
    db: &Database,
    provider_id: &str,
    reviewed: &McpReadOnlyToolCandidate,
    reviewed_provider_config_hash: &str,
    argument_mapping: &Value,
) -> AppResult<McpReadOnlyToolAttestation> {
    let provider_id = provider_id.trim();
    let argument_mapping = normalized_argument_mapping(argument_mapping)?;
    let properties = reviewed
        .input_schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| safe_error("external_tool_schema_invalid"))?;
    if argument_mapping.as_object().is_some_and(|mapping| {
        mapping
            .keys()
            .any(|source| !properties.contains_key(source))
    }) {
        return Err(safe_error("external_tool_mapping_invalid"));
    }
    db.with_read_conn(|conn| {
        let provider: Option<(String, String, i64, String, String, String, String)> = conn
            .query_row(
                "SELECT name, kind, enabled, provider_config_hash, transport_kind,
                        transport_config_json, credential_refs_json
                 FROM web_evidence_providers WHERE id = ?1",
                [provider_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            provider_display_name,
            provider_kind,
            provider_enabled,
            provider_config_hash,
            transport_kind,
            transport_config_json,
            credential_refs_json,
        )) = provider
        else {
            return Err(safe_error("external_tool_provider_missing"));
        };
        if provider_kind != "mcp" {
            return Err(safe_error("external_tool_provider_invalid"));
        }
        if provider_enabled != 1 {
            return Err(safe_error("external_tool_provider_disabled"));
        }
        if provider_config_hash != reviewed_provider_config_hash {
            return Err(safe_error("external_tool_provider_config_changed"));
        }
        let provider_launch_hash = crate::ai_runtime::mcp_host_runtime::frozen_provider_launch_hash(
            provider_id,
            &transport_kind,
            &transport_config_json,
            &credential_refs_json,
        );
        let output_policy = output_policy();
        let binding_config_hash = binding_hash(
            (provider_id, &provider_config_hash, &provider_launch_hash),
            (
                &reviewed.name,
                &reviewed.input_schema,
                &argument_mapping,
                &output_policy,
            ),
        );
        Ok(McpReadOnlyToolAttestation {
            provider_display_name,
            provider_config_hash,
            binding_config_hash,
            name: reviewed.name.clone(),
            input_schema: reviewed.input_schema.clone(),
            risk_class: reviewed.risk_class.clone(),
            read_only: reviewed.read_only,
        })
    })
}

fn snapshot_integrity_hash(
    identity: (&str, &str, &str, &str, &str),
    contract: (&str, &str, &str, &str),
    authorization: (&str, &str, i64, i64),
    provider: (&str, &str, &str, &str, &str),
    frozen_at: &str,
) -> String {
    let (run_id, binding_id, provider_id, exposed_name, mcp_tool_name) = identity;
    let (input_schema_json, argument_mapping_json, output_policy_json, binding_config_hash) =
        contract;
    let (capability, risk_class, read_only, user_trusted) = authorization;
    let (
        provider_config_hash,
        provider_launch_hash,
        transport_kind,
        transport_config_json,
        credential_refs_json,
    ) = provider;
    full_hash_json(&serde_json::json!({
        "runId": run_id,
        "bindingId": binding_id,
        "providerId": provider_id,
        "exposedName": exposed_name,
        "mcpToolName": mcp_tool_name,
        "inputSchemaJson": input_schema_json,
        "argumentMappingJson": argument_mapping_json,
        "outputPolicyJson": output_policy_json,
        "capability": capability,
        "riskClass": risk_class,
        "readOnly": read_only,
        "userTrusted": user_trusted,
        "providerConfigHash": provider_config_hash,
        "providerLaunchHash": provider_launch_hash,
        "transportKind": transport_kind,
        "transportConfigJson": transport_config_json,
        "credentialRefsJson": credential_refs_json,
        "bindingConfigHash": binding_config_hash,
        "frozenAt": frozen_at
    }))
}

fn parse_json_column(raw: &str, column_index: usize) -> rusqlite::Result<Value> {
    serde_json::from_str(raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column_index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn parse_binding_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<McpCapabilityBindingSummary> {
    let input_schema_json: String = row.get(4)?;
    let argument_mapping_json: String = row.get(5)?;
    let output_policy_json: String = row.get(6)?;
    let stored_provider_hash: String = row.get(7)?;
    let current_provider_hash: String = row.get(11)?;
    Ok(McpCapabilityBindingSummary {
        id: row.get(0)?,
        provider_id: row.get(1)?,
        exposed_name: row.get(2)?,
        mcp_tool_name: row.get(3)?,
        input_schema: parse_json_column(&input_schema_json, 4)?,
        argument_mapping: parse_json_column(&argument_mapping_json, 5)?,
        output_policy: parse_json_column(&output_policy_json, 6)?,
        provider_config_hash: stored_provider_hash.clone(),
        binding_config_hash: row.get(8)?,
        provider_enabled: row.get::<_, i64>(9)? != 0,
        config_matches: row.get::<_, String>(10)? == "mcp"
            && stored_provider_hash == current_provider_hash,
        user_trusted: row.get::<_, i64>(12)? != 0,
    })
}

pub(crate) fn upsert_binding(
    db: &Database,
    input: &McpCapabilityBindingInput,
    reviewed: &McpReadOnlyToolCandidate,
    reviewed_provider_config_hash: &str,
) -> AppResult<McpCapabilityBindingSummary> {
    let provider_id = input.provider_id.trim();
    let tool_name = input.mcp_tool_name.trim();
    if !validate_identifier(provider_id, 128) || !validate_tool_identifier(tool_name) {
        return Err(safe_error("external_tool_binding_invalid"));
    }
    if input.risk_class != "read_only"
        || !input.read_only
        || reviewed.risk_class != "read_only"
        || !reviewed.read_only
        || !input.user_trusted
        || reviewed.name != tool_name
    {
        return Err(safe_error("external_tool_not_read_only"));
    }
    let input_schema = normalized_input_schema(&input.input_schema)?;
    if input_schema != reviewed.input_schema {
        return Err(safe_error("external_tool_binding_config_changed"));
    }
    let argument_mapping = normalized_argument_mapping(&input.argument_mapping)?;
    let properties = input_schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| safe_error("external_tool_schema_invalid"))?;
    if argument_mapping.as_object().is_some_and(|mapping| {
        mapping
            .keys()
            .any(|source| !properties.contains_key(source))
    }) {
        return Err(safe_error("external_tool_mapping_invalid"));
    }
    let output_policy = output_policy();

    db.with_conn(|conn| {
        let provider: Option<(String, String, String, String, String)> = conn
            .query_row(
                "SELECT kind, provider_config_hash, transport_kind,
                        transport_config_json, credential_refs_json
                 FROM web_evidence_providers WHERE id = ?1",
                [provider_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            kind,
            provider_config_hash,
            transport_kind,
            transport_config_json,
            credential_refs_json,
        )) = provider
        else {
            return Err(safe_error("external_tool_provider_missing"));
        };
        if kind != "mcp" {
            return Err(safe_error("external_tool_provider_invalid"));
        }
        if provider_config_hash != reviewed_provider_config_hash {
            return Err(safe_error("external_tool_provider_config_changed"));
        }

        let existing = input
            .id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty());
        let (id, exposed_name) = if let Some(id) = existing {
            if !validate_identifier(id, 128) {
                return Err(safe_error("external_tool_binding_invalid"));
            }
            let exposed_name: String = conn
                .query_row(
                    "SELECT exposed_name FROM mcp_capability_bindings WHERE id = ?1",
                    [id],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| safe_error("external_tool_binding_missing"))?;
            (id.to_string(), exposed_name)
        } else {
            let id = Uuid::new_v4().to_string();
            let suffix = hash_json(&serde_json::json!({
                "providerId": provider_id,
                "toolName": tool_name,
                "bindingId": id
            }));
            (id, format!("external_read_{}", &suffix[..10]))
        };
        let provider_launch_hash = crate::ai_runtime::mcp_host_runtime::frozen_provider_launch_hash(
            provider_id,
            &transport_kind,
            &transport_config_json,
            &credential_refs_json,
        );
        let binding_config_hash = binding_hash(
            (provider_id, &provider_config_hash, &provider_launch_hash),
            (tool_name, &input_schema, &argument_mapping, &output_policy),
        );
        if input.attested_binding_config_hash.trim() != binding_config_hash {
            return Err(safe_error("external_tool_attestation_changed"));
        }
        conn.execute(
            "INSERT INTO mcp_capability_bindings
             (id, provider_id, exposed_name, mcp_tool_name, input_schema_json,
              argument_mapping_json, output_policy_json, capability, risk_class,
              read_only, user_trusted, provider_config_hash, binding_config_hash,
              created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'external.read', 'read_only',
                     1, 1, ?8, ?9, datetime('now'), datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
               provider_id = excluded.provider_id,
               mcp_tool_name = excluded.mcp_tool_name,
               input_schema_json = excluded.input_schema_json,
               argument_mapping_json = excluded.argument_mapping_json,
               output_policy_json = excluded.output_policy_json,
               capability = excluded.capability,
               risk_class = excluded.risk_class,
               read_only = excluded.read_only,
               user_trusted = excluded.user_trusted,
               provider_config_hash = excluded.provider_config_hash,
               binding_config_hash = excluded.binding_config_hash,
               updated_at = datetime('now')",
            params![
                id,
                provider_id,
                exposed_name,
                tool_name,
                input_schema.to_string(),
                argument_mapping.to_string(),
                output_policy.to_string(),
                provider_config_hash,
                binding_config_hash
            ],
        )
        .map_err(|_| safe_error("external_tool_binding_conflict"))?;

        list_bindings_with_conn(conn, Some(provider_id))?
            .into_iter()
            .find(|binding| binding.id == id)
            .ok_or_else(|| safe_error("external_tool_binding_missing"))
    })
}

pub fn list_bindings(
    db: &Database,
    provider_id: Option<&str>,
) -> AppResult<Vec<McpCapabilityBindingSummary>> {
    db.with_read_conn(|conn| list_bindings_with_conn(conn, provider_id))
}

fn list_bindings_with_conn(
    conn: &Connection,
    provider_id: Option<&str>,
) -> AppResult<Vec<McpCapabilityBindingSummary>> {
    let mut statement = conn.prepare(
        "SELECT binding.id, binding.provider_id, binding.exposed_name,
                binding.mcp_tool_name, binding.input_schema_json,
                binding.argument_mapping_json, binding.output_policy_json,
                binding.provider_config_hash, binding.binding_config_hash,
                provider.enabled, provider.kind, provider.provider_config_hash,
                binding.user_trusted
         FROM mcp_capability_bindings AS binding
         JOIN web_evidence_providers AS provider ON provider.id = binding.provider_id
         WHERE (?1 IS NULL OR binding.provider_id = ?1)
         ORDER BY binding.provider_id, binding.exposed_name",
    )?;
    let rows = statement.query_map([provider_id], parse_binding_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn delete_binding(db: &Database, binding_id: &str) -> AppResult<()> {
    if !validate_identifier(binding_id, 128) {
        return Err(safe_error("external_tool_binding_invalid"));
    }
    db.with_conn(|conn| {
        conn.execute(
            "DELETE FROM mcp_capability_bindings WHERE id = ?1",
            [binding_id],
        )?;
        Ok(())
    })
}

pub(crate) fn freeze_run_grants(
    conn: &Connection,
    run_id: &str,
    grants: &[ExternalToolGrantRef],
) -> AppResult<()> {
    if grants.len() > 8 {
        return Err(safe_error("external_tool_grant_limit_exceeded"));
    }
    let mut seen = HashSet::new();
    for grant in grants {
        let binding_id = grant.binding_id.trim();
        let grant_hash = grant.binding_config_hash.trim();
        if !validate_identifier(binding_id, 128)
            || !validate_identifier(grant_hash, 128)
            || !seen.insert(binding_id)
        {
            return Err(safe_error("external_tool_grant_invalid"));
        }
        let binding = conn
            .query_row(
                "SELECT binding.provider_id, binding.exposed_name,
                        binding.mcp_tool_name, binding.input_schema_json,
                        binding.argument_mapping_json, binding.output_policy_json,
                        binding.capability, binding.risk_class, binding.read_only,
                        binding.user_trusted,
                        binding.provider_config_hash, binding.binding_config_hash,
                        provider.kind, provider.enabled, provider.provider_config_hash,
                        provider.transport_kind, provider.transport_config_json,
                        provider.credential_refs_json
                 FROM mcp_capability_bindings AS binding
                 JOIN web_evidence_providers AS provider
                   ON provider.id = binding.provider_id
                 WHERE binding.id = ?1",
                [binding_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                        row.get::<_, String>(10)?,
                        row.get::<_, String>(11)?,
                        row.get::<_, String>(12)?,
                        row.get::<_, i64>(13)?,
                        row.get::<_, String>(14)?,
                        row.get::<_, String>(15)?,
                        row.get::<_, String>(16)?,
                        row.get::<_, String>(17)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| safe_error("external_tool_grant_missing"))?;
        let (
            provider_id,
            exposed_name,
            mcp_tool_name,
            input_schema_json,
            argument_mapping_json,
            output_policy_json,
            capability,
            risk_class,
            read_only,
            user_trusted,
            binding_provider_hash,
            binding_config_hash,
            provider_kind,
            provider_enabled,
            current_provider_hash,
            transport_kind,
            transport_config_json,
            credential_refs_json,
        ) = binding;
        if grant_hash != binding_config_hash {
            return Err(safe_error("external_tool_binding_config_changed"));
        }
        let input_schema: Value = serde_json::from_str(&input_schema_json)
            .map_err(|_| safe_error("external_tool_binding_config_changed"))?;
        let argument_mapping: Value = serde_json::from_str(&argument_mapping_json)
            .map_err(|_| safe_error("external_tool_binding_config_changed"))?;
        let stored_output_policy: Value = serde_json::from_str(&output_policy_json)
            .map_err(|_| safe_error("external_tool_binding_config_changed"))?;
        let provider_launch_hash = crate::ai_runtime::mcp_host_runtime::frozen_provider_launch_hash(
            &provider_id,
            &transport_kind,
            &transport_config_json,
            &credential_refs_json,
        );
        if normalized_input_schema(&input_schema)? != input_schema
            || normalized_argument_mapping(&argument_mapping)? != argument_mapping
            || stored_output_policy != output_policy()
            || binding_hash(
                (&provider_id, &binding_provider_hash, &provider_launch_hash),
                (
                    &mcp_tool_name,
                    &input_schema,
                    &argument_mapping,
                    &stored_output_policy,
                ),
            ) != binding_config_hash
        {
            return Err(safe_error("external_tool_binding_config_changed"));
        }
        if provider_kind != "mcp"
            || capability != EXTERNAL_READ_CAPABILITY
            || risk_class != "read_only"
            || read_only != 1
            || user_trusted != 1
            || tool_category_is_forbidden(&mcp_tool_name)
        {
            return Err(safe_error("external_tool_not_read_only"));
        }
        if provider_enabled != 1 {
            return Err(safe_error("external_tool_provider_disabled"));
        }
        if binding_provider_hash != current_provider_hash {
            return Err(safe_error("external_tool_provider_config_changed"));
        }
        let frozen_at = chrono::Utc::now().to_rfc3339();
        let snapshot_integrity_hash = snapshot_integrity_hash(
            (
                run_id,
                binding_id,
                &provider_id,
                &exposed_name,
                &mcp_tool_name,
            ),
            (
                &input_schema_json,
                &argument_mapping_json,
                &output_policy_json,
                &binding_config_hash,
            ),
            (&capability, &risk_class, read_only, user_trusted),
            (
                &binding_provider_hash,
                &provider_launch_hash,
                &transport_kind,
                &transport_config_json,
                &credential_refs_json,
            ),
            &frozen_at,
        );
        conn.execute(
            "INSERT INTO agent_run_mcp_tool_snapshots
             (run_id, binding_id, provider_id, exposed_name, mcp_tool_name,
              input_schema_json, argument_mapping_json, output_policy_json,
              capability, risk_class, read_only, user_trusted, provider_config_hash,
              provider_launch_hash, transport_kind, transport_config_json,
              credential_refs_json, binding_config_hash, frozen_at,
              snapshot_integrity_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                     ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                     ?17, ?18, ?19, ?20)",
            params![
                run_id,
                binding_id,
                provider_id,
                exposed_name,
                mcp_tool_name,
                input_schema_json,
                argument_mapping_json,
                output_policy_json,
                capability,
                risk_class,
                read_only,
                user_trusted,
                binding_provider_hash,
                provider_launch_hash,
                transport_kind,
                transport_config_json,
                credential_refs_json,
                binding_config_hash,
                frozen_at,
                snapshot_integrity_hash
            ],
        )?;
    }
    Ok(())
}

pub(crate) fn load_run_snapshots(
    db: &Database,
    run_id: &str,
) -> AppResult<Vec<FrozenMcpToolSnapshot>> {
    db.with_read_conn(|conn| {
        let mut statement = conn.prepare(
            "SELECT run_id, binding_id, provider_id, exposed_name, mcp_tool_name,
                    input_schema_json, argument_mapping_json, output_policy_json,
                    capability, risk_class, read_only, user_trusted,
                    provider_config_hash, provider_launch_hash, transport_kind,
                    transport_config_json, credential_refs_json,
                    binding_config_hash, frozen_at, snapshot_integrity_hash
             FROM agent_run_mcp_tool_snapshots
             WHERE run_id = ?1
             ORDER BY exposed_name",
        )?;
        let mut rows = statement.query([run_id])?;
        let mut snapshots = Vec::new();
        while let Some(row) = rows.next()? {
            let stored_run_id: String = row.get(0)?;
            let binding_id: String = row.get(1)?;
            let provider_id: String = row.get(2)?;
            let exposed_name: String = row.get(3)?;
            let mcp_tool_name: String = row.get(4)?;
            let input_schema_json: String = row.get(5)?;
            let argument_mapping_json: String = row.get(6)?;
            let output_policy_json: String = row.get(7)?;
            let capability: String = row.get(8)?;
            let risk_class: String = row.get(9)?;
            let read_only: i64 = row.get(10)?;
            let user_trusted: i64 = row.get(11)?;
            let provider_config_hash: String = row.get(12)?;
            let provider_launch_hash: String = row.get(13)?;
            let transport_kind: String = row.get(14)?;
            let transport_config_json: String = row.get(15)?;
            let credential_refs_json: String = row.get(16)?;
            let binding_config_hash: String = row.get(17)?;
            let frozen_at: String = row.get(18)?;
            let stored_integrity_hash: String = row.get(19)?;
            let computed_integrity_hash = snapshot_integrity_hash(
                (
                    &stored_run_id,
                    &binding_id,
                    &provider_id,
                    &exposed_name,
                    &mcp_tool_name,
                ),
                (
                    &input_schema_json,
                    &argument_mapping_json,
                    &output_policy_json,
                    &binding_config_hash,
                ),
                (&capability, &risk_class, read_only, user_trusted),
                (
                    &provider_config_hash,
                    &provider_launch_hash,
                    &transport_kind,
                    &transport_config_json,
                    &credential_refs_json,
                ),
                &frozen_at,
            );
            if stored_run_id != run_id || computed_integrity_hash != stored_integrity_hash {
                return Err(safe_error("external_tool_snapshot_integrity_failed"));
            }
            snapshots.push(FrozenMcpToolSnapshot {
                run_id: stored_run_id,
                binding_id,
                provider_id,
                exposed_name,
                mcp_tool_name,
                input_schema: parse_json_column(&input_schema_json, 5)?,
                argument_mapping: parse_json_column(&argument_mapping_json, 6)?,
                output_policy: parse_json_column(&output_policy_json, 7)?,
                provider_config_hash,
                provider_launch_hash,
                transport_kind,
                transport_config_json,
                credential_refs_json,
                binding_config_hash,
                capability,
                risk_class,
                read_only: read_only == 1,
                user_trusted: user_trusted == 1,
                frozen_at,
                snapshot_integrity_hash: stored_integrity_hash,
            });
        }
        Ok(snapshots)
    })
}

pub(crate) fn validate_and_map_arguments(
    snapshot: &FrozenMcpToolSnapshot,
    arguments: &Value,
) -> AppResult<Value> {
    validate_schema_value(&snapshot.input_schema, arguments)?;
    let arguments = arguments
        .as_object()
        .ok_or_else(|| safe_error("external_tool_arguments_invalid"))?;
    let mapping = snapshot
        .argument_mapping
        .as_object()
        .ok_or_else(|| safe_error("external_tool_mapping_invalid"))?;
    let mut mapped = serde_json::Map::new();
    for (key, value) in arguments {
        let target = mapping.get(key).and_then(Value::as_str).unwrap_or(key);
        if mapped.insert(target.to_string(), value.clone()).is_some() {
            return Err(safe_error("external_tool_mapping_invalid"));
        }
    }
    Ok(Value::Object(mapped))
}

fn value_matches_type(value: &Value, expected: &str) -> bool {
    match expected {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "string" => value.is_string(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => false,
    }
}

fn validate_schema_value(schema: &Value, value: &Value) -> AppResult<()> {
    let schema = schema
        .as_object()
        .ok_or_else(|| safe_error("external_tool_schema_invalid"))?;
    if let Some(expected) = schema.get("type").and_then(Value::as_str) {
        if !value_matches_type(value, expected) {
            return Err(safe_error("external_tool_arguments_schema_mismatch"));
        }
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array) {
        if !allowed.contains(value) {
            return Err(safe_error("external_tool_arguments_schema_mismatch"));
        }
    }
    if let Some(object) = value.as_object() {
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            if required
                .iter()
                .filter_map(Value::as_str)
                .any(|key| !object.contains_key(key))
            {
                return Err(safe_error("external_tool_arguments_schema_mismatch"));
            }
        }
        if schema.get("additionalProperties").and_then(Value::as_bool) == Some(false)
            && object.keys().any(|key| !properties.contains_key(key))
        {
            return Err(safe_error("external_tool_arguments_schema_mismatch"));
        }
        for (key, child) in object {
            if let Some(child_schema) = properties.get(key) {
                validate_schema_value(child_schema, child)?;
            }
        }
    }
    if let Some(items) = schema.get("items") {
        if let Some(array) = value.as_array() {
            for item in array {
                validate_schema_value(items, item)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn provider_is_current(
    db: &Database,
    snapshot: &FrozenMcpToolSnapshot,
) -> AppResult<bool> {
    db.with_read_conn(|conn| {
        let current: Option<(String, i64, String)> = conn
            .query_row(
                "SELECT kind, enabled, provider_config_hash
                 FROM web_evidence_providers WHERE id = ?1",
                [snapshot.provider_id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        Ok(current.is_some_and(|(kind, enabled, config_hash)| {
            kind == "mcp" && enabled == 1 && config_hash == snapshot.provider_config_hash
        }))
    })
}

pub(crate) fn snapshot_contract_is_valid(snapshot: &FrozenMcpToolSnapshot) -> bool {
    let provider_launch_hash = crate::ai_runtime::mcp_host_runtime::frozen_provider_launch_hash(
        &snapshot.provider_id,
        &snapshot.transport_kind,
        &snapshot.transport_config_json,
        &snapshot.credential_refs_json,
    );
    let input_schema_json = snapshot.input_schema.to_string();
    let argument_mapping_json = snapshot.argument_mapping.to_string();
    let output_policy_json = snapshot.output_policy.to_string();
    let snapshot_integrity_hash = snapshot_integrity_hash(
        (
            &snapshot.run_id,
            &snapshot.binding_id,
            &snapshot.provider_id,
            &snapshot.exposed_name,
            &snapshot.mcp_tool_name,
        ),
        (
            &input_schema_json,
            &argument_mapping_json,
            &output_policy_json,
            &snapshot.binding_config_hash,
        ),
        (
            &snapshot.capability,
            &snapshot.risk_class,
            i64::from(snapshot.read_only),
            i64::from(snapshot.user_trusted),
        ),
        (
            &snapshot.provider_config_hash,
            &snapshot.provider_launch_hash,
            &snapshot.transport_kind,
            &snapshot.transport_config_json,
            &snapshot.credential_refs_json,
        ),
        &snapshot.frozen_at,
    );
    validate_identifier(&snapshot.run_id, 128)
        && validate_identifier(&snapshot.provider_id, 128)
        && validate_tool_identifier(&snapshot.mcp_tool_name)
        && validate_identifier(&snapshot.exposed_name, 128)
        && snapshot.exposed_name.starts_with("external_")
        && !tool_category_is_forbidden(&snapshot.mcp_tool_name)
        && normalized_input_schema(&snapshot.input_schema)
            .is_ok_and(|schema| schema == snapshot.input_schema)
        && normalized_argument_mapping(&snapshot.argument_mapping)
            .is_ok_and(|mapping| mapping == snapshot.argument_mapping)
        && snapshot.output_policy == output_policy()
        && snapshot.capability == EXTERNAL_READ_CAPABILITY
        && snapshot.risk_class == "read_only"
        && snapshot.read_only
        && snapshot.user_trusted
        && snapshot_integrity_hash == snapshot.snapshot_integrity_hash
        && provider_launch_hash == snapshot.provider_launch_hash
        && binding_hash(
            (
                &snapshot.provider_id,
                &snapshot.provider_config_hash,
                &snapshot.provider_launch_hash,
            ),
            (
                &snapshot.mcp_tool_name,
                &snapshot.input_schema,
                &snapshot.argument_mapping,
                &snapshot.output_policy,
            ),
        ) == snapshot.binding_config_hash
}

pub(crate) fn frozen_provider_config(
    snapshot: &FrozenMcpToolSnapshot,
) -> crate::ai_runtime::mcp_host_runtime::FrozenMcpProviderConfig {
    crate::ai_runtime::mcp_host_runtime::FrozenMcpProviderConfig {
        provider_id: snapshot.provider_id.clone(),
        transport_kind: snapshot.transport_kind.clone(),
        transport_config_json: snapshot.transport_config_json.clone(),
        credential_refs_json: snapshot.credential_refs_json.clone(),
        provider_launch_hash: snapshot.provider_launch_hash.clone(),
    }
}

pub(crate) fn normalize_external_output(result: &Value) -> AppResult<String> {
    let normalized = match result {
        Value::String(text) => text.clone(),
        Value::Object(object) => {
            if object.get("isError").and_then(Value::as_bool) == Some(true) {
                return Err(safe_error("external_tool_call_failed"));
            }
            if let Some(structured) = object.get("structuredContent") {
                serde_json::to_string(structured)
                    .map_err(|_| safe_error("external_tool_output_unsupported"))?
            } else {
                let content = object
                    .get("content")
                    .and_then(Value::as_array)
                    .ok_or_else(|| safe_error("external_tool_output_unsupported"))?;
                let mut text = Vec::with_capacity(content.len());
                for item in content {
                    let item = item
                        .as_object()
                        .ok_or_else(|| safe_error("external_tool_output_unsupported"))?;
                    if item.get("type").and_then(Value::as_str) != Some("text") {
                        return Err(safe_error("external_tool_output_unsupported"));
                    }
                    text.push(
                        item.get("text")
                            .and_then(Value::as_str)
                            .ok_or_else(|| safe_error("external_tool_output_unsupported"))?,
                    );
                }
                text.join("\n")
            }
        }
        _ => serde_json::to_string(result)
            .map_err(|_| safe_error("external_tool_output_unsupported"))?,
    };
    if normalized.chars().count() > MAX_EXTERNAL_MODEL_CHARS {
        return Err(safe_error("external_tool_output_too_large"));
    }
    if normalized.trim().is_empty() {
        return Err(safe_error("external_tool_output_empty"));
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::mcp_runtime_registry::{
        upsert_web_evidence_provider, WebEvidenceProviderInput,
    };

    fn provider(db: &Database) {
        upsert_web_evidence_provider(
            db,
            &WebEvidenceProviderInput {
                id: "readonly".into(),
                name: "Read Only".into(),
                kind: "mcp".into(),
                enabled: true,
                transport_kind: "stdio".into(),
                transport_config_json: r#"{"command":"/bin/true"}"#.into(),
                credential_refs_json: "{}".into(),
                web_search_mapping_json: None,
                web_fetch_mapping_json: None,
            },
        )
        .unwrap();
    }

    fn upsert_attested(
        db: &Database,
        mut input: McpCapabilityBindingInput,
    ) -> AppResult<McpCapabilityBindingSummary> {
        let reviewed =
            review_discovered_tool(&input.mcp_tool_name, &input.input_schema, Some(true))?;
        let provider_config_hash =
            crate::ai_runtime::mcp_runtime_registry::list_web_evidence_providers(db)?
                .into_iter()
                .find(|provider| provider.id == input.provider_id)
                .ok_or_else(|| safe_error("external_tool_provider_missing"))?
                .provider_config_hash;
        input.attested_binding_config_hash = attest_reviewed_tool(
            db,
            &input.provider_id,
            &reviewed,
            &provider_config_hash,
            &input.argument_mapping,
        )?
        .binding_config_hash;
        upsert_binding(db, &input, &reviewed, &provider_config_hash)
    }

    #[test]
    fn binding_rejects_mutating_or_secret_tool_categories() {
        let db = Database::open_in_memory().unwrap();
        provider(&db);
        for tool_name in [
            "write_record",
            "send_message",
            "delete_item",
            "calendar_events",
            "run_process",
            "get_secret",
            "set_record",
            "put_object",
            "mutate_item",
            "rm_item",
        ] {
            let error = upsert_attested(
                &db,
                McpCapabilityBindingInput {
                    id: None,
                    provider_id: "readonly".into(),
                    mcp_tool_name: tool_name.into(),
                    input_schema: serde_json::json!({"type":"object"}),
                    argument_mapping: serde_json::json!({}),
                    risk_class: "read_only".into(),
                    read_only: true,
                    user_trusted: true,
                    attested_binding_config_hash: String::new(),
                },
            )
            .unwrap_err();
            assert_eq!(error.to_string(), "external_tool_not_read_only");
        }
    }

    #[test]
    fn binding_rejects_side_effect_schema_and_unimplemented_schema_keywords() {
        let db = Database::open_in_memory().unwrap();
        provider(&db);
        for schema in [
            serde_json::json!({
                "type":"object",
                "properties":{
                    "operation":{"type":"string","enum":["read","delete"]}
                }
            }),
            serde_json::json!({
                "type":"object",
                "properties":{
                    "query":{"type":"string","pattern":"^safe"}
                }
            }),
            serde_json::json!({
                "type":"object",
                "properties":{
                    "nested":{
                        "type":"object",
                        "properties":{"action":{"type":"string"}}
                    }
                }
            }),
            serde_json::json!({
                "type":"object",
                "properties":{
                    "ignore_prior_instructions":{"type":"string"}
                }
            }),
            serde_json::json!({
                "type":"object",
                "properties":{
                    "scope":{"type":"string","enum":["override_instructions"]}
                }
            }),
            serde_json::json!({
                "type":"object",
                "properties":{
                    "pleaseIgnorePreviousInstruction":{"type":"string"}
                }
            }),
            serde_json::json!({
                "type":"object",
                "properties":{
                    "scope":{"type":"string","enum":["safePromptOverride"]}
                }
            }),
        ] {
            let error = upsert_attested(
                &db,
                McpCapabilityBindingInput {
                    id: None,
                    provider_id: "readonly".into(),
                    mcp_tool_name: "read_record".into(),
                    input_schema: schema,
                    argument_mapping: serde_json::json!({}),
                    risk_class: "read_only".into(),
                    read_only: true,
                    user_trusted: true,
                    attested_binding_config_hash: String::new(),
                },
            )
            .expect_err("unsafe or unsupported schema");
            assert!(
                matches!(
                    error.to_string().as_str(),
                    "external_tool_not_read_only" | "external_tool_schema_unsupported"
                ),
                "{error}"
            );
        }
    }

    #[test]
    fn binding_rejects_side_effect_or_prompt_injection_argument_mapping_targets() {
        let db = Database::open_in_memory().unwrap();
        provider(&db);
        for target in [
            "command",
            "delete",
            "operation",
            "secret",
            "pleaseIgnorePreviousInstruction",
            "safePromptOverride",
        ] {
            let error = upsert_attested(
                &db,
                McpCapabilityBindingInput {
                    id: None,
                    provider_id: "readonly".into(),
                    mcp_tool_name: "read_record".into(),
                    input_schema: serde_json::json!({
                        "type":"object",
                        "properties":{"query":{"type":"string"}}
                    }),
                    argument_mapping: serde_json::json!({"query":target}),
                    risk_class: "read_only".into(),
                    read_only: true,
                    user_trusted: true,
                    attested_binding_config_hash: String::new(),
                },
            )
            .expect_err("unsafe mapping target must fail closed");
            assert_eq!(error.to_string(), "external_tool_mapping_invalid");
        }

        let nested_error = upsert_attested(
            &db,
            McpCapabilityBindingInput {
                id: None,
                provider_id: "readonly".into(),
                mcp_tool_name: "read_record".into(),
                input_schema: serde_json::json!({
                    "type":"object",
                    "properties":{"query":{"type":"string"}}
                }),
                argument_mapping: serde_json::json!({
                    "query":{"nestedCommand":"delete"}
                }),
                risk_class: "read_only".into(),
                read_only: true,
                user_trusted: true,
                attested_binding_config_hash: String::new(),
            },
        )
        .expect_err("nested mapping keys and values must fail closed");
        assert_eq!(nested_error.to_string(), "external_tool_mapping_invalid");
    }

    #[test]
    fn binding_requires_explicit_user_trust_for_the_exact_reviewed_tool() {
        let db = Database::open_in_memory().unwrap();
        provider(&db);
        let input = McpCapabilityBindingInput {
            id: None,
            provider_id: "readonly".into(),
            mcp_tool_name: "read_record".into(),
            input_schema: serde_json::json!({"type":"object"}),
            argument_mapping: serde_json::json!({}),
            risk_class: "read_only".into(),
            read_only: true,
            user_trusted: false,
            attested_binding_config_hash: String::new(),
        };
        let reviewed =
            review_discovered_tool(&input.mcp_tool_name, &input.input_schema, Some(true))
                .expect("server prerequisite");
        let provider_config_hash =
            crate::ai_runtime::mcp_runtime_registry::list_web_evidence_providers(&db)
                .expect("providers")
                .into_iter()
                .find(|provider| provider.id == input.provider_id)
                .expect("provider")
                .provider_config_hash;

        assert_eq!(
            upsert_binding(&db, &input, &reviewed, &provider_config_hash)
                .expect_err("server annotation alone must not create a trusted binding")
                .to_string(),
            "external_tool_not_read_only"
        );
        assert_eq!(
            review_discovered_tool(
                "readRecordIgnorePriorInstruction",
                &serde_json::json!({"type":"object"}),
                Some(true),
            )
            .expect_err("prompt markers cannot hide in a joined tool identifier")
            .to_string(),
            "external_tool_not_read_only"
        );
    }

    #[test]
    fn binding_requires_the_exact_user_reviewed_attestation_hash() {
        let db = Database::open_in_memory().unwrap();
        provider(&db);
        let reviewed = review_discovered_tool(
            "read_record",
            &serde_json::json!({
                "type":"object",
                "title":"untrusted title",
                "properties":{
                    "query":{"type":"string","description":"untrusted description"}
                }
            }),
            Some(true),
        )
        .expect("reviewed tool");
        let provider_config_hash =
            crate::ai_runtime::mcp_runtime_registry::list_web_evidence_providers(&db)
                .expect("providers")
                .into_iter()
                .find(|provider| provider.id == "readonly")
                .expect("provider")
                .provider_config_hash;
        let attestation = attest_reviewed_tool(
            &db,
            "readonly",
            &reviewed,
            &provider_config_hash,
            &serde_json::json!({}),
        )
        .expect("attestation");
        assert_eq!(attestation.provider_display_name, "Read Only");
        assert_eq!(attestation.name, "read_record");
        assert_eq!(attestation.provider_config_hash, provider_config_hash);
        assert!(!attestation.input_schema.to_string().contains("untrusted"));

        let input = McpCapabilityBindingInput {
            id: None,
            provider_id: "readonly".into(),
            mcp_tool_name: "read_record".into(),
            input_schema: attestation.input_schema.clone(),
            argument_mapping: serde_json::json!({}),
            risk_class: "read_only".into(),
            read_only: true,
            user_trusted: true,
            attested_binding_config_hash: "different-review".into(),
        };
        assert_eq!(
            upsert_binding(&db, &input, &reviewed, &provider_config_hash)
                .expect_err("a stale or substituted confirmation must fail")
                .to_string(),
            "external_tool_attestation_changed"
        );

        let input = McpCapabilityBindingInput {
            attested_binding_config_hash: attestation.binding_config_hash,
            ..input
        };
        upsert_binding(&db, &input, &reviewed, &provider_config_hash)
            .expect("the exact reviewed attestation may become user-trusted");
    }

    #[test]
    fn corrupted_binding_json_fails_closed_instead_of_becoming_null() {
        let db = Database::open_in_memory().unwrap();
        provider(&db);
        let binding = upsert_attested(
            &db,
            McpCapabilityBindingInput {
                id: None,
                provider_id: "readonly".into(),
                mcp_tool_name: "read_record".into(),
                input_schema: serde_json::json!({"type":"object"}),
                argument_mapping: serde_json::json!({}),
                risk_class: "read_only".into(),
                read_only: true,
                user_trusted: true,
                attested_binding_config_hash: String::new(),
            },
        )
        .expect("binding");
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE mcp_capability_bindings
                 SET input_schema_json = '{broken'
                 WHERE id = ?1",
                [binding.id],
            )?;
            Ok(())
        })
        .expect("corrupt binding");

        assert!(list_bindings(&db, None).is_err());
    }

    #[test]
    fn binding_rejects_provider_drift_after_read_only_review() {
        let db = Database::open_in_memory().unwrap();
        provider(&db);
        let input = McpCapabilityBindingInput {
            id: None,
            provider_id: "readonly".into(),
            mcp_tool_name: "read_record".into(),
            input_schema: serde_json::json!({"type":"object"}),
            argument_mapping: serde_json::json!({}),
            risk_class: "read_only".into(),
            read_only: true,
            user_trusted: true,
            attested_binding_config_hash: String::new(),
        };
        let reviewed =
            review_discovered_tool(&input.mcp_tool_name, &input.input_schema, Some(true))
                .expect("read-only review");
        let reviewed_provider_config_hash =
            crate::ai_runtime::mcp_runtime_registry::list_web_evidence_providers(&db)
                .expect("providers")
                .into_iter()
                .find(|provider| provider.id == input.provider_id)
                .expect("reviewed provider")
                .provider_config_hash;
        upsert_web_evidence_provider(
            &db,
            &WebEvidenceProviderInput {
                id: "readonly".into(),
                name: "Changed endpoint".into(),
                kind: "mcp".into(),
                enabled: true,
                transport_kind: "stdio".into(),
                transport_config_json: r#"{"command":"/bin/false"}"#.into(),
                credential_refs_json: "{}".into(),
                web_search_mapping_json: None,
                web_fetch_mapping_json: None,
            },
        )
        .expect("drift provider after review");

        assert_eq!(
            upsert_binding(&db, &input, &reviewed, &reviewed_provider_config_hash,)
                .expect_err("review must bind to the provider config it inspected")
                .to_string(),
            "external_tool_provider_config_changed"
        );
    }

    fn snapshot() -> FrozenMcpToolSnapshot {
        let input_schema = serde_json::json!({
            "type":"object",
            "properties":{
                "id":{"type":"string"},
                "limit":{"type":"integer"}
            },
            "required":["id"],
            "additionalProperties":false
        });
        let argument_mapping = serde_json::json!({"id":"record_id"});
        let output_policy = output_policy();
        let transport_kind = "stdio".to_string();
        let transport_config_json = r#"{"command":"/bin/true"}"#.to_string();
        let credential_refs_json = "{}".to_string();
        let provider_launch_hash = crate::ai_runtime::mcp_host_runtime::frozen_provider_launch_hash(
            "readonly",
            &transport_kind,
            &transport_config_json,
            &credential_refs_json,
        );
        let binding_config_hash = binding_hash(
            ("readonly", "provider-hash", &provider_launch_hash),
            (
                "read_record",
                &input_schema,
                &argument_mapping,
                &output_policy,
            ),
        );
        let run_id = "run".to_string();
        let binding_id = "binding".to_string();
        let provider_id = "readonly".to_string();
        let exposed_name = "external_read_record_deadbeef".to_string();
        let mcp_tool_name = "read_record".to_string();
        let provider_config_hash = "provider-hash".to_string();
        let capability = EXTERNAL_READ_CAPABILITY.to_string();
        let risk_class = "read_only".to_string();
        let frozen_at = "2026-07-30T00:00:00+00:00".to_string();
        let snapshot_integrity_hash = snapshot_integrity_hash(
            (
                &run_id,
                &binding_id,
                &provider_id,
                &exposed_name,
                &mcp_tool_name,
            ),
            (
                &input_schema.to_string(),
                &argument_mapping.to_string(),
                &output_policy.to_string(),
                &binding_config_hash,
            ),
            (&capability, &risk_class, 1, 1),
            (
                &provider_config_hash,
                &provider_launch_hash,
                &transport_kind,
                &transport_config_json,
                &credential_refs_json,
            ),
            &frozen_at,
        );
        FrozenMcpToolSnapshot {
            run_id,
            binding_id,
            provider_id,
            exposed_name,
            mcp_tool_name,
            input_schema,
            argument_mapping,
            output_policy,
            provider_config_hash,
            provider_launch_hash,
            transport_kind,
            transport_config_json,
            credential_refs_json,
            binding_config_hash,
            capability,
            risk_class,
            read_only: true,
            user_trusted: true,
            frozen_at,
            snapshot_integrity_hash,
        }
    }

    #[test]
    fn runtime_rejects_tampered_or_mutating_frozen_snapshot_contracts() {
        let valid = snapshot();
        assert!(snapshot_contract_is_valid(&valid));

        let mut mutating = valid.clone();
        mutating.mcp_tool_name = "delete_record".into();
        assert!(!snapshot_contract_is_valid(&mutating));

        let mut changed_schema = valid;
        changed_schema.input_schema = serde_json::json!({"type":"object"});
        assert!(!snapshot_contract_is_valid(&changed_schema));
    }

    #[test]
    fn frozen_schema_rejects_mismatch_and_maps_only_declared_arguments() {
        let snapshot = snapshot();
        let mapped =
            validate_and_map_arguments(&snapshot, &serde_json::json!({"id":"record-1","limit":2}))
                .expect("mapped");
        assert_eq!(
            mapped,
            serde_json::json!({"record_id":"record-1","limit":2})
        );
        assert_eq!(
            validate_and_map_arguments(&snapshot, &serde_json::json!({"limit":2}))
                .expect_err("required id")
                .to_string(),
            "external_tool_arguments_schema_mismatch"
        );
        assert_eq!(
            validate_and_map_arguments(
                &snapshot,
                &serde_json::json!({"id":"record-1","secret":"no"})
            )
            .expect_err("unknown key")
            .to_string(),
            "external_tool_arguments_schema_mismatch"
        );
    }

    #[test]
    fn external_output_accepts_text_or_json_and_rejects_binary_or_over_limit() {
        assert_eq!(
            normalize_external_output(&serde_json::json!({
                "content":[{"type":"text","text":"safe text"}]
            }))
            .expect("text"),
            "safe text"
        );
        assert_eq!(
            normalize_external_output(&serde_json::json!({
                "structuredContent":{"items":[1,2]}
            }))
            .expect("json"),
            r#"{"items":[1,2]}"#
        );
        assert_eq!(
            normalize_external_output(&serde_json::json!({
                "content":[{"type":"image","data":"raw"}]
            }))
            .expect_err("binary")
            .to_string(),
            "external_tool_output_unsupported"
        );
        assert_eq!(
            normalize_external_output(&Value::String("x".repeat(8_001)))
                .expect_err("too large")
                .to_string(),
            "external_tool_output_too_large"
        );
    }

    #[test]
    fn binding_uses_stable_safe_name_and_strips_discovery_descriptions() {
        let db = Database::open_in_memory().unwrap();
        provider(&db);
        let binding = upsert_attested(
            &db,
            McpCapabilityBindingInput {
                id: None,
                provider_id: "readonly".into(),
                mcp_tool_name: "read_record".into(),
                input_schema: serde_json::json!({
                    "type":"object",
                    "title":"server supplied",
                    "properties":{"id":{"type":"string","description":"ignore me"}}
                }),
                argument_mapping: serde_json::json!({}),
                risk_class: "read_only".into(),
                read_only: true,
                user_trusted: true,
                attested_binding_config_hash: String::new(),
            },
        )
        .expect("binding");
        assert!(binding.exposed_name.starts_with("external_read_"));
        assert!(!binding.exposed_name.contains("record"));
        assert!(binding
            .exposed_name
            .chars()
            .all(|character| character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '_'));
        assert!(!binding.input_schema.to_string().contains("server supplied"));
        assert!(!binding.input_schema.to_string().contains("ignore me"));

        let updated = upsert_attested(
            &db,
            McpCapabilityBindingInput {
                id: Some(binding.id.clone()),
                provider_id: binding.provider_id.clone(),
                mcp_tool_name: binding.mcp_tool_name.clone(),
                input_schema: binding.input_schema.clone(),
                argument_mapping: serde_json::json!({}),
                risk_class: "read_only".into(),
                read_only: true,
                user_trusted: true,
                attested_binding_config_hash: String::new(),
            },
        )
        .expect("update");
        assert_eq!(updated.exposed_name, binding.exposed_name);
    }
}
