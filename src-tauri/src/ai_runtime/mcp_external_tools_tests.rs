//! Tests extracted from `mcp_external_tools` to satisfy `size:check`.

use super::mcp_external_tools::*;
use crate::ai_runtime::mcp_runtime_registry::{
    upsert_web_evidence_provider, WebEvidenceProviderInput,
};
use crate::ai_runtime::run_contract::DomainOperation;
use crate::error::AppResult;
use crate::storage::db::Database;
use rusqlite::params;
use serde_json::Value;
use std::collections::BTreeMap;

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
    let reviewed = review_discovered_tool(&input.mcp_tool_name, &input.input_schema, Some(true))?;
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
                domain_operation: None,
                output_mapping: None,
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
                domain_operation: None,
                output_mapping: None,
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
                domain_operation: None,
                output_mapping: None,
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
            domain_operation: None,
            output_mapping: None,
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
        domain_operation: None,
        output_mapping: None,
    };
    let reviewed = review_discovered_tool(&input.mcp_tool_name, &input.input_schema, Some(true))
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
        domain_operation: None,
        output_mapping: None,
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
            domain_operation: None,
            output_mapping: None,
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
        domain_operation: None,
        output_mapping: None,
    };
    let reviewed = review_discovered_tool(&input.mcp_tool_name, &input.input_schema, Some(true))
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

#[test]
fn list_does_not_invalidate_existing_binding_hash() {
    let db = Database::open_in_memory().unwrap();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO web_evidence_providers
             (id, name, kind, enabled, transport_kind, transport_config_json,
              credential_refs_json, web_search_mapping_json, web_fetch_mapping_json,
              provider_config_hash, updated_at)
             VALUES (?1, ?2, 'mcp', 1, 'stdio', ?3, '{}', ?4, NULL, 'legacy', datetime('now'))",
            params![
                "readonly-legacy",
                "AnySearch",
                r#"{"command":"/bin/true"}"#,
                r#"{"tool":"search"}"#,
            ],
        )?;
        Ok(())
    })
    .unwrap();

    let mut input = McpCapabilityBindingInput {
        id: None,
        provider_id: "readonly-legacy".into(),
        mcp_tool_name: "read_record".into(),
        input_schema: serde_json::json!({"type":"object"}),
        argument_mapping: serde_json::json!({}),
        risk_class: "read_only".into(),
        read_only: true,
        user_trusted: true,
        attested_binding_config_hash: String::new(),
        domain_operation: None,
        output_mapping: None,
    };
    let reviewed = review_discovered_tool(&input.mcp_tool_name, &input.input_schema, Some(true))
        .expect("read-only review");
    input.attested_binding_config_hash = attest_reviewed_tool(
        &db,
        &input.provider_id,
        &reviewed,
        "legacy",
        &input.argument_mapping,
    )
    .expect("attest stored hash")
    .binding_config_hash;
    upsert_binding(&db, &input, &reviewed, "legacy").expect("bind to stored hash");

    crate::ai_runtime::mcp_runtime_registry::list_web_evidence_providers(&db)
        .expect("list must stay readable");
    assert!(
        list_bindings(&db, Some("readonly-legacy")).unwrap()[0].config_matches,
        "listing providers must not rotate the stored provider hash"
    );

    upsert_web_evidence_provider(
        &db,
        &WebEvidenceProviderInput {
            id: "readonly-legacy".into(),
            name: "AnySearch".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json: r#"{"command":"/bin/true"}"#.into(),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: Some(r#"{"tool":"search"}"#.into()),
            web_fetch_mapping_json: None,
        },
    )
    .expect("explicit save upgrades mapping");
    assert!(
        !list_bindings(&db, Some("readonly-legacy")).unwrap()[0].config_matches,
        "explicit save may change the hash and must not auto-expand trust"
    );
    assert_eq!(
        upsert_binding(&db, &input, &reviewed, "legacy")
            .expect_err("stale review hash must be rejected after save")
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
        None,
        None,
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
        (None, "{}"),
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
        domain_operation: None,
        output_mapping: None,
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
fn new_domain_binding_input_is_rejected_before_it_can_recreate_retired_routing() {
    let db = Database::open_in_memory().unwrap();
    provider(&db);
    let reviewed =
        review_discovered_tool("weather", &serde_json::json!({"type":"object"}), Some(true))
            .expect("reviewed tool");
    let provider_config_hash =
        crate::ai_runtime::mcp_runtime_registry::list_web_evidence_providers(&db)
            .expect("providers")
            .into_iter()
            .find(|provider| provider.id == "readonly")
            .expect("provider")
            .provider_config_hash;
    let output_mapping = DomainOutputMapping {
        records_path: "$.records".into(),
        fields: BTreeMap::from([("temperature".to_string(), "$.temp".to_string())]),
    };
    let domain_operation = DomainOperation::WeatherCurrent;
    let input = McpCapabilityBindingInput {
        id: None,
        provider_id: "readonly".into(),
        mcp_tool_name: "weather".into(),
        input_schema: reviewed.input_schema.clone(),
        argument_mapping: serde_json::json!({}),
        domain_operation: Some(domain_operation),
        output_mapping: Some(output_mapping.clone()),
        risk_class: "read_only".into(),
        read_only: true,
        user_trusted: true,
        attested_binding_config_hash: String::new(),
    };
    assert_eq!(
        upsert_binding(&db, &input, &reviewed, &provider_config_hash)
            .expect_err("retired domain input must not create a new binding")
            .to_string(),
        "external_tool_binding_invalid"
    );
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO sessions (session_key, created_at, updated_at)
             VALUES ('retired-domain-binding', datetime('now'), datetime('now'))",
            [],
        )?;
        let session_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO agent_runs
             (run_id, client_request_id, session_id, turn_id, status, state_version,
              effect, effort, security_domain, risk, envelope_json, goal_summary,
              created_at, updated_at)
             VALUES ('retired-domain-binding-run', 'retired-domain-binding-client', ?1,
                     'turn', 'accepted', 0, 'answer', 'direct', 'normal', 'read_only',
                     '{}', '', datetime('now'), datetime('now'))",
            [session_id],
        )?;
        freeze_domain_run_grants(
            conn,
            "retired-domain-binding-run",
            &[domain_operation],
            None,
        )
    })
    .expect("a rejected new domain binding cannot freeze a legacy grant");
    let snapshot_count = db
        .with_read_conn(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM agent_run_mcp_tool_snapshots
                 WHERE run_id = 'retired-domain-binding-run'",
                [],
                |row| row.get::<_, i64>(0),
            )?)
        })
        .expect("snapshot count");
    assert_eq!(snapshot_count, 0);
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
            domain_operation: None,
            output_mapping: None,
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
            domain_operation: None,
            output_mapping: None,
        },
    )
    .expect("update");
    assert_eq!(updated.exposed_name, binding.exposed_name);
}
