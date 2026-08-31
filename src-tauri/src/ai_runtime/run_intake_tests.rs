use super::run_contract::{
    AssistantRunControlRequest, AssistantRunRetryRequest, AssistantRunStartRequest,
    AssistantTurnDraft, ContextMode, DisplayMention, DisplayMentionKind, DisplayMentionRange,
    Effect, Effort, ExplicitAction, ExplicitTarget, Freshness, RiskClass, RunControlAction,
    RunEventPayload, RunEventType, RunRecoveryKind, RunState, SecurityDomain, SelectionSnapshot,
    VerificationRequirement, WebDecisionReason,
};
use super::run_engine::RunEventSink;
use super::run_intake::RunIntake;
use super::{
    agent_run_repository::{AgentRunRepository, AppendRunEventInput, DurableApplyCheckpointStage},
    frozen_change_plan::{FrozenChangePlan, FrozenChangePlanInput},
};
use std::sync::Arc;

use crate::error::AppResult;
use crate::storage::db::Database;

fn request() -> AssistantRunStartRequest {
    AssistantRunStartRequest {
        client_request_id: "intake-client-request".to_string(),
        session: None,
        turn: AssistantTurnDraft {
            message: "请概述这份资料的要点".to_string(),
            content_parts: None,
            explicit_references: vec![],
            retrieval_scope: Default::default(),
            display_mentions: vec![],
        },
        explicit_action: None,
        web_enabled: false,
        model_override: None,
        external_tool_grants: Vec::new(),
        security_domain: SecurityDomain::Normal,
        classified_context_ref: None,
    }
}

fn valid_content_hash() -> String {
    "a".repeat(64)
}

fn valid_reference() -> crate::ai_types::ContextReferenceWire {
    crate::ai_types::ContextReferenceWire {
        id: "reference".into(),
        kind: crate::ai_types::ContextReferenceKind::Note,
        file_path: Some("notes/reference.md".into()),
        content_hash: Some(valid_content_hash()),
        utf8_range: None,
        editor_range: None,
        excerpt: String::new(),
        heading_path: None,
        anchor: None,
        stale: false,
        invalid_reason: None,
    }
}

#[test]
fn context_reference_wire_rejects_unknown_fields() {
    let mut value = serde_json::to_value(valid_reference()).expect("reference json");
    value["unexpectedField"] = serde_json::json!(true);

    assert!(serde_json::from_value::<crate::ai_types::ContextReferenceWire>(value).is_err());
}

#[test]
fn intake_rejects_unbounded_or_malformed_explicit_references() {
    let invalid_references = [
        crate::ai_types::ContextReferenceWire {
            file_path: None,
            ..valid_reference()
        },
        crate::ai_types::ContextReferenceWire {
            content_hash: None,
            ..valid_reference()
        },
        crate::ai_types::ContextReferenceWire {
            id: "x".repeat(161),
            ..valid_reference()
        },
        crate::ai_types::ContextReferenceWire {
            content_hash: Some("A".repeat(64)),
            ..valid_reference()
        },
        crate::ai_types::ContextReferenceWire {
            excerpt: "x".repeat(513),
            ..valid_reference()
        },
        crate::ai_types::ContextReferenceWire {
            utf8_range: Some(crate::ai_types::SourceSpan { start: 4, end: 4 }),
            ..valid_reference()
        },
        crate::ai_types::ContextReferenceWire {
            editor_range: Some(crate::ai_types::EditorRangeWire { from: 2, to: 1 }),
            ..valid_reference()
        },
        crate::ai_types::ContextReferenceWire {
            kind: crate::ai_types::ContextReferenceKind::Artifact,
            ..valid_reference()
        },
    ];

    for (index, reference) in invalid_references.into_iter().enumerate() {
        let mut invalid = request();
        invalid.client_request_id = format!("invalid-reference-{index}");
        invalid.turn.explicit_references = vec![reference];
        assert_eq!(
            RunIntake::start(&Database::open_in_memory().expect("database"), invalid)
                .expect_err("invalid reference must be rejected")
                .to_string(),
            "agent_run_invalid_explicit_reference"
        );
    }

    let mut too_many = request();
    too_many.client_request_id = "too-many-references".into();
    too_many.turn.explicit_references = (0..13)
        .map(|index| crate::ai_types::ContextReferenceWire {
            id: format!("reference-{index}"),
            ..valid_reference()
        })
        .collect();
    assert_eq!(
        RunIntake::start(&Database::open_in_memory().expect("database"), too_many)
            .expect_err("reference count must be bounded")
            .to_string(),
        "agent_run_invalid_explicit_reference"
    );
}

#[test]
fn explicit_external_grant_is_frozen_atomically_and_enters_the_run_surface() {
    use super::mcp_external_tools::{
        list_bindings, load_run_snapshots, review_discovered_tool, upsert_binding,
        McpCapabilityBindingInput,
    };
    use super::mcp_runtime_registry::{
        list_web_evidence_providers, upsert_web_evidence_provider, WebEvidenceProviderInput,
    };
    use super::run_contract::ExternalToolGrantRef;

    let db = Database::open_in_memory().expect("database");
    upsert_web_evidence_provider(
        &db,
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
    .expect("provider");
    let binding_input = McpCapabilityBindingInput {
        id: None,
        provider_id: "readonly".into(),
        mcp_tool_name: "read_record".into(),
        input_schema: serde_json::json!({
            "type":"object",
            "properties":{"id":{"type":"string","description":"untrusted"}},
            "required":["id"],
            "additionalProperties":false
        }),
        argument_mapping: serde_json::json!({"id":"record_id"}),
        risk_class: "read_only".into(),
        read_only: true,
        user_trusted: true,
        attested_binding_config_hash: String::new(),
        domain_operation: None,
        output_mapping: None,
    };
    let reviewed = review_discovered_tool(
        &binding_input.mcp_tool_name,
        &binding_input.input_schema,
        Some(true),
    )
    .expect("read-only attestation");
    let reviewed_provider_hash = list_web_evidence_providers(&db)
        .expect("providers")
        .into_iter()
        .find(|provider| provider.id == binding_input.provider_id)
        .expect("reviewed provider")
        .provider_config_hash;
    let attestation = super::mcp_external_tools::attest_reviewed_tool(
        &db,
        &binding_input.provider_id,
        &reviewed,
        &reviewed_provider_hash,
        &binding_input.argument_mapping,
    )
    .expect("binding attestation");
    let binding_input = McpCapabilityBindingInput {
        attested_binding_config_hash: attestation.binding_config_hash,
        ..binding_input
    };
    let binding =
        upsert_binding(&db, &binding_input, &reviewed, &reviewed_provider_hash).expect("binding");
    assert_eq!(list_bindings(&db, None).expect("list").len(), 1);

    let mut granted_request = request();
    granted_request.external_tool_grants = vec![ExternalToolGrantRef {
        binding_id: binding.id.clone(),
        binding_config_hash: binding.binding_config_hash.clone(),
    }];
    let envelope = RunIntake::resolve_envelope(&granted_request).expect("envelope");
    assert_eq!(envelope.effort, Effort::ToolLoop);
    assert_eq!(envelope.freshness, Freshness::Offline);
    assert_eq!(
        envelope.verification_requirement,
        VerificationRequirement::CurrentRunExternal
    );
    assert!(envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "external.read"));

    let accepted = RunIntake::start(&db, granted_request).expect("accepted");
    let snapshots = load_run_snapshots(&db, &accepted.run_id).expect("snapshots");
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].binding_id, binding.id);
    assert_eq!(
        snapshots[0].provider_config_hash,
        binding.provider_config_hash
    );
    assert!(snapshots[0]
        .input_schema
        .to_string()
        .find("untrusted")
        .is_none());

    let registry =
        super::tool_executor::ToolRegistry::for_run(&db, &accepted.run_id).expect("registry");
    let visible = registry.tools_for_authorized_capabilities(
        &[super::run_contract::CapabilityId::new("external.read")],
        true,
    );
    let tool = visible
        .iter()
        .find(|tool| tool.name == binding.exposed_name)
        .expect("granted external tool");
    assert!(tool.description.contains("用户已显式信任"));
    assert!(!tool.description.contains("untrusted"));

    let mut ungranted = request();
    ungranted.client_request_id = "intake-ungranted-external".into();
    let ungranted = RunIntake::start(&db, ungranted).expect("ungranted run");
    let ungranted_registry =
        super::tool_executor::ToolRegistry::for_run(&db, &ungranted.run_id).expect("registry");
    assert!(ungranted_registry
        .tools_for_authorized_capabilities(
            &[super::run_contract::CapabilityId::new("external.read")],
            true,
        )
        .iter()
        .all(|tool| tool.name != binding.exposed_name));

    db.with_conn(|conn| {
        conn.execute(
            "UPDATE agent_run_mcp_tool_snapshots SET run_id = ?1 WHERE run_id = ?2",
            [&ungranted.run_id, &accepted.run_id],
        )?;
        Ok(())
    })
    .expect("move snapshot to a different accepted run");
    assert_eq!(
        load_run_snapshots(&db, &ungranted.run_id)
            .expect_err("snapshot integrity must bind the original run id")
            .to_string(),
        "external_tool_snapshot_integrity_failed"
    );
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE agent_run_mcp_tool_snapshots SET run_id = ?1 WHERE run_id = ?2",
            [&accepted.run_id, &ungranted.run_id],
        )?;
        conn.execute(
            "UPDATE web_evidence_providers SET enabled = 0 WHERE id = 'readonly'",
            [],
        )?;
        Ok(())
    })
    .expect("restore snapshot and revoke provider");
    assert_eq!(
        super::tool_executor::ToolRegistry::for_run(&db, &accepted.run_id)
            .err()
            .expect("registry must fail closed before exposing a revoked provider")
            .to_string(),
        "external_tool_provider_config_changed"
    );
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE web_evidence_providers SET enabled = 1 WHERE id = 'readonly'",
            [],
        )?;
        conn.execute(
            "UPDATE agent_run_mcp_tool_snapshots
             SET frozen_at = '2099-01-01 00:00:00'
             WHERE run_id = ?1",
            [&accepted.run_id],
        )?;
        Ok(())
    })
    .expect("tamper frozen timestamp");
    assert_eq!(
        load_run_snapshots(&db, &accepted.run_id)
            .expect_err("all persisted snapshot fields must be integrity bound")
            .to_string(),
        "external_tool_snapshot_integrity_failed"
    );

    db.with_conn(|conn| {
        conn.execute(
            "UPDATE agent_run_mcp_tool_snapshots
             SET exposed_name = 'external_tampered_snapshot'
             WHERE run_id = ?1",
            [&accepted.run_id],
        )?;
        Ok(())
    })
    .expect("tamper frozen snapshot");
    let registry_error = super::tool_executor::ToolRegistry::for_run(&db, &accepted.run_id)
        .err()
        .expect("registry must validate snapshot integrity before exposure");
    assert_eq!(
        registry_error.to_string(),
        "external_tool_snapshot_integrity_failed"
    );
}

