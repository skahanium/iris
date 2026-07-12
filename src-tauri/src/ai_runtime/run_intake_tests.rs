use super::run_contract::{
    AssistantRunControlRequest, AssistantRunStartRequest, ContextMode, Effect, Freshness,
    RunControlAction, RunEventPayload, RunEventType, RunState, SecurityDomain,
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
        message: "请概述这份资料的要点".to_string(),
        content_parts: None,
        explicit_references: vec![],
        explicit_action: None,
        web_enabled: false,
        security_domain: SecurityDomain::Normal,
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
        let (scene, note_path): (String, Option<String>) = conn.query_row(
            "SELECT scene, note_path FROM sessions WHERE session_key = ?1",
            [&accepted.session.session_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert!(scene.is_empty());
        assert!(note_path.is_none());
        let tasks: i64 =
            conn.query_row("SELECT COUNT(*) FROM agent_tasks", [], |row| row.get(0))?;
        let traces: i64 = conn.query_row("SELECT COUNT(*) FROM ai_traces", [], |row| row.get(0))?;
        assert_eq!(tasks, 0);
        assert_eq!(traces, 0);
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
        let tasks: i64 =
            conn.query_row("SELECT COUNT(*) FROM agent_tasks", [], |row| row.get(0))?;
        let traces: i64 = conn.query_row("SELECT COUNT(*) FROM ai_traces", [], |row| row.get(0))?;
        assert_eq!(tasks, 0);
        assert_eq!(traces, 0);
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
fn minimal_intake_resolves_a_direct_offline_answer_envelope() {
    let db = Database::open_in_memory().expect("database");

    let resolved = RunIntake::resolve_minimal_envelope(&request()).expect("minimal envelope");

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

fn accepted_run_awaiting_frozen_change_confirmation(
) -> (Database, super::run_contract::AssistantRunAccepted, String, u64) {
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
