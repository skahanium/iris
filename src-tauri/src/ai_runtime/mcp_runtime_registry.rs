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
    validate_provider_json("transport config", &input.transport_config_json)?;
    validate_provider_json("credential refs", &input.credential_refs_json)?;
    validate_optional_mapping("web.search mapping", &input.web_search_mapping_json)?;
    validate_optional_mapping("web.fetch mapping", &input.web_fetch_mapping_json)?;

    Ok(WebEvidenceProviderInput {
        id,
        name: name.to_string(),
        kind,
        enabled: input.enabled,
        transport_kind,
        transport_config_json: input.transport_config_json.trim().to_string(),
        credential_refs_json: input.credential_refs_json.trim().to_string(),
        web_search_mapping_json: input
            .web_search_mapping_json
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
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
        Ok(())
    })
}

pub fn list_web_evidence_providers(db: &Database) -> AppResult<Vec<WebEvidenceProviderSummary>> {
    db.with_conn(|conn| {
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
    fn web_evidence_provider_accepts_structured_header_and_env_refs() {
        let db = Database::open_in_memory().unwrap();
        let mut input = provider();
        input.transport_kind = "https".into();
        input.transport_config_json =
            r#"{"url":"https://api.anysearch.com/mcp","allow_localhost_dev":false}"#.into();
        input.credential_refs_json = r#"{"headers":{"Authorization":{"scheme":"bearer","credential":"credential://iris.mcp.anysearch"}},"env":{"ANYSEARCH_API_KEY":"iris.mcp.anysearch"}}"#.into();

        upsert_web_evidence_provider(&db, &input).unwrap();

        let stored = list_web_evidence_providers(&db).unwrap();
        assert_eq!(stored[0].credential_refs_json, input.credential_refs_json);
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