#[test]
fn provider_config_drift_rolls_back_run_acceptance() {
    use super::mcp_external_tools::{
        review_discovered_tool, upsert_binding, McpCapabilityBindingInput,
    };
    use super::mcp_runtime_registry::{
        list_web_evidence_providers, upsert_web_evidence_provider, WebEvidenceProviderInput,
    };
    use super::run_contract::ExternalToolGrantRef;

    let db = Database::open_in_memory().expect("database");
    upsert_web_evidence_provider(
        &db,
        &WebEvidenceProviderInput {
            id: "drifted".into(),
            name: "Drifted".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json: r#"{"command":"/bin/true"}"#.into(),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: None,
            web_fetch_mapping_json: None,
        },
    )
    .expect("provider");
    let binding_input = McpCapabilityBindingInput {
        id: None,
        provider_id: "drifted".into(),
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
    let reviewed = review_discovered_tool(
        &binding_input.mcp_tool_name,
        &binding_input.input_schema,
        Some(true),
    )
    .expect("read-only attestation");
    let reviewed_provider_hash = list_web_evidence_providers(&db)
        .expect("providers")
        .into_iter()
        .find(|provider| provider.id == binding_input.provider_id)
        .expect("reviewed provider")
        .provider_config_hash;
    let attestation = super::mcp_external_tools::attest_reviewed_tool(
        &db,
        &binding_input.provider_id,
        &reviewed,
        &reviewed_provider_hash,
        &binding_input.argument_mapping,
    )
    .expect("binding attestation");
    let binding_input = McpCapabilityBindingInput {
        attested_binding_config_hash: attestation.binding_config_hash,
        ..binding_input
    };
    let binding =
        upsert_binding(&db, &binding_input, &reviewed, &reviewed_provider_hash).expect("binding");
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE web_evidence_providers
             SET provider_config_hash = 'changed' WHERE id = 'drifted'",
            [],
        )?;
        Ok(())
    })
    .expect("drift provider");
    let mut request = request();
    request.external_tool_grants = vec![ExternalToolGrantRef {
        binding_id: binding.id,
        binding_config_hash: binding.binding_config_hash,
    }];
    assert_eq!(
        RunIntake::start(&db, request)
            .expect_err("drift must fail")
            .to_string(),
        "external_tool_provider_config_changed"
    );
    db.with_read_conn(|conn| {
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row
                .get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM session_messages", [], |row| {
                row.get::<_, i64>(0)
            })?,
            0
        );
        Ok(())
    })
    .expect("accept rollback");
}

#[test]
fn classified_run_rejects_external_tool_grants() {
    use super::run_contract::ExternalToolGrantRef;

    let mut request = request();
    request.security_domain = SecurityDomain::Classified;
    request.external_tool_grants = vec![ExternalToolGrantRef {
        binding_id: "binding".into(),
        binding_config_hash: "hash".into(),
    }];
    assert_eq!(
        RunIntake::resolve_envelope(&request)
            .expect_err("classified grant must be rejected")
            .to_string(),
        "agent_run_invalid_request"
    );
}

#[test]
fn local_only_request_rejects_external_tool_grants_before_acceptance() {
    use super::run_contract::ExternalToolGrantRef;

    let mut request = request();
    request.turn.message = "仅用本地资料回答，不要联网".into();
    request.external_tool_grants = vec![ExternalToolGrantRef {
        binding_id: "binding".into(),
        binding_config_hash: "hash".into(),
    }];
    assert_eq!(
        RunIntake::resolve_envelope(&request)
            .expect_err("local-only external grant must fail")
            .to_string(),
        "agent_run_external_tool_local_only_conflict"
    );
}

#[derive(Default)]
struct RecordingSink(std::sync::Mutex<Vec<serde_json::Value>>);

impl RunEventSink for RecordingSink {
    fn emit(&self, event: &super::run_contract::AssistantRunEvent) -> AppResult<()> {
        self.0
            .lock()
            .expect("recording sink lock")
            .push(serde_json::to_value(event)?);
        Ok(())
    }
}

#[test]
fn accepted_retry_does_not_spawn_again() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted run");
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE agent_runs SET status = 'failed' WHERE run_id = ?1",
            [&accepted.run_id],
        )?;
        Ok(())
    })
    .expect("make source run retryable");
    let sink = RecordingSink::default();
    let retry = AssistantRunRetryRequest {
        session: accepted.session.clone(),
        source_run_id: accepted.run_id.clone(),
        client_request_id: "retry-replay-once".into(),
    };

    let first = RunIntake::retry_with_sink_outcome(&db, retry.clone(), &sink).expect("first retry");
    let second =
        RunIntake::retry_with_sink_outcome(&db, retry, &sink).expect("idempotent retry replay");

    assert!(first.is_new, "the first retry must win execution");
    assert!(
        !second.is_new,
        "an idempotent retry replay must not win execution again"
    );
    assert_eq!(first.accepted.run_id, second.accepted.run_id);
    assert_eq!(
        sink.0.lock().expect("sink lock").len(),
        1,
        "an idempotent retry replay must not emit a duplicate accepted event"
    );
}

#[test]
fn concurrent_retry_starts_executor_once() {
    let directory = tempfile::tempdir().expect("temporary database directory");
    let db =
        Arc::new(Database::open(&directory.path().join("concurrent-retry.db")).expect("database"));
    let accepted = RunIntake::start(&db, request()).expect("accepted run");
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE agent_runs SET status = 'failed' WHERE run_id = ?1",
            [&accepted.run_id],
        )?;
        Ok(())
    })
    .expect("make source run retryable");
    let sink = Arc::new(RecordingSink::default());
    let retry = AssistantRunRetryRequest {
        session: accepted.session.clone(),
        source_run_id: accepted.run_id.clone(),
        client_request_id: "concurrent-retry-once".into(),
    };

    let mut new_count = 0;
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..2 {
            let db = Arc::clone(&db);
            let sink = Arc::clone(&sink);
            let retry = retry.clone();
            handles.push(scope.spawn(move || {
                RunIntake::retry_with_sink_outcome(&db, retry, &*sink)
                    .expect("concurrent retry accepted")
            }));
        }
        for handle in handles {
            if handle.join().expect("retry thread").is_new {
                new_count += 1;
            }
        }
    });

    assert_eq!(
        new_count, 1,
        "two concurrent retries must produce exactly one execution owner"
    );
    assert_eq!(
        sink.0.lock().expect("sink lock").len(),
        1,
        "concurrent retries must emit only one accepted notification"
    );
}

struct RejectingSink;

impl RunEventSink for RejectingSink {
    fn emit(&self, _event: &super::run_contract::AssistantRunEvent) -> AppResult<()> {
        Err(crate::error::AppError::msg(
            "simulated_event_delivery_failure",
        ))
    }
}

#[test]
fn intake_rejects_actions_that_do_not_bind_to_the_explicit_reference() {
    let mut invalid = request();
    invalid.explicit_action = Some(ExplicitAction {
        effect: Effect::Draft,
        target: Some(ExplicitTarget {
            reference_id: "missing-reference".to_string(),
            content_hash: valid_content_hash(),
        }),
        selection_snapshot: None,
    });

    assert_eq!(
        RunIntake::resolve_envelope(&invalid)
            .expect_err("an action target must be explicitly referenced")
            .to_string(),
        "agent_run_invalid_request"
    );
}

#[test]
fn intake_rejects_apply_without_an_explicit_document_target() {
    let mut invalid = request();
    invalid.explicit_action = Some(ExplicitAction {
        effect: Effect::Apply,
        target: None,
        selection_snapshot: None,
    });

    assert_eq!(
        RunIntake::resolve_envelope(&invalid)
            .expect_err("an Apply Run must bind to a document or selection")
            .to_string(),
        "agent_run_invalid_request"
    );
}

#[test]
fn intake_rejects_a_classified_context_capability_in_normal_domain() {
    let mut invalid = request();
    invalid.classified_context_ref = Some("opaque-classified-context".into());

    assert_eq!(
        RunIntake::resolve_envelope(&invalid)
            .expect_err("normal Runs must not carry classified context")
            .to_string(),
        "agent_run_invalid_request"
    );
}

#[test]
fn intake_rejects_normal_reference_scope_and_display_metadata_in_classified_domain() {
    let mut classified = request();
    classified.security_domain = SecurityDomain::Classified;
    classified
        .turn
        .explicit_references
        .push(crate::ai_types::ContextReferenceWire {
            id: "ordinary-note".into(),
            kind: crate::ai_types::ContextReferenceKind::Note,
            file_path: Some("notes/ordinary.md".into()),
            content_hash: Some(valid_content_hash()),
            utf8_range: None,
            editor_range: None,
            excerpt: String::new(),
            heading_path: None,
            anchor: None,
            stale: false,
            invalid_reason: None,
        });
    assert_eq!(
        RunIntake::resolve_envelope(&classified)
            .expect_err("classified requests must reject normal references")
            .to_string(),
        "agent_run_invalid_request"
    );

    classified.turn.explicit_references.clear();
    classified.turn.retrieval_scope.paths = vec!["notes/ordinary.md".into()];
    assert_eq!(
        RunIntake::resolve_envelope(&classified)
            .expect_err("classified requests must reject normal retrieval scope")
            .to_string(),
        "agent_run_invalid_request"
    );

    classified.turn.retrieval_scope.paths.clear();
    classified.turn.display_mentions = vec![DisplayMention {
        kind: DisplayMentionKind::File,
        value: "notes/ordinary.md".into(),
        label: "普通笔记".into(),
        range: DisplayMentionRange { from: 0, to: 4 },
    }];
    assert_eq!(
        RunIntake::resolve_envelope(&classified)
            .expect_err("classified requests must reject normal display annotations")
            .to_string(),
        "agent_run_invalid_request"
    );
}

#[test]
fn intake_validates_display_mentions_against_utf16_message_ranges() {
    let mut valid = request();
    valid.turn.message = "分析 项目😀".into();
    valid.turn.display_mentions = vec![DisplayMention {
        kind: DisplayMentionKind::File,
        value: "notes/project.md".into(),
        label: "项目😀".into(),
        range: DisplayMentionRange { from: 3, to: 7 },
    }];
    RunIntake::resolve_envelope(&valid).expect("UTF-16 range must accept a surrogate pair");

    valid.turn.display_mentions[0].range.to = 8;
    assert_eq!(
        RunIntake::resolve_envelope(&valid)
            .expect_err("range beyond UTF-16 message length must fail")
            .to_string(),
        "agent_run_invalid_request"
    );
}

#[test]
fn intake_rejects_unsafe_retrieval_scope_paths_before_persistence() {
    for unsafe_path in [
        "../outside.md",
        "/absolute.md",
        ".iris/runtime.md",
        ".classified/secret.md",
        "notes/../../outside.md",
    ] {
        let mut invalid = request();
        invalid.client_request_id = format!("unsafe-scope-{unsafe_path}");
        invalid.turn.retrieval_scope.paths = vec![unsafe_path.to_string()];

        assert_eq!(
            RunIntake::resolve_envelope(&invalid)
                .expect_err("unsafe retrieval paths must be rejected at intake")
                .to_string(),
            "agent_run_invalid_retrieval_scope",
            "{unsafe_path}"
        );
    }
}

