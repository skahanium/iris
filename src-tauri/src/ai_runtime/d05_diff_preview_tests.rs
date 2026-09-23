//! D05 wave 5: C24 bounded confirmation diff preview (candidate visible ≠ write).
//! Not D05 / N04 / Q14 / C24 acceptance; no V05 and no K15 fields.

use std::sync::{Arc, Mutex};

use super::agent_run_repository::{AgentRunRepository, AppendRunEventInput};
use super::agent_tool_loop::{ToolLoopExecutor, CONFIRMATION_PENDING_ERROR};
use super::confirmation_diff::{
    build_confirmation_diff, diff_bodies, preview_pending_confirmation_diff,
    AssistantRunConfirmationDiffRequest, ConfirmationDiffHunk, ConfirmationDiffLineKind,
};
use super::frozen_change_plan::{
    FrozenChangeOperationInput, FrozenChangePlan, FrozenChangeSetInput,
};
use super::run_context::RunContextAssembler;
use super::run_contract::{
    AssistantRunStartRequest, AssistantTurnDraft, CapabilityId, RunBudgetPolicy, RunEventPayload,
    RunEventType, RunState, SecurityDomain,
};
use super::run_engine::{RunEngine, RunEventSink};
use super::run_intake::RunIntake;
use super::run_tool_loop::NormalRunToolExecutor;
use crate::ai_runtime::ToolCall;
use crate::app::AppState;
use crate::cas::hash::content_hash_str;
use crate::error::AppResult;

const NOTE_PATH: &str = "notes/a.md";
const NOTE_BODY: &str = "alpha\nbeta\ngamma\n";
const POLISHED_BODY: &str = "please do not use this note";
const POLISH_MESSAGE: &str = "将这段话润色成正式通知";

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

#[allow(clippy::too_many_arguments)]
fn edit_op(
    call_id: &str,
    path: &str,
    original_body: &str,
    start: usize,
    end: usize,
    original_text: &str,
    replacement: &str,
    expected_body: &str,
) -> FrozenChangeOperationInput {
    let change = serde_json::json!({
        "target_path": path,
        "base_content_hash": content_hash_str(original_body),
        "range": { "start": start, "end": end },
        "original_text": original_text,
        "replacement": replacement,
    });
    FrozenChangeOperationInput {
        tool_call_id: call_id.into(),
        operation: "replace_selection".into(),
        relative_paths: vec![path.into()],
        base_content_hashes: vec![(path.into(), content_hash_str(original_body))],
        expected_post_content_hashes: vec![(path.into(), content_hash_str(expected_body))],
        change,
        rollback_summary: "rollback".into(),
    }
}

fn freeze_plan(vault_id: &str, operations: Vec<FrozenChangeOperationInput>) -> FrozenChangePlan {
    FrozenChangePlan::freeze_set(FrozenChangeSetInput {
        confirmation_id: "diff-preview-conf".into(),
        run_id: "diff-preview-run".into(),
        session_id: 1,
        request_id: "diff-preview-req".into(),
        vault_id: vault_id.into(),
        operations,
        expires_at_unix_ms: chrono::Utc::now().timestamp_millis() + 60_000,
    })
    .expect("freeze plan")
}

#[test]
fn diff_bodies_marks_replaced_line() {
    let hunks = diff_bodies("a\nb\nc\n", "a\nB\nc\n");
    assert_eq!(
        line_kinds(&hunks),
        vec![(' ', "a"), ('-', "b"), ('+', "B"), (' ', "c")]
    );
}

#[test]
fn diff_bodies_keeps_two_context_lines() {
    let original = "1\n2\n3\n4\n5\n6\n7\n";
    let candidate = "1\n2\n3\nFOUR\n5\n6\n7\n";
    let hunks = diff_bodies(original, candidate);
    assert_eq!(hunks.len(), 1);
    assert_eq!(
        line_kinds(&hunks),
        vec![
            (' ', "2"),
            (' ', "3"),
            ('-', "4"),
            ('+', "FOUR"),
            (' ', "5"),
            (' ', "6"),
        ]
    );
}

#[test]
fn diff_bodies_insert_only_hunk() {
    let hunks = diff_bodies("a\nc\n", "a\nb\nc\n");
    assert_eq!(line_kinds(&hunks), vec![(' ', "a"), ('+', "b"), (' ', "c")]);
}

#[test]
fn build_preview_marks_unpreviewable_on_base_hash_drift() {
    let directory = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(directory.path().join("notes")).expect("notes dir");
    std::fs::write(directory.path().join(NOTE_PATH), "drifted\n").expect("write drifted");
    let plan = freeze_plan(
        "vault",
        vec![edit_op(
            "call-1",
            NOTE_PATH,
            NOTE_BODY,
            0,
            5,
            "alpha",
            "ALPHA",
            "ALPHA\nbeta\ngamma\n",
        )],
    );
    let preview = build_confirmation_diff(&plan, directory.path());
    assert!(!preview.truncated);
    assert_eq!(preview.files.len(), 1);
    assert!(!preview.files[0].previewable);
    assert!(preview.files[0].hunks.is_empty());
}

