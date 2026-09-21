//! D05 wave 1: Q14 local-transform zero web.search, Draft≠Apply, and
//! unknown write-receipt recovery. Not D05 acceptance; not V05.

use std::sync::{Arc, Mutex};

use super::agent_run_repository::{
    AgentRunRepository, AppendRunCheckpointInput, AppendRunEventInput, DurableApplyCheckpoint,
    DurableApplyCheckpointStage,
};
use super::frozen_change_plan::{
    FrozenChangeOperationInput, FrozenChangePlan, FrozenChangeSetInput,
};
use super::run_context::RunContextAssembler;
use super::run_contract::{
    AssistantRunAccepted, AssistantRunEvent, AssistantRunStartRequest, AssistantTurnDraft,
    CapabilityId, Effect, Effort, ExplicitAction, ExplicitTarget, Freshness, RunBudgetPolicy,
    RunEventPayload, RunEventType, RunRecoveryKind, RunState, SecurityDomain, WebDecisionReason,
};
use super::run_engine::{RunEngine, RunEventSink};
use super::run_intake::RunIntake;
use super::run_tool_loop::NormalRunToolExecutor;
use crate::app::AppState;
use crate::cas::hash::content_hash_str;
use crate::error::AppResult;

fn request() -> AssistantRunStartRequest {
    AssistantRunStartRequest {
        client_request_id: "d05-client".into(),
        session: None,
        turn: AssistantTurnDraft {
            message: "请概述这份资料的要点".into(),
            content_parts: None,
            explicit_references: vec![],
            retrieval_scope: Default::default(),
            display_mentions: vec![],
        },
        explicit_action: None,
        web_enabled: false,
        model_override: None,
        external_tool_grants: Vec::new(),
        security_domain: SecurityDomain::Normal,
        classified_context_ref: None,
    }
}

fn valid_content_hash() -> String {
    "a".repeat(64)
}

fn valid_reference() -> crate::ai_types::ContextReferenceWire {
    crate::ai_types::ContextReferenceWire {
        id: "reference".into(),
        kind: crate::ai_types::ContextReferenceKind::Note,
        file_path: Some("notes/reference.md".into()),
        content_hash: Some(valid_content_hash()),
        utf8_range: None,
        editor_range: None,
        excerpt: String::new(),
        heading_path: None,
        anchor: None,
        stale: false,
        invalid_reason: None,
    }
}

fn has_capability(envelope: &super::run_contract::ExecutionEnvelope, name: &str) -> bool {
    envelope
        .required_capabilities
        .iter()
        .any(|capability| capability.as_str() == name)
}

#[test]
fn q14_polish_with_web_toggle_on_does_not_grant_web_search() {
    let mut start = request();
    start.web_enabled = true;
    start.turn.message = "将这段话润色成正式通知。".into();

    let envelope = RunIntake::resolve_envelope(&start).expect("resolve polish");

    assert_eq!(envelope.freshness, Freshness::Offline);
    assert_eq!(envelope.web_reason, WebDecisionReason::LocalTransformation);
    assert!(!has_capability(&envelope, "web.search"));
    assert_eq!(
        envelope.effort,
        Effort::Direct,
        "web toggle must not promote a local polish into ToolLoop"
    );
}

#[test]
fn q14_format_request_with_attached_note_stays_offline_for_web() {
    let mut start = request();
    start.web_enabled = true;
    start.turn.message = "请把这篇笔记做格式整理。".into();
    start.turn.explicit_references = vec![valid_reference()];

    let envelope = RunIntake::resolve_envelope(&start).expect("resolve attached format");

    assert_eq!(envelope.freshness, Freshness::Offline);
    assert_eq!(envelope.web_reason, WebDecisionReason::LocalTransformation);
    assert!(!has_capability(&envelope, "web.search"));
    assert!(
        !has_capability(&envelope, "note.propose_patch"),
        "Answer effect must not gain propose_patch"
    );
}