#[test]
fn intake_persists_only_the_canonical_deduplicated_retrieval_scope() {
    let db = Database::open_in_memory().expect("database");
    let mut scoped = request();
    scoped.client_request_id = "canonical-scope".into();
    scoped.turn.retrieval_scope.paths = vec![" ./notes\\same.md ".into(), "notes/same.md".into()];
    scoped.turn.retrieval_scope.path_prefixes =
        vec![" ./projects\\alpha ".into(), "projects/alpha/".into()];
    scoped.turn.retrieval_scope.required_tags = vec![" #Project ".into(), "project".into()];

    let accepted = RunIntake::start(&db, scoped).expect("accepted scoped run");
    let prompt = AgentRunRepository::prompt_input_for_session(
        &db,
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .expect("prompt input")
    .expect("run exists");

    assert_eq!(prompt.retrieval_scope.paths, vec!["notes/same.md"]);
    assert_eq!(
        prompt.retrieval_scope.path_prefixes,
        vec!["projects/alpha/"]
    );
    assert_eq!(prompt.retrieval_scope.required_tags, vec!["project"]);
}

#[test]
fn intake_normalizes_explicit_reference_paths_before_persistence() {
    let db = Database::open_in_memory().expect("database");
    let mut referenced = request();
    referenced.client_request_id = "canonical-reference-path".into();
    referenced
        .turn
        .explicit_references
        .push(crate::ai_types::ContextReferenceWire {
            id: "note".into(),
            kind: crate::ai_types::ContextReferenceKind::Note,
            file_path: Some(" ./notes\\a.md ".into()),
            content_hash: Some(valid_content_hash()),
            utf8_range: None,
            editor_range: None,
            excerpt: String::new(),
            heading_path: None,
            anchor: None,
            stale: false,
            invalid_reason: None,
        });

    let accepted = RunIntake::start(&db, referenced).expect("accepted referenced run");
    let prompt = AgentRunRepository::prompt_input_for_session(
        &db,
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .expect("prompt input")
    .expect("run exists");

    assert_eq!(
        prompt.explicit_references[0].file_path.as_deref(),
        Some("notes/a.md")
    );
}

#[test]
fn intake_rejects_unsafe_explicit_reference_paths_before_persistence() {
    let mut invalid = request();
    invalid
        .turn
        .explicit_references
        .push(crate::ai_types::ContextReferenceWire {
            id: "unsafe".into(),
            kind: crate::ai_types::ContextReferenceKind::Note,
            file_path: Some("../outside.md".into()),
            content_hash: Some(valid_content_hash()),
            utf8_range: None,
            editor_range: None,
            excerpt: String::new(),
            heading_path: None,
            anchor: None,
            stale: false,
            invalid_reason: None,
        });

    assert_eq!(
        RunIntake::resolve_envelope(&invalid)
            .expect_err("unsafe explicit reference paths must fail at intake")
            .to_string(),
        "agent_run_invalid_explicit_reference"
    );
}

#[test]
fn retrieval_scope_without_full_material_forces_a_local_tool_loop() {
    let mut scoped = request();
    scoped.turn.retrieval_scope.path_prefixes = vec!["notes/".into()];

    let envelope = RunIntake::resolve_envelope(&scoped).expect("resolve scoped envelope");

    assert_eq!(envelope.context, ContextMode::ExplicitScope);
    assert_eq!(envelope.effort, Effort::ToolLoop);
}

#[test]
fn intake_rejects_selection_snapshot_with_inconsistent_utf8_range() {
    let mut invalid = request();
    invalid
        .turn
        .explicit_references
        .push(crate::ai_types::ContextReferenceWire {
            id: "selection-reference".to_string(),
            kind: crate::ai_types::ContextReferenceKind::Selection,
            file_path: Some("notes/a.md".to_string()),
            content_hash: Some(valid_content_hash()),
            utf8_range: Some(crate::ai_types::SourceSpan { start: 0, end: 3 }),
            editor_range: None,
            excerpt: String::new(),
            heading_path: None,
            anchor: None,
            stale: false,
            invalid_reason: None,
        });
    invalid.explicit_action = Some(ExplicitAction {
        effect: Effect::Draft,
        target: None,
        selection_snapshot: Some(SelectionSnapshot {
            reference_id: "selection-reference".to_string(),
            content_hash: valid_content_hash(),
            utf8_range: crate::ai_types::SourceSpan { start: 0, end: 8 },
            text: "短文本".to_string(),
        }),
    });

    assert_eq!(
        RunIntake::resolve_envelope(&invalid)
            .expect_err("range must equal the supplied UTF-8 selection snapshot")
            .to_string(),
        "agent_run_invalid_request"
    );
}

#[test]
fn intake_ignores_and_never_persists_client_selection_snapshot_text() {
    let db = Database::open_in_memory().expect("database");
    let mut scoped = request();
    scoped.client_request_id = "ignore-selection-client-body".into();
    scoped
        .turn
        .explicit_references
        .push(crate::ai_types::ContextReferenceWire {
            id: "selection-reference".into(),
            kind: crate::ai_types::ContextReferenceKind::Selection,
            file_path: Some("notes/a.md".into()),
            content_hash: Some(valid_content_hash()),
            utf8_range: Some(crate::ai_types::SourceSpan { start: 0, end: 5 }),
            editor_range: None,
            excerpt: "also untrusted".into(),
            heading_path: None,
            anchor: None,
            stale: false,
            invalid_reason: None,
        });
    scoped.explicit_action = Some(ExplicitAction {
        effect: Effect::Draft,
        target: None,
        selection_snapshot: Some(SelectionSnapshot {
            reference_id: "selection-reference".into(),
            content_hash: valid_content_hash(),
            utf8_range: crate::ai_types::SourceSpan { start: 0, end: 5 },
            text: "CLIENT BODY MUST BE IGNORED".into(),
        }),
    });

    let accepted = RunIntake::start(&db, scoped)
        .expect("client selection text must not participate in request validation");
    db.with_read_conn(|conn| {
        let stored: String = conn.query_row(
            "SELECT explicit_action_json FROM agent_runs WHERE run_id = ?1",
            [&accepted.run_id],
            |row| row.get(0),
        )?;
        assert!(!stored.contains("CLIENT BODY MUST BE IGNORED"));
        assert!(!stored.contains("text"));
        Ok(())
    })
    .expect("inspect persisted action");
}
#[test]
fn intake_creates_scene_free_normal_session_and_accepted_run_without_legacy_writes() {
    let db = Database::open_in_memory().expect("database");

    let accepted = RunIntake::start(&db, request()).expect("accepted run");

    assert_eq!(accepted.session.domain, SecurityDomain::Normal);
    assert_eq!(accepted.state, RunState::Accepted);
    assert_eq!(accepted.state_version, 0);
    let persisted = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("get run")
        .expect("persisted run");
    assert_eq!(persisted.run.state, RunState::Accepted);
    assert_eq!(persisted.events.len(), 1);

    db.with_read_conn(|conn| {
        let (session_key, vault_id): (String, Option<String>) = conn.query_row(
            "SELECT session_key, vault_id FROM sessions WHERE session_key = ?1",
            [&accepted.session.session_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(session_key, accepted.session.session_key);
        assert!(vault_id.is_none());
        let legacy_tables: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name IN ('agent_tasks', 'ai_traces')",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(legacy_tables, 0);
        Ok(())
    })
    .expect("new intake facts");
}

#[test]
fn intake_emits_the_already_persisted_accepted_event_on_the_unified_sink() {
    let db = Database::open_in_memory().expect("database");
    let sink = RecordingSink::default();

    let accepted = RunIntake::start_with_sink(&db, request(), &sink).expect("accepted");

    let events = sink.0.lock().expect("recording sink lock");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["runId"], accepted.run_id);
    assert_eq!(events[0]["type"], "accepted");
}

#[test]
fn intake_idempotent_replay_does_not_emit_the_accepted_event_twice() {
    let db = Database::open_in_memory().expect("database");
    let sink = RecordingSink::default();

    let first = RunIntake::start_with_sink(&db, request(), &sink).expect("first acceptance");
    let replay = RunIntake::start_with_sink(&db, request(), &sink).expect("idempotent replay");

    assert_eq!(replay, first);
    let events = sink.0.lock().expect("recording sink lock");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["type"], "accepted");
}

#[test]
fn intake_event_delivery_failure_does_not_strand_or_duplicate_the_run() {
    let db = Database::open_in_memory().expect("database");

    let first = RunIntake::start_with_sink(&db, request(), &RejectingSink)
        .expect("durable acceptance survives notification loss");
    let replay = RunIntake::start_with_sink(&db, request(), &RecordingSink::default())
        .expect("client recovers the original identity");

    assert_eq!(replay, first);
    db.with_read_conn(|conn| {
        let facts: (i64, i64, i64) = (
            conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?,
            conn.query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row.get(0))?,
            conn.query_row("SELECT COUNT(*) FROM agent_run_events", [], |row| {
                row.get(0)
            })?,
        );
        assert_eq!(facts, (1, 1, 1));
        Ok(())
    })
    .expect("single durable intake");
}

#[test]
fn control_event_delivery_failure_keeps_the_committed_terminal_state() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted run");

    let outcome = RunIntake::control_with_sink(
        &db,
        AssistantRunControlRequest {
            session: accepted.session.clone(),
            run_id: accepted.run_id.clone(),
            expected_state_version: accepted.state_version,
            action: RunControlAction::Cancel,
        },
        &RejectingSink,
    )
    .expect("durable control survives notification loss");

    assert_eq!(outcome, super::run_intake::NormalRunControlOutcome::Applied);
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Cancelled);
    crate::ai_runtime::model_gateway::clear_abort(&accepted.run_id);
}

#[test]
fn concurrent_intake_replays_converge_on_one_run() {
    let directory = tempfile::tempdir().expect("temporary database directory");
    let db = Database::open(&directory.path().join("concurrent.sqlite3")).expect("database");
    let sink = RecordingSink::default();

    let accepted = std::thread::scope(|scope| {
        let handles = (0..4)
            .map(|_| scope.spawn(|| RunIntake::start_with_sink(&db, request(), &sink)))
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("intake thread").expect("accepted"))
            .collect::<Vec<_>>()
    });

    assert!(accepted.iter().all(|item| item == &accepted[0]));
    assert_eq!(sink.0.lock().expect("recording sink lock").len(), 1);
    db.with_read_conn(|conn| {
        let facts: (i64, i64, i64) = (
            conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?,
            conn.query_row("SELECT COUNT(*) FROM session_messages", [], |row| {
                row.get(0)
            })?,
            conn.query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row.get(0))?,
        );
        assert_eq!(facts, (1, 1, 1));
        Ok(())
    })
    .expect("single concurrent intake");
}

#[test]
fn intake_scoped_get_does_not_expose_a_run_to_another_session() {
    let db = Database::open_in_memory().expect("database");
    let first = RunIntake::start(&db, request()).expect("first accepted run");
    let mut second_request = request();
    second_request.client_request_id = "second-client-request".to_string();
    let second = RunIntake::start(&db, second_request).expect("second accepted run");

    assert!(RunIntake::get(&db, &first.session, &first.run_id)
        .expect("owner read")
        .is_some());
    assert!(RunIntake::get(&db, &second.session, &first.run_id)
        .expect("other session read")
        .is_none());
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE agent_run_events SET payload_json = '{invalid json}' WHERE run_id = ?1",
            [&first.run_id],
        )?;
        Ok(())
    })
    .expect("corrupt only the other session's private event");
    assert!(RunIntake::get(&db, &second.session, &first.run_id)
        .expect("a non-owner must not parse or expose the other run")
        .is_none());
}

#[test]
fn reconnect_lookup_returns_only_the_owner_latest_nonterminal_run() {
    let db = Database::open_in_memory().expect("database");
    let first = RunIntake::start(&db, request()).expect("first accepted run");

    let recovered = RunIntake::get_latest_active(&db, &first.session)
        .expect("recover latest")
        .expect("active run");
    assert_eq!(recovered.run.run_id, first.run_id);

    RunIntake::control(
        &db,
        AssistantRunControlRequest {
            session: first.session.clone(),
            run_id: first.run_id.clone(),
            expected_state_version: 0,
            action: RunControlAction::Cancel,
        },
    )
    .expect("cancel first run");
    assert!(RunIntake::get_latest_active(&db, &first.session)
        .expect("recover with no active run")
        .is_none());

    let mut second_request = request();
    second_request.client_request_id = "latest-active-client-request".to_string();
    second_request.session = Some(first.session.clone());
    let second = RunIntake::start(&db, second_request).expect("second accepted run");
    let recovered = RunIntake::get_latest_active(&db, &first.session)
        .expect("recover replacement run")
        .expect("replacement active run");
    assert_eq!(recovered.run.run_id, second.run_id);

    crate::ai_runtime::model_gateway::clear_abort(&first.run_id);
}

