//! D05 wave 3: C24 edit-candidate contract (generate ≠ write).
//! Not D05 / N04 / Q14 / C24 acceptance; no candidate UI and no K15 fields.

use std::sync::{Arc, Mutex};

use super::agent_run_repository::{AgentRunRepository, AppendRunEventInput};
use super::agent_tool_loop::{ToolLoopExecutor, CONFIRMATION_PENDING_ERROR};
use super::edit_candidate::{prepare_edit_candidate, safe_candidate_summary, EditCandidateError};
use super::run_context::RunContextAssembler;
use super::run_contract::{
    AssistantRunStartRequest, AssistantTurnDraft, CapabilityId, Effect, ExplicitAction, Freshness,
    RunBudgetPolicy, RunEventPayload, RunEventType, RunState, SecurityDomain, WebDecisionReason,
};
use super::run_engine::{RunEngine, RunEventSink};
use super::run_intake::RunIntake;
use super::run_tool_loop::NormalRunToolExecutor;
use crate::ai_runtime::ToolCall;
use crate::app::AppState;
use crate::cas::hash::content_hash_str;
use crate::error::AppResult;

const NOTE_BODY: &str = "don't use 3.14";

#[test]
fn c24_replace_builds_candidate_without_writing() {
    let directory = tempfile::tempdir().expect("tempdir");
    let note = directory.path().join("note.md");
    std::fs::write(&note, NOTE_BODY).expect("write note");
    let original = std::fs::read_to_string(&note).expect("read note");
    let args = serde_json::json!({
        "target_path": "note.md",
        "base_content_hash": content_hash_str(&original),
        "range": { "start": 10, "end": 14 },
        "original_text": "3.14",
        "replacement": "pi",
    });

    let candidate = prepare_edit_candidate("replace_selection", &args, &original)
        .expect("replace must prepare a candidate");

    assert_eq!(candidate.operation, "replace_selection");
    assert_eq!(candidate.target_path, "note.md");
    assert_eq!(candidate.base_content_hash, content_hash_str(&original));
    assert_eq!(
        candidate.expected_post_content_hash,
        content_hash_str("don't use pi")
    );
    assert_eq!(std::fs::read_to_string(&note).expect("reread"), NOTE_BODY);
}

#[test]
fn c24_missing_replacement_cannot_prepare() {
    let args = serde_json::json!({
        "target_path": "note.md",
        "base_content_hash": content_hash_str(NOTE_BODY),
        "range": { "start": 10, "end": 14 },
        "original_text": "3.14",
    });
    let err = prepare_edit_candidate("replace_selection", &args, NOTE_BODY)
        .expect_err("missing replacement cannot be a candidate");
    assert_eq!(err, EditCandidateError::MissingFields);
}

#[test]
fn c24_stale_base_hash_cannot_prepare() {
    let args = serde_json::json!({
        "target_path": "note.md",
        "base_content_hash": "sha256:deadbeef",
        "range": { "start": 10, "end": 14 },
        "original_text": "3.14",
        "replacement": "pi",
    });
    let err = prepare_edit_candidate("replace_selection", &args, NOTE_BODY)
        .expect_err("stale hash cannot be a candidate");
    assert_eq!(err, EditCandidateError::HashMismatch);
}

#[test]
fn c24_insert_counts_added_chars() {
    let original = "ab";
    let args = serde_json::json!({
        "target_path": "note.md",
        "base_content_hash": content_hash_str(original),
        "range": { "start": 1, "end": 1 },
        "original_text": "",
        "text": "hello",
    });
    let candidate = prepare_edit_candidate("insert_text_at_cursor", &args, original)
        .expect("insert must prepare a candidate");
    assert_eq!(candidate.added_chars, 5);
    assert_eq!(candidate.removed_chars, 0);
    assert_eq!(candidate.original_chars, 2);
    assert_eq!(candidate.candidate_chars, 7);
    assert_eq!(
        candidate.expected_post_content_hash,
        content_hash_str("ahellob")
    );
}

