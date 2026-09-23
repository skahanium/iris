//! D05 ten-task campaign: named production-entry traces for waves 1–5.
//! Not D05 / N04 / Q14 / C24 / C25 acceptance; not V05; not the 52-case Q&A matrix.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::agent_run_repository::{AgentRunRepository, AppendRunEventInput};
use super::agent_tool_loop::{ToolLoopExecutor, CONFIRMATION_PENDING_ERROR};
use super::confirmation_diff::{
    preview_pending_confirmation_diff, AssistantRunConfirmationDiffRequest, ConfirmationDiffHunk,
    ConfirmationDiffLineKind,
};
use super::content_preservation::check_format_preservation;
use super::delivery_outcome::{classify_task_outcome, DeliveryFacts, TaskOutcome};
use super::edit_candidate::prepare_edit_candidate;
use super::frozen_change_plan::FrozenChangePlan;
use super::run_context::RunContextAssembler;
use super::run_contract::{
    AssistantRunAccepted, AssistantRunEvent, AssistantRunStartRequest, AssistantTurnDraft,
    CapabilityId, Freshness, RunBudgetPolicy, RunEventPayload, RunEventType, RunState,
    SecurityDomain, WebDecisionReason,
};
use super::run_engine::{finalize_host_authored_limitation, RunEngine, RunEventSink};
use super::run_intake::RunIntake;
use super::run_tool_loop::NormalRunToolExecutor;
use crate::ai_runtime::ToolCall;
use crate::app::AppState;
use crate::cas::hash::content_hash_str;
use crate::error::AppResult;

const NOTE_PATH: &str = "notes/a.md";
const NOTE_BODY: &str = "don't use 3.14";
const POLISH_MESSAGE: &str = "将这段话润色成正式通知。";
const POLISHED_BODY: &str = "please do not use 3.14";
const FORMAT_MESSAGE: &str = "请格式整理这段笔记";
const FORMAT_ORIGINAL: &str = "# Title\n- keep\n\nSee [x](https://a.example)\n";
const FORMAT_CANDIDATE: &str = "## Title\n* keep\n\nSee [x](https://a.example)\n";
const FORMAT_BROKEN: &str = "## Title\n* keep\n\nSee [x](https://b.example)\n";
const T08_UI_V07: &str = "../tests/assistant-run-confirmation-diff.test.tsx";

fn start_request(message: &str, web_enabled: bool) -> AssistantRunStartRequest {
    AssistantRunStartRequest {
        client_request_id: format!("d05-t10-{}", uuid::Uuid::new_v4()),
        session: None,
        turn: AssistantTurnDraft {
            message: message.into(),
            content_parts: None,
            explicit_references: vec![],
            retrieval_scope: Default::default(),
            display_mentions: vec![],
        },
        explicit_action: None,
        web_enabled,
        model_override: None,
        external_tool_grants: Vec::new(),
        security_domain: SecurityDomain::Normal,
        classified_context_ref: None,
    }
}

fn has_capability(envelope: &super::run_contract::ExecutionEnvelope, name: &str) -> bool {
    envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == name)
}

fn line_kinds(hunks: &[ConfirmationDiffHunk]) -> Vec<(char, &str)> {
    hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .map(|line| {
            let marker = match line.kind {
                ConfirmationDiffLineKind::Context => ' ',
                ConfirmationDiffLineKind::Add => '+',
                ConfirmationDiffLineKind::Del => '-',
            };
            (marker, line.text.as_str())
        })
        .collect()
}

#[derive(Default)]
struct RecordingSink {
    events: Mutex<Vec<serde_json::Value>>,
}

impl RunEventSink for RecordingSink {
    fn emit(&self, event: &AssistantRunEvent) -> AppResult<()> {
        self.events
            .lock()
            .expect("recording sink lock")
            .push(serde_json::to_value(event)?);
        Ok(())
    }
}

struct PendingEditFixture {
    _directory: tempfile::TempDir,
    state: Arc<AppState>,
    vault: PathBuf,
    accepted: AssistantRunAccepted,
    context: super::run_context::RunContext,
    sink: Arc<RecordingSink>,
}