#[test]
fn cancel_control_updates_only_the_owned_new_run() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted run");

    RunIntake::control(
        &db,
        AssistantRunControlRequest {
            session: accepted.session.clone(),
            run_id: accepted.run_id.clone(),
            expected_state_version: 0,
            action: RunControlAction::Cancel,
        },
    )
    .expect("cancel run");

    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("get cancelled run")
        .expect("run exists");
    assert_eq!(replay.run.state, RunState::Cancelled);
    assert_eq!(replay.events.len(), 2);
    assert!(
        crate::ai_runtime::model_gateway::is_abort_requested(&accepted.run_id),
        "cancelling a Run must signal its in-flight provider request"
    );
    crate::ai_runtime::model_gateway::clear_abort(&accepted.run_id);
    RunIntake::control(
        &db,
        AssistantRunControlRequest {
            session: accepted.session.clone(),
            run_id: accepted.run_id.clone(),
            expected_state_version: replay.run.state_version,
            action: RunControlAction::Cancel,
        },
    )
    .expect("duplicate cancellation is idempotent");
    assert_eq!(
        RunIntake::get(&db, &accepted.session, &accepted.run_id)
            .expect("replay duplicate cancellation")
            .expect("run exists")
            .events
            .len(),
        2
    );
    db.with_read_conn(|conn| {
        let legacy_tables: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name IN ('agent_tasks', 'ai_traces')",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(legacy_tables, 0);
        Ok(())
    })
    .expect("cancel must not use old lifecycle tables");
}

#[test]
fn intake_rejects_classified_requests_without_normal_sqlite_writes() {
    let db = Database::open_in_memory().expect("database");
    let mut classified = request();
    classified.security_domain = SecurityDomain::Classified;

    let error = RunIntake::start(&db, classified).expect_err("classified must use CEF intake");
    assert_eq!(
        error.to_string(),
        "agent_run_classified_domain_not_supported"
    );
    db.with_read_conn(|conn| {
        let sessions: i64 =
            conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;
        let runs: i64 = conn.query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row.get(0))?;
        assert_eq!(sessions, 0);
        assert_eq!(runs, 0);
        Ok(())
    })
    .expect("no normal-domain facts");
}

#[test]
fn classified_intake_accepts_only_cef_facts_without_normal_sqlite_writes() {
    let _test_lock = crate::crypto::vault_key::VAULT_KEY_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::crypto::vault_key::init_vault_key();
    let mut key = crate::crypto::vault_key::VAULT_KEY
        .get()
        .expect("vault key initialized")
        .write()
        .expect("vault key write lock");
    key.set_test_key([11; 32]);
    drop(key);
    let vault =
        std::env::temp_dir().join(format!("iris-classified-intake-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&vault).unwrap();
    let db = Database::open_in_memory().expect("database");
    let mut classified = request();
    classified.client_request_id = "classified-intake-request".into();
    classified.security_domain = SecurityDomain::Classified;

    let accepted = RunIntake::start_classified(&vault, classified).expect("classified accepted");

    assert_eq!(accepted.session.domain, SecurityDomain::Classified);
    let thread = crate::ai_runtime::classified_session::classified_ai_thread_load(
        &vault,
        accepted.session.session_key,
    )
    .expect("CEF conversation");
    assert_eq!(thread.messages.len(), 1);
    assert_eq!(thread.runs.len(), 1);
    db.with_read_conn(|conn| {
        let sessions: i64 =
            conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;
        let runs: i64 = conn.query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row.get(0))?;
        assert_eq!(sessions, 0);
        assert_eq!(runs, 0);
        Ok(())
    })
    .expect("no normal-domain facts");
    std::fs::remove_dir_all(vault).unwrap();
}

#[test]
fn classified_intake_rejects_nonempty_content_parts_at_the_start_boundary() {
    let _test_lock = crate::crypto::vault_key::VAULT_KEY_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::crypto::vault_key::init_vault_key();
    let mut key = crate::crypto::vault_key::VAULT_KEY
        .get()
        .expect("vault key initialized")
        .write()
        .expect("vault key write lock");
    key.set_test_key([12; 32]);
    drop(key);
    let vault = std::env::temp_dir().join(format!(
        "iris-classified-content-parts-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&vault).unwrap();
    let mut classified = request();
    classified.client_request_id = "classified-content-parts".into();
    classified.security_domain = SecurityDomain::Classified;
    classified.turn.content_parts = Some(vec![crate::ai_types::ContentPart::Text {
        text: "不得写入 CEF 的普通域内容分片".into(),
    }]);

    let result = RunIntake::start_classified(&vault, classified);
    std::fs::remove_dir_all(vault).unwrap();

    assert_eq!(
        result
            .expect_err("classified intake must reject content parts before CEF acceptance")
            .to_string(),
        "agent_run_invalid_request"
    );
}
#[test]
fn envelope_resolver_applies_security_action_and_web_rules_without_scene_inference() {
    let mut classified_apply = request();
    classified_apply.client_request_id = "classified-apply".into();
    classified_apply.turn.message = "请联网核实最新合规规则后应用这项变更".into();
    classified_apply.web_enabled = true;
    classified_apply.security_domain = SecurityDomain::Classified;
    classified_apply.explicit_action = Some(ExplicitAction {
        effect: Effect::Apply,
        target: None,
        selection_snapshot: None,
    });

    let resolved = RunIntake::resolve_envelope(&classified_apply).expect("resolve envelope");

    assert_eq!(resolved.security_domain, SecurityDomain::Classified);
    assert_eq!(resolved.effect, Effect::Apply);
    assert_eq!(resolved.context, ContextMode::ExplicitScope);
    assert_eq!(resolved.freshness, Freshness::Offline);
    assert_eq!(
        resolved.web_reason,
        WebDecisionReason::SecurityDomainOffline
    );
    assert_eq!(resolved.effort, Effort::Durable);
    assert_eq!(resolved.risk, RiskClass::BoundedWrite);
    let wire = serde_json::to_value(&resolved).expect("serialize envelope");
    assert!(wire["requiredCapabilities"]
        .as_array()
        .expect("capability array")
        .iter()
        .any(|value| value == "note.apply_patch"));
}

#[test]
fn envelope_resolver_keeps_explicit_local_only_boundary_before_apply_action() {
    let mut constrained = request();
    constrained.client_request_id = "constrained-action".into();
    constrained.turn.message = "只用本地资料，不要修改文件；请继续创作小说。".into();
    constrained.web_enabled = true;
    constrained.explicit_action = Some(ExplicitAction {
        effect: Effect::Apply,
        target: None,
        selection_snapshot: None,
    });

    let resolved = RunIntake::resolve_envelope(&constrained).expect("resolve envelope");

    assert_eq!(resolved.effect, Effect::Answer);
    assert_eq!(resolved.context, ContextMode::ExplicitScope);
    assert_eq!(resolved.freshness, Freshness::Offline);
    assert_eq!(resolved.effort, Effort::ToolLoop);
    assert!(resolved.material_needs.is_empty());
}

#[test]
fn new_writing_run_does_not_get_a_domain_specific_conversation_mode() {
    let mut novel = request();
    novel.client_request_id = "novel-conversation".into();
    novel.turn.message = "请继续创作这部小说的下一章。".into();

    let resolved = RunIntake::resolve_envelope(&novel).expect("resolve envelope");

    assert_eq!(resolved.context, ContextMode::None);
    assert_eq!(resolved.freshness, Freshness::Offline);
    assert!(resolved.material_needs.is_empty());
}
#[test]
fn intake_declares_model_text_and_forces_classified_requests_offline_before_cef_acceptance() {
    let resolved = RunIntake::resolve_envelope(&request()).expect("resolved envelope");
    assert!(resolved.required_capabilities.contains(
        &crate::ai_runtime::run_contract::CapabilityId::new("model.text")
    ));

    let _test_lock = crate::crypto::vault_key::VAULT_KEY_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::crypto::vault_key::init_vault_key();
    let mut key = crate::crypto::vault_key::VAULT_KEY
        .get()
        .expect("vault key initialized")
        .write()
        .expect("vault key write lock");
    key.set_test_key([13; 32]);
    drop(key);
    let vault = std::env::temp_dir().join(format!(
        "iris-classified-web-policy-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&vault).unwrap();
    let mut classified = request();
    classified.client_request_id = "classified-web-request".into();
    classified.security_domain = SecurityDomain::Classified;
    classified.web_enabled = true;

    let accepted = RunIntake::start_classified(&vault, classified)
        .expect("classified Run must remain offline instead of requesting Web");
    assert_eq!(accepted.session.domain, SecurityDomain::Classified);
    std::fs::remove_dir_all(vault).unwrap();
}
#[test]
fn minimal_intake_resolves_a_direct_offline_answer_envelope() {
    let db = Database::open_in_memory().expect("database");

    let resolved = RunIntake::resolve_envelope(&request()).expect("resolved envelope");

    assert_eq!(resolved.effect, Effect::Answer);
    assert_eq!(resolved.context, ContextMode::None);
    assert_eq!(resolved.freshness, Freshness::Offline);
    assert!(
        resolved.material_needs.is_empty(),
        "a direct answer without explicit references must not request material"
    );
    assert_eq!(
        RunIntake::start(&db, request()).unwrap().state,
        RunState::Accepted
    );
}

#[test]
fn input_submission_resumes_the_same_run_and_replay_is_noop() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted run");
    let preparing = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: 0,
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Preparing,
                stage: "正在准备".into(),
                stage_code: None,
            },
        },
    )
    .expect("preparing");
    let running = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: preparing.state_version(),
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Running,
                stage: "正在查询".into(),
                stage_code: None,
            },
        },
    )
    .expect("running");
    let required = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: running.state_version(),
            event_type: RunEventType::InputRequired,
            payload: RunEventPayload::InputRequired {
                input_id: "location-test".into(),
                input_kind: "location".into(),
                fields: vec!["city".into()],
                prompt: "请告诉我城市".into(),
            },
        },
    )
    .expect("input request");
    let session = accepted.session.clone();
    let mut values = std::collections::BTreeMap::new();
    values.insert("city".into(), "上海".into());
    let outcome = RunIntake::control_with_sink(
        &db,
        AssistantRunControlRequest {
            session: session.clone(),
            run_id: accepted.run_id.clone(),
            expected_state_version: required.state_version(),
            action: RunControlAction::SubmitInput {
                input_id: "location-test".into(),
                values: values.clone(),
            },
        },
        &RecordingSink::default(),
    )
    .expect("input submission");
    assert_eq!(
        outcome,
        super::run_intake::NormalRunControlOutcome::InputProvided
    );
    let replay = RunIntake::control_with_sink(
        &db,
        AssistantRunControlRequest {
            session: session.clone(),
            run_id: accepted.run_id.clone(),
            expected_state_version: required.state_version(),
            action: RunControlAction::SubmitInput {
                input_id: "location-test".into(),
                values,
            },
        },
        &RecordingSink::default(),
    )
    .expect("replayed input");
    assert_eq!(replay, super::run_intake::NormalRunControlOutcome::Noop);
    let snapshot = RunIntake::get(&db, &session, &accepted.run_id)
        .expect("snapshot")
        .expect("run exists");
    assert_eq!(snapshot.run.state, RunState::Preparing);
    assert!(snapshot.run.pending_input.is_none());
    assert!(snapshot.events.iter().any(|event| {
        matches!(
            event.payload(),
            RunEventPayload::InputProvided { values, .. }
                if values == &std::collections::BTreeMap::from([("city".to_string(), "上海".to_string())])
        )
    }));
}

