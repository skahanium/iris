use super::agent_evidence_repository::{AgentEvidenceRepository, LocalEvidenceInput, MaterialRole};
use super::agent_run_repository::{
    AcceptRunInput, AgentRunRepository, AppendRunCheckpointInput, AppendRunEventInput,
    FinalizeRunInput, RetryRunInput,
};
use super::frozen_change_plan::{FrozenChangePlan, FrozenChangePlanInput};
use super::normal_session_repository::NormalSessionRepository;
use super::run_contract::{
    AssistantRunEvent, ContextMode, DisplayMention, DisplayMentionKind, DisplayMentionRange,
    Effect, Effort, ExecutionEnvelope, ExplicitConstraint, Freshness, MaterialNeed, Modality,
    RiskClass, RunEventPayload, RunEventType, RunState, SecurityDomain, WebDecisionReason,
};
use crate::ai_types::{ContextReferenceKind, ContextReferenceWire, EditorRangeWire, SourceSpan};
use crate::storage::db::Database;

fn setup() -> (Database, i64, String) {
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("normal session");
    (db, session.session_id, session.session_key)
}

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
        explicit_constraints: vec![ExplicitConstraint {
            kind: "local_only".to_string(),
            value: Some("enabled".to_string()),
        }],
    }
}

fn explicit_reference() -> ContextReferenceWire {
    ContextReferenceWire {
        id: "ref-1".to_string(),
        kind: ContextReferenceKind::Selection,
        file_path: Some("notes/roadmap.md".to_string()),
        content_hash: Some("content-hash".to_string()),
        utf8_range: Some(SourceSpan { start: 4, end: 12 }),
        editor_range: Some(EditorRangeWire { from: 5, to: 13 }),
        excerpt: "用户选中的秘密正文，不得复制到 Run 或 Event".to_string(),
        heading_path: Some("计划/阶段二".to_string()),
        anchor: Some("stage-2".to_string()),
        stale: false,
        invalid_reason: None,
    }
}

fn accept_input(session_id: i64, session_key: String) -> AcceptRunInput {
    AcceptRunInput {
        session_id,
        session_key,
        client_request_id: "client-request-1".to_string(),
        run_id: "run-1".to_string(),
        turn_id: "turn-1".to_string(),
        message: "用户的完整提问只能保存到 session_messages".to_string(),
        content_parts: None,
        explicit_references: vec![explicit_reference()],
        context_scope: Default::default(),
        display_mentions: vec![],
        explicit_action: None,
        envelope: envelope(),
    }
}