fn pending_edit_fixture(message: &str, body: &str) -> PendingEditFixture {
    let directory = tempfile::tempdir().expect("tempdir");
    let vault = directory.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes dir");
    std::fs::write(vault.join(NOTE_PATH), body).expect("write note");
    let state = AppState::new(directory.path().join("data")).expect("app state");
    state.set_vault(vault.clone()).expect("activate vault");
    let vault = state.vault_path().expect("live vault");
    let mut start = start_request(message, false);
    start.turn.explicit_references = vec![crate::ai_types::ContextReferenceWire {
        id: "target-note".into(),
        kind: crate::ai_types::ContextReferenceKind::Note,
        file_path: Some(NOTE_PATH.into()),
        content_hash: Some(content_hash_str(body)),
        utf8_range: None,
        editor_range: None,
        excerpt: String::new(),
        heading_path: None,
        anchor: None,
        stale: false,
        invalid_reason: None,
    }];
    let accepted = RunIntake::start(&state.db, start).expect("accept run");
    let sink = Arc::new(RecordingSink::default());
    let preparing = RunEngine::mark_preparing_with_sink(
        &state.db,
        &accepted.session,
        &accepted.run_id,
        sink.as_ref(),
    )
    .expect("preparing");
    AgentRunRepository::append_event(
        &state.db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: preparing,
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Running,
                stage: "十任务战役".into(),
                stage_code: None,
            },
        },
    )
    .expect("running");
    let context = RunContextAssembler::assemble(
        &state.db,
        Some(&vault),
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .expect("assemble context");
    PendingEditFixture {
        _directory: directory,
        state,
        vault,
        accepted,
        context,
        sink,
    }
}

impl PendingEditFixture {
    fn note_body(&self) -> String {
        std::fs::read_to_string(self.vault.join(NOTE_PATH)).expect("read note")
    }

