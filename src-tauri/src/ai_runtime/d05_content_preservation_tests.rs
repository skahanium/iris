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
    let original = "# Title\r\n- don't  \n\n\n3.14\n\n1. keep\n";
    let candidate = "## Title\n* don't\n\n3.14\n\n2. keep\n";
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
    assert_ne!(bad.body_text, ReportStatus::Passed);
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
fn n04_table_is_unknown_not_passed() {
    let table = "a | b\n---|---\n1 | 2\n";
    let report = check_format_preservation(table, table);
    assert_eq!(report.body_text, ReportStatus::Unknown);
    assert!(!report.is_proven());
    assert!(!report.has_failed());
}

#[test]
fn n04_html_is_unknown_not_passed() {
    let html = "<div>keep</div>\n";
    let report = check_format_preservation(html, html);
    assert_eq!(report.body_text, ReportStatus::Unknown);
    assert!(!report.is_proven());
    assert!(!report.has_failed());
}

#[test]
fn n04_zwsp_is_unknown_not_passed() {
    let zwsp = "hello\u{200B}world";
    let report = check_format_preservation(zwsp, zwsp);
    assert_eq!(report.body_text, ReportStatus::Unknown);
    assert!(!report.is_proven());
    assert!(!report.has_failed());
}

#[test]
fn n04_link_label_must_stay_when_target_stays() {
    let report = check_format_preservation(
        "[x](https://a.example) and [[note-a|shown]]",
        "[label](https://a.example) and [[note-a]]",
    );
    assert_eq!(report.link_targets, ReportStatus::Passed);
    assert!(!report.is_proven());
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

    /// The `formatPreservation` projection carried by the pending confirmation
    /// event, if any.
    fn pending_format_preservation(&self) -> Option<serde_json::Value> {
        let rows: Vec<String> = self
            .state
            .db
            .with_read_conn(|conn| {
                let mut statement =
                    conn.prepare("SELECT payload_json FROM agent_run_events WHERE run_id = ?1")?;
                let rows =
                    statement.query_map([&self.accepted.run_id], |row| row.get::<_, String>(0))?;
                rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
            })
            .expect("confirmation events");
        rows.iter()
            .filter_map(|payload| serde_json::from_str::<serde_json::Value>(payload).ok())
            .find_map(|payload| payload.get("formatPreservation").cloned())
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
async fn n04_format_table_unknown_still_requests_run_confirmation() {
    let original = "a | b\n---|---\n1 | 2\n";
    let fixture = format_write_fixture(FORMAT_MESSAGE, original);

    let error = fixture
        .execute_replace(original, original)
        .await
        .expect_err("table unknown must freeze confirmation instead of only feeding the model");

    assert_eq!(error.to_string(), CONFIRMATION_PENDING_ERROR);
    assert_eq!(fixture.confirmation_count(), 1);
    assert_eq!(fixture.note_body(), original);
}

// F3 closure (K16 delivery of 「差异与不确定项」): an `unknown` candidate's
// frozen confirmation must carry the bounded uncertainty projection — states
// and fixed labels only, never note text.
#[tokio::test]
async fn n04_unknown_run_confirmation_carries_uncertainty_notice() {
    let original = "a | b\n---|---\n1 | 2\n";
    let fixture = format_write_fixture(FORMAT_MESSAGE, original);

    let error = fixture
        .execute_replace(original, original)
        .await
        .expect_err("table unknown freezes confirmation");

    assert_eq!(error.to_string(), CONFIRMATION_PENDING_ERROR);
    let notice = fixture
        .pending_format_preservation()
        .expect("confirmation must deliver the uncertainty notice");
    let checks = notice["checks"].as_array().expect("checks");
    assert_eq!(checks.len(), 3);
    assert!(
        checks.iter().any(|check| check["state"] == "unknown"),
        "unknown states must reach the confirmation surface: {notice}"
    );
    for check in checks {
        assert!(
            check["label"]
                .as_str()
                .is_some_and(|label| !label.is_empty()),
            "every check carries a fixed label: {notice}"
        );
    }
    let encoded = notice.to_string();
    assert!(
        !encoded.contains("a | b"),
        "the notice must never carry note text: {notice}"
    );
}

#[test]
fn n04_preservation_notice_is_content_free_with_fixed_labels() {
    let report = crate::ai_runtime::content_preservation::check_format_preservation(
        "hello\u{200B}world",
        "hello\u{200B}world",
    );
    let notice = crate::ai_runtime::content_preservation::preservation_notice(&report);
    let checks = notice["checks"].as_array().expect("checks");
    let fields: Vec<&str> = checks
        .iter()
        .map(|check| check["field"].as_str().expect("field"))
        .collect();
    assert_eq!(fields, ["bodyText", "blockOrder", "linkTargets"]);
    for check in checks {
        assert!(
            check["state"].as_str().is_some_and(|state| {
                matches!(state, "passed" | "failed" | "unknown" | "not-applicable")
            }),
            "states stay inside the K16 four-state vocabulary: {notice}"
        );
    }
    let encoded = notice.to_string();
    assert!(!encoded.contains("\u{200B}"), "notice is content-free");
}

#[tokio::test]
async fn n04_format_html_unknown_still_requests_run_confirmation() {
    let original = "<div>keep</div>\n";
    let fixture = format_write_fixture(FORMAT_MESSAGE, original);

    let error = fixture
        .execute_replace(original, original)
        .await
        .expect_err("HTML unknown must freeze confirmation instead of only feeding the model");

    assert_eq!(error.to_string(), CONFIRMATION_PENDING_ERROR);
    assert_eq!(fixture.confirmation_count(), 1);
    assert_eq!(fixture.note_body(), original);
}

#[tokio::test]
async fn n04_format_zwsp_unknown_still_requests_run_confirmation() {
    let original = "hello\u{200B}world";
    let fixture = format_write_fixture(FORMAT_MESSAGE, original);

    let error = fixture
        .execute_replace(original, original)
        .await
        .expect_err("ZWSP unknown must freeze confirmation instead of only feeding the model");

    assert_eq!(error.to_string(), CONFIRMATION_PENDING_ERROR);
    assert_eq!(fixture.confirmation_count(), 1);
    assert_eq!(fixture.note_body(), original);
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

#[test]
fn review_regression_b_preserves_links_code_and_fence_information() {
    for (original, candidate) in [
        ("[[note#A]]", "[[note#B]]"),
        ("[allow](x)", "[deny](x)"),
        ("[[note|allow]]", "[[note|deny]]"),
        ("`[allow](x)`", "`[deny](x)`"),
        ("```rust\nkeep\n```\n", "```python\nkeep\n```\n"),
        ("```\nkeep\n``` not-a-close\n", "```\nkeep\n```\n"),
        ("```\na\rb\n```\n", "```\na\nb\n```\n"),
    ] {
        assert!(
            !check_format_preservation(original, candidate).is_proven(),
            "must preserve {original:?}"
        );
    }
}

fn review_replace_call(
    id: &str,
    body: &str,
    range: std::ops::Range<usize>,
    replacement: &str,
) -> ToolCall {
    ToolCall::new(
        id,
        "replace_selection",
        serde_json::json!({
            "target_path": NOTE_PATH,
            "base_content_hash": content_hash_str(body),
            "range": { "start": range.start, "end": range.end },
            "original_text": &body[range],
            "replacement": replacement,
        })
        .to_string(),
    )
}

struct ReviewFormatProvider {
    responses: Mutex<std::collections::VecDeque<AppResult<super::model_gateway::GatewayResponse>>>,
    transcripts: Mutex<Vec<Vec<crate::ai_runtime::LlmMessage>>>,
}

impl super::agent_tool_loop::ToolLoopProvider for ReviewFormatProvider {
    fn answer_turn<'a>(
        &'a self,
        _run_id: &'a str,
        messages: &'a [crate::ai_runtime::LlmMessage],
        _tools: &'a [crate::ai_runtime::ToolSpec],
        _budget: super::agent_tool_loop::AgentModelTurnBudget,
        _observer: &'a mut dyn super::model_gateway::StreamEventObserver,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = AppResult<super::model_gateway::GatewayResponse>>
                + Send
                + 'a,
        >,
    > {
        self.transcripts.lock().unwrap().push(messages.to_vec());
        Box::pin(async move {
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .expect("scripted turn")
        })
    }
}

struct ReviewFormatObserver;
impl super::model_gateway::StreamEventObserver for ReviewFormatObserver {
    fn observe(&mut self, _: &super::model_gateway::StreamEvent, _: u32) -> AppResult<()> {
        Ok(())
    }
}

async fn review_format_loop(
    fixture: &FormatWriteFixture,
    calls: Vec<ToolCall>,
) -> (
    AppResult<super::agent_tool_loop::AgentToolLoopOutcome>,
    ReviewFormatProvider,
) {
    review_format_script(
        fixture,
        vec![
            Ok(super::model_gateway::GatewayResponse {
                tool_calls: calls,
                finish_reason: "tool_calls".into(),
                ..Default::default()
            }),
            Ok(super::model_gateway::GatewayResponse {
                content: Some("候选改变了内容，尚未写入。".into()),
                finish_reason: "stop".into(),
                ..Default::default()
            }),
        ],
        RunBudgetPolicy::standard(),
    )
    .await
}

async fn review_format_script(
    fixture: &FormatWriteFixture,
    responses: Vec<AppResult<super::model_gateway::GatewayResponse>>,
    policy: RunBudgetPolicy,
) -> (
    AppResult<super::agent_tool_loop::AgentToolLoopOutcome>,
    ReviewFormatProvider,
) {
    let provider = ReviewFormatProvider {
        responses: Mutex::new(responses.into()),
        transcripts: Mutex::new(Vec::new()),
    };
    let caps = vec![CapabilityId::new("note.apply_patch")];
    let tools = super::tool_executor::ToolRegistry::new()
        .tools_for_authorized_capabilities(&caps, false)
        .into_iter()
        .filter(|tool| tool.name == "replace_selection")
        .collect::<Vec<_>>();
    let executor = NormalRunToolExecutor::new(
        &fixture.state,
        None,
        &fixture.accepted,
        &fixture.context,
        caps,
        policy.clone(),
        &fixture.sink,
        Vec::new(),
    )
    .with_allowed_tool_names(&["replace_selection".into()]);
    let result = super::agent_tool_loop::AgentToolLoop::from_policy(&policy)
        .execute(
            &provider,
            &executor,
            &fixture.accepted.run_id,
            Vec::new(),
            tools,
            &mut ReviewFormatObserver,
        )
        .await;
    (result, provider)
}

#[tokio::test]
async fn review_regression_b_partial_number_cannot_freeze() {
    let body = "balance 123. units";
    let fixture = format_write_fixture(FORMAT_MESSAGE, body);
    let (result, provider) = review_format_loop(
        &fixture,
        vec![review_replace_call("partial", body, 8..12, "1.")],
    )
    .await;
    assert!(
        result.is_ok(),
        "unproven candidate must receive feedback: {result:?}"
    );
    assert_eq!(fixture.confirmation_count(), 0);
    assert_eq!(fixture.note_body(), body);
    assert_eq!(provider.transcripts.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn review_regression_b_rejected_batch_returns_every_call_without_freezing() {
    let body = "# title\nkeep";
    let fixture = format_write_fixture(FORMAT_MESSAGE, body);
    let (result, provider) = review_format_loop(
        &fixture,
        vec![
            review_replace_call("first", body, 0..1, "##"),
            review_replace_call("second", "## title\nkeep", 9..13, "lost"),
        ],
    )
    .await;
    let outcome = result.expect("rejected batch must allow a model correction turn");
    assert_eq!(
        outcome.tool_calls, 0,
        "validation is not dispatched execution"
    );
    assert_eq!(fixture.confirmation_count(), 0);
    assert_eq!(fixture.note_body(), body);
    let transcripts = provider.transcripts.lock().unwrap();
    let feedback = transcripts[1]
        .iter()
        .filter_map(|message| message.tool_call_id.as_deref())
        .collect::<Vec<_>>();
    assert_eq!(feedback, ["first", "second"], "no hanging provider call");
    assert!(transcripts[1].iter().any(|message| message
        .content
        .text_content()
        .contains("format_preservation_unproven")));
}

#[tokio::test]
async fn review_regression_b_stale_base_is_rejected_with_complete_feedback() {
    let body = "# title\nkeep";
    let fixture = format_write_fixture(FORMAT_MESSAGE, body);
    let mut call = review_replace_call("stale", body, 0..1, "##");
    let mut args: serde_json::Value = serde_json::from_str(&call.function.arguments).unwrap();
    args["base_content_hash"] = serde_json::json!(content_hash_str("stale document"));
    call.function.arguments = args.to_string();
    let (result, provider) = review_format_loop(&fixture, vec![call]).await;
    assert!(
        result.is_ok(),
        "stale proposal must be correctable: {result:?}"
    );
    assert_eq!(fixture.confirmation_count(), 0);
    assert_eq!(fixture.note_body(), body);
    assert_eq!(
        provider.transcripts.lock().unwrap()[1]
            .iter()
            .filter_map(|message| message.tool_call_id.as_deref())
            .collect::<Vec<_>>(),
        ["stale"]
    );
}

#[test]
fn review_regression_b_unknown_block_context_is_not_proven() {
    for (before, after) in [
        ("paragraph\n123. item", "paragraph\n1. item"),
        ("a | b\n---|---\n1. item | b", "a | b\n---|---\n2. item | b"),
        ("* * *", "- * *"),
        ("1234567890. reference", "1. reference"),
    ] {
        assert!(!check_format_preservation(before, after).is_proven());
    }
}

#[tokio::test]
async fn review_regression_b_mixed_batch_has_no_hanging_calls() {
    let body = "# title";
    let fixture = format_write_fixture(FORMAT_MESSAGE, body);
    let (result, provider) = review_format_loop(
        &fixture,
        vec![
            review_replace_call("write", body, 0..1, "##"),
            ToolCall::new("unavailable", "read_note", "{}"),
        ],
    )
    .await;
    assert!(
        result.is_ok(),
        "invalid mixed batch is recoverable: {result:?}"
    );
    let transcripts = provider.transcripts.lock().unwrap();
    assert_eq!(
        transcripts[1]
            .iter()
            .filter_map(|message| message.tool_call_id.as_deref())
            .collect::<Vec<_>>(),
        ["write", "unavailable"]
    );
    assert_eq!(fixture.confirmation_count(), 0);
    assert_eq!(fixture.note_body(), body);
}

fn review_format_response(call: ToolCall) -> super::model_gateway::GatewayResponse {
    super::model_gateway::GatewayResponse {
        tool_calls: vec![call],
        finish_reason: "tool_calls".into(),
        usage: crate::ai_types::TokenUsage {
            prompt_tokens: 1,
            completion_tokens: 1,
            total_tokens: 2,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[tokio::test]
async fn review_regression_b_second_corrected_candidate_freezes_only_new_plan() {
    let body = "# title";
    let fixture = format_write_fixture(FORMAT_MESSAGE, body);
    let (result, provider) = review_format_script(
        &fixture,
        vec![
            Ok(review_format_response(review_replace_call(
                "bad",
                body,
                2..7,
                "lost",
            ))),
            Ok(review_format_response(review_replace_call(
                "corrected",
                body,
                0..1,
                "##",
            ))),
        ],
        RunBudgetPolicy::standard(),
    )
    .await;
    assert_eq!(result.unwrap_err().to_string(), CONFIRMATION_PENDING_ERROR);
    assert_eq!(fixture.confirmation_count(), 1);
    let plan_json: String = fixture
        .state
        .db
        .with_read_conn(|conn| {
            Ok(conn.query_row(
                "SELECT plan_json FROM agent_run_confirmations WHERE run_id = ?1",
                [&fixture.accepted.run_id],
                |row| row.get(0),
            )?)
        })
        .unwrap();
    let plan =
        super::frozen_change_plan::FrozenChangePlan::from_persisted_plan_json(&plan_json).unwrap();
    assert_eq!(plan.operations().len(), 1);
    assert_eq!(plan.operations()[0].tool_call_id(), "corrected");
    assert_eq!(
        plan.operations()[0].base_content_hashes()[0].1,
        content_hash_str(body)
    );
    assert_eq!(
        plan.operations()[0].expected_post_content_hashes()[0].1,
        content_hash_str("## title")
    );
    assert_eq!(fixture.note_body(), body);
    let transcripts = provider.transcripts.lock().unwrap();
    assert_eq!(transcripts.len(), 2);
    assert!(transcripts[1]
        .iter()
        .any(|message| message.tool_call_id.as_deref() == Some("bad")
            && message
                .content
                .text_content()
                .contains("format_preservation_unproven")));
}

#[tokio::test]
async fn review_regression_b_exhausted_correction_reports_unconfirmed_checks() {
    for provider_limit in [false, true] {
        let body = "# title";
        let fixture = format_write_fixture(FORMAT_MESSAGE, body);
        let mut policy = RunBudgetPolicy::standard();
        policy.max_model_turns = 2;
        let bad = review_format_response(review_replace_call("bad", body, 2..7, "lost"));
        let second = if provider_limit {
            Err(crate::error::AppError::run(
                super::run_contract::SafeRunErrorCode::ToolLoopLimit,
            ))
        } else {
            Ok(review_format_response(review_replace_call(
                "still-bad",
                body,
                2..7,
                "lost",
            )))
        };
        let (result, provider) =
            review_format_script(&fixture, vec![Ok(bad), second], policy).await;
        let outcome = result.expect("budget exhaustion after rejection is a bounded limitation");
        assert!(outcome.terminal.is_host_authored());
        assert!(
            outcome.content.contains("未进入确认"),
            "{}",
            outcome.content
        );
        assert!(outcome.content.contains("未写入"), "{}", outcome.content);
        assert!(outcome.content.contains("正文内容"), "{}", outcome.content);
        assert!(outcome.content.contains("预算"), "{}", outcome.content);
        assert!(!outcome.content.contains(body));
        assert_eq!(outcome.tool_calls, 0);
        assert_eq!(fixture.confirmation_count(), 0);
        assert_eq!(fixture.note_body(), body);
        assert_eq!(provider.transcripts.lock().unwrap().len(), 2);
    }
}

#[tokio::test]
async fn review_regression_b_cancel_after_rejection_does_not_publish_limitation() {
    let body = "# title";
    let fixture = format_write_fixture(FORMAT_MESSAGE, body);
    let (result, _) = review_format_script(
        &fixture,
        vec![
            Ok(review_format_response(review_replace_call(
                "bad",
                body,
                2..7,
                "lost",
            ))),
            Err(crate::error::AppError::run(
                super::run_contract::SafeRunErrorCode::Cancelled,
            )),
        ],
        RunBudgetPolicy::standard(),
    )
    .await;
    assert_eq!(
        super::run_contract::SafeRunErrorCode::from_app_error(&result.unwrap_err()),
        super::run_contract::SafeRunErrorCode::Cancelled
    );
    assert_eq!(fixture.confirmation_count(), 0);
    assert_eq!(fixture.note_body(), body);
}