#[test]
fn accept_is_atomic_and_persists_only_safe_reference_metadata() {
    let (db, session_id, session_key) = setup();
    let mut input = accept_input(session_id, session_key.clone());
    input.explicit_references.push(ContextReferenceWire {
        id: "note-ref".into(),
        kind: ContextReferenceKind::Note,
        file_path: Some("notes/full.md".into()),
        content_hash: Some("note-hash".into()),
        utf8_range: None,
        editor_range: None,
        excerpt: "整篇笔记的客户端正文同样不得持久化".into(),
        heading_path: None,
        anchor: None,
        stale: false,
        invalid_reason: None,
    });
    let accepted = AgentRunRepository::accept(&db, input).expect("accepted run");

    assert_eq!(accepted.run_id, "run-1");
    assert_eq!(accepted.turn_id, "turn-1");
    assert_eq!(accepted.session.session_key, session_key);
    assert_eq!(accepted.state, RunState::Accepted);
    assert_eq!(accepted.state_version, 0);

    db.with_read_conn(|conn| {
        let message: String = conn.query_row(
            "SELECT content FROM session_messages WHERE session_id = ?1",
            [session_id],
            |row| row.get(0),
        )?;
        assert_eq!(message, "用户的完整提问只能保存到 session_messages");

        let (status, goal_summary, refs_json): (String, String, String) = conn.query_row(
            "SELECT status, goal_summary, explicit_references_json
             FROM agent_runs r
             JOIN session_messages m ON m.session_id = r.session_id AND m.turn_id = r.turn_id
             WHERE r.run_id = 'run-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(status, "accepted");
        assert!(goal_summary.contains("sha256="));
        assert!(!goal_summary.contains("完整提问"));
        assert!(refs_json.contains("notes/roadmap.md"));
        assert!(!refs_json.contains("秘密正文"));
        assert!(!refs_json.contains("整篇笔记的客户端正文"));
        assert!(!refs_json.contains("excerpt"));

        let (event_seq, event_type, payload): (i64, String, String) = conn.query_row(
            "SELECT event_seq, event_type, payload_json FROM agent_run_events WHERE run_id = 'run-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(event_seq, 1);
        assert_eq!(event_type, "accepted");
        assert!(!payload.contains("完整提问"));
        Ok(())
    })
    .expect("read accepted facts");
}

#[test]
fn accept_persists_immutable_scope_and_display_mentions_for_prompt_and_history_consumers() {
    let (db, session_id, session_key) = setup();
    let mut input = accept_input(session_id, session_key.clone());
    input.context_scope.paths = vec!["notes/roadmap.md".into()];
    input.context_scope.path_prefixes = vec!["notes/".into()];
    input.context_scope.required_tags = vec!["project".into()];
    input.display_mentions = vec![DisplayMention {
        kind: DisplayMentionKind::File,
        value: "notes/roadmap.md".into(),
        label: "路线图".into(),
        range: DisplayMentionRange { from: 0, to: 3 },
    }];

    AgentRunRepository::accept(&db, input).expect("accepted scoped run");

    let prompt = AgentRunRepository::prompt_input_for_session(&db, &session_key, "run-1")
        .expect("prompt input")
        .expect("run exists");
    assert_eq!(prompt.retrieval_scope.paths, vec!["notes/roadmap.md"]);
    db.with_read_conn(|conn| {
        let stored: (String, String) = conn.query_row(
            "SELECT context_scope_json, display_mentions_json
             FROM session_messages WHERE session_id = ?1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&stored.0)?["requiredTags"][0],
            "project"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&stored.1)?[0]["label"],
            "路线图"
        );
        Ok(())
    })
    .expect("immutable turn inputs persisted");
}

#[test]
fn prompt_input_treats_legacy_empty_array_scope_as_an_empty_boundary() {
    let (db, session_id, session_key) = setup();
    AgentRunRepository::accept(&db, accept_input(session_id, session_key.clone()))
        .expect("accepted run");
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE session_messages SET context_scope_json = '[]' WHERE session_id = ?1",
            [session_id],
        )?;
        Ok(())
    })
    .expect("simulate migration default");

    let prompt = AgentRunRepository::prompt_input_for_session(&db, &session_key, "run-1")
        .expect("legacy scope must remain readable")
        .expect("run exists");
    assert!(prompt.retrieval_scope.paths.is_empty());
    assert!(prompt.retrieval_scope.path_prefixes.is_empty());
    assert!(prompt.retrieval_scope.required_tags.is_empty());
}

#[test]
fn policy_request_rebuilds_only_persisted_envelope_and_reference_paths() {
    let (db, session_id, session_key) = setup();
    AgentRunRepository::accept(&db, accept_input(session_id, session_key.clone()))
        .expect("accepted run");

    let request = AgentRunRepository::policy_request_for_session(&db, &session_key, "run-1")
        .expect("read policy request")
        .expect("run exists");

    assert_eq!(request.envelope, envelope());
    assert_eq!(request.explicit_reference_paths, vec!["notes/roadmap.md"]);
    assert!(request.requested_capabilities.is_empty());
}
#[test]
fn accept_is_idempotent_for_client_request_id_without_duplicate_message_or_event() {
    let (db, session_id, session_key) = setup();
    let first = AgentRunRepository::accept(&db, accept_input(session_id, session_key.clone()))
        .expect("first accepted run");
    let mut retry = accept_input(session_id, session_key);
    retry.run_id = "must-not-be-used".to_string();
    retry.turn_id = "must-not-be-used".to_string();
    retry.message = "重试不能新增第二条用户消息".to_string();
    let second = AgentRunRepository::accept(&db, retry).expect("idempotent retry");

    assert_eq!(second, first);
    db.with_read_conn(|conn| {
        let messages: i64 = conn.query_row(
            "SELECT COUNT(*) FROM session_messages WHERE session_id = ?1",
            [session_id],
            |row| row.get(0),
        )?;
        let events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM agent_run_events WHERE run_id = 'run-1'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(messages, 1);
        assert_eq!(events, 1);
        Ok(())
    })
    .expect("idempotency facts");
}

