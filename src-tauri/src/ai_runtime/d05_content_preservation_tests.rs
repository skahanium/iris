//! D05 wave 2: format-preservation content check (N04 / C23).
//! Not D05 / N04 / Q14 acceptance; T25 whitespace normalize is not a proof.

use std::sync::{Arc, Mutex};

use super::agent_run_repository::{AgentRunRepository, AppendRunEventInput};
use super::agent_tool_loop::{ToolLoopExecutor, CONFIRMATION_PENDING_ERROR};
use super::content_preservation::{
    blocked_format_write_result, check_format_preservation, is_format_preservation_request,
    ReportStatus,
};
use super::run_context::RunContextAssembler;
use super::run_contract::{
    AssistantRunEvent, AssistantRunStartRequest, AssistantTurnDraft, CapabilityId, RunBudgetPolicy,
    RunEventPayload, RunEventType, RunState, SecurityDomain,
};
use super::run_engine::{RunEngine, RunEventSink};
use super::run_intake::RunIntake;
use super::run_tool_loop::NormalRunToolExecutor;
use crate::ai_runtime::ToolCall;
use crate::app::AppState;
use crate::cas::hash::content_hash_str;
use crate::error::AppResult;

const FORMAT_MESSAGE: &str = "请格式整理这段笔记";
const POLISH_MESSAGE: &str = "将这段话润色成正式通知";

#[test]
fn n04_whitespace_and_list_markers_pass() {
    let original = "# Title\r\n- don't  \n\n\n3.14\n1. keep\n";
    let candidate = "## Title\n* don't\n\n3.14\n2. keep\n";
    let report = check_format_preservation(original, candidate);
    assert_eq!(report.body_text, ReportStatus::Passed);
    assert_eq!(report.block_order, ReportStatus::Passed);
    assert_eq!(report.link_targets, ReportStatus::Passed);
    assert!(report.is_proven());
}

#[test]
fn n04_punctuation_and_prose_digits_are_not_stripped() {
    let report = check_format_preservation("don't use 3.14", "dont use 314");
    assert_eq!(report.body_text, ReportStatus::Failed);
    assert!(!report.is_proven());
}

#[test]
fn n04_paragraph_reorder_fails_block_order() {
    let report = check_format_preservation("alpha\n\nbeta", "beta\n\nalpha");
    assert_eq!(report.block_order, ReportStatus::Failed);
    assert!(!report.is_proven());
}

#[test]
fn n04_link_target_change_fails() {
    let report = check_format_preservation(
        "[x](https://a.example) and [[note-a]]",
        "[x](https://b.example) and [[note-a]]",
    );
    assert_eq!(report.link_targets, ReportStatus::Failed);
    assert!(!report.is_proven());
}

#[test]
fn n04_unclosed_fence_is_unknown_not_passed() {
    let report = check_format_preservation("```\ncode", "```\ncode");
    assert_ne!(report.body_text, ReportStatus::Passed);
    assert_ne!(report.block_order, ReportStatus::Passed);
    assert_ne!(report.link_targets, ReportStatus::Passed);
    assert_eq!(report.body_text, ReportStatus::Unknown);
    assert!(!report.is_proven());
}

#[test]
fn n04_t25_normalize_is_not_a_preservation_proof() {
    let original = "hello  \n\n\nworld don't 3.14\n";
    let normalized = super::tool_dispatch::normalize_markdown(original);
    let ok = check_format_preservation(original, &normalized);
    assert!(
        ok.is_proven(),
        "T25 whitespace output may pass the checker, got {ok:?}"
    );

    let stripped = normalized.replacen("world", "", 1);
    let bad = check_format_preservation(original, &stripped);
    assert_eq!(bad.body_text, ReportStatus::Failed);
    assert!(!bad.is_proven());
}

#[test]
fn n04_polish_rewrite_is_not_format_gate() {
    assert!(!is_format_preservation_request(POLISH_MESSAGE));
    assert!(!is_format_preservation_request("规范化"));
    assert!(!is_format_preservation_request("polish this paragraph"));
    assert!(!is_format_preservation_request("翻译这段"));
    assert!(is_format_preservation_request(FORMAT_MESSAGE));
    assert!(is_format_preservation_request("整理格式"));
    assert!(is_format_preservation_request("格式规范化"));
    assert!(is_format_preservation_request("规范化格式"));
    assert!(is_format_preservation_request("Please format this note"));
    assert!(is_format_preservation_request(
        "normalize the markdown formatting"
    ));
}

#[test]
fn n04_fullwidth_space_is_unknown_not_passed() {
    let report = check_format_preservation("hello\u{3000}world", "hello\u{3000}world");
    assert_eq!(report.body_text, ReportStatus::Unknown);
    assert_eq!(report.block_order, ReportStatus::Unknown);
    assert_eq!(report.link_targets, ReportStatus::Unknown);
    assert!(!report.is_proven());
}

#[test]
fn n04_link_label_may_change_when_target_stays() {
    let report = check_format_preservation(
        "[x](https://a.example) and [[note-a|shown]]",
        "[label](https://a.example) and [[note-a]]",
    );
    assert_eq!(report.link_targets, ReportStatus::Passed);
    assert!(report.is_proven());
}

#[test]
fn n04_format_replace_that_changes_prose_does_not_request_confirmation() {
    let blocked = blocked_format_write_result(
        FORMAT_MESSAGE,
        "replace_selection",
        &serde_json::json!({
            "original_text": "don't use 3.14",
            "replacement": "dont use 314",
        }),
    )
    .expect("format rewrite must be blocked before frozen confirmation");
    assert!(!blocked.success);
    assert_eq!(
        blocked.error.as_deref(),
        Some("format_preservation_unproven")
    );
    assert_eq!(blocked.output["error"], "format_preservation_unproven");
    assert_eq!(blocked.output["bodyText"], "failed");
    assert!(
        !blocked.output.to_string().contains("don't"),
        "unproven output must not echo note body"
    );
    assert!(!format!(
        "{}{}",
        blocked.error.clone().unwrap_or_default(),
        blocked.output
    )
    .contains("模型能力降级"));
}