#[test]
fn build_preview_replays_chained_operations_on_one_file() {
    let directory = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(directory.path().join("notes")).expect("notes dir");
    std::fs::write(directory.path().join(NOTE_PATH), NOTE_BODY).expect("write note");
    let after_first = "ALPHA\nbeta\ngamma\n";
    let plan = freeze_plan(
        "vault",
        vec![
            edit_op(
                "call-1",
                NOTE_PATH,
                NOTE_BODY,
                0,
                5,
                "alpha",
                "ALPHA",
                after_first,
            ),
            edit_op(
                "call-2",
                NOTE_PATH,
                after_first,
                11,
                16,
                "gamma",
                "GAMMA",
                "ALPHA\nbeta\nGAMMA\n",
            ),
        ],
    );
    let preview = build_confirmation_diff(&plan, directory.path());
    assert!(!preview.truncated);
    assert_eq!(preview.files.len(), 1);
    assert!(preview.files[0].previewable);
    assert_eq!(
        line_kinds(&preview.files[0].hunks),
        vec![
            ('-', "alpha"),
            ('+', "ALPHA"),
            (' ', "beta"),
            ('-', "gamma"),
            ('+', "GAMMA"),
        ]
    );
}

#[test]
fn build_preview_marks_non_edit_operation_unpreviewable() {
    let directory = tempfile::tempdir().expect("tempdir");
    let change = serde_json::json!({ "target_path": "notes/new.md" });
    let plan = freeze_plan(
        "vault",
        vec![FrozenChangeOperationInput {
            tool_call_id: "call-1".into(),
            operation: "note_create".into(),
            relative_paths: vec!["notes/new.md".into()],
            base_content_hashes: Vec::new(),
            expected_post_content_hashes: Vec::new(),
            change,
            rollback_summary: "rollback".into(),
        }],
    );
    let preview = build_confirmation_diff(&plan, directory.path());
    assert_eq!(preview.files.len(), 1);
    assert!(!preview.files[0].previewable);
    assert!(preview.files[0].hunks.is_empty());
}