#[test]
fn c24_safe_summary_omits_note_body() {
    let args = serde_json::json!({
        "target_path": "note.md",
        "base_content_hash": content_hash_str(NOTE_BODY),
        "range": { "start": 10, "end": 14 },
        "original_text": "3.14",
        "replacement": "pi",
    });
    let candidate =
        prepare_edit_candidate("replace_selection", &args, NOTE_BODY).expect("candidate");
    let summary = safe_candidate_summary(&candidate);
    assert!(
        !summary.contains("don't"),
        "safe summary must not echo note body, got {summary}"
    );
    assert!(
        summary.contains("replace_selection"),
        "summary should name the operation, got {summary}"
    );
    assert!(
        summary.contains('+') && (summary.contains('−') || summary.contains('-')),
        "summary should include character counts, got {summary}"
    );
}

#[test]
fn c24_unsupported_tool_is_not_a_candidate() {
    let err = prepare_edit_candidate("web_search", &serde_json::json!({}), NOTE_BODY)
        .expect_err("web_search is not an edit candidate");
    assert_eq!(err, EditCandidateError::UnsupportedTool);
}

const FORMAT_MESSAGE: &str = "请格式整理这段笔记";
const POLISH_MESSAGE: &str = "将这段话润色成正式通知";
const NOTE_PATH: &str = "notes/a.md";
const REWRITTEN_BODY: &str = "dont use 314";

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

struct CandidateWriteFixture {
    _directory: tempfile::TempDir,
    state: Arc<AppState>,
    vault: std::path::PathBuf,
    accepted: super::run_contract::AssistantRunAccepted,
    context: super::run_context::RunContext,
    sink: RecordingSink,
}

