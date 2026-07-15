use super::domain_executor::DomainMaterialRole;
use super::run_context::{RunContext, RunContextAssembler, RunContextMaterial};
use crate::ai_runtime::agent_evidence_repository::{
    AgentEvidenceRepository, LocalEvidenceInput, MaterialRole,
};
use crate::ai_runtime::agent_run_repository::{
    AcceptRunInput, AgentRunRepository, AppendRunEventInput, FinalizeRunInput,
};
use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
use crate::ai_runtime::run_contract::{
    ContextMode, Effect, Effort, ExecutionEnvelope, Freshness, MaterialNeed, Modality, RiskClass,
    SecurityDomain, WebDecisionReason,
};
use crate::ai_types::{ContextReferenceKind, ContextReferenceWire};
use crate::storage::db::Database;

fn envelope() -> ExecutionEnvelope {
    ExecutionEnvelope {
        effect: Effect::Answer,
        context: ContextMode::ExplicitReferences,
        freshness: Freshness::Offline,
        web_reason: WebDecisionReason::LegacyUnknown,
        effort: Effort::Direct,
        security_domain: SecurityDomain::Normal,
        risk: RiskClass::ReadOnly,
        modalities: vec![Modality::Text],
        material_needs: vec![MaterialNeed::Reference],
        required_capabilities: vec![],
        explicit_constraints: vec![],
    }
}

#[test]
fn assemble_reads_only_the_run_persisted_explicit_reference() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    std::fs::write(vault.join("notes/attached.md"), "attached evidence").expect("attached note");
    std::fs::write(vault.join("notes/unattached.md"), "must never be read")
        .expect("unattached note");

    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "explicit-reference-context".into(),
            run_id: "run-explicit-reference-context".into(),
            turn_id: "turn-explicit-reference-context".into(),
            message: "请根据附件回答".into(),
            content_parts: None,
            explicit_references: vec![ContextReferenceWire {
                id: "attached".into(),
                kind: ContextReferenceKind::Note,
                file_path: Some("notes/attached.md".into()),
                content_hash: None,
                utf8_range: None,
                editor_range: None,
                excerpt: "untrusted client excerpt".into(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            }],
            explicit_action: None,
            envelope: envelope(),
        },
    )
    .expect("accepted run");

    let context = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-explicit-reference-context",
    )
    .expect("assembled context");

    assert_eq!(context.user_message, "请根据附件回答");
    assert_eq!(context.materials.len(), 1);
    assert_eq!(context.materials[0].content, "attached evidence");
    assert_eq!(context.materials[0].source_path, "notes/attached.md");
    assert!(!context
        .prompt_with_domain_plan(&context.domain_plan())
        .contains("must never be read"));
    assert!(!context
        .prompt_with_domain_plan(&context.domain_plan())
        .contains("untrusted client excerpt"));
}

#[test]
fn assemble_rejects_reserved_or_changed_explicit_references() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join(".classified")).expect("classified directory");
    std::fs::write(vault.join(".classified/secret.md"), "secret").expect("classified note");

    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "reserved-reference-context".into(),
            run_id: "run-reserved-reference-context".into(),
            turn_id: "turn-reserved-reference-context".into(),
            message: "请回答".into(),
            content_parts: None,
            explicit_references: vec![ContextReferenceWire {
                id: "secret".into(),
                kind: ContextReferenceKind::Note,
                file_path: Some(".classified/secret.md".into()),
                content_hash: None,
                utf8_range: None,
                editor_range: None,
                excerpt: String::new(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            }],
            explicit_action: None,
            envelope: envelope(),
        },
    )
    .expect("accepted run");

    let error = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-reserved-reference-context",
    )
    .expect_err("normal runs must reject classified inputs");
    assert_eq!(error.to_string(), "agent_run_invalid_explicit_reference");
}

#[test]
fn assemble_allows_direct_chat_without_a_vault_when_no_reference_exists() {
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "direct-chat-without-vault".into(),
            run_id: "run-direct-chat-without-vault".into(),
            turn_id: "turn-direct-chat-without-vault".into(),
            message: "不引用文件的普通问答".into(),
            content_parts: None,
            explicit_references: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                context: ContextMode::None,
                material_needs: vec![],
                ..envelope()
            },
        },
    )
    .expect("accepted run");

    let context = RunContextAssembler::assemble(
        &db,
        None,
        &session.session_key,
        "run-direct-chat-without-vault",
    )
    .expect("direct chat context");

    assert!(context.materials.is_empty());
    assert_eq!(context.user_message, "不引用文件的普通问答");
}