#[test]
fn build_preview_truncates_beyond_hunk_bound() {
    let directory = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(directory.path().join("notes")).expect("notes dir");
    let mut original = String::new();
    let mut candidate = String::new();
    for index in 0..60 {
        let block = format!(
            "old-{index}\nkeep-{index}-a\nkeep-{index}-b\nkeep-{index}-c\nkeep-{index}-d\nkeep-{index}-e\nkeep-{index}-f\n"
        );
        original.push_str(&block);
        candidate.push_str(&block.replace(&format!("old-{index}"), &format!("new-{index}")));
    }
    std::fs::write(directory.path().join(NOTE_PATH), &original).expect("write note");
    let plan = freeze_plan(
        "vault",
        vec![edit_op(
            "call-1",
            NOTE_PATH,
            &original,
            0,
            original.len(),
            &original,
            &candidate,
            &candidate,
        )],
    );
    let preview = build_confirmation_diff(&plan, directory.path());
    assert!(preview.truncated, "60 separated hunks must hit the bound");
    assert!(!preview.files[0].hunks.is_empty());
    assert!(preview.files[0].hunks.len() < 60);
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

struct DiffPreviewFixture {
    _directory: tempfile::TempDir,
    state: Arc<AppState>,
    accepted: super::run_contract::AssistantRunAccepted,
    confirmation_id: String,
    plan_hash: String,
    sink: Arc<RecordingSink>,
}

async fn diff_preview_fixture() -> DiffPreviewFixture {
    let directory = tempfile::tempdir().expect("tempdir");
    let vault = directory.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes dir");
    std::fs::write(vault.join(NOTE_PATH), NOTE_BODY).expect("write note");
    let state = AppState::new(directory.path().join("data")).expect("app state");
    state.set_vault(vault.clone()).expect("activate vault");
    let start = AssistantRunStartRequest {
        client_request_id: format!("d05-diff-{}", uuid::Uuid::new_v4()),
        session: None,
        turn: AssistantTurnDraft {
            message: POLISH_MESSAGE.into(),
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
                stage: "测试差异预览".into(),
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
    let executor = NormalRunToolExecutor::new(
        &state,
        None,
        &accepted,
        &context,
        vec![CapabilityId::new("note.apply_patch")],
        RunBudgetPolicy::for_envelope(&context.envelope),
        sink.as_ref(),
        Vec::new(),
    )
    .with_allowed_tool_names(&["replace_selection".to_string()]);
    let args = serde_json::json!({
        "target_path": NOTE_PATH,
        "base_content_hash": content_hash_str(NOTE_BODY),
        "range": { "start": 0, "end": NOTE_BODY.len() },
        "original_text": NOTE_BODY,
        "replacement": POLISHED_BODY,
    });
    let error = executor
        .execute(
            &accepted.run_id,
            &ToolCall::new("diff-replace", "replace_selection", args.to_string()),
            1,
        )
        .await
        .expect_err("a polished rewrite must wait for confirmation");
    assert_eq!(error.to_string(), CONFIRMATION_PENDING_ERROR);
    let (confirmation_id, plan_hash): (String, String) = state
        .db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT confirmation_id, plan_hash FROM agent_run_confirmations
                 WHERE run_id = ?1 AND status = 'pending'",
                [&accepted.run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(Into::into)
        })
        .expect("pending confirmation row");
    DiffPreviewFixture {
        _directory: directory,
        state,
        accepted,
        confirmation_id,
        plan_hash,
        sink,
    }
}

impl DiffPreviewFixture {
    fn request(&self) -> AssistantRunConfirmationDiffRequest {
        AssistantRunConfirmationDiffRequest {
            session: self.accepted.session.clone(),
            run_id: self.accepted.run_id.clone(),
            confirmation_id: self.confirmation_id.clone(),
            plan_hash: self.plan_hash.clone(),
        }
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
}

#[tokio::test]
async fn diff_preview_entry_projects_lines_without_persisting_body() {
    let fixture = diff_preview_fixture().await;
    let before = fixture.persisted_events_json();
    let preview = preview_pending_confirmation_diff(fixture.state.as_ref(), fixture.request())
        .expect("pending confirmation must produce a bounded diff for its owner");
    assert!(!preview.truncated);
    assert_eq!(preview.files.len(), 1);
    assert_eq!(preview.files[0].path, NOTE_PATH);
    assert!(preview.files[0].previewable);
    let kinds = line_kinds(&preview.files[0].hunks);
    assert!(
        kinds.contains(&('-', "alpha")) && kinds.contains(&('-', "gamma")),
        "deleted lines must show the frozen original, got {kinds:?}"
    );
    assert!(
        kinds.contains(&('+', POLISHED_BODY)),
        "added lines must show the frozen candidate, got {kinds:?}"
    );
    let after = fixture.persisted_events_json();
    assert_eq!(
        before, after,
        "preview must not append or rewrite run events"
    );
    assert!(
        !after.contains("alpha"),
        "persisted events must keep omitting note body: {after}"
    );
    let emitted = fixture
        .sink
        .events
        .lock()
        .expect("recording sink lock")
        .iter()
        .map(serde_json::Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !emitted.contains("alpha"),
        "emitted events must keep omitting note body: {emitted}"
    );
}

#[tokio::test]
async fn diff_preview_entry_rejects_plan_hash_mismatch() {
    let fixture = diff_preview_fixture().await;
    let mut request = fixture.request();
    request.plan_hash = "sha256:deadbeef".into();
    let error =
        preview_pending_confirmation_diff(fixture.state.as_ref(), request).expect_err("mismatch");
    assert_eq!(
        error.to_string(),
        "agent_run_confirmation_plan_hash_mismatch"
    );
}

#[tokio::test]
async fn diff_preview_entry_rejects_missing_pending_row() {
    let fixture = diff_preview_fixture().await;
    let mut request = fixture.request();
    request.confirmation_id = "not-this-confirmation".into();
    let error =
        preview_pending_confirmation_diff(fixture.state.as_ref(), request).expect_err("missing");
    assert_eq!(error.to_string(), "agent_run_confirmation_diff_unavailable");
}

#[tokio::test]
async fn diff_preview_entry_rejects_expired_confirmation() {
    let fixture = diff_preview_fixture().await;
    fixture
        .state
        .db
        .with_conn(|conn| {
            conn.execute(
                "UPDATE agent_run_confirmations SET expires_at = 1 WHERE confirmation_id = ?1",
                [&fixture.confirmation_id],
            )?;
            Ok(())
        })
        .expect("expire confirmation");
    let error = preview_pending_confirmation_diff(fixture.state.as_ref(), fixture.request())
        .expect_err("expired");
    assert_eq!(error.to_string(), "agent_run_confirmation_expired");
}

#[tokio::test]
async fn diff_preview_entry_rejects_classified_domain() {
    let fixture = diff_preview_fixture().await;
    let mut request = fixture.request();
    request.session.domain = SecurityDomain::Classified;
    let error =
        preview_pending_confirmation_diff(fixture.state.as_ref(), request).expect_err("domain");
    assert_eq!(error.to_string(), "agent_run_control_not_available");
}