    fn confirmation_count(&self) -> i64 {
        self.state
            .db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM agent_run_confirmations WHERE run_id = ?1",
                    [&self.accepted.run_id],
                    |row| row.get(0),
                )?)
            })
            .expect("confirmation count")
    }

    fn confirmation_event(&self) -> Option<serde_json::Value> {
        self.sink
            .events
            .lock()
            .expect("recording sink lock")
            .iter()
            .find(|event| {
                event.get("type").and_then(serde_json::Value::as_str)
                    == Some("confirmation_required")
            })
            .cloned()
    }

    fn pending_ids(&self) -> (String, String, String) {
        self.state
            .db
            .with_read_conn(|conn| {
                conn.query_row(
                    "SELECT confirmation_id, plan_hash, plan_json FROM agent_run_confirmations
                     WHERE run_id = ?1 AND status = 'pending'",
                    [&self.accepted.run_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .map_err(Into::into)
            })
            .expect("pending confirmation")
    }

    fn persisted_events_json(&self) -> String {
        let snapshot = AgentRunRepository::get_for_session(
            &self.state.db,
            &self.accepted.session.session_key,
            &self.accepted.run_id,
        )
        .expect("snapshot")
        .expect("run snapshot");
        serde_json::to_string(&snapshot.events).expect("serialize persisted events")
    }

    fn preview_request(
        &self,
        confirmation_id: String,
        plan_hash: String,
    ) -> AssistantRunConfirmationDiffRequest {
        AssistantRunConfirmationDiffRequest {
            session: self.accepted.session.clone(),
            run_id: self.accepted.run_id.clone(),
            confirmation_id,
            plan_hash,
        }
    }

    async fn execute_replace(
        &self,
        original: &str,
        replacement: &str,
    ) -> AppResult<crate::ai_runtime::ToolCallResult> {
        let args = serde_json::json!({
            "target_path": NOTE_PATH,
            "base_content_hash": content_hash_str(original),
            "range": { "start": 0, "end": original.len() },
            "original_text": original,
            "replacement": replacement,
        });
        let executor = NormalRunToolExecutor::new(
            &self.state,
            None,
            &self.accepted,
            &self.context,
            vec![CapabilityId::new("note.apply_patch")],
            RunBudgetPolicy::for_envelope(&self.context.envelope),
            self.sink.as_ref(),
            Vec::new(),
        )
        .with_allowed_tool_names(&["replace_selection".to_string()]);
        executor
            .execute(
                &self.accepted.run_id,
                &ToolCall::new("campaign-replace", "replace_selection", args.to_string()),
                1,
            )
            .await
    }
}

#[test]
fn t01_polish_with_web_enabled_stays_offline() {
    let envelope = RunIntake::resolve_envelope(&start_request(POLISH_MESSAGE, true)).expect("T01");
    assert_eq!(envelope.freshness, Freshness::Offline);
    assert_eq!(envelope.web_reason, WebDecisionReason::LocalTransformation);
    assert!(!has_capability(&envelope, "web.search"));
}

#[test]
fn t02_explicit_verify_keeps_web_search() {
    let envelope = RunIntake::resolve_envelope(&start_request("请核实这篇", true)).expect("T02");
    assert!(has_capability(&envelope, "web.search"));
    assert_ne!(envelope.web_reason, WebDecisionReason::LocalTransformation);
}

#[test]
fn t03_colon_stripped_verify_still_uses_full_text() {
    let envelope = RunIntake::resolve_envelope(&start_request("润色：请核实这篇笔记的出处", true))
        .expect("T03");
    assert!(
        has_capability(&envelope, "web.search"),
        "colon-split 核实 must keep web.search, got {:?}",
        envelope.web_reason
    );
}

#[test]
fn t04_greeting_stays_default_online() {
    let envelope = RunIntake::resolve_envelope(&start_request("你好", true)).expect("T04");
    assert_eq!(envelope.freshness, Freshness::WebPreferred);
    assert_eq!(envelope.web_reason, WebDecisionReason::DefaultOnline);
    assert_ne!(envelope.web_reason, WebDecisionReason::LocalTransformation);
    assert!(has_capability(&envelope, "web.search"));
}

#[tokio::test]
async fn t05_format_preserving_candidate_still_freezes() {
    let report = check_format_preservation(FORMAT_ORIGINAL, FORMAT_CANDIDATE);
    assert!(report.is_proven(), "T05 checker must pass, got {report:?}");
    let fixture = pending_edit_fixture(FORMAT_MESSAGE, FORMAT_ORIGINAL);
    let error = fixture
        .execute_replace(FORMAT_ORIGINAL, FORMAT_CANDIDATE)
        .await
        .expect_err("preserving format still freezes confirmation");
    assert_eq!(error.to_string(), CONFIRMATION_PENDING_ERROR);
    assert_eq!(fixture.confirmation_count(), 1);
    assert_eq!(fixture.note_body(), FORMAT_ORIGINAL);
}

#[tokio::test]
async fn t06_format_rewrite_is_unproven_and_does_not_freeze() {
    let report = check_format_preservation(FORMAT_ORIGINAL, FORMAT_BROKEN);
    assert!(!report.is_proven());
    let fixture = pending_edit_fixture(FORMAT_MESSAGE, FORMAT_ORIGINAL);
    let result = fixture
        .execute_replace(FORMAT_ORIGINAL, FORMAT_BROKEN)
        .await
        .expect("unproven format returns a tool result");
    assert!(!result.success);
    assert_eq!(
        result.error.as_deref(),
        Some("format_preservation_unproven")
    );
    assert_eq!(fixture.confirmation_count(), 0);
    assert_eq!(fixture.note_body(), FORMAT_ORIGINAL);
}

#[tokio::test]
async fn t07_replace_candidate_confirmation_omits_body() {
    let original = NOTE_BODY;
    let candidate = prepare_edit_candidate(
        "replace_selection",
        &serde_json::json!({
            "target_path": NOTE_PATH,
            "base_content_hash": content_hash_str(original),
            "range": { "start": 0, "end": original.len() },
            "original_text": original,
            "replacement": POLISHED_BODY,
        }),
        original,
    )
    .expect("T07 candidate");
    assert_eq!(
        candidate.expected_post_content_hash,
        content_hash_str(POLISHED_BODY)
    );
    let fixture = pending_edit_fixture(POLISH_MESSAGE, NOTE_BODY);
    let error = fixture
        .execute_replace(NOTE_BODY, POLISHED_BODY)
        .await
        .expect_err("polish replace waits for confirmation");
    assert_eq!(error.to_string(), CONFIRMATION_PENDING_ERROR);
    assert_eq!(fixture.note_body(), NOTE_BODY);
    let event = fixture.confirmation_event().expect("confirmation_required");
    let encoded = event.to_string();
    assert!(
        !encoded.contains("don't"),
        "confirmation must not echo note body: {encoded}"
    );
    assert!(event.pointer("/payload/summary").is_some());
    let targets = event
        .pointer("/payload/targets")
        .and_then(serde_json::Value::as_array)
        .expect("targets");
    assert!(!targets.is_empty());
}

#[tokio::test]
async fn t08_pending_diff_is_previewable_and_not_persisted() {
    let fixture = pending_edit_fixture(POLISH_MESSAGE, NOTE_BODY);
    fixture
        .execute_replace(NOTE_BODY, POLISHED_BODY)
        .await
        .expect_err("wait for confirmation");
    let (confirmation_id, plan_hash, _) = fixture.pending_ids();
    let before = fixture.persisted_events_json();
    let preview = preview_pending_confirmation_diff(
        fixture.state.as_ref(),
        fixture.preview_request(confirmation_id, plan_hash),
    )
    .expect("T08 preview");
    assert!(preview.files[0].previewable);
    let kinds = line_kinds(&preview.files[0].hunks);
    assert!(
        kinds.iter().any(|(marker, _)| *marker == '-')
            && kinds.iter().any(|(marker, _)| *marker == '+'),
        "bounded unified diff must include add/del, got {kinds:?}"
    );
    assert_eq!(before, fixture.persisted_events_json());
}

#[test]
fn t08_ui_v07_contract_is_cited_not_rewritten() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(T08_UI_V07);
    let source = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("T08 cites {T08_UI_V07}: {error}");
    });
    assert!(source.contains("展开更改差异"));
    assert!(source.contains("assistantRunConfirmationDiff"));
    assert!(source.contains("loads on mount while expanded"));
    assert!(source.contains("keeps approve disabled until a visible diff has loaded"));
}