#[test]
fn web_retry_reuses_the_original_turn_without_duplicate_user_message() {
    let (db, session_id, session_key) = setup();
    let mut input = accept_input(session_id, session_key.clone());
    input.envelope.freshness = Freshness::Online;
    input.envelope.effort = Effort::ToolLoop;
    AgentRunRepository::accept(&db, input).expect("accepted source run");
    let preparing = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-1".into(),
            state_version: 0,
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Preparing,
                stage: "Preparing".into(),
            },
        },
    )
    .expect("preparing");
    let running = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-1".into(),
            state_version: preparing.state_version(),
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Running,
                stage: "Running".into(),
            },
        },
    )
    .expect("running");
    AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-1".into(),
            state_version: running.state_version(),
            event_type: RunEventType::WebVerificationFailed,
            payload: RunEventPayload::WebVerificationFailed {
                code: super::run_contract::SafeRunErrorCode::WebProviderTimeout,
                failure_reason: super::run_contract::WebEvidenceFailureReason::ProviderTimeout,
                retryable: true,
                attempt_count: 4,
                duration_bucket: "budget_exhausted".into(),
                diagnostic_id: "run-1".into(),
            },
        },
    )
    .expect("safe diagnostic");
    AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-1".into(),
            state_version: running.state_version(),
            event_type: RunEventType::Failed,
            payload: RunEventPayload::Failed {
                code: super::run_contract::SafeRunErrorCode::WebProviderTimeout,
                message: "Timed out".into(),
            },
        },
    )
    .expect("terminal source");

    let retry = AgentRunRepository::accept_web_retry(
        &db,
        RetryRunInput {
            session_key: session_key.clone(),
            source_run_id: "run-1".into(),
            client_request_id: "retry-request-1".into(),
            run_id: "run-2".into(),
        },
    )
    .expect("accepted retry");
    assert_eq!(retry.turn_id, "turn-1");
    db.with_read_conn(|conn| {
        let messages: i64 = conn.query_row(
            "SELECT COUNT(*) FROM session_messages WHERE session_id = ?1",
            [session_id],
            |row| row.get(0),
        )?;
        let retry_turn: String = conn.query_row(
            "SELECT turn_id FROM agent_runs WHERE run_id = 'run-2'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(messages, 1);
        assert_eq!(retry_turn, "turn-1");
        Ok(())
    })
    .expect("retry persistence facts");
}

#[test]
fn normal_sqlite_repository_refuses_classified_run_before_any_write() {
    let (db, session_id, session_key) = setup();
    let mut input = accept_input(session_id, session_key);
    input.envelope.security_domain = SecurityDomain::Classified;

    let result = AgentRunRepository::accept(&db, input);
    assert_eq!(
        result.unwrap_err().to_string(),
        "agent_run_classified_domain_not_supported"
    );
    db.with_read_conn(|conn| {
        let runs: i64 = conn.query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row.get(0))?;
        let messages: i64 = conn.query_row("SELECT COUNT(*) FROM session_messages", [], |row| {
            row.get(0)
        })?;
        assert_eq!(runs, 0);
        assert_eq!(messages, 0);
        Ok(())
    })
    .expect("classified input must not write normal SQLite");
}

