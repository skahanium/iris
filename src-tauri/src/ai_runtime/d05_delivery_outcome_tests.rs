//! D05 wave 4: C25 delivery expression (K15 task outcome).
//! Not D05 / C25 / K15 / N11 / Q14 / N04 acceptance; no DiffView and no new columns.

use std::sync::{Arc, Mutex};

use super::agent_run_repository::{
    AgentRunRepository, AppendRunCheckpointInput, AppendRunEventInput, DurableApplyCheckpoint,
    DurableApplyCheckpointStage,
};
use super::delivery_outcome::{classify_task_outcome, DeliveryFacts, TaskOutcome};
use super::run_contract::{
    AssistantRunStartRequest, AssistantTurnDraft, Effect, ExplicitAction, ExplicitTarget,
    RunEventPayload, RunEventType, RunState, SecurityDomain,
};
use super::run_engine::{finalize_host_authored_limitation, RunEngine, RunEventSink};
use super::run_intake::RunIntake;
use crate::app::AppState;
use crate::cas::hash::content_hash_str;
use crate::error::AppResult;

const NOTE_BODY: &str = "don't use 3.14";
const NOTE_PATH: &str = "notes/a.md";
const LIMITATION_BODY: &str = "当前无法完成核实。don't use 3.14";

#[test]
fn c25_host_limitation_is_blocked_without_reading_body() {
    let outcome = classify_task_outcome(&DeliveryFacts {
        host_authored_limitation: true,
        change_ops_complete: None,
    });
    assert_eq!(outcome, TaskOutcome::Blocked);
}

#[test]
fn c25_host_limitation_wins_over_complete_change_ops() {
    let outcome = classify_task_outcome(&DeliveryFacts {
        host_authored_limitation: true,
        change_ops_complete: Some(true),
    });
    assert_eq!(outcome, TaskOutcome::Blocked);
}

#[test]
fn c25_partial_change_ops_are_partial() {
    let outcome = classify_task_outcome(&DeliveryFacts {
        host_authored_limitation: false,
        change_ops_complete: Some(false),
    });
    assert_eq!(outcome, TaskOutcome::Partial);
}

#[test]
fn c25_full_change_ops_are_completed() {
    let outcome = classify_task_outcome(&DeliveryFacts {
        host_authored_limitation: false,
        change_ops_complete: Some(true),
    });
    assert_eq!(outcome, TaskOutcome::Completed);
}

#[test]
fn c25_model_answer_without_change_set_is_completed() {
    let outcome = classify_task_outcome(&DeliveryFacts {
        host_authored_limitation: false,
        change_ops_complete: None,
    });
    assert_eq!(outcome, TaskOutcome::Completed);
}

#[test]
fn c25_historical_completed_json_without_task_outcome_deserializes() {
    let payload: RunEventPayload = serde_json::from_value(serde_json::json!({
        "kind": "completed",
        "messageId": "msg-historical",
    }))
    .expect("historical completed payload must remain readable");
    match payload {
        RunEventPayload::Completed { task_outcome, .. } => {
            assert_eq!(task_outcome, None);
        }
        other => panic!("expected completed payload, got {other:?}"),
    }
}

#[test]
fn c25_new_completed_json_has_task_outcome_and_omits_note_body() {
    let payload = RunEventPayload::Completed {
        message_id: Some("msg-1".into()),
        source_summary: Vec::new(),
        task_outcome: Some(TaskOutcome::Blocked),
    };
    let json = serde_json::to_value(&payload).expect("serialize completed");
    assert_eq!(json["kind"], "completed");
    assert_eq!(json["taskOutcome"], "blocked");
    let blob = json.to_string();
    assert!(
        !blob.contains("don't") && !blob.contains(NOTE_BODY),
        "completed payload must not carry note body: {blob}"
    );
}

#[derive(Default)]
struct RecordingSink {
    events: Mutex<Vec<serde_json::Value>>,
}

impl RunEventSink for RecordingSink {
    fn emit(&self, event: &super::run_contract::AssistantRunEvent) -> AppResult<()> {
        self.events
            .lock()
            .expect("recording sink lock")
            .push(serde_json::to_value(event)?);
        Ok(())
    }
}

struct RunningFixture {
    _directory: tempfile::TempDir,
    state: Arc<AppState>,
    accepted: super::run_contract::AssistantRunAccepted,
    state_version: u64,
    sink: RecordingSink,
}

