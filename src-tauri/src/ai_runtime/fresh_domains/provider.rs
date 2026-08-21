//! Deterministic provider resolution for current-fact domain operations.
//!
//! The resolver never inspects provider output. It only selects a frozen MCP
//! binding that was reviewed, user-trusted, enabled, hash-current, and carries a
//! whitelist output mapping. Only a missing News binding may fall back to the
//! generic Web evidence broker; ambiguity is always rejected.

use crate::ai_runtime::mcp_external_tools::{
    list_bindings, DomainOperation, FrozenMcpToolSnapshot, McpCapabilityBindingSummary,
    WEB_DOMAIN_READ_CAPABILITY,
};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

/// One deterministic route for a single current-fact operation.
///
/// The snapshot variant is intentionally large: it carries the full immutable
/// launch contract and is only cloned at dispatch boundaries.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum DomainProviderRoute {
    /// A user-reviewed, frozen MCP mapping with a whitelist output mapping.
    FrozenMcp(FrozenMcpToolSnapshot),
    /// No deterministic structured mapping is available; use WebEvidenceBroker.
    WebEvidence,
}

/// Resolve the deterministic provider route for one domain operation.
///
/// Selection order:
/// 1. The operation itself is the explicit filter.
/// 2. A single eligible mapping is selected automatically.
/// 3. Multiple eligible mappings always fail closed.
///
/// Disabled providers or providers whose config hash drifted are rejected when
/// they were the only configured mapping for the operation.
pub(crate) fn resolve_domain_provider(
    db: &Database,
    operation: DomainOperation,
    _selected_web_provider_id: Option<&str>,
) -> AppResult<DomainProviderRoute> {
    let bindings = list_bindings(db, None)?;
    let operation_bindings = bindings
        .iter()
        .filter(|binding| binding.domain_operation == Some(operation))
        .collect::<Vec<_>>();

    let eligible = operation_bindings
        .iter()
        .filter(|binding| {
            binding.provider_enabled
                && binding.config_matches
                && binding.user_trusted
                && binding.output_mapping.is_some()
        })
        .collect::<Vec<_>>();

    if eligible.is_empty() {
        if let Some(_binding) = operation_bindings
            .iter()
            .find(|binding| !binding.provider_enabled)
        {
            return Err(AppError::msg("external_tool_provider_disabled"));
        }
        if let Some(_binding) = operation_bindings
            .iter()
            .find(|binding| !binding.config_matches)
        {
            return Err(AppError::msg("external_tool_provider_config_changed"));
        }
        return if operation == DomainOperation::NewsSearch {
            Ok(DomainProviderRoute::WebEvidence)
        } else {
            Err(AppError::msg("agent_run_structured_provider_unavailable"))
        };
    }

    if eligible.len() == 1 {
        return Ok(DomainProviderRoute::FrozenMcp(snapshot_from_summary(
            (*eligible[0]).clone(),
            operation,
        )));
    }

    Err(AppError::msg("agent_run_structured_provider_ambiguous"))
}

/// Build a route snapshot from live binding metadata.
///
/// Live bindings do not carry the launch/transport details required to execute
/// an MCP call. This snapshot is only a deterministic route marker; the
/// `FreshDomainService` always prefers the actual run-frozen snapshot loaded by
/// run ID and validates it before execution.
fn snapshot_from_summary(
    summary: McpCapabilityBindingSummary,
    operation: DomainOperation,
) -> FrozenMcpToolSnapshot {
    let output_mapping = summary.output_mapping.clone().unwrap_or_else(|| {
        crate::ai_runtime::mcp_external_tools::DomainOutputMapping {
            records_path: String::new(),
            fields: Default::default(),
        }
    });
    FrozenMcpToolSnapshot {
        run_id: String::new(),
        binding_id: summary.id,
        provider_id: summary.provider_id,
        exposed_name: summary.exposed_name,
        mcp_tool_name: summary.mcp_tool_name,
        input_schema: summary.input_schema,
        argument_mapping: summary.argument_mapping,
        output_policy: summary.output_policy,
        domain_operation: Some(operation),
        output_mapping: Some(output_mapping),
        provider_config_hash: summary.provider_config_hash,
        provider_launch_hash: String::new(),
        transport_kind: String::new(),
        transport_config_json: String::new(),
        credential_refs_json: String::new(),
        binding_config_hash: summary.binding_config_hash,
        capability: WEB_DOMAIN_READ_CAPABILITY.to_string(),
        risk_class: "read_only".to_string(),
        read_only: true,
        user_trusted: true,
        frozen_at: String::new(),
        snapshot_integrity_hash: String::new(),
    }
}