#[test]
fn accept_persists_explicit_action_with_its_run() {
    let (db, session_id, session_key) = setup();
    let mut input = accept_input(session_id, session_key);
    input.explicit_action = Some(super::run_contract::ExplicitAction {
        effect: Effect::Draft,
        target: Some(super::run_contract::ExplicitTarget {
            reference_id: "ref-1".to_string(),
            content_hash: "content-hash".to_string(),
        }),
        selection_snapshot: Some(super::run_contract::SelectionSnapshot {
            reference_id: "ref-1".to_string(),
            content_hash: "content-hash".to_string(),
            utf8_range: SourceSpan { start: 4, end: 12 },
            text: "只允许本次 Run 使用的明确选区".to_string(),
        }),
    });

    AgentRunRepository::accept(&db, input).expect("accepted run");
    db.with_read_conn(|conn| {
        let action_json: String = conn.query_row(
            "SELECT explicit_action_json FROM agent_runs WHERE run_id = 'run-1'",
            [],
            |row| row.get(0),
        )?;
        let action: super::run_contract::ExplicitAction = serde_json::from_str(&action_json)?;
        assert_eq!(action.effect, Effect::Draft);
        assert_eq!(
            action
                .selection_snapshot
                .as_ref()
                .map(|snapshot| snapshot.text.as_str()),
            Some("")
        );
        Ok(())
    })
    .expect("action persisted");
}
#[test]
fn failed_accept_rolls_back_message_run_and_event_together() {
    let (db, session_id, session_key) = setup();
    db.with_conn(|conn| {
        conn.execute(
            "CREATE TRIGGER reject_accepted_event
             BEFORE INSERT ON agent_run_events
             WHEN NEW.event_type = 'accepted'
             BEGIN SELECT RAISE(ABORT, 'forced failure'); END",
            [],
        )?;
        Ok(())
    })
    .expect("failure trigger");

    assert!(AgentRunRepository::accept(&db, accept_input(session_id, session_key)).is_err());
    db.with_read_conn(|conn| {
        for table in ["session_messages", "agent_runs", "agent_run_events"] {
            let count: i64 =
                conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })?;
            assert_eq!(count, 0, "{table} must roll back");
        }
        Ok(())
    })
    .expect("rolled back facts");
}

#[test]
fn append_event_assigns_strict_sequence_and_safe_reader_replays_in_order() {
    let (db, session_id, session_key) = setup();
    AgentRunRepository::accept(&db, accept_input(session_id, session_key)).expect("accepted run");

    let persisted = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-1".to_string(),
            state_version: 0,
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Preparing,
                stage: "正在准备".to_string(),
            },
        },
    )
    .expect("append event");
    let persisted_json = serde_json::to_value(&persisted).expect("serialized event");
    let expected = AssistantRunEvent::new(
        "run-1",
        2,
        1,
        RunEventType::StageChanged,
        persisted_json["timestamp"]
            .as_str()
            .expect("event timestamp")
            .to_string(),
        RunEventPayload::StageChanged {
            state: RunState::Preparing,
            stage: "正在准备".to_string(),
        },
    )
    .expect("expected event");
    assert_eq!(persisted, expected);

    let snapshot = AgentRunRepository::get(&db, "run-1")
        .expect("get result")
        .expect("run exists");
    assert_eq!(snapshot.run.state, RunState::Preparing);
    assert_eq!(snapshot.run.state_version, 1);
    assert_eq!(snapshot.events.len(), 2);
    assert_eq!(serde_json::to_value(&snapshot.events[0]).unwrap()["seq"], 1);
    assert_eq!(serde_json::to_value(&snapshot.events[1]).unwrap()["seq"], 2);

    let stale_event = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-1".to_string(),
            state_version: 0,
            event_type: RunEventType::ContentDelta,
            payload: RunEventPayload::ContentDelta {
                delta: "过期写入不得追加".to_string(),
            },
        },
    );
    assert_eq!(
        stale_event.unwrap_err().to_string(),
        "agent_run_state_version_conflict"
    );
}

#[test]
fn repository_refuses_second_completed_event_for_terminal_run() {
    let (db, session_id, session_key) = setup();
    AgentRunRepository::accept(&db, accept_input(session_id, session_key)).expect("accepted run");
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE agent_runs SET status = 'completed', state_version = 3 WHERE run_id = 'run-1'",
            [],
        )?;
        Ok(())
    })
    .expect("terminal fixture");

    let result = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-1".to_string(),
            state_version: 3,
            event_type: RunEventType::Completed,
            payload: RunEventPayload::Completed { message_id: None },
        },
    );
    assert_eq!(result.unwrap_err().to_string(), "agent_run_terminal_state");

    let later_event = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-1".to_string(),
            state_version: 3,
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Running,
                stage: "终态后不得继续".to_string(),
            },
        },
    );
    assert_eq!(
        later_event.unwrap_err().to_string(),
        "agent_run_terminal_state"
    );
}

