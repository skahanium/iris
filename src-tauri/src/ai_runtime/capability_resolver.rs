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

    let providers = crate::ai_runtime::mcp_runtime_registry::list_enabled_web_provider_mappings(db)
        .map_err(|err| {
            blocked(
                &capability,
                CapabilityBlockReason::ProviderNotImplemented,
                format!("failed to read web evidence provider mappings: {err}"),
            )
        })?;

    for provider in providers {
        if provider.kind != "mcp" {
            continue;
        }
        let mapping_json = match capability.as_str() {
            "web.search" => provider.web_search_mapping_json.as_deref(),
            "web.fetch" => provider.web_fetch_mapping_json.as_deref(),
            _ => None,
        };
        let Some(tool_name) = mapping_json.and_then(mapping_tool_name) else {
            continue;
        };
        return Ok(ResolvedCapabilityProvider {
            capability,
            provider_kind: "mcp".into(),
            profile_id: provider.id,
            tool_name,
            schema_hash: provider.provider_config_hash,
            requires_confirmation: true,
        });
    }

    Err(blocked(
        &capability,
        CapabilityBlockReason::MissingMcpProfile,
        "no enabled MCP web evidence provider exposes an explicit mapping for this capability",
    ))
}

fn mapping_tool_name(mapping_json: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(mapping_json).ok()?;
    value
        .get("tool")
        .or_else(|| value.get("tool_name"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
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
    matches!(capability, "web.search" | "web.fetch")
}

fn normalize_capability(raw: &str) -> String {
    raw.trim().to_lowercase().replace('_', ".")
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
        upsert_web_evidence_provider, WebEvidenceProviderInput,
    };

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
    fn target_state_supports_only_web_search_and_fetch() {
        assert!(is_supported_capability("web.search"));
        assert!(is_supported_capability("web.fetch"));

        for capability in [
            "web.to_markdown",
            "web.download_to_assets",
            "app_state.read",
            "app_state.write",
            "secret.exists",
            "secret.use_named",
            "process.run_readonly",
            "process.long_running",
            "unsupported.runtime_bridge",
        ] {
            assert!(
                !is_supported_capability(capability),
                "{capability} must not remain in target-state capability vocabulary"
            );
        }
    }

    #[test]
    fn resolves_explicit_enabled_web_provider_mapping() {
        let db = Database::open_in_memory().unwrap();
        upsert_web_evidence_provider(&db, &provider()).unwrap();

        let resolved = resolve_required_capability(&db, "web.search").unwrap();
        assert_eq!(resolved.provider_kind, "mcp");
        assert_eq!(resolved.profile_id, "anysearch");
        assert_eq!(resolved.tool_name, "search");
        assert_eq!(resolved.schema_hash.len(), 24);
    }

    #[test]
    fn disabled_provider_mapping_is_not_resolved() {
        let db = Database::open_in_memory().unwrap();
        let mut input = provider();
        input.enabled = false;
        upsert_web_evidence_provider(&db, &input).unwrap();

        let err = resolve_required_capability(&db, "web.search").unwrap_err();
        assert_eq!(err.reason, CapabilityBlockReason::MissingMcpProfile);
    }
}