#[test]
fn approval_consumes_the_exact_frozen_plan_and_resumes_the_owned_run_once() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted run");
    let session_id = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT id FROM sessions WHERE session_key = ?1",
                [&accepted.session.session_key],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .expect("owning session");

    let preparing = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: 0,
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Preparing,
                stage: "正在准备".to_string(),
                stage_code: None,
            },
        },
    )
    .expect("preparing");
    let running = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: event_state_version(&preparing),
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Running,
                stage: "正在生成变更预览".to_string(),
                stage_code: None,
            },
        },
    )
    .expect("running");
    let plan = FrozenChangePlan::freeze(FrozenChangePlanInput {
        confirmation_id: "confirmation-1".to_string(),
        run_id: accepted.run_id.clone(),
        session_id,
        request_id: accepted.run_id.clone(),
        tool_call_id: "tool-1".to_string(),
        vault_id: "vault-1".to_string(),
        relative_paths: vec!["notes/a.md".to_string()],
        operation: "note.apply_patch".to_string(),
        base_content_hashes: vec![("notes/a.md".to_string(), "hash-a".to_string())],
        expected_post_content_hashes: vec![("notes/a.md".to_string(), "hash-after".to_string())],
        change: serde_json::json!({ "replacement": "新内容" }),
        affected_file_count: 1,
        rollback_summary: "可撤销".to_string(),
        expires_at_unix_ms: i64::MAX,
    })
    .expect("frozen plan");
    AgentRunRepository::save_frozen_confirmation(&db, &plan).expect("persist plan");
    let awaiting = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: event_state_version(&running),
            event_type: RunEventType::ConfirmationRequired,
            payload: RunEventPayload::ConfirmationRequired {
                confirmation_id: plan.confirmation_id().to_string(),
                plan_hash: plan.plan_hash().to_string(),
                summary: "更新 1 个笔记".to_string(),
                effect: None,
                targets: None,
                expires_at: None,
            },
        },
    )
    .expect("await confirmation");

    let before = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("get pending run")
        .expect("pending run exists");
    assert_eq!(before.run.state, RunState::AwaitingConfirmation);
    assert_eq!(
        before
            .run
            .pending_confirmation
            .expect("safe confirmation summary")
            .confirmation_id,
        plan.confirmation_id()
    );

    RunIntake::control(
        &db,
        AssistantRunControlRequest {
            session: accepted.session.clone(),
            run_id: accepted.run_id.clone(),
            expected_state_version: event_state_version(&awaiting),
            action: RunControlAction::ApproveChange {
                confirmation_id: plan.confirmation_id().to_string(),
                plan_hash: plan.plan_hash().to_string(),
            },
        },
    )
    .expect("exact plan approval");

    let approved = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("get approved run")
        .expect("approved run exists");
    assert_eq!(approved.run.state, RunState::Running);
    assert_eq!(
        serde_json::to_value(approved.events.last().expect("resumed event"))
            .expect("serialize resumed event")["type"],
        "resumed"
    );
    assert_eq!(approved.run.state_version, 4);
    assert_eq!(
        AgentRunRepository::latest_durable_apply_checkpoint(&db, &accepted.run_id)
            .expect("latest checkpoint")
            .expect("approval checkpoint")
            .stage(),
        DurableApplyCheckpointStage::Approved
    );

    RunIntake::control(
        &db,
        AssistantRunControlRequest {
            session: accepted.session.clone(),
            run_id: accepted.run_id.clone(),
            expected_state_version: approved.run.state_version,
            action: RunControlAction::ApproveChange {
                confirmation_id: plan.confirmation_id().to_string(),
                plan_hash: plan.plan_hash().to_string(),
            },
        },
    )
    .expect("duplicate approval is idempotent");
    assert_eq!(
        RunIntake::get(&db, &accepted.session, &accepted.run_id)
            .expect("get duplicate approval")
            .expect("run exists")
            .events
            .len(),
        approved.events.len(),
    );
}

#[test]
fn rejected_confirmation_cancels_without_write() {
    let (db, accepted, confirmation_id, awaiting_state_version) =
        accepted_run_awaiting_frozen_change_confirmation();

    RunIntake::control(
        &db,
        AssistantRunControlRequest {
            session: accepted.session.clone(),
            run_id: accepted.run_id.clone(),
            expected_state_version: awaiting_state_version,
            action: RunControlAction::RejectChange {
                confirmation_id: confirmation_id.clone(),
            },
        },
    )
    .expect("reject exact frozen plan");

    let rejected = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("get rejected run")
        .expect("rejected run exists");
    assert_eq!(rejected.run.state, RunState::Cancelled);
    assert!(rejected.run.pending_confirmation.is_none());
    assert!(rejected.run.final_message_id.is_none());
    assert_eq!(
        serde_json::to_value(rejected.events.last().expect("cancelled event"))
            .expect("serialize cancelled event")["type"],
        "cancelled"
    );
    assert_eq!(
        serde_json::to_value(rejected.events.last().expect("cancelled event"))
            .expect("serialize cancelled event")["payload"]["reason"],
        "user_rejected_change"
    );
    db.with_read_conn(|conn| {
        let (status, assistant_messages): (String, i64) = conn.query_row(
            "SELECT c.status,
                    (SELECT COUNT(*) FROM session_messages WHERE role = 'assistant')
             FROM agent_run_confirmations c WHERE c.confirmation_id = ?1",
            [&confirmation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(status, "rejected");
        assert_eq!(assistant_messages, 0);
        Ok(())
    })
    .expect("confirmation rejected atomically");

    RunIntake::control(
        &db,
        AssistantRunControlRequest {
            session: accepted.session.clone(),
            run_id: accepted.run_id.clone(),
            expected_state_version: rejected.run.state_version,
            action: RunControlAction::RejectChange { confirmation_id },
        },
    )
    .expect("duplicate rejection is idempotent");
    assert_eq!(
        RunIntake::get(&db, &accepted.session, &accepted.run_id)
            .expect("get duplicate rejection")
            .expect("run exists")
            .events
            .len(),
        rejected.events.len(),
    );
}

#[test]
fn resume_is_cas_guarded_and_only_available_for_paused_durable_apply() {
    let (db, accepted, confirmation_id, awaiting_state_version) =
        accepted_run_awaiting_frozen_change_confirmation();
    let plan_hash: String = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT plan_hash FROM agent_run_confirmations WHERE run_id = ?1",
                [&accepted.run_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .expect("plan hash");
    RunIntake::control(
        &db,
        AssistantRunControlRequest {
            session: accepted.session.clone(),
            run_id: accepted.run_id.clone(),
            expected_state_version: awaiting_state_version,
            action: RunControlAction::ApproveChange {
                confirmation_id,
                plan_hash,
            },
        },
    )
    .expect("approve plan");
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE agent_runs SET effort = 'durable', effect = 'apply' WHERE run_id = ?1",
            [&accepted.run_id],
        )?;
        Ok(())
    })
    .expect("make durable apply fixture");
    let running = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("running replay")
        .expect("run");
    let paused = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: running.run.state_version,
            event_type: RunEventType::Paused,
            payload: RunEventPayload::Paused {
                reason: "可安全继续".into(),
                recovery: Some(RunRecoveryKind::ResumeAvailable),
            },
        },
    )
    .expect("paused");

    RunIntake::control(
        &db,
        AssistantRunControlRequest {
            session: accepted.session.clone(),
            run_id: accepted.run_id.clone(),
            expected_state_version: event_state_version(&paused),
            action: RunControlAction::Resume,
        },
    )
    .expect("resume durable apply");
    let resumed = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("resumed replay")
        .expect("run");
    assert_eq!(resumed.run.state, RunState::Running);
    assert!(resumed.run.recovery.is_none());
    assert_eq!(
        RunIntake::control(
            &db,
            AssistantRunControlRequest {
                session: accepted.session.clone(),
                run_id: accepted.run_id.clone(),
                expected_state_version: event_state_version(&paused),
                action: RunControlAction::Resume,
            },
        )
        .expect_err("stale repeated resume must not dispatch twice")
        .to_string(),
        "agent_run_state_version_conflict"
    );
}

fn accepted_run_awaiting_frozen_change_confirmation() -> (
    Database,
    super::run_contract::AssistantRunAccepted,
    String,
    u64,
) {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted run");
    let session_id = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT id FROM sessions WHERE session_key = ?1",
                [&accepted.session.session_key],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .expect("owning session");
    let preparing = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: 0,
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Preparing,
                stage: "正在准备".to_string(),
                stage_code: None,
            },
        },
    )
    .expect("preparing");
    let running = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: event_state_version(&preparing),
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Running,
                stage: "正在生成变更预览".to_string(),
                stage_code: None,
            },
        },
    )
    .expect("running");
    let plan = FrozenChangePlan::freeze(FrozenChangePlanInput {
        confirmation_id: "confirmation-for-rejection".to_string(),
        run_id: accepted.run_id.clone(),
        session_id,
        request_id: accepted.run_id.clone(),
        tool_call_id: "tool-for-rejection".to_string(),
        vault_id: "vault-1".to_string(),
        relative_paths: vec!["notes/a.md".to_string()],
        operation: "note.apply_patch".to_string(),
        base_content_hashes: vec![("notes/a.md".to_string(), "hash-a".to_string())],
        expected_post_content_hashes: vec![("notes/a.md".to_string(), "hash-after".to_string())],
        change: serde_json::json!({ "replacement": "新内容" }),
        affected_file_count: 1,
        rollback_summary: "可撤销".to_string(),
        expires_at_unix_ms: i64::MAX,
    })
    .expect("frozen plan");
    AgentRunRepository::save_frozen_confirmation(&db, &plan).expect("persist plan");
    let awaiting = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: event_state_version(&running),
            event_type: RunEventType::ConfirmationRequired,
            payload: RunEventPayload::ConfirmationRequired {
                confirmation_id: plan.confirmation_id().to_string(),
                plan_hash: plan.plan_hash().to_string(),
                summary: "更新 1 个笔记".to_string(),
                effect: None,
                targets: None,
                expires_at: None,
            },
        },
    )
    .expect("await confirmation");

    (
        db,
        accepted,
        plan.confirmation_id().to_string(),
        event_state_version(&awaiting),
    )
}

fn event_state_version(event: &super::run_contract::AssistantRunEvent) -> u64 {
    serde_json::to_value(event).expect("serialize event")["stateVersion"]
        .as_u64()
        .expect("state version")
}

#[test]
fn web_enabled_rewrite_keeps_the_generic_preferred_web_surface_available() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message =
        "Rewrite this sentence more clearly: The team met yesterday.".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::WebPreferred);
    assert_eq!(envelope.effort, Effort::ToolLoop);
    assert!(envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "web.search"));
}

#[test]
fn web_enabled_time_sensitive_movie_question_enters_tool_loop() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "最近有什么好看的电影吗？".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.effort, Effort::ToolLoop);
    assert!(envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "web.search"));
}

#[test]
fn completed_conversation_accepts_a_third_current_movie_turn() {
    use super::mcp_runtime_registry::{upsert_web_evidence_provider, WebEvidenceProviderInput};

    let db = Database::open_in_memory().expect("database");
    upsert_web_evidence_provider(
        &db,
        &WebEvidenceProviderInput {
            id: "generic-search".into(),
            name: "Generic Search".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json: r#"{"command":"/bin/true"}"#.into(),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: Some(r#"{"tool":"web_search"}"#.into()),
            web_fetch_mapping_json: None,
        },
    )
    .expect("generic Web provider");
    let mut first = request();
    first.client_request_id = "three-turn-first".into();
    first.turn.message = "你好？".into();
    let first = RunIntake::start(&db, first).expect("first turn accepted");
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE agent_runs SET status = 'completed' WHERE run_id = ?1",
            [&first.run_id],
        )?;
        Ok(())
    })
    .expect("complete first turn");

    let mut second = request();
    second.client_request_id = "three-turn-second".into();
    second.session = Some(first.session.clone());
    second.turn.message = "今天是几月几日？".into();
    let second = RunIntake::start(&db, second).expect("second turn accepted");
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE agent_runs SET status = 'completed' WHERE run_id = ?1",
            [&second.run_id],
        )?;
        Ok(())
    })
    .expect("complete second turn");

    let mut third = request();
    third.client_request_id = "three-turn-third".into();
    third.session = Some(first.session);
    third.web_enabled = true;
    third.turn.message = "最近有什么好看的电影上映吗？".into();

    let accepted = RunIntake::start(&db, third).expect("third turn accepted");
    let persisted = AgentRunRepository::get(&db, &accepted.run_id)
        .expect("load third Run")
        .expect("third Run persisted");
    assert_eq!(persisted.run.state, RunState::Accepted);
}

