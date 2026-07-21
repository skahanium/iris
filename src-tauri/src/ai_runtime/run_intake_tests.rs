use super::run_contract::{
    AssistantRunControlRequest, AssistantRunStartRequest, AssistantTurnDraft, ContextMode,
    DisplayMention, DisplayMentionKind, DisplayMentionRange, Effect, Effort, ExplicitAction,
    ExplicitTarget, Freshness, RiskClass, RunControlAction, RunEventPayload, RunEventType,
    RunState, SecurityDomain, SelectionSnapshot, WebDecisionReason,
};
use super::run_engine::RunEventSink;
use super::run_intake::RunIntake;
use super::{
    agent_run_repository::{AgentRunRepository, AppendRunEventInput},
    frozen_change_plan::{FrozenChangePlan, FrozenChangePlanInput},
};
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
        security_domain: SecurityDomain::Normal,
        classified_context_ref: None,
    }
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
fn intake_rejects_actions_that_do_not_bind_to_the_explicit_reference() {
    let mut invalid = request();
    invalid.explicit_action = Some(ExplicitAction {
        effect: Effect::Draft,
        target: Some(ExplicitTarget {
            reference_id: "missing-reference".to_string(),
            content_hash: "hash".to_string(),
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
            content_hash: Some("hash".into()),
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
            content_hash: Some("hash".into()),
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
            content_hash: Some("hash".into()),
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
            content_hash: Some("selection-hash".to_string()),
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
            content_hash: "selection-hash".to_string(),
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
            content_hash: Some("selection-hash".into()),
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
            content_hash: "selection-hash".into(),
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
    let mut second_request = request();
    second_request.client_request_id = "latest-active-client-request".to_string();
    second_request.session = Some(first.session.clone());
    let second = RunIntake::start(&db, second_request).expect("second accepted run");

    let recovered = RunIntake::get_latest_active(&db, &first.session)
        .expect("recover latest")
        .expect("active run");
    assert_eq!(recovered.run.run_id, second.run_id);

    RunIntake::control(
        &db,
        AssistantRunControlRequest {
            session: first.session.clone(),
            run_id: second.run_id.clone(),
            expected_state_version: 0,
            action: RunControlAction::Cancel,
        },
    )
    .expect("cancel latest run");
    assert_eq!(
        RunIntake::get_latest_active(&db, &first.session)
            .expect("recover remaining active")
            .expect("first run remains active")
            .run
            .run_id,
        first.run_id
    );

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
    crate::ai_runtime::model_gateway::clear_abort(&first.run_id);
    crate::ai_runtime::model_gateway::clear_abort(&second.run_id);
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
fn envelope_resolver_uses_user_constraints_before_explicit_apply_action() {
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
    assert_eq!(resolved.effort, Effort::Direct);
    assert!(resolved.material_needs.is_empty());
}

#[test]
fn envelope_resolver_keeps_novel_writing_in_conversation_without_implicit_retrieval() {
    let mut novel = request();
    novel.client_request_id = "novel-conversation".into();
    novel.turn.message = "请继续创作这部小说的下一章。".into();

    let resolved = RunIntake::resolve_envelope(&novel).expect("resolve envelope");

    assert_eq!(resolved.context, ContextMode::Conversation);
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
fn rejection_consumes_the_owned_frozen_plan_without_dispatching_it() {
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
    assert_eq!(rejected.run.state, RunState::Running);
    assert!(rejected.run.pending_confirmation.is_none());
    assert_eq!(
        serde_json::to_value(rejected.events.last().expect("resumed event"))
            .expect("serialize resumed event")["type"],
        "resumed"
    );
    db.with_read_conn(|conn| {
        let status: String = conn.query_row(
            "SELECT status FROM agent_run_confirmations WHERE confirmation_id = ?1",
            [&confirmation_id],
            |row| row.get(0),
        )?;
        assert_eq!(status, "rejected");
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
fn web_enabled_pure_rewrite_remains_direct_without_tool_loop() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message =
        "Rewrite this sentence more clearly: The team met yesterday.".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::Offline);
    assert_eq!(envelope.effort, Effort::Direct);
    assert!(!envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "web.search"));
}

#[test]
fn web_enabled_external_question_is_online_without_forced_web_capability() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "最近世界杯战况如何？".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::Online);
    assert_eq!(envelope.web_reason, WebDecisionReason::VolatileExternalFact);
    assert!(!envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "web.search"));
}

#[test]
fn web_enabled_general_question_is_online_by_default_without_keywords() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "量子力学的核心概念是什么？".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::Online);
    assert_eq!(envelope.web_reason, WebDecisionReason::DefaultOnline);
    assert!(!envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "web.search"));
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
fn web_enabled_conversation_meta_questions_never_trigger_search() {
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

        assert_eq!(envelope.freshness, Freshness::Offline, "{message}");
        assert_eq!(
            envelope.web_reason,
            WebDecisionReason::ConversationMeta,
            "{message}"
        );
    }
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
fn quoted_web_instruction_inside_a_transformation_remains_offline() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "把‘请联网搜索最新消息’翻译成英文。".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::Offline);
    assert_eq!(envelope.web_reason, WebDecisionReason::LocalTransformation);
}

#[test]
fn quoted_offline_instruction_inside_a_transformation_is_not_a_user_directive() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message =
        "Translate the quoted sentence 'Do not browse the web' into Chinese.".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::Offline);
    assert_eq!(envelope.web_reason, WebDecisionReason::LocalTransformation);
}

#[test]
fn transformation_word_does_not_hide_an_unbound_current_facts_request() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "Summarize the latest breaking news.".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::Online);
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
fn web_enabled_short_greeting_remains_a_direct_offline_answer() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "你好！".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::Offline);
    assert_eq!(envelope.effort, Effort::Direct);
    assert!(!envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "web.search"));
}

#[test]
fn explicit_web_instruction_overrides_the_local_transformation_shortcut() {
    let mut request = request();
    request.web_enabled = true;
    request.turn.message = "请联网搜索最新报道后翻译成中文。".to_string();

    let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");

    assert_eq!(envelope.freshness, Freshness::Online);
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