#[test]
fn assemble_rejects_reference_when_persisted_hash_no_longer_matches() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    std::fs::write(vault.join("notes/changed.md"), "accepted version").expect("accepted version");
    let accepted_hash = crate::cas::hash::content_hash_str("accepted version");

    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "changed-reference-context".into(),
            run_id: "run-changed-reference-context".into(),
            turn_id: "turn-changed-reference-context".into(),
            message: "引用文件发生变化后不得继续".into(),
            content_parts: None,
            explicit_references: vec![ContextReferenceWire {
                id: "changed".into(),
                kind: ContextReferenceKind::Note,
                file_path: Some("notes/changed.md".into()),
                content_hash: Some(accepted_hash),
                utf8_range: None,
                editor_range: None,
                excerpt: String::new(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            }],
            explicit_action: None,
            envelope: envelope(),
        },
    )
    .expect("accepted run");
    std::fs::write(vault.join("notes/changed.md"), "changed after acceptance").expect("changed");

    let error = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-changed-reference-context",
    )
    .expect_err("changed reference must block Provider prompt assembly");
    assert_eq!(error.to_string(), "agent_run_explicit_reference_changed");
}

#[test]
fn explicit_materials_register_as_run_owned_evidence_without_storing_bodies() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    std::fs::write(vault.join("notes/evidence.md"), "evidence body").expect("evidence note");

    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "evidence-ledger-context".into(),
            run_id: "run-evidence-ledger-context".into(),
            turn_id: "turn-evidence-ledger-context".into(),
            message: "请引用附件".into(),
            content_parts: None,
            explicit_references: vec![ContextReferenceWire {
                id: "evidence".into(),
                kind: ContextReferenceKind::Note,
                file_path: Some("notes/evidence.md".into()),
                content_hash: None,
                utf8_range: None,
                editor_range: None,
                excerpt: String::new(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            }],
            explicit_action: None,
            envelope: envelope(),
        },
    )
    .expect("accepted run");

    let context = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-evidence-ledger-context",
    )
    .expect("assembled context");
    let evidence_ids =
        RunContextAssembler::register_evidence(&db, "run-evidence-ledger-context", &context)
            .expect("registered evidence");

    assert_eq!(evidence_ids.len(), 1);
    db.with_read_conn(|conn| {
        let (source_path, body_column_count): (String, i64) = conn.query_row(
            "SELECT source_path, COUNT(*) OVER () FROM session_evidence WHERE id = ?1",
            [evidence_ids[0]],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(source_path, "notes/evidence.md");
        assert_eq!(body_column_count, 1);
        Ok(())
    })
    .expect("ledger metadata");
}

#[test]
fn prompt_applies_the_domain_executor_rules_without_expanding_explicit_context() {
    let context = RunContext {
        session_id: 1,
        message_seq_first: 1,
        user_message: "请结合制度写一份请示".into(),
        content_parts: None,
        envelope: ExecutionEnvelope {
            material_needs: vec![MaterialNeed::Authority, MaterialNeed::Exemplar],
            ..envelope()
        },
        materials: vec![RunContextMaterial {
            role: DomainMaterialRole::Reference,
            source_path: "notes/attached.md".into(),
            content_hash: "hash".into(),
            source_span_start: 0,
            source_span_end: 8,
            content: "用户明确附上的事实".into(),
        }],
        recent_messages: vec![],
        conversation_memory: None,
        prompt_profile: Default::default(),
        previous_run_summary: None,
    };

    let prompt = context.prompt_with_domain_plan(&context.domain_plan());
    assert!(prompt.contains("内容依据"));
    assert!(prompt.contains("写法参考"));
    assert!(prompt.contains("role=\"reference\""));
    assert!(prompt.contains("用户明确附上的事实"));
    assert!(!prompt.contains("当前活动文档"));
}

#[test]
fn normal_context_includes_six_prior_messages_but_never_duplicates_the_current_turn() {
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "history-first".into(),
            run_id: "history-run-first".into(),
            turn_id: "history-turn-first".into(),
            message: "Why did you search the web?".into(),
            content_parts: None,
            explicit_references: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                context: ContextMode::Conversation,
                ..envelope()
            },
        },
    )
    .expect("first accepted run");
    for (state_version, state) in [
        (0, crate::ai_runtime::run_contract::RunState::Preparing),
        (1, crate::ai_runtime::run_contract::RunState::Running),
    ] {
        AgentRunRepository::append_event(
            &db,
            AppendRunEventInput {
                run_id: "history-run-first".into(),
                state_version,
                event_type: crate::ai_runtime::run_contract::RunEventType::StageChanged,
                payload: crate::ai_runtime::run_contract::RunEventPayload::StageChanged {
                    state,
                    stage: "history fixture".into(),
                },
            },
        )
        .expect("advance first run");
    }
    AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "history-run-first".into(),
            state_version: 2,
            event_type: crate::ai_runtime::run_contract::RunEventType::CapabilityDegraded,
            payload: crate::ai_runtime::run_contract::RunEventPayload::CapabilityDegraded {
                capability: "web.search".into(),
                code: crate::ai_runtime::run_contract::SafeRunErrorCode::WebProviderTimeout,
                retryable: true,
                attempt_count: 2,
                message: "联网核实暂不可用，已继续生成受约束答复。".into(),
            },
        },
    )
    .expect("record safe Web degradation");
    AgentRunRepository::finalize(
        &db,
        FinalizeRunInput {
            run_id: "history-run-first".into(),
            state_version: 2,
            content: "The previous web attempt timed out, so I should explain the degradation."
                .into(),
            evidence_ids: vec![],
            citation_map: serde_json::json!({}),
        },
    )
    .expect("first run finalized");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "history-current".into(),
            run_id: "history-run-current".into(),
            turn_id: "history-turn-current".into(),
            message: "What went wrong just now?".into(),
            content_parts: None,
            explicit_references: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                context: ContextMode::Conversation,
                ..envelope()
            },
        },
    )
    .expect("current accepted run");

    let context =
        RunContextAssembler::assemble(&db, None, &session.session_key, "history-run-current")
            .expect("assembled conversation context");
    assert_eq!(context.recent_messages.len(), 2);
    assert!(context
        .recent_messages
        .iter()
        .all(|message| message.seq < context.message_seq_first));
    let prior_summary = context
        .previous_run_summary
        .as_deref()
        .expect("previous Run safety summary");
    assert!(prior_summary.contains("status=completed"));
    assert!(prior_summary.contains("webResult=degraded"));
    assert!(prior_summary.contains("attemptCount=2"));
    assert!(prior_summary.contains("safeCode=agent_run_web_provider_timeout"));
    assert!(!prior_summary.contains("Why did you search the web?"));

    let messages = context.messages_with_domain_plan(&context.domain_plan());
    let serialized = serde_json::to_string(&messages).expect("messages JSON");
    assert_eq!(serialized.matches("What went wrong just now?").count(), 1);
    assert!(serialized.contains("Why did you search the web?"));
    assert!(serialized.contains("previous web attempt timed out"));
    assert!(messages[0]
        .content
        .text_content()
        .contains("Web access is permission, not a requirement"));
    assert!(messages[0].content.text_content().contains("Local date"));
}