#[test]
fn generic_event_append_cannot_complete_a_run_without_final_message_transaction() {
    let (db, session_id, session_key) = setup();
    AgentRunRepository::accept(&db, accept_input(session_id, session_key)).expect("accepted run");
    for (state_version, state) in [(0, RunState::Preparing), (1, RunState::Running)] {
        AgentRunRepository::append_event(
            &db,
            AppendRunEventInput {
                run_id: "run-1".to_string(),
                state_version,
                event_type: RunEventType::StageChanged,
                payload: RunEventPayload::StageChanged {
                    state,
                    stage: "推进运行状态".to_string(),
                },
            },
        )
        .expect("advance state");
    }

    let completed = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-1".to_string(),
            state_version: 2,
            event_type: RunEventType::Completed,
            payload: RunEventPayload::Completed {
                message_id: Some("message-1".to_string()),
            },
        },
    );
    assert_eq!(
        completed.unwrap_err().to_string(),
        "agent_run_finalization_required"
    );
    assert_eq!(
        AgentRunRepository::get(&db, "run-1")
            .expect("get run")
            .expect("run exists")
            .run
            .state,
        RunState::Running
    );
}

#[test]
fn finalization_writes_assistant_message_run_terminal_state_and_event_atomically() {
    let (db, session_id, session_key) = setup();
    AgentRunRepository::accept(&db, accept_input(session_id, session_key)).expect("accepted run");
    for (state_version, state) in [(0, RunState::Preparing), (1, RunState::Running)] {
        AgentRunRepository::append_event(
            &db,
            AppendRunEventInput {
                run_id: "run-1".to_string(),
                state_version,
                event_type: RunEventType::StageChanged,
                payload: RunEventPayload::StageChanged {
                    state,
                    stage: "推进运行状态".to_string(),
                },
            },
        )
        .expect("advance state");
    }

    let message_id = AgentRunRepository::finalize(
        &db,
        FinalizeRunInput {
            run_id: "run-1".to_string(),
            state_version: 2,
            content: "这是唯一的最终答复。".to_string(),
            evidence_ids: vec![],
            citation_map: serde_json::json!({}),
        },
    )
    .expect("finalize run");

    let replay = AgentRunRepository::get(&db, "run-1")
        .expect("get run")
        .expect("run exists");
    assert_eq!(replay.run.state, RunState::Completed);
    assert_eq!(replay.run.state_version, 3);
    assert_eq!(
        replay.run.final_message_id.as_deref(),
        Some(message_id.as_str())
    );
    assert_eq!(replay.events.len(), 4);
    db.with_read_conn(|conn| {
        let messages: i64 = conn.query_row(
            "SELECT COUNT(*) FROM session_messages WHERE session_id = ?1 AND role = 'assistant'",
            [session_id],
            |row| row.get(0),
        )?;
        assert_eq!(messages, 1);
        Ok(())
    })
    .expect("final message persisted");
}

#[test]
fn tool_call_identifiers_are_unique_and_must_start_before_completion() {
    let (db, session_id, session_key) = setup();
    AgentRunRepository::accept(&db, accept_input(session_id, session_key)).expect("accepted run");
    AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-1".to_string(),
            state_version: 0,
            event_type: RunEventType::ToolStarted,
            payload: RunEventPayload::ToolStarted {
                capability: "vault.search".to_string(),
                tool_call_id: "tool-call-1".to_string(),
            },
        },
    )
    .expect("first tool start");

    let duplicate = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-1".to_string(),
            state_version: 0,
            event_type: RunEventType::ToolStarted,
            payload: RunEventPayload::ToolStarted {
                capability: "vault.search".to_string(),
                tool_call_id: "tool-call-1".to_string(),
            },
        },
    );
    assert_eq!(
        duplicate.unwrap_err().to_string(),
        "agent_run_duplicate_tool_call_id"
    );

    let unknown_completion = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-1".to_string(),
            state_version: 0,
            event_type: RunEventType::ToolCompleted,
            payload: RunEventPayload::ToolCompleted {
                capability: "vault.search".to_string(),
                tool_call_id: "tool-call-unknown".to_string(),
                summary: "不应完成未开始的调用".to_string(),
            },
        },
    );
    assert_eq!(
        unknown_completion.unwrap_err().to_string(),
        "agent_run_unknown_tool_call_id"
    );
}

