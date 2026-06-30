//! Resolve stable Iris capabilities to controlled runtime providers.
//!
//! MCP tool inventory is metadata until a user-approved capability mapping makes
//! it usable through Iris policy and confirmation boundaries.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityBlockReason {
    UnsupportedCapability,
    MissingMcpProfile,
    ProfileDisabled,
    ProfileUnhealthy,
    MissingGrant,
    PolicyBlocked,
    ProviderNotImplemented,
}

impl CapabilityBlockReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedCapability => "unsupported_capability",
            Self::MissingMcpProfile => "missing_mcp_profile",
            Self::ProfileDisabled => "profile_disabled",
            Self::ProfileUnhealthy => "profile_unhealthy",
            Self::MissingGrant => "missing_grant",
            Self::PolicyBlocked => "policy_blocked",
            Self::ProviderNotImplemented => "provider_not_implemented",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityResolutionError {
    pub capability: String,
    pub reason: CapabilityBlockReason,
    pub message: String,
}

impl CapabilityResolutionError {
    pub fn reason_code(&self) -> &'static str {
        self.reason.as_str()
    }
}

impl std::fmt::Display for CapabilityResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.reason.as_str(), self.message)
    }
}

impl std::error::Error for CapabilityResolutionError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedCapabilityProvider {
    pub capability: String,
    pub provider_kind: String,
    pub profile_id: String,
    pub tool_name: String,
    pub schema_hash: String,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityGrantRequirement {
    pub capability: String,
    pub permission_name: String,
    pub risk_level: String,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedCapabilitySummary {
    pub capability: String,
    pub reason_code: String,
    pub message: String,
}

pub fn resolve_required_capability(
    db: &Database,
    capability: &str,
) -> Result<ResolvedCapabilityProvider, CapabilityResolutionError> {
    let capability = normalize_capability(capability);
    if !is_supported_capability(&capability) {
        return Err(blocked(
            &capability,
            CapabilityBlockReason::UnsupportedCapability,
            "capability is not part of the Iris capability vocabulary",
        ));
    }

    let profiles =
        crate::ai_runtime::mcp_runtime_registry::list_runtime_profiles(db).map_err(|err| {
            blocked(
                &capability,
                CapabilityBlockReason::ProviderNotImplemented,
                format!("failed to read MCP runtime profiles: {err}"),
            )
        })?;

    let mut saw_disabled = false;
    let mut saw_unhealthy = false;
    for profile in profiles {
        let tools = crate::ai_runtime::mcp_runtime_registry::list_tool_inventory(db, &profile.id)
            .map_err(|err| {
            blocked(
                &capability,
                CapabilityBlockReason::ProviderNotImplemented,
                format!("failed to read MCP tool inventory: {err}"),
            )
        })?;
        for tool in tools {
            if !explicit_mapping_contains_capability(&tool.capability_mapping_json, &capability) {
                continue;
            }
            if !profile.enabled {
                saw_disabled = true;
                continue;
            }
            if profile.status != crate::ai_runtime::mcp_runtime_registry::McpRuntimeStatus::Ready {
                saw_unhealthy = true;
                continue;
            }
            return Ok(ResolvedCapabilityProvider {
                capability,
                provider_kind: "mcp".into(),
                profile_id: profile.id,
                tool_name: tool.tool_name,
                schema_hash: tool.schema_hash,
                requires_confirmation: true,
            });
        }
    }

    if saw_disabled {
        return Err(blocked(
            &capability,
            CapabilityBlockReason::ProfileDisabled,
            "a mapped MCP profile exists but is disabled",
        ));
    }
    if saw_unhealthy {
        return Err(blocked(
            &capability,
            CapabilityBlockReason::ProfileUnhealthy,
            "a mapped MCP profile exists but is not ready",
        ));
    }
    Err(blocked(
        &capability,
        CapabilityBlockReason::MissingMcpProfile,
        "no enabled ready MCP profile exposes an approved mapping for this capability",
    ))
}

fn blocked(
    capability: &str,
    reason: CapabilityBlockReason,
    message: impl Into<String>,
) -> CapabilityResolutionError {
    CapabilityResolutionError {
        capability: capability.to_string(),
        reason,
        message: message.into(),
    }
}

fn is_supported_capability(capability: &str) -> bool {
    matches!(
        capability,
        "web.search"
            | "web.fetch"
            | "web.to_markdown"
            | "web.download_to_assets"
            | "skill.read_resource"
            | "skill.write_storage"
            | "skill.mcp_bridge"
            | "app_state.read"
            | "app_state.write"
            | "secret.exists"
            | "secret.use_named"
            | "process.run_readonly"
            | "process.long_running"
    )
}

fn normalize_capability(raw: &str) -> String {
    raw.trim().to_lowercase().replace('_', ".")
}

fn explicit_mapping_contains_capability(mapping_json: &str, capability: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(mapping_json) else {
        return false;
    };
    match value {
        serde_json::Value::String(raw) => normalize_capability(&raw) == capability,
        serde_json::Value::Array(items) => items.iter().any(|item| {
            item.as_str()
                .map(|raw| normalize_capability(raw) == capability)
                .unwrap_or(false)
        }),
        serde_json::Value::Object(map) => {
            map.get("capability")
                .and_then(|value| value.as_str())
                .map(|raw| normalize_capability(raw) == capability)
                .unwrap_or(false)
                || map
                    .get("capabilities")
                    .and_then(|value| value.as_array())
                    .map(|items| {
                        items.iter().any(|item| {
                            item.as_str()
                                .map(|raw| normalize_capability(raw) == capability)
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
        }
        _ => false,
    }
}

pub fn grant_requirement_for_capability(capability: &str) -> CapabilityGrantRequirement {
    CapabilityGrantRequirement {
        capability: capability.to_string(),
        permission_name: capability.to_string(),
        risk_level: "high".into(),
        requires_confirmation: true,
    }
}

pub fn blocked_summary(error: &CapabilityResolutionError) -> BlockedCapabilitySummary {
    BlockedCapabilitySummary {
        capability: error.capability.clone(),
        reason_code: error.reason_code().into(),
        message: error.message.clone(),
    }
}

impl From<CapabilityResolutionError> for AppError {
    fn from(value: CapabilityResolutionError) -> Self {
        AppError::msg(value.to_string())
    }
}

pub fn resolve_required_capability_app(
    db: &Database,
    capability: &str,
) -> AppResult<ResolvedCapabilityProvider> {
    resolve_required_capability(db, capability).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::mcp_runtime_registry::{
        record_tool_inventory, upsert_runtime_profile, upsert_server_catalog,
        McpRuntimeProfileInput, McpRuntimeStatus, McpServerCatalogInput, McpToolInventoryInput,
    };

    fn db() -> Database {
        Database::open_in_memory().unwrap()
    }

    fn server(id: &str) -> McpServerCatalogInput {
        McpServerCatalogInput {
            id: id.into(),
            display_name: id.into(),
            transport: "stdio".into(),
            command: Some("fake-mcp".into()),
            args_json: "[]".into(),
            url: None,
            env_schema_json: "{}".into(),
            capability_tags_json: "[\"web.search\"]".into(),
            source: "test".into(),
        }
    }

    fn profile(
        profile_id: &str,
        server_id: &str,
        enabled: bool,
        status: McpRuntimeStatus,
    ) -> McpRuntimeProfileInput {
        McpRuntimeProfileInput {
            id: profile_id.into(),
            server_id: server_id.into(),
            vault_scope_hash: None,
            display_name: profile_id.into(),
            enabled,
            transport_config_json: "{}".into(),
            env_bindings_json: "{}".into(),
            status,
            last_error: None,
        }
    }

    fn inventory(profile_id: &str, tool_name: &str, mapping: &str) -> McpToolInventoryInput {
        McpToolInventoryInput {
            profile_id: profile_id.into(),
            tool_name: tool_name.into(),
            schema_hash: "sha256:test".into(),
            capability_mapping_json: mapping.into(),
            description: Some("test tool".into()),
        }
    }

    #[test]
    fn resolver_distinguishes_missing_profile_from_unsupported_capability() {
        let db = db();

        let missing = resolve_required_capability(&db, "web.search").unwrap_err();
        assert_eq!(missing.reason_code(), "missing_mcp_profile");

        let unsupported = resolve_required_capability(&db, "unknown.capability").unwrap_err();
        assert_eq!(unsupported.reason_code(), "unsupported_capability");
    }

    #[test]
    fn resolver_uses_only_explicit_approved_capability_mapping() {
        let db = db();
        upsert_server_catalog(&db, &server("search-server")).unwrap();
        upsert_runtime_profile(
            &db,
            &profile(
                "search-profile",
                "search-server",
                true,
                McpRuntimeStatus::Ready,
            ),
        )
        .unwrap();
        record_tool_inventory(
            &db,
            &inventory(
                "search-profile",
                "annotated_search",
                "{\"annotations\":{\"capability\":\"web.search\"}}",
            ),
        )
        .unwrap();

        let missing = resolve_required_capability(&db, "web.search").unwrap_err();
        assert_eq!(missing.reason_code(), "missing_mcp_profile");

        record_tool_inventory(
            &db,
            &inventory(
                "search-profile",
                "approved_search",
                "{\"capability\":\"web.search\"}",
            ),
        )
        .unwrap();

        let resolved = resolve_required_capability(&db, "web.search").unwrap();
        assert_eq!(resolved.provider_kind, "mcp");
        assert_eq!(resolved.profile_id, "search-profile");
        assert_eq!(resolved.tool_name, "approved_search");
        assert!(resolved.requires_confirmation);
    }

    #[test]
    fn resolver_reports_disabled_and_unhealthy_profiles_separately() {
        let db = db();
        upsert_server_catalog(&db, &server("search-server")).unwrap();
        upsert_runtime_profile(
            &db,
            &profile(
                "disabled-profile",
                "search-server",
                false,
                McpRuntimeStatus::Ready,
            ),
        )
        .unwrap();
        record_tool_inventory(
            &db,
            &inventory(
                "disabled-profile",
                "search",
                "{\"capability\":\"web.search\"}",
            ),
        )
        .unwrap();

        let disabled = resolve_required_capability(&db, "web.search").unwrap_err();
        assert_eq!(disabled.reason_code(), "profile_disabled");

        upsert_runtime_profile(
            &db,
            &profile(
                "disabled-profile",
                "search-server",
                true,
                McpRuntimeStatus::Unavailable,
            ),
        )
        .unwrap();
        let unhealthy = resolve_required_capability(&db, "web.search").unwrap_err();
        assert_eq!(unhealthy.reason_code(), "profile_unhealthy");
    }
}