#[test]
fn n04_format_whitespace_only_replace_still_requests_confirmation() {
    let blocked = blocked_format_write_result(
        FORMAT_MESSAGE,
        "replace_selection",
        &serde_json::json!({
            "original_text": "# Title\r\n- don't  \n\n\n3.14\n",
            "replacement": "## Title\n* don't\n\n3.14\n",
        }),
    );
    assert!(
        blocked.is_none(),
        "whitespace-only format replace must still enter confirmation"
    );
}

#[test]
fn n04_polish_replace_still_requests_confirmation() {
    let blocked = blocked_format_write_result(
        POLISH_MESSAGE,
        "replace_selection",
        &serde_json::json!({
            "original_text": "don't use 3.14",
            "replacement": "please do not use 3.14",
        }),
    );
    assert!(
        blocked.is_none(),
        "polish rewrite must not enter the format-preservation gate"
    );
}

#[test]
fn n04_format_insert_nonempty_is_unproven() {
    let blocked = blocked_format_write_result(
        FORMAT_MESSAGE,
        "insert_text_at_cursor",
        &serde_json::json!({ "text": "extra" }),
    )
    .expect("non-empty insert is not format preservation");
    assert_eq!(
        blocked.error.as_deref(),
        Some("format_preservation_unproven")
    );
    assert_eq!(blocked.output["bodyText"], "failed");
}

#[test]
fn n04_format_replace_missing_fields_is_unproven() {
    let blocked =
        blocked_format_write_result(FORMAT_MESSAGE, "replace_selection", &serde_json::json!({}))
            .expect("missing original/replacement cannot be proven");
    assert_eq!(
        blocked.error.as_deref(),
        Some("format_preservation_unproven")
    );
    assert_eq!(blocked.output["bodyText"], "unknown");
    assert_eq!(blocked.output["blockOrder"], "unknown");
    assert_eq!(blocked.output["linkTargets"], "unknown");
}

const NOTE_PATH: &str = "notes/a.md";
const ORIGINAL_BODY: &str = "don't use 3.14";
const REWRITTEN_BODY: &str = "dont use 314";

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

struct FormatWriteFixture {
    _directory: tempfile::TempDir,
    state: Arc<AppState>,
    vault: std::path::PathBuf,
    accepted: super::run_contract::AssistantRunAccepted,
    context: super::run_context::RunContext,
    sink: RecordingSink,
}

fn format_write_fixture(message: &str, body: &str) -> FormatWriteFixture {
    let directory = tempfile::tempdir().expect("tempdir");
    let vault = directory.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes dir");
    std::fs::write(vault.join(NOTE_PATH), body).expect("write note");
    let state = AppState::new(directory.path().join("data")).expect("app state");
    state.set_vault(vault.clone()).expect("activate vault");
    let vault = state.vault_path().expect("live vault");
    let start = AssistantRunStartRequest {
        client_request_id: format!("d05-n04-{}", uuid::Uuid::new_v4()),
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
                stage: "测试格式保持门".into(),
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
    FormatWriteFixture {
        _directory: directory,
        state,
        vault,
        accepted,
        context,
        sink,
    }
}

impl FormatWriteFixture {
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
            &self.sink,
            Vec::new(),
        )
        .with_allowed_tool_names(&["replace_selection".to_string()]);
        executor
            .execute(
                &self.accepted.run_id,
                &ToolCall::new("format-replace", "replace_selection", args.to_string()),
                1,
            )
            .await
    }
}

#[tokio::test]
async fn n04_format_unproven_replace_leaves_zero_confirmations() {
    let fixture = format_write_fixture(FORMAT_MESSAGE, ORIGINAL_BODY);

    let result = fixture
        .execute_replace(ORIGINAL_BODY, REWRITTEN_BODY)
        .await
        .expect("unproven format replace must return a tool result, not freeze confirmation");

    assert!(!result.success);
    assert_eq!(
        result.error.as_deref(),
        Some("format_preservation_unproven")
    );
    assert_eq!(fixture.confirmation_count(), 0);
    assert_eq!(fixture.note_body(), ORIGINAL_BODY);
}

#[tokio::test]
async fn n04_format_whitespace_only_replace_still_requests_run_confirmation() {
    let original = "# Title\n- don't\n\n3.14\n";
    let fixture = format_write_fixture(FORMAT_MESSAGE, original);

    let error = fixture
        .execute_replace(original, "## Title\n* don't\n\n3.14\n")
        .await
        .expect_err("whitespace-only format replace must still freeze confirmation");

    assert_eq!(error.to_string(), CONFIRMATION_PENDING_ERROR);
    assert_eq!(fixture.confirmation_count(), 1);
    assert_eq!(fixture.note_body(), original);
}

#[tokio::test]
async fn n04_polish_replace_still_requests_run_confirmation() {
    let fixture = format_write_fixture(POLISH_MESSAGE, ORIGINAL_BODY);

    let error = fixture
        .execute_replace(ORIGINAL_BODY, "please do not use 3.14")
        .await
        .expect_err("polish rewrite must still freeze confirmation");

    assert_eq!(error.to_string(), CONFIRMATION_PENDING_ERROR);
    assert_eq!(fixture.confirmation_count(), 1);
    assert_eq!(fixture.note_body(), ORIGINAL_BODY);
}