#[test]
fn frozen_confirmation_is_bound_to_its_run_hash_and_single_consumption() {
    let (db, session_id, session_key) = setup();
    AgentRunRepository::accept(&db, accept_input(session_id, session_key)).expect("accepted run");
    let plan = FrozenChangePlan::freeze(FrozenChangePlanInput {
        confirmation_id: "confirmation-1".to_string(),
        run_id: "run-1".to_string(),
        session_id,
        request_id: "client-request-1".to_string(),
        tool_call_id: "tool-1".to_string(),
        vault_id: "vault-1".to_string(),
        relative_paths: vec!["notes/a.md".to_string()],
        operation: "note.apply_patch".to_string(),
        base_content_hashes: vec![("notes/a.md".to_string(), "hash-a".to_string())],
        change: serde_json::json!({ "replacement": "new" }),
        affected_file_count: 1,
        rollback_summary: "可撤销".to_string(),
        expires_at_unix_ms: i64::MAX,
    })
    .expect("plan");

    AgentRunRepository::save_frozen_confirmation(&db, &plan).expect("save plan");
    AgentRunRepository::consume_frozen_confirmation(
        &db,
        "run-1",
        "confirmation-1",
        plan.plan_hash(),
        0,
    )
    .expect("consume exact plan");
    assert_eq!(
        AgentRunRepository::consume_frozen_confirmation(
            &db,
            "run-1",
            "confirmation-1",
            plan.plan_hash(),
            0,
        )
        .unwrap_err()
        .to_string(),
        "agent_run_confirmation_expired"
    );
}

#[test]
fn atomic_confirmation_request_binds_the_pending_plan_to_awaiting_state() {
    let (db, session_id, session_key) = setup();
    AgentRunRepository::accept(&db, accept_input(session_id, session_key.clone()))
        .expect("accepted run");
    let preparing = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-1".to_string(),
            state_version: 0,
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Preparing,
                stage: "preparing".to_string(),
            },
        },
    )
    .expect("preparing");
    let running = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-1".to_string(),
            state_version: preparing.state_version(),
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Running,
                stage: "running".to_string(),
            },
        },
    )
    .expect("running");
    let started = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-1".to_string(),
            state_version: running.state_version(),
            event_type: RunEventType::ToolStarted,
            payload: RunEventPayload::ToolStarted {
                capability: "memory_write".to_string(),
                tool_call_id: "tool-1".to_string(),
            },
        },
    )
    .expect("tool started");
    let plan = FrozenChangePlan::freeze(FrozenChangePlanInput {
        confirmation_id: "confirmation-atomic".to_string(),
        run_id: "run-1".to_string(),
        session_id,
        request_id: "request-1".to_string(),
        tool_call_id: "tool-1".to_string(),
        vault_id: "vault-1".to_string(),
        relative_paths: vec!["application://memory/profile".to_string()],
        operation: "memory_write".to_string(),
        base_content_hashes: vec![],
        change: serde_json::json!({ "key": "profile", "content": "approved" }),
        affected_file_count: 1,
        rollback_summary: "can update later".to_string(),
        expires_at_unix_ms: i64::MAX,
    })
    .expect("frozen plan");

    let awaiting = AgentRunRepository::request_frozen_confirmation(
        &db,
        &plan,
        started.state_version(),
        "Awaiting confirmation: memory_write affects 1 target",
    )
    .expect("atomic confirmation request");
    assert_eq!(awaiting.state_version(), started.state_version() + 1);
    let snapshot = AgentRunRepository::get_for_session(&db, &session_key, "run-1")
        .expect("snapshot")
        .expect("run");
    assert_eq!(snapshot.run.state, RunState::AwaitingConfirmation);

    let approval = AgentRunRepository::approve_frozen_confirmation(
        &db,
        &session_key,
        "run-1",
        plan.confirmation_id(),
        plan.plan_hash(),
        awaiting.state_version(),
        0,
    )
    .expect("approve exact plan");
    assert!(matches!(
        approval,
        super::agent_run_repository::FrozenConfirmationApproval::Resumed(_)
    ));
    let consumed = AgentRunRepository::consumed_frozen_confirmation_for_session(
        &db,
        &session_key,
        "run-1",
        plan.confirmation_id(),
    )
    .expect("consumed plan");
    let restored = FrozenChangePlan::from_persisted_plan_json(&consumed.plan_json)
        .expect("restore exact plan");
    assert_eq!(restored.plan_hash(), consumed.plan_hash);
    assert_eq!(restored.change()["content"], "approved");
}