fn start_running(apply: bool) -> RunningFixture {
    let directory = tempfile::tempdir().expect("tempdir");
    let vault = directory.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes dir");
    std::fs::write(vault.join(NOTE_PATH), NOTE_BODY).expect("write note");
    let state = AppState::new(directory.path().join("data")).expect("app state");
    state.set_vault(vault.clone()).expect("activate vault");
    let mut start = AssistantRunStartRequest {
        client_request_id: format!("d05-c25-{}", uuid::Uuid::new_v4()),
        session: None,
        turn: AssistantTurnDraft {
            message: if apply {
                "将已确认的修改应用到笔记".into()
            } else {
                "请核实这条笔记的出处".into()
            },
            content_parts: None,
            explicit_references: vec![crate::ai_types::ContextReferenceWire {
                id: "target-note".into(),
                kind: crate::ai_types::ContextReferenceKind::Note,
                file_path: Some(NOTE_PATH.into()),
                content_hash: Some(content_hash_str(NOTE_BODY)),
                utf8_range: None,
                editor_range: None,
                excerpt: String::new(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            }],
            retrieval_scope: Default::default(),
            display_mentions: vec![],
        },
        explicit_action: None,
        web_enabled: false,
        model_override: None,
        external_tool_grants: Vec::new(),
        security_domain: SecurityDomain::Normal,
        classified_context_ref: None,
    };
    if apply {
        start.explicit_action = Some(ExplicitAction {
            effect: Effect::Apply,
            target: Some(ExplicitTarget {
                reference_id: "target-note".into(),
                content_hash: content_hash_str(NOTE_BODY),
            }),
            selection_snapshot: None,
        });
    }
    let accepted = RunIntake::start(&state.db, start).expect("accept run");
    let sink = RecordingSink::default();
    let preparing =
        RunEngine::mark_preparing_with_sink(&state.db, &accepted.session, &accepted.run_id, &sink)
            .expect("preparing");
    let running = AgentRunRepository::append_event(
        &state.db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: preparing,
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Running,
                stage: "测试交付表达".into(),
                stage_code: None,
            },
        },
    )
    .expect("running");
    RunningFixture {
        _directory: directory,
        state,
        accepted,
        state_version: running.state_version(),
        sink,
    }
}

impl RunningFixture {
    fn run_status(&self) -> String {
        self.state
            .db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT status FROM agent_runs WHERE run_id = ?1",
                    [&self.accepted.run_id],
                    |row| row.get(0),
                )?)
            })
            .expect("run status")
    }

    fn completed_event_json(&self) -> serde_json::Value {
        self.sink
            .events
            .lock()
            .expect("recording sink lock")
            .iter()
            .find(|event| {
                event.get("type").and_then(serde_json::Value::as_str) == Some("completed")
            })
            .cloned()
            .expect("completed event")
    }

    fn persist_checkpoint(&self, stage: DurableApplyCheckpointStage, next_index: usize) {
        AgentRunRepository::append_checkpoint_step(
            &self.state.db,
            AppendRunCheckpointInput {
                run_id: self.accepted.run_id.clone(),
                state_version: self.state_version,
                checkpoint: DurableApplyCheckpoint::new_change_set(
                    format!("confirmation-{}", self.accepted.run_id),
                    format!("plan-{}", self.accepted.run_id),
                    stage,
                    vec![content_hash_str(NOTE_BODY)],
                    vec![content_hash_str("please do not use 3.14")],
                    next_index,
                    1,
                    Vec::new(),
                )
                .expect("checkpoint"),
            },
        )
        .expect("persist checkpoint");
    }
}

#[test]
fn c25_host_authored_limitation_emits_blocked_and_keeps_run_completed() {
    let fixture = start_running(false);
    finalize_host_authored_limitation(
        &fixture.state.db,
        &fixture.accepted.session,
        &fixture.accepted.run_id,
        fixture.state_version,
        LIMITATION_BODY.into(),
        &fixture.sink,
    )
    .expect("host limitation must publish");
    assert_eq!(fixture.run_status(), "completed");
    let event = fixture.completed_event_json();
    assert_eq!(event["payload"]["taskOutcome"], "blocked");
    let blob = event.to_string();
    assert!(
        !blob.contains("don't") && !blob.contains(NOTE_BODY),
        "completed event must not echo note body: {blob}"
    );
}

#[test]
fn c25_partial_change_report_emits_partial() {
    let fixture = start_running(true);
    fixture.persist_checkpoint(DurableApplyCheckpointStage::Approved, 0);
    RunEngine::finalize_confirmed_change_report_with_sink(
        &fixture.state.db,
        &fixture.accepted.session,
        &fixture.accepted.run_id,
        "已执行 0/1 项变更；后续操作未执行。don't use 3.14",
        false,
        &fixture.sink,
    )
    .expect("partial report must publish");
    assert_eq!(fixture.run_status(), "completed");
    let event = fixture.completed_event_json();
    assert_eq!(event["payload"]["taskOutcome"], "partial");
    let blob = event.to_string();
    assert!(
        !blob.contains("don't") && !blob.contains(NOTE_BODY),
        "completed event must not echo note body: {blob}"
    );
}

#[test]
fn c25_complete_change_report_emits_completed() {
    let fixture = start_running(true);
    fixture.persist_checkpoint(DurableApplyCheckpointStage::Approved, 0);
    fixture.persist_checkpoint(DurableApplyCheckpointStage::Dispatching, 0);
    fixture.persist_checkpoint(DurableApplyCheckpointStage::Applied, 1);
    RunEngine::finalize_confirmed_change_report_with_sink(
        &fixture.state.db,
        &fixture.accepted.session,
        &fixture.accepted.run_id,
        "已按确认顺序执行 1/1 项变更。",
        true,
        &fixture.sink,
    )
    .expect("complete report must publish");
    assert_eq!(fixture.run_status(), "completed");
    assert_eq!(
        fixture.completed_event_json()["payload"]["taskOutcome"],
        "completed"
    );
}