#[test]
fn offline_local_note_dependency_without_explicit_refs_enters_tool_loop() {
    let mut request = request();
    request.web_enabled = false;
    request.turn.message =
        "根据授权的本地项目资料总结里程碑；联网开关不改变所需证据范围。".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::Offline);
    assert_eq!(envelope.context, ContextMode::ImplicitVault);
    assert_eq!(envelope.effort, Effort::ToolLoop);
    assert_eq!(envelope.web_reason, WebDecisionReason::UserDisabled);
    assert_eq!(
        envelope.verification_requirement,
        VerificationRequirement::None
    );
}

#[test]
fn allow_implicit_vault_decision_table_covers_work_creative_and_scoped_cases() {
    use crate::ai_runtime::run_contract::SecurityDomain;
    use crate::ai_runtime::run_intake::allow_implicit_vault_for_run;

    assert!(allow_implicit_vault_for_run(
        SecurityDomain::Normal,
        "根据授权的本地项目资料总结里程碑",
        false,
    ));
    assert!(!allow_implicit_vault_for_run(
        SecurityDomain::Normal,
        "请继续创作这部小说的下一章。",
        false,
    ));
    assert!(!allow_implicit_vault_for_run(
        SecurityDomain::Normal,
        "Rewrite this sentence more clearly: The team met yesterday.",
        false,
    ));
    assert!(!allow_implicit_vault_for_run(
        SecurityDomain::Normal,
        "bounded security boundary request",
        false,
    ));
    assert!(allow_implicit_vault_for_run(
        SecurityDomain::Normal,
        "bounded security boundary request",
        true,
    ));
    assert!(!allow_implicit_vault_for_run(
        SecurityDomain::Classified,
        "根据本地笔记回答",
        false,
    ));
}

#[test]
fn request_that_rejects_local_material_never_enters_the_implicit_vault_boundary() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "不要使用本地材料；请联网核实当前公开状态。".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.context, ContextMode::None);
    assert_eq!(envelope.freshness, Freshness::WebRequired);
    assert!(!envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "vault.read"));
}

#[test]
fn web_enabled_external_question_persists_the_web_capability_contract() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "最近世界杯战况如何？".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::WebRequired);
    assert_eq!(
        envelope.verification_requirement,
        VerificationRequirement::CurrentRunWeb
    );
    assert_eq!(envelope.web_reason, WebDecisionReason::VolatileExternalFact);
    assert!(envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "web.search"));
}

#[test]
fn hr2_new_runs_never_grant_domain_capabilities_or_freeze_domain_plans() {
    for message in [
        "上海未来一周天气",
        "今天有什么重要新闻",
        "上海正在上映什么电影",
        "苹果现在股价多少",
        "今晚湖人比赛几点",
    ] {
        let mut request = request();
        request.web_enabled = true;
        request.turn.message = message.to_string();

        let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

        assert_eq!(envelope.fresh_fact, Default::default(), "{message}");
        assert!(
            !envelope
                .required_capabilities
                .iter()
                .any(|capability| capability.as_str() == "web.domain.read"),
            "{message} must not carry a domain-only capability"
        );
        assert!(
            envelope
                .required_capabilities
                .iter()
                .any(|capability| capability.as_str() == "web.search"),
            "{message} must carry web.search"
        );
        assert!(
            !envelope
                .required_capabilities
                .iter()
                .any(|capability| capability.as_str() == "external.read"),
            "{message} must not grant external.read"
        );
    }
}

#[test]
fn web_disabled_current_fact_does_not_add_domain_capability() {
    let mut request = request();
    request.web_enabled = false;
    request.turn.message = "上海未来一周天气".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert!(!envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "web.domain.read"));
    assert_eq!(envelope.fresh_fact, Default::default());
}

#[test]
fn web_toggle_is_the_only_authority_that_grants_web_search() {
    let mut disabled = request();
    disabled.web_enabled = false;
    disabled.turn.message = "请联网检索并核实今天的市场消息。".to_string();

    let disabled_envelope = RunIntake::resolve_envelope(&disabled).expect("resolve disabled");
    assert_eq!(disabled_envelope.freshness, Freshness::Offline);
    assert_eq!(
        disabled_envelope.web_reason,
        WebDecisionReason::UserDisabled
    );
    assert!(!disabled_envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "web.search"));

    let mut enabled = disabled;
    enabled.web_enabled = true;
    let enabled_envelope = RunIntake::resolve_envelope(&enabled).expect("resolve enabled");
    assert_eq!(enabled_envelope.freshness, Freshness::WebRequired);
    assert!(enabled_envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "web.search"));
}

#[test]
fn ordinary_external_fact_without_strong_temporal_or_risk_signal_is_web_preferred() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "2026美加墨世界杯的四强分别是谁？".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::WebPreferred);
    assert_eq!(envelope.web_reason, WebDecisionReason::DefaultOnline);
    assert_eq!(
        envelope.verification_requirement,
        super::run_contract::VerificationRequirement::None
    );
    assert!(envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "web.search"));
}

#[test]
fn rejecting_local_notes_as_a_factual_source_still_requires_web_verification() {
    for message in [
        "确认 synthetic 软件当前稳定版本，不使用本地笔记作为版本事实。",
        "Confirm the current synthetic software version; do not use local notes as version facts.",
    ] {
        let mut request = request();
        request.web_enabled = true;
        request.turn.message = message.to_string();

        let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

        assert_eq!(envelope.freshness, Freshness::WebRequired, "{message}");
        assert_eq!(
            envelope.verification_requirement,
            VerificationRequirement::CurrentRunWeb,
            "{message}"
        );
    }
}

#[test]
fn creative_copy_request_uses_generic_webpreferred_when_authorized() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "请写一个三句式的产品发布开场白。".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::WebPreferred);
    assert_eq!(envelope.web_reason, WebDecisionReason::DefaultOnline);
    assert_eq!(
        envelope.verification_requirement,
        super::run_contract::VerificationRequirement::None
    );
}

#[test]
fn strong_temporal_and_high_risk_requests_require_current_run_web_evidence() {
    let cases = [
        "今天世界杯决赛结果是什么？",
        "Who won the World Cup final today?",
        "现任法国总统是谁？请核实后回答。",
        "What is the current share price?",
        "今天发生了哪些重要新闻？",
        "这场比赛的当前比分是多少？",
        "请联网核实这个赛事的最终结果。",
        "本周的监管规则是否已经生效？",
        "请给出当前用药建议。",
    ];

    for message in cases {
        let mut online = request();
        online.web_enabled = true;
        online.turn.message = message.to_string();
        let online_envelope = RunIntake::resolve_envelope(&online).expect("online envelope");
        assert_eq!(
            online_envelope.freshness,
            Freshness::WebRequired,
            "{message}"
        );
        assert_eq!(
            online_envelope.verification_requirement,
            super::run_contract::VerificationRequirement::CurrentRunWeb,
            "{message}"
        );

        let mut offline = online;
        offline.web_enabled = false;
        let offline_envelope = RunIntake::resolve_envelope(&offline).expect("offline envelope");
        assert_eq!(offline_envelope.freshness, Freshness::Offline, "{message}");
        assert_eq!(
            offline_envelope.verification_requirement,
            super::run_contract::VerificationRequirement::CurrentRunWeb,
            "{message}"
        );
        assert!(!offline_envelope
            .required_capabilities
            .iter()
            .any(|capability| capability.as_str() == "web.search"));
    }
}

/// HR-2 regression: ordinary external questions use the progressive route.
#[test]
fn ordinary_external_questions_use_webpreferred_without_strict_finalization() {
    for message in [
        "推荐三本理解组织治理的入门书，并说明适合什么读者。",
        "比较两种常见的知识管理方法，各自适合什么场景？",
        "帮我梳理公开可得的 Markdown 写作建议。",
    ] {
        let mut request = request();
        request.web_enabled = true;
        request.turn.message = message.into();

        let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

        assert_eq!(
            envelope.freshness,
            Freshness::WebPreferred,
            "HR-2-target: ordinary external question should be progressive: {message}"
        );
        assert_eq!(
            envelope.verification_requirement,
            VerificationRequirement::None,
            "HR-2-target: ordinary external question should not require current Run Web evidence: {message}"
        );
    }
}

#[test]
fn hr2_task_matrix_uses_task_contracts_instead_of_fresh_fact_domains() {
    struct Case {
        name: &'static str,
        request: AssistantRunStartRequest,
        effect: Effect,
        freshness: Freshness,
        verification: VerificationRequirement,
        effort: Effort,
        web_capability: bool,
    }

    let mut ask_notes = request();
    ask_notes.client_request_id = "hr2-ask-notes".into();
    ask_notes.turn.message = "请根据附带笔记概述要点。".into();
    ask_notes.turn.explicit_references = vec![valid_reference()];
    ask_notes.turn.retrieval_scope.path_prefixes = vec!["notes/".into()];
    ask_notes.web_enabled = true;

    let mut research = request();
    research.client_request_id = "hr2-research".into();
    research.turn.message = "比较两种常见的知识管理方法，各自适合什么场景？".into();
    research.web_enabled = true;

    let mut citation_check = request();
    citation_check.client_request_id = "hr2-citation-check".into();
    citation_check.turn.message =
        "请联网核实 https://example.com/release-notes 的当前版本。".into();
    citation_check.web_enabled = true;

    let mut draft = request();
    draft.client_request_id = "hr2-draft".into();
    draft.turn.message = "将这段话润色成正式通知。".into();
    draft.explicit_action = Some(ExplicitAction {
        effect: Effect::Draft,
        target: None,
        selection_snapshot: None,
    });

    let mut apply = request();
    apply.client_request_id = "hr2-apply".into();
    apply.turn.message = "将附带笔记的标题改得更清晰。".into();
    apply.turn.explicit_references = vec![valid_reference()];
    apply.explicit_action = Some(ExplicitAction {
        effect: Effect::Apply,
        target: Some(ExplicitTarget {
            reference_id: "reference".into(),
            content_hash: valid_content_hash(),
        }),
        selection_snapshot: None,
    });

    let mut chat = request();
    chat.client_request_id = "hr2-chat".into();
    chat.turn.message = "你好！".into();
    chat.web_enabled = true;

    let cases = [
        Case {
            name: "chat",
            request: chat,
            effect: Effect::Answer,
            freshness: Freshness::WebPreferred,
            verification: VerificationRequirement::None,
            effort: Effort::ToolLoop,
            web_capability: true,
        },
        Case {
            name: "ask-notes",
            request: ask_notes,
            effect: Effect::Answer,
            freshness: Freshness::WebPreferred,
            verification: VerificationRequirement::None,
            effort: Effort::ToolLoop,
            web_capability: true,
        },
        Case {
            name: "research",
            request: research,
            effect: Effect::Answer,
            freshness: Freshness::WebPreferred,
            verification: VerificationRequirement::None,
            effort: Effort::ToolLoop,
            web_capability: true,
        },
        Case {
            name: "citation-check",
            request: citation_check,
            effect: Effect::Answer,
            freshness: Freshness::WebRequired,
            verification: VerificationRequirement::CurrentRunWeb,
            effort: Effort::ToolLoop,
            web_capability: true,
        },
        Case {
            name: "draft",
            request: draft,
            effect: Effect::Draft,
            freshness: Freshness::Offline,
            verification: VerificationRequirement::None,
            effort: Effort::Direct,
            web_capability: false,
        },
        Case {
            name: "apply",
            request: apply,
            effect: Effect::Apply,
            freshness: Freshness::Offline,
            verification: VerificationRequirement::None,
            effort: Effort::Durable,
            web_capability: false,
        },
    ];

    for case in cases {
        let envelope = RunIntake::resolve_envelope(&case.request).expect(case.name);
        assert_eq!(envelope.effect, case.effect, "{} effect", case.name);
        assert_eq!(
            envelope.freshness, case.freshness,
            "{} freshness",
            case.name
        );
        assert_eq!(
            envelope.verification_requirement, case.verification,
            "{} verification",
            case.name
        );
        assert_eq!(envelope.effort, case.effort, "{} effort", case.name);
        assert_eq!(
            envelope
                .required_capabilities
                .iter()
                .any(|capability| capability.as_str() == "web.search"),
            case.web_capability,
            "{} web capability",
            case.name
        );
        assert_eq!(
            envelope.fresh_fact,
            Default::default(),
            "{} fresh plan",
            case.name
        );
    }
}