#[tokio::test]
async fn t09_disk_drift_unpreviewable_and_does_not_write_candidate() {
    let fixture = pending_edit_fixture(POLISH_MESSAGE, NOTE_BODY);
    fixture
        .execute_replace(NOTE_BODY, POLISHED_BODY)
        .await
        .expect_err("wait for confirmation");
    let (confirmation_id, plan_hash, plan_json) = fixture.pending_ids();
    std::fs::write(fixture.vault.join(NOTE_PATH), "drifted\n").expect("drift");
    let preview = preview_pending_confirmation_diff(
        fixture.state.as_ref(),
        fixture.preview_request(confirmation_id.clone(), plan_hash.clone()),
    )
    .expect("T09 preview");
    assert!(!preview.files[0].previewable);
    assert!(preview.files[0].hunks.is_empty());
    let plan = FrozenChangePlan::from_persisted_plan_json(&plan_json).expect("plan");
    let state_version = RunIntake::get(
        &fixture.state.db,
        &fixture.accepted.session,
        &fixture.accepted.run_id,
    )
    .expect("replay")
    .expect("run")
    .run
    .state_version;
    AgentRunRepository::approve_frozen_confirmation(
        &fixture.state.db,
        &fixture.accepted.session.session_key,
        &fixture.accepted.run_id,
        &confirmation_id,
        &plan_hash,
        state_version,
        0,
    )
    .expect("approve drifted confirmation");
    let results = NormalRunToolExecutor::new(
        &fixture.state,
        None,
        &fixture.accepted,
        &fixture.context,
        vec![CapabilityId::new("note.apply_patch")],
        RunBudgetPolicy::for_envelope(&fixture.context.envelope),
        fixture.sink.as_ref(),
        Vec::new(),
    )
    .execute_confirmed_frozen_change_set(&plan)
    .await
    .expect("execute drifted plan");
    assert_eq!(
        results[0].error.as_deref(),
        Some("frozen_change_base_hash_drift"),
        "Approved confirmation must not write onto drifted disk"
    );
    assert!(!results[0].success);
    assert_eq!(fixture.note_body(), "drifted\n");
    assert_ne!(fixture.note_body(), POLISHED_BODY);
}

#[test]
fn t10_host_limitation_completes_blocked_not_failed() {
    assert_eq!(
        classify_task_outcome(&DeliveryFacts {
            host_authored_limitation: true,
            change_ops_complete: Some(true),
        }),
        TaskOutcome::Blocked,
        "host limitation must not become partial/completed even if writes finished"
    );
    let fixture = pending_edit_fixture("请核实这条笔记的出处", NOTE_BODY);
    let state_version = RunIntake::get(
        &fixture.state.db,
        &fixture.accepted.session,
        &fixture.accepted.run_id,
    )
    .expect("replay")
    .expect("run")
    .run
    .state_version;
    finalize_host_authored_limitation(
        &fixture.state.db,
        &fixture.accepted.session,
        &fixture.accepted.run_id,
        state_version,
        "当前无法完成核实。don't use 3.14".into(),
        fixture.sink.as_ref(),
    )
    .expect("publish host limitation");
    let status: String = fixture
        .state
        .db
        .with_read_conn(|conn| {
            Ok(conn.query_row(
                "SELECT status FROM agent_runs WHERE run_id = ?1",
                [&fixture.accepted.run_id],
                |row| row.get(0),
            )?)
        })
        .expect("status");
    assert_eq!(status, "completed");
    assert_ne!(status, "failed");
    let event = fixture
        .sink
        .events
        .lock()
        .expect("lock")
        .iter()
        .find(|event| event.get("type").and_then(serde_json::Value::as_str) == Some("completed"))
        .cloned()
        .expect("completed event");
    assert_eq!(event["payload"]["taskOutcome"], "blocked");
    let blob = event.to_string();
    assert!(
        !blob.contains("don't"),
        "completed payload must not echo note body: {blob}"
    );
    assert!(
        !blob.contains("capability_degraded"),
        "taskOutcome must not be derived from capability_degraded: {blob}"
    );
}