#[test]
fn q14_translate_colon_verify_url_keeps_web_search() {
    let mut start = request();
    start.web_enabled = true;
    start.turn.message = "翻译：请核实 https://example.com/release".into();

    let envelope = RunIntake::resolve_envelope(&start).expect("resolve colon verify");

    assert!(
        has_capability(&envelope, "web.search"),
        "colon-split body 核实/URL must not be eaten by LocalTransformation, got {:?}",
        envelope.web_reason
    );
    assert_ne!(envelope.web_reason, WebDecisionReason::LocalTransformation);
}

#[test]
fn q14_colon_body_url_without_verify_stays_offline() {
    let mut start = request();
    start.web_enabled = true;
    start.turn.message = "翻译：正文里引用 https://example.com/spec 作为资料".into();

    let envelope = RunIntake::resolve_envelope(&start).expect("resolve colon body url");

    assert_eq!(envelope.freshness, Freshness::Offline);
    assert_eq!(envelope.web_reason, WebDecisionReason::LocalTransformation);
    assert!(!has_capability(&envelope, "web.search"));
}

#[test]
fn q14_chat_greeting_keeps_default_online_when_web_toggle_on() {
    let mut start = request();
    start.web_enabled = true;
    start.turn.message = "你好".into();

    let envelope = RunIntake::resolve_envelope(&start).expect("resolve greeting");

    assert_eq!(envelope.freshness, Freshness::WebPreferred);
    assert_eq!(envelope.web_reason, WebDecisionReason::DefaultOnline);
    assert!(has_capability(&envelope, "web.search"));
}

#[test]
fn q14_standardization_question_is_not_local_transformation() {
    let mut start = request();
    start.web_enabled = true;
    start.turn.message = "规范化的项目管理方法有哪些？".into();

    let envelope = RunIntake::resolve_envelope(&start).expect("resolve standardization question");

    assert_ne!(envelope.web_reason, WebDecisionReason::LocalTransformation);
    assert!(
        has_capability(&envelope, "web.search"),
        "bare 规范化 in a knowledge question must not revoke web.search"
    );
}

#[test]
fn q14_translate_and_verify_keeps_web_search() {
    let mut start = request();
    start.web_enabled = true;
    start.turn.message = "润色这段并请联网核实发布日期".into();

    let envelope = RunIntake::resolve_envelope(&start).expect("resolve translate and verify");

    assert!(
        has_capability(&envelope, "web.search"),
        "explicit web instruction must keep web.search"
    );
    assert_ne!(envelope.web_reason, WebDecisionReason::LocalTransformation);
}

#[test]
fn q14_summarize_latest_news_keeps_web_search() {
    let mut start = request();
    start.web_enabled = true;
    start.turn.message = "Summarize the latest breaking news.".into();

    let envelope = RunIntake::resolve_envelope(&start).expect("resolve summarize news");

    assert_eq!(envelope.freshness, Freshness::WebPreferred);
    assert_eq!(envelope.web_reason, WebDecisionReason::VolatileExternalFact);
    assert!(has_capability(&envelope, "web.search"));
    assert_ne!(envelope.web_reason, WebDecisionReason::LocalTransformation);
}

#[test]
fn q14_draft_effect_with_web_toggle_still_has_no_apply_patch() {
    let mut start = request();
    start.web_enabled = true;
    start.turn.message = "将这段话润色成正式通知。".into();
    start.explicit_action = Some(ExplicitAction {
        effect: Effect::Draft,
        target: None,
        selection_snapshot: None,
    });

    let envelope = RunIntake::resolve_envelope(&start).expect("resolve draft polish");

    assert_eq!(envelope.freshness, Freshness::Offline);
    assert_eq!(envelope.web_reason, WebDecisionReason::LocalTransformation);
    assert!(has_capability(&envelope, "note.propose_patch"));
    assert!(!has_capability(&envelope, "note.apply_patch"));
    assert!(!has_capability(&envelope, "web.search"));
}

const NOTE_PATH: &str = "notes/a.md";
const BASE_BODY: &str = "base";
const AFTER_BODY: &str = "after";
const FINAL_BODY: &str = "final";
const UNKNOWN_BODY: &str = "other";

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

struct DispatchingApplyFixture {
    _directory: tempfile::TempDir,
    state: Arc<AppState>,
    vault: std::path::PathBuf,
    accepted: AssistantRunAccepted,
    context: super::run_context::RunContext,
    plan: FrozenChangePlan,
    sink: RecordingSink,
}