#[test]
fn strict_web_boundaries_remain_required_in_the_hr1_baseline() {
    for message in [
        "请联网核实 https://example.com/release-notes 的当前版本。",
        "今天该证券的最新收盘价格是多少？",
        "当前这项法规是否已经生效？请核实后回答。",
    ] {
        let mut online = request();
        online.web_enabled = true;
        online.turn.message = message.into();

        let online_envelope = RunIntake::resolve_envelope(&online).expect("online envelope");
        assert_eq!(
            online_envelope.freshness,
            Freshness::WebRequired,
            "{message}"
        );
        assert_eq!(
            online_envelope.verification_requirement,
            VerificationRequirement::CurrentRunWeb,
            "{message}"
        );

        let mut offline = online;
        offline.web_enabled = false;
        let offline_envelope = RunIntake::resolve_envelope(&offline).expect("offline envelope");
        assert_eq!(offline_envelope.freshness, Freshness::Offline, "{message}");
        assert_eq!(
            offline_envelope.verification_requirement,
            VerificationRequirement::CurrentRunWeb,
            "{message}"
        );
        assert!(
            !offline_envelope
                .required_capabilities
                .iter()
                .any(|capability| capability.as_str() == "web.search"),
            "{message}"
        );
    }
}

#[test]
fn explicit_file_reference_keeps_the_generic_preferred_web_surface_available() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message =
        "根据 问题线索工作思路（刘CG），我们应该怎样分析刘CG的责任？".to_string();
    request
        .turn
        .explicit_references
        .push(crate::ai_types::ContextReferenceWire {
            id: "note-liu".into(),
            kind: crate::ai_types::ContextReferenceKind::Note,
            file_path: Some("线索/问题线索工作思路（刘CG）.md".into()),
            content_hash: Some(valid_content_hash()),
            utf8_range: None,
            editor_range: None,
            excerpt: String::new(),
            heading_path: None,
            anchor: None,
            stale: false,
            invalid_reason: None,
        });

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.context, ContextMode::ExplicitReferences);
    assert_eq!(envelope.freshness, Freshness::WebPreferred);
    assert_eq!(
        envelope.verification_requirement,
        VerificationRequirement::None
    );
    assert_eq!(envelope.effort, Effort::ToolLoop);
}

#[test]
fn explicit_reference_with_retrieval_scope_still_uses_tool_loop() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "在这个文件夹里再找相关笔记并总结".to_string();
    request
        .turn
        .explicit_references
        .push(crate::ai_types::ContextReferenceWire {
            id: "note-liu".into(),
            kind: crate::ai_types::ContextReferenceKind::Note,
            file_path: Some("线索/问题线索工作思路（刘CG）.md".into()),
            content_hash: Some(valid_content_hash()),
            utf8_range: None,
            editor_range: None,
            excerpt: String::new(),
            heading_path: None,
            anchor: None,
            stale: false,
            invalid_reason: None,
        });
    request.turn.retrieval_scope.path_prefixes = vec!["线索/".into()];

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.effort, Effort::ToolLoop);
}

#[test]
fn web_enabled_trusted_runtime_questions_remain_offline() {
    for message in [
        "今天星期几？",
        "现在几点？",
        "当前应用版本是什么？",
        "Which day of the week is it today?",
    ] {
        let mut request = request();
        request.web_enabled = true;
        request.turn.message = message.to_string();

        let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

        assert_eq!(envelope.freshness, Freshness::Offline, "{message}");
        assert_eq!(
            envelope.web_reason,
            WebDecisionReason::TrustedRuntimeFact,
            "{message}"
        );
    }
}

#[test]
fn today_date_question_uses_trusted_runtime_without_freezing_a_domain_plan() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "你好，今天是几月几日？".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.fresh_fact, Default::default());
    assert_eq!(envelope.freshness, Freshness::Offline);
    assert_eq!(envelope.web_reason, WebDecisionReason::TrustedRuntimeFact);
    assert!(!envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "web.search"));
}

#[test]
fn broad_recent_question_is_webrequired_and_enters_the_generic_tool_loop() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "最近有什么好看的电影".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::WebRequired);
    assert_eq!(
        envelope.verification_requirement,
        VerificationRequirement::CurrentRunWeb
    );
    assert_eq!(envelope.fresh_fact, Default::default());
    assert_eq!(envelope.effort, Effort::ToolLoop);
    assert!(envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "web.search"));
}

#[test]
fn web_disabled_new_runs_keep_no_domain_plan_and_only_strict_tasks_keep_evidence_obligations() {
    for (message, verification_requirement) in [
        ("上海未来一周天气", VerificationRequirement::CurrentRunWeb),
        ("今天有什么重要新闻", VerificationRequirement::CurrentRunWeb),
        (
            "最近有什么好看的电影",
            VerificationRequirement::CurrentRunWeb,
        ),
        ("苹果现在股价多少", VerificationRequirement::CurrentRunWeb),
        ("今晚湖人比赛几点", VerificationRequirement::CurrentRunWeb),
    ] {
        let mut request = request();
        request.web_enabled = false;
        request.turn.message = message.to_string();

        let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

        assert_eq!(envelope.fresh_fact, Default::default(), "{message}");
        assert_eq!(envelope.freshness, Freshness::Offline, "{message}");
        assert!(
            !envelope
                .required_capabilities
                .iter()
                .any(|capability| capability.as_str() == "web.search"),
            "{message}"
        );
        assert_eq!(
            envelope.verification_requirement, verification_requirement,
            "{message}"
        );
    }
}

#[test]
fn web_enabled_conversation_followups_keep_the_generic_web_tool_surface_available() {
    for message in [
        "这么简单的问题你还联网搜索？",
        "刚刚问你为什么简单问题也联网搜索，你就坏掉了？",
        "为什么你刚才调用了 web search？",
        "Why did you browse the web for my previous question?",
    ] {
        let mut request = request();
        request.web_enabled = true;
        request.turn.message = message.to_string();

        let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

        assert_eq!(envelope.freshness, Freshness::WebPreferred, "{message}");
        assert_eq!(envelope.effort, Effort::ToolLoop, "{message}");
        assert!(
            envelope
                .required_capabilities
                .iter()
                .any(|capability| capability.as_str() == "web.search"),
            "{message}"
        );
    }
}

#[test]
fn explicit_reverification_of_a_prior_answer_outranks_conversation_meta_heuristics() {
    let mut request = request();
    request.web_enabled = true;
    request.session = Some(crate::ai_runtime::run_contract::AssistantSessionRef {
        domain: crate::ai_runtime::run_contract::SecurityDomain::Normal,
        session_key: "session-reverify".to_string(),
    });
    request.turn.message = "请联网核实你刚才关于这个事实的回答，不要再凭记忆编造。".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::WebRequired);
    assert_eq!(
        envelope.verification_requirement,
        VerificationRequirement::CurrentRunWeb
    );
    assert_eq!(envelope.effort, Effort::ToolLoop);
    assert!(envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "web.search"));
}

#[test]
fn bilingual_web_intent_fixture_has_120_deterministic_cases() {
    #[derive(serde::Deserialize)]
    struct FixtureGroup {
        freshness: String,
        reason: String,
        cases: Vec<String>,
    }

    let groups: Vec<FixtureGroup> =
        serde_json::from_str(include_str!("fixtures/web_intent_v1.json"))
            .expect("web intent fixture JSON");
    let mut count = 0;
    let mut mismatches = Vec::new();
    for group in groups {
        for message in group.cases {
            count += 1;
            let mut request = request();
            request.web_enabled = true;
            request.turn.message = message.clone();
            let envelope = RunIntake::resolve_envelope(&request).expect("resolve fixture");
            let freshness = serde_json::to_value(envelope.freshness)
                .expect("freshness")
                .as_str()
                .expect("freshness string")
                .to_string();
            let reason = serde_json::to_value(envelope.web_reason)
                .expect("reason")
                .as_str()
                .expect("reason string")
                .to_string();
            if freshness != group.freshness || reason != group.reason {
                mismatches.push(format!(
                    "{message}: got {freshness}/{reason}, expected {}/{}",
                    group.freshness, group.reason
                ));
            }
        }
    }
    assert_eq!(count, 120);
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn quoted_web_instruction_is_data_but_keeps_the_generic_preferred_surface() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "把‘请联网搜索最新消息’翻译成英文。".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::WebPreferred);
    assert_eq!(envelope.web_reason, WebDecisionReason::DefaultOnline);
}

#[test]
fn quoted_offline_instruction_is_data_but_keeps_the_generic_preferred_surface() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message =
        "Translate the quoted sentence 'Do not browse the web' into Chinese.".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::WebPreferred);
    assert_eq!(envelope.web_reason, WebDecisionReason::DefaultOnline);
}

#[test]
fn transformation_word_does_not_hide_an_unbound_current_facts_request() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "Summarize the latest breaking news.".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::WebRequired);
    assert_eq!(envelope.web_reason, WebDecisionReason::VolatileExternalFact);
}

#[test]
fn explicit_local_reference_does_not_downgrade_a_comparison_with_public_evidence() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message =
        "Compare the authorized local hypothesis with current public synthetic evidence and cite both."
            .to_string();
    request
        .turn
        .explicit_references
        .push(crate::ai_types::ContextReferenceWire {
            id: "authorized-note".into(),
            kind: crate::ai_types::ContextReferenceKind::Note,
            file_path: Some("notes/authorized.md".into()),
            content_hash: Some(valid_content_hash()),
            utf8_range: None,
            editor_range: None,
            excerpt: String::new(),
            heading_path: None,
            anchor: None,
            stale: false,
            invalid_reason: None,
        });

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::WebRequired);
    assert_eq!(
        envelope.verification_requirement,
        super::run_contract::VerificationRequirement::CurrentRunWeb
    );
    assert_eq!(envelope.web_reason, WebDecisionReason::VolatileExternalFact);
}

#[test]
fn continuing_a_normal_session_authorizes_bounded_conversation_context() {
    let mut request = request();
    request.session = Some(super::run_contract::AssistantSessionRef {
        domain: SecurityDomain::Normal,
        session_key: "existing-session".into(),
    });

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.context, ContextMode::Conversation);
}

#[test]
fn web_enabled_short_greeting_keeps_the_generic_preferred_web_surface_available() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "你好！".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::WebPreferred);
    assert_eq!(envelope.effort, Effort::ToolLoop);
    assert!(envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "web.search"));
}