fn candidate_write_fixture(message: &str, body: &str) -> CandidateWriteFixture {
    let directory = tempfile::tempdir().expect("tempdir");
    let vault = directory.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes dir");
    std::fs::write(vault.join(NOTE_PATH), body).expect("write note");
    let state = AppState::new(directory.path().join("data")).expect("app state");
    state.set_vault(vault.clone()).expect("activate vault");
    let vault = state.vault_path().expect("live vault");
    let start = AssistantRunStartRequest {
        client_request_id: format!("d05-c24-{}", uuid::Uuid::new_v4()),
        session: None,
        turn: AssistantTurnDraft {
            message: message.into(),
            content_parts: None,
            explicit_references: vec![crate::ai_types::ContextReferenceWire {
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
    let sink = RecordingSink::default();
    let preparing =
        RunEngine::mark_preparing_with_sink(&state.db, &accepted.session, &accepted.run_id, &sink)
            .expect("preparing");
    AgentRunRepository::append_event(
        &state.db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: preparing,
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Running,
                stage: "测试编辑候选门".into(),
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
    CandidateWriteFixture {
        _directory: directory,
        state,
        vault,
        accepted,
        context,
        sink,
    }
}

impl CandidateWriteFixture {
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

    fn note_body(&self) -> String {
        std::fs::read_to_string(self.vault.join(NOTE_PATH)).expect("read note")
    }

    fn confirmation_event_json(&self) -> Option<serde_json::Value> {
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

    async fn execute_replace_args(
        &self,
        args: serde_json::Value,
    ) -> AppResult<crate::ai_runtime::ToolCallResult> {
        let executor = NormalRunToolExecutor::new(
            &self.state,
            None,
            &self.accepted,
            &self.context,
            vec![CapabilityId::new("note.apply_patch")],
            RunBudgetPolicy::for_envelope(&self.context.envelope),
            &self.sink,
            Vec::new(),
        )
        .with_allowed_tool_names(&["replace_selection".to_string()]);
        executor
            .execute(
                &self.accepted.run_id,
                &ToolCall::new("c24-replace", "replace_selection", args.to_string()),
                1,
            )
            .await
    }

    async fn execute_replace(
        &self,
        original: &str,
        replacement: &str,
    ) -> AppResult<crate::ai_runtime::ToolCallResult> {
        self.execute_replace_args(serde_json::json!({
            "target_path": NOTE_PATH,
            "base_content_hash": content_hash_str(original),
            "range": { "start": 0, "end": original.len() },
            "original_text": original,
            "replacement": replacement,
        }))
        .await
    }
}

#[tokio::test]
async fn c24_unproven_format_replace_does_not_create_confirmation() {
    let fixture = candidate_write_fixture(FORMAT_MESSAGE, NOTE_BODY);
    let result = fixture
        .execute_replace(NOTE_BODY, REWRITTEN_BODY)
        .await
        .expect("unproven format replace must return a tool result, not freeze");
    assert!(!result.success);
    assert_eq!(
        result.error.as_deref(),
        Some("format_preservation_unproven")
    );
    assert_eq!(fixture.confirmation_count(), 0);
    assert_eq!(fixture.note_body(), NOTE_BODY);
}

#[tokio::test]
async fn c24_incomplete_replace_does_not_create_confirmation() {
    let fixture = candidate_write_fixture(POLISH_MESSAGE, NOTE_BODY);
    let result = fixture
        .execute_replace_args(serde_json::json!({
            "target_path": NOTE_PATH,
            "base_content_hash": content_hash_str(NOTE_BODY),
            "range": { "start": 10, "end": 14 },
            "original_text": "3.14",
        }))
        .await
        .expect("missing replacement must fail as a tool result, not freeze");
    assert!(!result.success);
    assert_eq!(
        result.error.as_deref(),
        Some("edit_candidate_missing_fields")
    );
    let blob = serde_json::to_string(&result).expect("serialize tool result");
    assert!(
        !blob.contains("模型能力降级")
            && !result
                .error
                .as_deref()
                .unwrap_or("")
                .contains("模型能力降级"),
        "incomplete candidate must not be framed as a model-capability downgrade: {blob}"
    );
    assert_eq!(fixture.confirmation_count(), 0);
    assert_eq!(fixture.note_body(), NOTE_BODY);
}

#[tokio::test]
async fn c24_polish_replace_still_requests_confirmation() {
    let fixture = candidate_write_fixture(POLISH_MESSAGE, NOTE_BODY);
    let error = fixture
        .execute_replace(NOTE_BODY, "please do not use 3.14")
        .await
        .expect_err("polish rewrite must still freeze confirmation");
    assert_eq!(error.to_string(), CONFIRMATION_PENDING_ERROR);
    assert_eq!(fixture.confirmation_count(), 1);
    assert_eq!(fixture.note_body(), NOTE_BODY);
}

#[tokio::test]
async fn c24_confirmation_event_omits_note_body() {
    let fixture = candidate_write_fixture(POLISH_MESSAGE, NOTE_BODY);
    let _ = fixture
        .execute_replace(NOTE_BODY, "please do not use 3.14")
        .await
        .expect_err("polish rewrite waits for confirmation");
    let event = fixture
        .confirmation_event_json()
        .expect("pending confirmation event");
    let encoded = event.to_string();
    assert!(
        !encoded.contains("don't"),
        "confirmation event must not echo note body: {encoded}"
    );
    let summary = event
        .pointer("/payload/summary")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    assert!(
        !summary.contains("don't"),
        "confirmation summary must not echo note body: {summary}"
    );
    assert!(
        summary.contains("将修改 1 个目标"),
        "summary should name the single target, got {summary}"
    );
    assert!(
        summary.contains('+') && (summary.contains('−') || summary.contains('-')),
        "summary should include character counts, got {summary}"
    );
}

#[test]
fn c24_draft_envelope_still_has_no_apply_patch() {
    let start = AssistantRunStartRequest {
        client_request_id: "d05-c24-draft".into(),
        session: None,
        turn: AssistantTurnDraft {
            message: "将这段话润色成正式通知。".into(),
            content_parts: None,
            explicit_references: vec![],
            retrieval_scope: Default::default(),
            display_mentions: vec![],
        },
        explicit_action: Some(ExplicitAction {
            effect: Effect::Draft,
            target: None,
            selection_snapshot: None,
        }),
        web_enabled: true,
        model_override: None,
        external_tool_grants: Vec::new(),
        security_domain: SecurityDomain::Normal,
        classified_context_ref: None,
    };
    let envelope = RunIntake::resolve_envelope(&start).expect("resolve draft polish");
    assert_eq!(envelope.freshness, Freshness::Offline);
    assert_eq!(envelope.web_reason, WebDecisionReason::LocalTransformation);
    assert!(envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "note.propose_patch"));
    assert!(!envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == "note.apply_patch"));
}