#[cfg(test)]
mod domain_provider_tests {
    use super::*;
    use crate::ai_runtime::mcp_external_tools::{
        attest_reviewed_tool, review_discovered_tool, upsert_binding, DomainOutputMapping,
        McpCapabilityBindingInput,
    };
    use crate::ai_runtime::mcp_runtime_registry::{
        upsert_web_evidence_provider, WebEvidenceProviderInput,
    };

    fn provider(db: &Database, id: &str, name: &str, enabled: bool) {
        upsert_web_evidence_provider(
            db,
            &WebEvidenceProviderInput {
                id: id.into(),
                name: name.into(),
                kind: "mcp".into(),
                enabled,
                transport_kind: "stdio".into(),
                transport_config_json: "{}".into(),
                credential_refs_json: "{}".into(),
                web_search_mapping_json: Some(r#"{"tool":"search"}"#.into()),
                web_fetch_mapping_json: None,
            },
        )
        .unwrap();
    }

    fn domain_binding(
        db: &Database,
        provider_id: &str,
        tool_name: &str,
        operation: DomainOperation,
    ) -> McpCapabilityBindingSummary {
        let reviewed =
            review_discovered_tool(tool_name, &serde_json::json!({"type":"object"}), Some(true))
                .unwrap();
        let provider_config_hash =
            crate::ai_runtime::mcp_runtime_registry::list_web_evidence_providers(db)
                .unwrap()
                .into_iter()
                .find(|provider| provider.id == provider_id)
                .unwrap()
                .provider_config_hash;
        let provider_launch_hash = crate::ai_runtime::mcp_host_runtime::frozen_provider_launch_hash(
            provider_id,
            "stdio",
            "{}",
            "{}",
        );
        let output_mapping = DomainOutputMapping {
            records_path: "$.records".into(),
            fields: [("sourceUrl".to_string(), "$.url".to_string())].into(),
        };
        let binding_config_hash = crate::ai_runtime::mcp_external_tools::test_binding_hash(
            provider_id,
            &provider_config_hash,
            &provider_launch_hash,
            tool_name,
            &reviewed.input_schema,
            operation,
            &output_mapping,
        );
        let input = McpCapabilityBindingInput {
            id: None,
            provider_id: provider_id.into(),
            mcp_tool_name: tool_name.into(),
            input_schema: reviewed.input_schema.clone(),
            argument_mapping: serde_json::json!({}),
            domain_operation: Some(operation),
            output_mapping: Some(output_mapping),
            risk_class: "read_only".into(),
            read_only: true,
            user_trusted: true,
            attested_binding_config_hash: binding_config_hash,
        };
        upsert_binding(db, &input, &reviewed, &provider_config_hash).unwrap()
    }

    #[test]
    fn explicit_operation_prefers_unique_healthy_mapping() {
        let db = Database::open_in_memory().unwrap();
        provider(&db, "weather-mcp", "Weather MCP", true);
        domain_binding(
            &db,
            "weather-mcp",
            "weather",
            DomainOperation::WeatherCurrent,
        );

        let route = resolve_domain_provider(&db, DomainOperation::WeatherCurrent, None).unwrap();

        match route {
            DomainProviderRoute::FrozenMcp(snapshot) => {
                assert_eq!(snapshot.provider_id, "weather-mcp");
                assert_eq!(
                    snapshot.domain_operation,
                    Some(DomainOperation::WeatherCurrent)
                );
            }
            DomainProviderRoute::WebEvidence => panic!("unique healthy mapping must be selected"),
        }
    }

    #[test]
    fn selected_web_provider_does_not_break_domain_mapping_ties() {
        let db = Database::open_in_memory().unwrap();
        provider(&db, "weather-a", "Weather A", true);
        provider(&db, "weather-b", "Weather B", true);
        domain_binding(
            &db,
            "weather-a",
            "weather_a",
            DomainOperation::WeatherCurrent,
        );
        domain_binding(
            &db,
            "weather-b",
            "weather_b",
            DomainOperation::WeatherCurrent,
        );
        let error =
            resolve_domain_provider(&db, DomainOperation::WeatherCurrent, Some("weather-b"))
                .expect_err("selected generic Web provider must not choose a domain binding");

        assert_eq!(error.to_string(), "agent_run_structured_provider_ambiguous");
    }

    #[test]
    fn multiple_eligible_news_mappings_fail_closed_instead_of_falling_back() {
        let db = Database::open_in_memory().unwrap();
        provider(&db, "news-a", "News A", true);
        provider(&db, "news-b", "News B", true);
        domain_binding(&db, "news-a", "news_a", DomainOperation::NewsSearch);
        domain_binding(&db, "news-b", "news_b", DomainOperation::NewsSearch);

        let error = resolve_domain_provider(&db, DomainOperation::NewsSearch, None).unwrap_err();

        assert_eq!(error.to_string(), "agent_run_structured_provider_ambiguous");
    }

    #[test]
    fn provider_hash_drift_is_rejected_instead_of_selected() {
        let db = Database::open_in_memory().unwrap();
        provider(&db, "weather-mcp", "Weather MCP", true);
        domain_binding(
            &db,
            "weather-mcp",
            "weather",
            DomainOperation::WeatherCurrent,
        );

        upsert_web_evidence_provider(
            &db,
            &WebEvidenceProviderInput {
                id: "weather-mcp".into(),
                name: "Changed endpoint".into(),
                kind: "mcp".into(),
                enabled: true,
                transport_kind: "stdio".into(),
                transport_config_json: r#"{"command":"/bin/false"}"#.into(),
                credential_refs_json: "{}".into(),
                web_search_mapping_json: Some(r#"{"tool":"search"}"#.into()),
                web_fetch_mapping_json: None,
            },
        )
        .unwrap();

        let error =
            resolve_domain_provider(&db, DomainOperation::WeatherCurrent, None).unwrap_err();

        assert_eq!(error.to_string(), "external_tool_provider_config_changed");
    }

    #[test]
    fn non_news_operation_without_a_domain_binding_fails_closed() {
        let db = Database::open_in_memory().unwrap();
        provider(&db, "readonly", "Read Only", true);
        let reviewed = review_discovered_tool(
            "read_record",
            &serde_json::json!({"type":"object"}),
            Some(true),
        )
        .unwrap();
        let provider_config_hash =
            crate::ai_runtime::mcp_runtime_registry::list_web_evidence_providers(&db)
                .unwrap()
                .into_iter()
                .find(|provider| provider.id == "readonly")
                .unwrap()
                .provider_config_hash;
        let attestation = attest_reviewed_tool(
            &db,
            "readonly",
            &reviewed,
            &provider_config_hash,
            &serde_json::json!({}),
        )
        .unwrap();
        let _ = upsert_binding(
            &db,
            &McpCapabilityBindingInput {
                id: None,
                provider_id: "readonly".into(),
                mcp_tool_name: "read_record".into(),
                input_schema: reviewed.input_schema.clone(),
                argument_mapping: serde_json::json!({}),
                domain_operation: None,
                output_mapping: None,
                risk_class: "read_only".into(),
                read_only: true,
                user_trusted: true,
                attested_binding_config_hash: attestation.binding_config_hash,
            },
            &reviewed,
            &provider_config_hash,
        )
        .unwrap();

        let error = resolve_domain_provider(&db, DomainOperation::WeatherCurrent, None)
            .expect_err("a generic external binding must not become Weather fallback");

        assert_eq!(
            error.to_string(),
            "agent_run_structured_provider_unavailable"
        );
    }

    #[test]
    fn disabled_provider_is_rejected_instead_of_silently_selected() {
        let db = Database::open_in_memory().unwrap();
        provider(&db, "weather-mcp", "Weather MCP", false);
        domain_binding(
            &db,
            "weather-mcp",
            "weather",
            DomainOperation::WeatherCurrent,
        );

        let error =
            resolve_domain_provider(&db, DomainOperation::WeatherCurrent, None).unwrap_err();

        assert_eq!(error.to_string(), "external_tool_provider_disabled");
    }
}