#[test]
fn previous_run_safety_does_not_treat_local_evidence_as_web_success() {
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "local-evidence-first".into(),
            run_id: "local-evidence-run-first".into(),
            turn_id: "local-evidence-turn-first".into(),
            message: "Summarize the attached local note.".into(),
            content_parts: None,
            explicit_references: vec![],
            explicit_action: None,
            envelope: envelope(),
        },
    )
    .expect("first accepted run");
    let message_seq_first = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT m.seq
                 FROM agent_runs r
                 JOIN session_messages m
                   ON m.session_id = r.session_id AND m.turn_id = r.turn_id AND m.role = 'user'
                 WHERE r.run_id = ?1",
                ["local-evidence-run-first"],
                |row| row.get::<_, i64>(0),
            )
            .map_err(Into::into)
        })
        .expect("message sequence");
    let local = AgentEvidenceRepository::register_local(
        &db,
        LocalEvidenceInput {
            session_id: session.session_id,
            run_id: "local-evidence-run-first".into(),
            message_seq_first,
            material_role: MaterialRole::Reference,
            title: "Local note".into(),
            source_path: "notes/local.md".into(),
            source_span_start: 0,
            source_span_end: 12,
            heading_path: None,
            content_hash: "local-note-hash".into(),
            retrieval_reason: Some("explicit_reference".into()),
            score: None,
        },
    )
    .expect("local evidence");
    for (state_version, state) in [
        (0, crate::ai_runtime::run_contract::RunState::Preparing),
        (1, crate::ai_runtime::run_contract::RunState::Running),
    ] {
        AgentRunRepository::append_event(
            &db,
            AppendRunEventInput {
                run_id: "local-evidence-run-first".into(),
                state_version,
                event_type: crate::ai_runtime::run_contract::RunEventType::StageChanged,
                payload: crate::ai_runtime::run_contract::RunEventPayload::StageChanged {
                    state,
                    stage: "local evidence fixture".into(),
                },
            },
        )
        .expect("advance first run");
    }
    AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "local-evidence-run-first".into(),
            state_version: 2,
            event_type: crate::ai_runtime::run_contract::RunEventType::EvidenceRegistered,
            payload: crate::ai_runtime::run_contract::RunEventPayload::EvidenceRegistered {
                evidence_id: local.evidence_id.to_string(),
            },
        },
    )
    .expect("local evidence event");
    AgentRunRepository::finalize(
        &db,
        FinalizeRunInput {
            run_id: "local-evidence-run-first".into(),
            state_version: 2,
            content: "Local-only summary.".into(),
            evidence_ids: vec![local.evidence_id],
            citation_map: serde_json::json!({}),
        },
    )
    .expect("first run finalized");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "local-evidence-current".into(),
            run_id: "local-evidence-run-current".into(),
            turn_id: "local-evidence-turn-current".into(),
            message: "What happened previously?".into(),
            content_parts: None,
            explicit_references: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                context: ContextMode::Conversation,
                ..envelope()
            },
        },
    )
    .expect("current accepted run");

    let context = RunContextAssembler::assemble(
        &db,
        None,
        &session.session_key,
        "local-evidence-run-current",
    )
    .expect("assembled context");
    let prior_summary = context
        .previous_run_summary
        .as_deref()
        .expect("previous Run summary");

    assert!(prior_summary.contains("webAttempted=false"));
    assert!(prior_summary.contains("webResult=skipped"));
}