fn replace_operation(tool_call_id: String, from: &str, to: &str) -> FrozenChangeOperationInput {
    FrozenChangeOperationInput {
        tool_call_id,
        operation: "replace_selection".into(),
        relative_paths: vec![NOTE_PATH.into()],
        base_content_hashes: vec![(NOTE_PATH.into(), content_hash_str(from))],
        expected_post_content_hashes: vec![(NOTE_PATH.into(), content_hash_str(to))],
        change: serde_json::json!({
            "target_path": NOTE_PATH,
            "base_content_hash": content_hash_str(from),
            "range": { "start": 0, "end": from.len() },
            "original_text": from,
            "replacement": to
        }),
        rollback_summary: "可通过版本历史撤销".into(),
    }
}

fn dispatching_apply_fixture(operation_count: usize) -> DispatchingApplyFixture {
    assert!(
        operation_count == 1 || operation_count == 2,
        "D05 receipt fixtures cover a single write or a two-op suffix"
    );
    let directory = tempfile::tempdir().expect("tempdir");
    let vault = directory.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes dir");
    std::fs::write(vault.join(NOTE_PATH), BASE_BODY).expect("write base note");
    let state = AppState::new(directory.path().join("data")).expect("app state");
    state.set_vault(vault.clone()).expect("activate vault");
    let vault = state.vault_path().expect("live vault");
    state
        .db
        .with_conn(|conn| crate::indexer::scan::index_file(conn, &vault, &vault.join(NOTE_PATH)))
        .expect("index note");

    let base_hash = content_hash_str(BASE_BODY);
    let mut start = request();
    start.client_request_id = format!("d05-receipt-{}", uuid::Uuid::new_v4());
    start.turn.message = "将已确认的修改应用到笔记".into();
    start
        .turn
        .explicit_references
        .push(crate::ai_types::ContextReferenceWire {
            id: "target-note".into(),
            kind: crate::ai_types::ContextReferenceKind::Note,
            file_path: Some(NOTE_PATH.into()),
            content_hash: Some(base_hash.clone()),
            utf8_range: None,
            editor_range: None,
            excerpt: String::new(),
            heading_path: None,
            anchor: None,
            stale: false,
            invalid_reason: None,
        });
    start.explicit_action = Some(ExplicitAction {
        effect: Effect::Apply,
        target: Some(ExplicitTarget {
            reference_id: "target-note".into(),
            content_hash: base_hash.clone(),
        }),
        selection_snapshot: None,
    });

    let accepted = RunIntake::start(&state.db, start).expect("accept durable apply");
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
                stage: "正在生成变更预览".into(),
                stage_code: None,
            },
        },
    )
    .expect("running");
    let session_id = state
        .db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT session_id FROM agent_runs WHERE run_id = ?1",
                [&accepted.run_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .expect("session id");
    let mut operations = vec![replace_operation(
        format!("tool-{}", accepted.run_id),
        BASE_BODY,
        AFTER_BODY,
    )];
    if operation_count == 2 {
        operations.push(replace_operation(
            format!("tool-{}-suffix", accepted.run_id),
            AFTER_BODY,
            FINAL_BODY,
        ));
    }
    let plan = FrozenChangePlan::freeze_set(FrozenChangeSetInput {
        confirmation_id: format!("confirmation-{}", accepted.run_id),
        run_id: accepted.run_id.clone(),
        session_id,
        request_id: accepted.run_id.clone(),
        vault_id: content_hash_str(&vault.to_string_lossy()),
        operations,
        expires_at_unix_ms: i64::MAX,
    })
    .expect("freeze change set");
    let mut tool_started_version = running.state_version();
    for operation in plan.operations() {
        let started = AgentRunRepository::append_event(
            &state.db,
            AppendRunEventInput {
                run_id: accepted.run_id.clone(),
                state_version: tool_started_version,
                event_type: RunEventType::ToolStarted,
                payload: RunEventPayload::ToolStarted {
                    capability: operation.operation().to_string(),
                    tool_call_id: operation.tool_call_id().to_string(),
                },
            },
        )
        .expect("tool started");
        tool_started_version = started.state_version();
    }
    let awaiting = AgentRunRepository::request_frozen_confirmation(
        &state.db,
        &plan,
        tool_started_version,
        "等待确认：更新 1 个目标",
    )
    .expect("await confirmation");
    AgentRunRepository::approve_frozen_confirmation(
        &state.db,
        &accepted.session.session_key,
        &accepted.run_id,
        plan.confirmation_id(),
        plan.plan_hash(),
        awaiting.state_version(),
        0,
    )
    .expect("consume confirmation");
    let state_version = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run")
        .run
        .state_version;
    AgentRunRepository::append_checkpoint_step(
        &state.db,
        AppendRunCheckpointInput {
            run_id: accepted.run_id.clone(),
            state_version,
            checkpoint: DurableApplyCheckpoint::new_change_set(
                plan.confirmation_id(),
                plan.plan_hash(),
                DurableApplyCheckpointStage::Dispatching,
                plan.all_base_content_hashes()
                    .iter()
                    .map(|(_, hash)| hash.clone())
                    .collect(),
                plan.all_expected_post_content_hashes()
                    .iter()
                    .map(|(_, hash)| hash.clone())
                    .collect(),
                0,
                plan.operations().len(),
                Vec::new(),
            )
            .expect("dispatching checkpoint"),
        },
    )
    .expect("persist dispatching");
    let context = RunContextAssembler::assemble(
        &state.db,
        Some(&vault),
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .expect("assemble apply context");
    DispatchingApplyFixture {
        _directory: directory,
        state,
        vault,
        accepted,
        context,
        plan,
        sink,
    }
}

impl DispatchingApplyFixture {
    fn write_note(&self, body: &str) {
        std::fs::write(self.vault.join(NOTE_PATH), body).expect("mutate note");
    }

    fn note_body(&self) -> String {
        std::fs::read_to_string(self.vault.join(NOTE_PATH)).expect("read note")
    }

    fn version_count(&self) -> usize {
        crate::version::version_list(&self.state, NOTE_PATH)
            .expect("version list")
            .len()
    }

    fn latest_checkpoint(&self) -> DurableApplyCheckpoint {
        AgentRunRepository::latest_durable_apply_checkpoint(&self.state.db, &self.accepted.run_id)
            .expect("latest checkpoint")
            .expect("durable apply checkpoint")
    }

    async fn execute_confirmed(&self) -> Vec<crate::ai_runtime::ToolCallResult> {
        NormalRunToolExecutor::new(
            &self.state,
            None,
            &self.accepted,
            &self.context,
            vec![CapabilityId::new("note.apply_patch")],
            RunBudgetPolicy::for_envelope(&self.context.envelope),
            &self.sink,
            Vec::new(),
        )
        .execute_confirmed_frozen_change_set(&self.plan)
        .await
        .expect("execute confirmed change set")
    }
}

#[tokio::test]
async fn d05_dispatching_checkpoint_skips_rewrite_when_expected_hash_already_on_disk() {
    let fixture = dispatching_apply_fixture(1);
    fixture.write_note(AFTER_BODY);

    let results = fixture.execute_confirmed().await;

    assert_eq!(results.len(), 1);
    assert!(
        results[0].success,
        "expected_post on disk must skip rewrite, got {:?}",
        results[0].error
    );
    assert_eq!(fixture.note_body(), AFTER_BODY);
    assert_eq!(
        fixture.version_count(),
        0,
        "skipping a landed write must not create another snapshot"
    );
    let checkpoint = fixture.latest_checkpoint();
    assert_eq!(checkpoint.stage(), DurableApplyCheckpointStage::Applied);
    assert_eq!(checkpoint.next_operation_index(), 1);
}

#[tokio::test]
async fn d05_dispatching_checkpoint_dispatches_once_when_base_hash_still_present() {
    let fixture = dispatching_apply_fixture(1);

    let results = fixture.execute_confirmed().await;

    assert_eq!(results.len(), 1);
    assert!(
        results[0].success,
        "base still on disk must dispatch once, got {:?}",
        results[0].error
    );
    assert_eq!(fixture.note_body(), AFTER_BODY);
    assert_eq!(fixture.version_count(), 1);
    let checkpoint = fixture.latest_checkpoint();
    assert_eq!(checkpoint.stage(), DurableApplyCheckpointStage::Applied);
    assert_eq!(checkpoint.next_operation_index(), 1);
}

#[tokio::test]
async fn d05_dispatching_checkpoint_unknown_receipt_does_not_retry() {
    let fixture = dispatching_apply_fixture(2);
    fixture.write_note(UNKNOWN_BODY);

    let results = fixture.execute_confirmed().await;

    assert_eq!(results.len(), 2);
    assert_eq!(
        results[0].error.as_deref(),
        Some("frozen_change_write_receipt_unknown")
    );
    assert!(!results[0].success);
    assert!(!results[1].success);
    assert_eq!(
        results[1].output["error"], "confirmed_operation_not_executed",
        "unknown receipt must stop the suffix"
    );
    let joined = format!(
        "{}{}",
        results[0].error.clone().unwrap_or_default(),
        results[0].output
    );
    assert!(
        !joined.contains("模型能力降级"),
        "unknown receipt must use a stable write-receipt error, not capability degradation copy"
    );
    assert_eq!(fixture.note_body(), UNKNOWN_BODY);
    assert_eq!(fixture.version_count(), 0);
    let checkpoint = fixture.latest_checkpoint();
    assert_eq!(
        checkpoint.stage(),
        DurableApplyCheckpointStage::Dispatching,
        "unknown receipt must not be recorded as Applied"
    );
    assert_eq!(checkpoint.next_operation_index(), 0);
}

#[tokio::test]
async fn d05_dispatching_checkpoint_skips_prefix_and_dispatches_suffix() {
    let fixture = dispatching_apply_fixture(2);
    fixture.write_note(AFTER_BODY);

    let results = fixture.execute_confirmed().await;

    assert_eq!(results.len(), 2);
    assert!(
        results[0].success,
        "prefix expected_post on disk must skip rewrite, got {:?}",
        results[0].error
    );
    assert!(
        results[1].success,
        "suffix still at its base must dispatch once, got {:?}",
        results[1].error
    );
    assert_eq!(fixture.note_body(), FINAL_BODY);
    assert_eq!(
        fixture.version_count(),
        1,
        "only the unapplied suffix may create a snapshot"
    );
    let checkpoint = fixture.latest_checkpoint();
    assert_eq!(checkpoint.stage(), DurableApplyCheckpointStage::Applied);
    assert_eq!(checkpoint.next_operation_index(), 2);
}

#[test]
fn d05_dispatching_recovery_resumes_suffix_when_prefix_hash_already_on_disk() {
    let fixture = dispatching_apply_fixture(2);
    fixture.write_note(AFTER_BODY);

    assert_eq!(
        RunEngine::recover_interrupted_runs(&fixture.state.db).expect("recover dispatching prefix"),
        1
    );
    let replay = RunIntake::get(
        &fixture.state.db,
        &fixture.accepted.session,
        &fixture.accepted.run_id,
    )
    .expect("replay")
    .expect("run");
    assert_eq!(replay.run.state, RunState::Paused);
    assert_eq!(replay.run.recovery, Some(RunRecoveryKind::ResumeAvailable));
    assert_eq!(fixture.note_body(), AFTER_BODY);
    let checkpoint = fixture.latest_checkpoint();
    assert_eq!(
        checkpoint.stage(),
        DurableApplyCheckpointStage::Dispatching,
        "recovery must not treat Dispatching as Applied without execute"
    );
    assert_eq!(checkpoint.next_operation_index(), 0);
}

#[test]
fn d05_write_receipt_classifier_distinguishes_base_expected_and_unknown() {
    use super::frozen_change_plan::{classify_frozen_write_receipt, FrozenWriteReceipt};
    assert_eq!(
        classify_frozen_write_receipt("base", "base", "after"),
        FrozenWriteReceipt::NotYetApplied
    );
    assert_eq!(
        classify_frozen_write_receipt("after", "base", "after"),
        FrozenWriteReceipt::AlreadyApplied
    );
    assert_eq!(
        classify_frozen_write_receipt("other", "base", "after"),
        FrozenWriteReceipt::Unknown
    );
}