#[test]
fn frozen_confirmation_cannot_be_saved_for_a_different_session() {
    let (db, session_id, session_key) = setup();
    AgentRunRepository::accept(&db, accept_input(session_id, session_key)).expect("accepted run");
    let mismatched_session_plan = FrozenChangePlan::freeze(FrozenChangePlanInput {
        confirmation_id: "confirmation-wrong-session".to_string(),
        run_id: "run-1".to_string(),
        session_id: session_id + 1,
        request_id: "client-request-1".to_string(),
        tool_call_id: "tool-1".to_string(),
        vault_id: "vault-1".to_string(),
        relative_paths: vec!["notes/a.md".to_string()],
        operation: "note.apply_patch".to_string(),
        base_content_hashes: vec![("notes/a.md".to_string(), "hash-a".to_string())],
        change: serde_json::json!({ "replacement": "new" }),
        affected_file_count: 1,
        rollback_summary: "可撤销".to_string(),
        expires_at_unix_ms: i64::MAX,
    })
    .expect("plan");

    assert_eq!(
        AgentRunRepository::save_frozen_confirmation(&db, &mismatched_session_plan)
            .expect_err("different session must not attach a plan")
            .to_string(),
        "agent_run_session_not_found"
    );
}

#[test]
fn durable_checkpoint_persists_only_validated_schema_and_evidence_ids() {
    let (db, session_id, session_key) = setup();
    let mut input = accept_input(session_id, session_key);
    input.envelope.effort = Effort::Durable;
    AgentRunRepository::accept(&db, input).expect("accepted durable run");
    let evidence = AgentEvidenceRepository::register_local(
        &db,
        LocalEvidenceInput {
            session_id,
            run_id: "run-1".to_string(),
            message_seq_first: 1,
            material_role: MaterialRole::Reference,
            title: "可恢复步骤的依据".to_string(),
            source_path: "notes/checkpoint.md".to_string(),
            source_span_start: 0,
            source_span_end: 12,
            heading_path: None,
            content_hash: "checkpoint-evidence-hash".to_string(),
            retrieval_reason: Some("explicit_reference".to_string()),
            score: None,
        },
    )
    .expect("session-owned evidence");

    AgentRunRepository::append_checkpoint_step(
        &db,
        AppendRunCheckpointInput {
            run_id: "run-1".to_string(),
            state_version: 0,
            kind: "official_drafting".to_string(),
            status: "paused_safe".to_string(),
            input_summary: "已完成提纲".to_string(),
            output_summary: "等待继续起草".to_string(),
            checkpoint: serde_json::json!({
                "schemaVersion": 1,
                "executor": "official_drafting",
                "goalSummary": "起草会议通知",
                "completedStepIds": ["outline"],
                "pendingStepId": "draft",
                "evidenceIds": [evidence.evidence_id],
                "requiredCapabilities": ["vault.search", "note.propose_patch"],
                "requiredPermissions": [],
                "pendingConfirmationId": null,
                "budgetRemaining": { "modelCalls": 2, "toolCalls": 8 },
                "safeState": { "outlineReady": true }
            }),
        },
    )
    .expect("persist safe checkpoint");

    db.with_read_conn(|conn| {
        let (step_seq, checkpoint_json, evidence_refs_json): (i64, String, String) = conn
            .query_row(
                "SELECT step_seq, resume_state_json, evidence_refs_json
             FROM agent_run_steps WHERE run_id = 'run-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
        assert_eq!(step_seq, 1);
        assert!(checkpoint_json.contains("official_drafting"));
        assert_eq!(evidence_refs_json, format!("[{}]", evidence.evidence_id));
        Ok(())
    })
    .expect("safe checkpoint row");

    let unsafe_checkpoint = AgentRunRepository::append_checkpoint_step(
        &db,
        AppendRunCheckpointInput {
            run_id: "run-1".to_string(),
            state_version: 0,
            kind: "official_drafting".to_string(),
            status: "paused_safe".to_string(),
            input_summary: "".to_string(),
            output_summary: "".to_string(),
            checkpoint: serde_json::json!({
                "schemaVersion": 1,
                "executor": "official_drafting",
                "goalSummary": "起草会议通知",
                "completedStepIds": [],
                "pendingStepId": null,
                "evidenceIds": [],
                "requiredCapabilities": [],
                "requiredPermissions": [],
                "pendingConfirmationId": null,
                "budgetRemaining": { "modelCalls": 2, "toolCalls": 8 },
                "safeState": { "apiKey": "不得持久化" }
            }),
        },
    );
    assert_eq!(
        unsafe_checkpoint.unwrap_err().to_string(),
        "agent_run_checkpoint_unsafe_key"
    );

    let missing_evidence = AgentRunRepository::append_checkpoint_step(
        &db,
        AppendRunCheckpointInput {
            run_id: "run-1".to_string(),
            state_version: 0,
            kind: "official_drafting".to_string(),
            status: "paused_safe".to_string(),
            input_summary: "".to_string(),
            output_summary: "".to_string(),
            checkpoint: serde_json::json!({
                "schemaVersion": 1,
                "executor": "official_drafting",
                "goalSummary": "起草会议通知",
                "completedStepIds": [],
                "pendingStepId": null,
                "evidenceIds": [999],
                "requiredCapabilities": [],
                "requiredPermissions": [],
                "pendingConfirmationId": null,
                "budgetRemaining": { "modelCalls": 2, "toolCalls": 8 },
                "safeState": {}
            }),
        },
    );
    assert_eq!(
        missing_evidence.unwrap_err().to_string(),
        "agent_run_evidence_not_found"
    );

    let secret_summary = AgentRunRepository::append_checkpoint_step(
        &db,
        AppendRunCheckpointInput {
            run_id: "run-1".to_string(),
            state_version: 0,
            kind: "official_drafting".to_string(),
            status: "paused_safe".to_string(),
            input_summary: "Authorization: Bearer token-value".to_string(),
            output_summary: "等待继续".to_string(),
            checkpoint: serde_json::json!({
                "schemaVersion": 1,
                "executor": "official_drafting",
                "goalSummary": "起草会议通知",
                "completedStepIds": [],
                "pendingStepId": null,
                "evidenceIds": [],
                "requiredCapabilities": [],
                "requiredPermissions": [],
                "pendingConfirmationId": null,
                "budgetRemaining": { "modelCalls": 2, "toolCalls": 8 },
                "safeState": {}
            }),
        },
    );
    assert_eq!(
        secret_summary.unwrap_err().to_string(),
        "agent_run_invalid_checkpoint_step"
    );
}

#[test]
fn non_durable_active_run_cannot_persist_checkpoint() {
    let (db, session_id, session_key) = setup();
    AgentRunRepository::accept(&db, accept_input(session_id, session_key)).expect("accepted run");

    let result = AgentRunRepository::append_checkpoint_step(
        &db,
        AppendRunCheckpointInput {
            run_id: "run-1".to_string(),
            state_version: 0,
            kind: "direct_answer".to_string(),
            status: "running".to_string(),
            input_summary: "".to_string(),
            output_summary: "".to_string(),
            checkpoint: serde_json::json!({
                "schemaVersion": 1,
                "executor": "direct_answer",
                "goalSummary": "回答问题",
                "completedStepIds": [],
                "pendingStepId": null,
                "evidenceIds": [],
                "requiredCapabilities": [],
                "requiredPermissions": [],
                "pendingConfirmationId": null,
                "budgetRemaining": { "modelCalls": 1, "toolCalls": 0 },
                "safeState": {}
            }),
        },
    );
    assert_eq!(
        result.unwrap_err().to_string(),
        "agent_run_checkpoint_not_durable"
    );
}