#[test]
fn short_failure_follow_up_keeps_the_generic_preferred_web_surface_available() {
    let mut request = request();
    request.web_enabled = true;
    request.session = Some(super::run_contract::AssistantSessionRef {
        domain: SecurityDomain::Normal,
        session_key: "existing-session".into(),
    });
    request.turn.message = "怎么了？为什么失败了？".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::WebPreferred);
    assert_eq!(envelope.effort, Effort::ToolLoop);
    assert_eq!(envelope.web_reason, WebDecisionReason::DefaultOnline);
}

#[test]
fn web_enabled_ordinary_external_fact_uses_the_progressive_tool_loop() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "When was the first iPhone announced?".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::WebPreferred);
    assert_eq!(envelope.effort, Effort::ToolLoop);
}

#[test]
fn explicit_subagent_request_adds_only_the_child_run_capability() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "请把调研拆成两个子任务并行交叉验证。".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert!(envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "harness.child_run"));
    assert_eq!(envelope.effort, Effort::ToolLoop);
    assert!(!envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "harness.conclude"));
}

#[test]
fn explicit_web_instruction_overrides_the_local_transformation_shortcut() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "请联网搜索最新报道后翻译成中文。".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::WebRequired);
}

#[test]
fn web_enabled_local_only_transformation_never_enters_the_web_tool_chain() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "只用本地资料，把“最近世界杯战况如何？”改写得更礼貌。".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.effect, Effect::Answer);
    assert_eq!(envelope.freshness, Freshness::Offline);
    assert_eq!(envelope.effort, Effort::Direct);
    assert!(!envelope
        .material_needs
        .contains(&super::run_contract::MaterialNeed::Web));
    assert!(!envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "web.search"));
}

#[test]
fn intake_directive_text_matrix_ignores_quoted_data_and_honors_real_constraints() {
    #[derive(Clone, Copy)]
    enum ActionFixture {
        None,
        ValidApply,
        ConflictingApply,
    }

    struct Case {
        name: &'static str,
        message: &'static str,
        action: ActionFixture,
        freshness: Freshness,
        effect: Effect,
        effort: Option<Effort>,
        constraint: Option<&'static str>,
        child_run: bool,
    }

    let cases = [
        Case {
            name: "中文否定",
            message: "不要联网，只根据本地材料总结当前法律建议。",
            action: ActionFixture::None,
            freshness: Freshness::Offline,
            effect: Effect::Answer,
            effort: None,
            constraint: Some("local_only"),
            child_run: false,
        },
        Case {
            name: "English negation",
            message:
                "Do not browse; answer the current legal question from supplied material only.",
            action: ActionFixture::None,
            freshness: Freshness::Offline,
            effect: Effect::Answer,
            effort: None,
            constraint: Some("local_only"),
            child_run: false,
        },
        Case {
            name: "quoted speech is data",
            message:
                "Translate 'do not modify, do not browse, and delegate a child task' into Chinese.",
            action: ActionFixture::None,
            freshness: Freshness::WebPreferred,
            effect: Effect::Answer,
            effort: Some(Effort::ToolLoop),
            constraint: None,
            child_run: false,
        },
        Case {
            name: "quoted vault words are not retrieval directives",
            message: "Translate “summarize the project notes” into Chinese.",
            action: ActionFixture::None,
            freshness: Freshness::WebPreferred,
            effect: Effect::Answer,
            effort: Some(Effort::ToolLoop),
            constraint: None,
            child_run: false,
        },
        Case {
            name: "quoted do-not-modify does not cancel explicit Apply",
            message: "把句子 “do not modify” 写入目标。",
            action: ActionFixture::ValidApply,
            freshness: Freshness::Offline,
            effect: Effect::Apply,
            effort: Some(Effort::Durable),
            constraint: None,
            child_run: false,
        },
        Case {
            name: "real do-not-modify wins over conflicting Apply",
            message: "不要修改文件，只解释这项变更。",
            action: ActionFixture::ConflictingApply,
            freshness: Freshness::Offline,
            effect: Effect::Answer,
            effort: Some(Effort::Direct),
            constraint: Some("do_not_modify"),
            child_run: false,
        },
        Case {
            name: "local-only remains a hard boundary for high-risk facts",
            message: "只用本地材料回答：当前用药剂量建议是什么？",
            action: ActionFixture::None,
            freshness: Freshness::Offline,
            effect: Effect::Answer,
            effort: None,
            constraint: Some("local_only"),
            child_run: false,
        },
        Case {
            name: "unquoted high-risk current fact requires Web",
            message: "What is the current recommended medical dosage?",
            action: ActionFixture::None,
            freshness: Freshness::WebRequired,
            effect: Effect::Answer,
            effort: Some(Effort::ToolLoop),
            constraint: None,
            child_run: false,
        },
        Case {
            name: "quoted high-risk facts remain transformation data",
            message: "Translate “current legal advice and medical dosage” into Chinese.",
            action: ActionFixture::None,
            freshness: Freshness::WebPreferred,
            effect: Effect::Answer,
            effort: Some(Effort::ToolLoop),
            constraint: None,
            child_run: false,
        },
    ];

    for case in cases {
        let mut input = request();
        input.client_request_id = format!("directive-matrix-{}", case.name);
        input.turn.message = case.message.into();
        input.web_enabled = true;
        match case.action {
            ActionFixture::None => {}
            ActionFixture::ValidApply => {
                input.web_enabled = false;
                input
                    .turn
                    .explicit_references
                    .push(crate::ai_types::ContextReferenceWire {
                        id: "directive-note".into(),
                        kind: crate::ai_types::ContextReferenceKind::Note,
                        file_path: Some("notes/directive.md".into()),
                        content_hash: Some(valid_content_hash()),
                        utf8_range: None,
                        editor_range: None,
                        excerpt: String::new(),
                        heading_path: None,
                        anchor: None,
                        stale: false,
                        invalid_reason: None,
                    });
                input.explicit_action = Some(ExplicitAction {
                    effect: Effect::Apply,
                    target: Some(ExplicitTarget {
                        reference_id: "directive-note".into(),
                        content_hash: valid_content_hash(),
                    }),
                    selection_snapshot: None,
                });
            }
            ActionFixture::ConflictingApply => {
                input.web_enabled = false;
                input.explicit_action = Some(ExplicitAction {
                    effect: Effect::Apply,
                    target: None,
                    selection_snapshot: None,
                });
            }
        }

        let envelope = RunIntake::resolve_envelope(&input)
            .unwrap_or_else(|error| panic!("{}: {error}", case.name));
        assert_eq!(envelope.freshness, case.freshness, "{}", case.name);
        assert_eq!(envelope.effect, case.effect, "{}", case.name);
        if let Some(effort) = case.effort {
            assert_eq!(envelope.effort, effort, "{}", case.name);
        }
        let constraint_kinds = envelope
            .explicit_constraints
            .iter()
            .map(|constraint| constraint.kind.as_str())
            .collect::<Vec<_>>();
        match case.constraint {
            Some(constraint) => assert!(
                constraint_kinds.contains(&constraint),
                "{}: {constraint_kinds:?}",
                case.name
            ),
            None => assert!(
                !constraint_kinds.contains(&"local_only")
                    && !constraint_kinds.contains(&"do_not_modify"),
                "{}: {constraint_kinds:?}",
                case.name
            ),
        }
        assert_eq!(
            envelope
                .required_capabilities
                .iter()
                .any(|capability| capability.as_str() == "harness.child_run"),
            case.child_run,
            "{}",
            case.name
        );
    }
}

#[test]
fn intake_freezes_the_run_budget_policy_matrix() {
    fn persisted_budget(request: AssistantRunStartRequest) -> serde_json::Value {
        let db = Database::open_in_memory().expect("database");
        let accepted = RunIntake::start(&db, request).expect("accepted run");
        db.with_read_conn(|conn| {
            let json: String = conn.query_row(
                "SELECT budget_policy_json FROM agent_runs WHERE run_id = ?1",
                [&accepted.run_id],
                |row| row.get(0),
            )?;
            serde_json::from_str(&json).map_err(Into::into)
        })
        .expect("persisted budget policy")
    }

    let mut direct = request();
    direct.client_request_id = "budget-direct".into();
    direct.turn.message = "你好".into();

    let mut standard = request();
    standard.client_request_id = "budget-standard".into();
    standard.turn.message = "根据本地项目笔记总结里程碑".into();

    let mut delegated = request();
    delegated.client_request_id = "budget-delegated".into();
    delegated.turn.message = "请委派子任务并行交叉验证".into();

    let mut durable = request();
    durable.client_request_id = "budget-durable".into();
    durable.turn.message = "应用这项修改".into();
    durable
        .turn
        .explicit_references
        .push(crate::ai_types::ContextReferenceWire {
            id: "budget-note".into(),
            kind: crate::ai_types::ContextReferenceKind::Note,
            file_path: Some("notes/budget.md".into()),
            content_hash: Some(valid_content_hash()),
            utf8_range: None,
            editor_range: None,
            excerpt: String::new(),
            heading_path: None,
            anchor: None,
            stale: false,
            invalid_reason: None,
        });
    durable.explicit_action = Some(ExplicitAction {
        effect: Effect::Apply,
        target: Some(ExplicitTarget {
            reference_id: "budget-note".into(),
            content_hash: valid_content_hash(),
        }),
        selection_snapshot: None,
    });

    let cases = [
        (
            direct,
            serde_json::json!({
                "schemaVersion": 1,
                "profile": "direct",
                "maxPromptTokens": 64000,
                "maxCompletionTokens": 8000,
                "maxTurnOutputTokens": 8000,
                "maxModelTurns": 1,
                "maxToolCalls": 0,
                "maxChildRuns": 0,
                "childMaxModelTurns": 0,
                "childMaxToolCalls": 0,
                "childInputTokensPerTurn": 0,
                "childOutputTokensPerTurn": 0,
                "postConfirmationMaxModelTurns": 0
            }),
        ),
        (
            standard,
            serde_json::json!({
                "schemaVersion": 1,
                "profile": "standard",
                "maxPromptTokens": 128000,
                "maxCompletionTokens": 16000,
                "maxTurnOutputTokens": 4000,
                "maxModelTurns": 8,
                "maxToolCalls": 24,
                "maxChildRuns": 0,
                "childMaxModelTurns": 0,
                "childMaxToolCalls": 0,
                "childInputTokensPerTurn": 0,
                "childOutputTokensPerTurn": 0,
                "postConfirmationMaxModelTurns": 0
            }),
        ),
        (
            delegated,
            serde_json::json!({
                "schemaVersion": 1,
                "profile": "delegated",
                "maxPromptTokens": 96000,
                "maxCompletionTokens": 12000,
                "maxTurnOutputTokens": 4000,
                "maxModelTurns": 8,
                "maxToolCalls": 24,
                "maxChildRuns": 3,
                "childMaxModelTurns": 2,
                "childMaxToolCalls": 6,
                "childInputTokensPerTurn": 2000,
                "childOutputTokensPerTurn": 1024,
                "postConfirmationMaxModelTurns": 0
            }),
        ),
        (
            durable,
            serde_json::json!({
                "schemaVersion": 1,
                "profile": "durable_apply",
                "maxPromptTokens": 128000,
                "maxCompletionTokens": 16000,
                "maxTurnOutputTokens": 4000,
                "maxModelTurns": 8,
                "maxToolCalls": 24,
                "maxChildRuns": 0,
                "childMaxModelTurns": 0,
                "childMaxToolCalls": 0,
                "childInputTokensPerTurn": 0,
                "childOutputTokensPerTurn": 0,
                "postConfirmationMaxModelTurns": 0
            }),
        ),
    ];

    for (request, expected) in cases {
        let actual = persisted_budget(request);
        assert_eq!(actual["profile"], expected["profile"]);
        assert_eq!(actual["maxModelTurns"], expected["maxModelTurns"]);
        assert_eq!(actual["maxToolCalls"], expected["maxToolCalls"]);
        assert_eq!(actual["schemaVersion"], 3);
    }
}
