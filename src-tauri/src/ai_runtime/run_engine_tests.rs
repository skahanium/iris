use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};

use super::domain_executor::{AuthorizedDomainMaterial, DomainExecutor, DomainMaterialRole};
use super::policy_decision_engine::RunPolicyDecision;
use super::run_contract::CapabilityId;
use super::run_contract::{
    AssistantRunStartRequest, RunEventPayload, RunEventType, RunState, SafeRunErrorCode,
    SecurityDomain,
};
use super::run_engine::{
    direct_gateway_request, AgentRunStreamObserver, DirectAnswerProvider, RunEngine, RunEventSink,
    StreamingDirectAnswerProvider,
};
use super::run_intake::RunIntake;
use crate::ai_runtime::agent_evidence_repository::{
    AgentEvidenceRepository, LocalEvidenceInput, MaterialRole,
};
use crate::ai_runtime::agent_run_repository::{AgentRunRepository, AppendRunEventInput};
use crate::ai_runtime::model_gateway::{
    StreamEvent, StreamEventData, StreamEventObserver, StreamEventType, StreamSurface,
};
use crate::ai_types::{CapabilitySlot, EndpointFamily, MessageRole, ProviderConfig};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

struct MockProvider {
    calls: Cell<u32>,
    response: Option<String>,
}

struct MockStreamingProvider {
    calls: AtomicU32,
    failure: Option<&'static str>,
}

#[derive(Default)]
struct RecordingSink {
    events: std::sync::Mutex<Vec<serde_json::Value>>,
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

impl DirectAnswerProvider for MockProvider {
    fn answer(&self, _run_id: &str, _message: &str) -> AppResult<String> {
        self.calls.set(self.calls.get() + 1);
        self.response
            .clone()
            .ok_or_else(|| AppError::msg("must not call provider"))
    }
}

impl StreamingDirectAnswerProvider for MockStreamingProvider {
    fn answer_streaming<'a>(
        &'a self,
        run_id: &'a str,
        _message: &'a str,
        observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<
        Box<
            dyn Future<Output = AppResult<crate::ai_runtime::model_gateway::GatewayResponse>>
                + Send
                + 'a,
        >,
    > {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if let Some(failure) = self.failure {
                return Err(AppError::msg(failure));
            }
            observer.observe(
                &StreamEvent {
                    request_id: run_id.to_string(),
                    event_type: StreamEventType::Token,
                    data: StreamEventData::Token {
                        token: "流式片段".to_string(),
                    },
                    surface: StreamSurface::VisibleAnswer,
                    classified: false,
                },
                0,
            )?;
            Ok(crate::ai_runtime::model_gateway::GatewayResponse {
                content: Some("流式最终答复".to_string()),
                tool_calls: vec![],
                usage: Default::default(),
                finish_reason: "stop".to_string(),
                reasoning_content: None,
            })
        })
    }
}

fn request() -> AssistantRunStartRequest {
    AssistantRunStartRequest {
        client_request_id: "engine-client-request".to_string(),
        session: None,
        message: "请给出最小直答".to_string(),
        content_parts: None,
        explicit_references: vec![],
        explicit_action: None,
        web_enabled: false,
        security_domain: SecurityDomain::Normal,
    }
}

#[test]
fn direct_engine_calls_provider_once_and_finalizes_one_run() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let provider = MockProvider {
        calls: Cell::new(0),
        response: Some("唯一答复".to_string()),
    };

    let sink = RecordingSink::default();
    RunEngine::execute_direct_with_sink(&db, &accepted.session, &accepted.run_id, &provider, &sink)
        .expect("direct execution");

    assert_eq!(provider.calls.get(), 1);
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("get run")
        .expect("run exists");
    assert_eq!(replay.run.state, RunState::Completed);
    assert_eq!(replay.events.len(), 4);
    assert!(replay.run.final_message_id.is_some());
    let emitted = sink.events.lock().expect("recording sink lock");
    assert_eq!(
        emitted.len(),
        3,
        "only persisted post-accepted events emit here"
    );
    assert_eq!(emitted[0]["type"], "stage_changed");
    assert_eq!(emitted[2]["type"], "completed");
}

#[test]
fn cancelled_run_never_dispatches_provider_or_completes() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    super::run_intake::RunIntake::control(
        &db,
        super::run_contract::AssistantRunControlRequest {
            session: accepted.session.clone(),
            run_id: accepted.run_id.clone(),
            expected_state_version: 0,
            action: super::run_contract::RunControlAction::Cancel,
        },
    )
    .expect("cancel");
    let provider = MockProvider {
        calls: Cell::new(0),
        response: None,
    };

    let error = RunEngine::execute_direct(&db, &accepted.session, &accepted.run_id, &provider)
        .expect_err("cancelled run cannot execute");
    assert_eq!(error.to_string(), "agent_run_terminal_state");
    assert_eq!(provider.calls.get(), 0);
    assert!(
        !crate::ai_runtime::model_gateway::is_abort_requested(&accepted.run_id),
        "a terminal Run that never reached dispatch must consume its abort marker"
    );
}

#[test]
fn provider_failure_persists_a_safe_failed_terminal_event_without_an_assistant_message() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let provider = MockProvider {
        calls: Cell::new(0),
        response: None,
    };
    let sink = RecordingSink::default();

    let error = RunEngine::execute_direct_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        &provider,
        &sink,
    )
    .expect_err("provider failure is surfaced as a safe run failure");
    assert_eq!(error.to_string(), "agent_run_provider_unavailable");
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("get failed run")
        .expect("run exists");
    assert_eq!(provider.calls.get(), 1);
    assert_eq!(replay.run.state, RunState::Failed);
    assert!(replay.run.final_message_id.is_none());
    assert_eq!(
        serde_json::to_value(replay.events.last().expect("failed event"))
            .expect("serialize failed event")["type"],
        "failed"
    );
    assert_eq!(
        sink.events
            .lock()
            .expect("recording sink lock")
            .last()
            .expect("emitted failed event")["type"],
        "failed"
    );
}

#[test]
fn denied_policy_is_persisted_before_provider_dispatch() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let sink = RecordingSink::default();
    let decision = RunPolicyDecision {
        allowed_capabilities: Vec::new(),
        denied_capabilities: vec![CapabilityId::new("model.text")],
        denial_code: Some(SafeRunErrorCode::PermissionDenied),
    };

    let allowed = RunEngine::enforce_policy_before_dispatch_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        &decision,
        &sink,
    )
    .expect("policy decision is persisted");

    assert!(!allowed);
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run exists");
    assert_eq!(replay.run.state, RunState::Failed);
    assert_eq!(replay.events.len(), 4);
    assert_eq!(
        serde_json::to_value(&replay.events[1]).expect("serialize permission event")["type"],
        "permission_denied"
    );
    assert_eq!(
        sink.events.lock().expect("sink lock")[0]["type"],
        "permission_denied"
    );
}
#[test]
fn preparation_failure_after_acceptance_persists_a_safe_failed_terminal_event() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let sink = RecordingSink::default();

    RunEngine::fail_before_dispatch_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        SafeRunErrorCode::ProviderUnavailable,
        &sink,
    )
    .expect("accepted run must become a safe failed terminal run");

    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("get failed run")
        .expect("run exists");
    assert_eq!(replay.run.state, RunState::Failed);
    assert!(replay.run.final_message_id.is_none());
    assert_eq!(replay.events.len(), 3);
    assert_eq!(
        serde_json::to_value(replay.events.last().expect("failed event"))
            .expect("serialize failed event")["type"],
        "failed"
    );
    assert_eq!(
        sink.events
            .lock()
            .expect("recording sink lock")
            .last()
            .expect("emitted failed event")["type"],
        "failed"
    );
}

#[test]
fn background_failure_guard_terminalizes_a_running_run_without_exposing_its_cause() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let preparing = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: 0,
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Preparing,
                stage: "正在准备".into(),
            },
        },
    )
    .expect("preparing");
    AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: event_state_version(&preparing),
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Running,
                stage: "正在处理".into(),
            },
        },
    )
    .expect("running");
    let sink = RecordingSink::default();

    assert!(
        RunEngine::fail_active_with_sink(&db, &accepted.session, &accepted.run_id, &sink,)
            .expect("guard failure")
    );
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    let failed =
        serde_json::to_value(replay.events.last().expect("failed")).expect("serialize failed");
    assert_eq!(replay.run.state, RunState::Failed);
    assert_eq!(failed["payload"]["code"], "agent_run_persistence_failed");
    assert!(!failed.to_string().contains("unexpected orchestration"));
}

#[test]
fn startup_recovery_terminalizes_interrupted_direct_runs_for_replay() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");

    assert_eq!(
        RunEngine::recover_interrupted_runs(&db).expect("recover interrupted runs"),
        1
    );
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Failed);
    assert_eq!(
        serde_json::to_value(replay.events.last().expect("failure")).expect("serialize failure")
            ["payload"]["message"],
        "运行因应用关闭而中断，请重新提交请求"
    );
}

#[test]
fn run_stream_observer_buffers_tokens_until_a_stable_flush() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
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
                stage: "正在生成答复".to_string(),
            },
        },
    )
    .expect("running");
    let sink = RecordingSink::default();
    let mut observer =
        AgentRunStreamObserver::new(&db, &accepted.run_id, event_state_version(&running), &sink);

    for (token_index, token) in ["稳定", "片段"].into_iter().enumerate() {
        observer
            .observe(
                &StreamEvent {
                    request_id: accepted.run_id.clone(),
                    event_type: StreamEventType::Token,
                    data: StreamEventData::Token {
                        token: token.to_string(),
                    },
                    surface: StreamSurface::VisibleAnswer,
                    classified: false,
                },
                token_index as u32,
            )
            .expect("buffer stream token");
    }

    let before_flush = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay before flush")
        .expect("run exists");
    assert_eq!(before_flush.events.len(), 3);
    assert!(sink.events.lock().expect("sink lock").is_empty());

    observer.flush().expect("flush stable stream fragment");
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run exists");
    assert_eq!(replay.run.state, RunState::Running);
    assert_eq!(replay.run.state_version, event_state_version(&running));
    assert_eq!(replay.events.len(), 4);
    assert_eq!(
        serde_json::to_value(replay.events.last().expect("delta event")).expect("serialize delta")
            ["payload"]["delta"],
        "稳定片段"
    );
    assert_eq!(sink.events.lock().expect("sink lock").len(), 1);
}

#[tokio::test]
async fn streaming_direct_engine_persists_deltas_and_one_terminal_message() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let provider = MockStreamingProvider {
        calls: AtomicU32::new(0),
        failure: None,
    };
    let sink = RecordingSink::default();

    RunEngine::execute_direct_streaming_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        &provider,
        &sink,
    )
    .await
    .expect("streaming direct run");

    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run exists");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(replay.run.state, RunState::Completed);
    assert!(replay.run.final_message_id.is_some());
    assert_eq!(replay.events.len(), 5);
    assert_eq!(
        serde_json::to_value(&replay.events[3]).expect("serialize delta")["payload"]["delta"],
        "流式片段"
    );
    assert_eq!(
        serde_json::to_value(replay.events.last().expect("completed"))
            .expect("serialize completed")["type"],
        "completed"
    );
}

#[tokio::test]
async fn streaming_provider_failure_persists_a_safe_failed_terminal_event() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let provider = MockStreamingProvider {
        calls: AtomicU32::new(0),
        failure: Some("provider transport error"),
    };
    let sink = RecordingSink::default();

    let error = RunEngine::execute_direct_streaming_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        &provider,
        &sink,
    )
    .await
    .expect_err("provider failure");

    assert_eq!(error.to_string(), "agent_run_provider_unavailable");
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run exists");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(replay.run.state, RunState::Failed);
    assert!(replay.run.final_message_id.is_none());
    assert_eq!(
        serde_json::to_value(replay.events.last().expect("failed")).expect("serialize failed")
            ["type"],
        "failed"
    );
}

#[tokio::test]
async fn streaming_first_response_timeout_persists_a_distinct_safe_failure() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let provider = MockStreamingProvider {
        calls: AtomicU32::new(0),
        failure: Some("llm_stream_first_response_timeout"),
    };
    let sink = RecordingSink::default();

    let error = RunEngine::execute_direct_streaming_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        &provider,
        &sink,
    )
    .await
    .expect_err("a first-response timeout must become terminal");

    assert_eq!(error.to_string(), "agent_run_provider_timeout");
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run exists");
    assert_eq!(replay.run.state, RunState::Failed);
    let failed = serde_json::to_value(replay.events.last().expect("failed event"))
        .expect("serialize failed event");
    assert_eq!(failed["payload"]["code"], "agent_run_provider_timeout");
    assert_eq!(failed["payload"]["message"], "模型服务响应超时，请稍后重试");
}

#[tokio::test]
async fn streaming_prompt_execution_binds_registered_evidence_to_final_message() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let (session_id, message_seq): (i64, i64) = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT r.session_id, m.seq
                 FROM agent_runs r
                 JOIN session_messages m ON m.session_id = r.session_id AND m.turn_id = r.turn_id
                 WHERE r.run_id = ?1 AND m.role = 'user'",
                [&accepted.run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(Into::into)
        })
        .expect("run ownership");
    let evidence = AgentEvidenceRepository::register_local(
        &db,
        LocalEvidenceInput {
            session_id,
            run_id: accepted.run_id.clone(),
            message_seq_first: message_seq,
            material_role: MaterialRole::Reference,
            title: "explicit material".into(),
            source_path: "notes/evidence.md".into(),
            source_span_start: 0,
            source_span_end: 8,
            heading_path: None,
            content_hash: "evidence-hash".into(),
            retrieval_reason: Some("explicit_reference".into()),
            score: None,
        },
    )
    .expect("registered evidence");
    let provider = MockStreamingProvider {
        calls: AtomicU32::new(0),
        failure: None,
    };
    let sink = RecordingSink::default();

    RunEngine::execute_direct_streaming_with_prompt_and_evidence_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        "authorized material",
        &[evidence.evidence_id],
        &provider,
        &sink,
    )
    .await
    .expect("streaming execution");

    db.with_read_conn(|conn| {
        let evidence_json: String = conn.query_row(
            "SELECT evidence_refs_json FROM session_messages
             WHERE session_id = ?1 AND role = 'assistant'",
            [session_id],
            |row| row.get(0),
        )?;
        assert_eq!(evidence_json, format!("[{}]", evidence.evidence_id));
        Ok(())
    })
    .expect("final message evidence binding");
}
#[test]
fn direct_gateway_request_separates_fixed_boundary_from_user_data() {
    let request = direct_gateway_request(
        ProviderConfig {
            name: "provider".to_string(),
            base_url: "https://provider.example/v1".to_string(),
            api_key: Some("test-key".to_string()),
            model: "model".to_string(),
            slot: CapabilitySlot::Fast,
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
        },
        "只回答这条消息",
        1024,
    );

    assert_eq!(request.messages.len(), 2);
    assert!(matches!(request.messages[0].role, MessageRole::System));
    assert!(request.messages[0]
        .content
        .text_content()
        .contains("不可信数据"));
    assert!(matches!(request.messages[1].role, MessageRole::User));
    assert_eq!(request.messages[1].content.text_content(), "只回答这条消息");
    assert!(request.tools.is_empty());
    assert!(request.stream);
    assert!(!request.thinking);
    assert_eq!(request.max_tokens, Some(1024));
}

fn event_state_version(event: &super::run_contract::AssistantRunEvent) -> u64 {
    serde_json::to_value(event).expect("serialize event")["stateVersion"]
        .as_u64()
        .expect("state version")
}

struct LeakingStreamingProvider;

impl StreamingDirectAnswerProvider for LeakingStreamingProvider {
    fn answer_streaming<'a>(
        &'a self,
        run_id: &'a str,
        _message: &'a str,
        observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<
        Box<
            dyn Future<Output = AppResult<crate::ai_runtime::model_gateway::GatewayResponse>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let leaked = "北京市教育局将于2026年3月12日组织专项检查。".to_string();
            observer.observe(
                &StreamEvent {
                    request_id: run_id.to_string(),
                    event_type: StreamEventType::Token,
                    data: StreamEventData::Token {
                        token: leaked.clone(),
                    },
                    surface: StreamSurface::VisibleAnswer,
                    classified: false,
                },
                0,
            )?;
            Ok(crate::ai_runtime::model_gateway::GatewayResponse {
                content: Some(leaked),
                tool_calls: vec![],
                usage: Default::default(),
                finish_reason: "stop".to_string(),
                reasoning_content: None,
            })
        })
    }
}

#[tokio::test]
async fn domain_verifier_rejects_exemplar_fact_before_any_visible_delta_or_final_persistence() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let plan = DomainExecutor::plan(
        &super::run_contract::ExecutionEnvelope {
            effect: super::run_contract::Effect::Draft,
            context: super::run_contract::ContextMode::ExplicitReferences,
            freshness: super::run_contract::Freshness::Offline,
            effort: super::run_contract::Effort::Direct,
            security_domain: SecurityDomain::Normal,
            risk: super::run_contract::RiskClass::ReadOnly,
            modalities: vec![super::run_contract::Modality::Text],
            material_needs: vec![super::run_contract::MaterialNeed::Exemplar],
            required_capabilities: vec![CapabilityId::new("model.text")],
            explicit_constraints: vec![],
        },
        "起草一份检查通知",
        &[AuthorizedDomainMaterial {
            role: DomainMaterialRole::Exemplar,
            label: "通知范文".into(),
            content: "北京市教育局将于2026年3月12日组织专项检查。".into(),
        }],
        &[],
    );
    let sink = RecordingSink::default();

    let error = RunEngine::execute_direct_streaming_with_prompt_evidence_and_domain_plan_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        "authorized prompt",
        &[],
        &plan,
        &LeakingStreamingProvider,
        &sink,
    )
    .await
    .expect_err("exemplar-only facts must be rejected before persistence");

    assert_eq!(error.to_string(), "agent_run_domain_verification_failed");
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run exists");
    assert_eq!(replay.run.state, RunState::Failed);
    assert!(replay.run.final_message_id.is_none());
    assert!(replay.events.iter().all(|event| {
        serde_json::to_value(event).expect("serialize event")["type"] != "content_delta"
    }));
}

#[tokio::test]
async fn run_tool_loop_web_required_fails_without_evidence() {
    let db = Database::open_in_memory().expect("database");
    let mut request = request();
    request.web_enabled = true;
    request.message = "verify the latest public rule".to_string();
    let accepted = RunIntake::start(&db, request).expect("accepted");
    let context = super::run_context::RunContextAssembler::assemble(
        &db,
        None,
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .expect("context");
    let calls = AtomicU32::new(0);
    let sink = RecordingSink::default();

    let error = super::run_tool_loop::collect_web_evidence_for_run(
        &db,
        &accepted,
        &context,
        &sink,
        |input| {
            calls.fetch_add(1, Ordering::SeqCst);
            assert!(input.enabled);
            async {
                Ok(
                    crate::ai_runtime::web_evidence_broker::WebEvidenceBrokerOutput {
                        items: Vec::new(),
                        usage: Default::default(),
                    },
                )
            }
        },
    )
    .await
    .expect_err("web_required must not continue without evidence");

    assert_eq!(error.to_string(), "agent_run_web_evidence_required");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        crate::ai_runtime::tool_audit::count_by_run(&db, &accepted.run_id).expect("audit count"),
        1
    );
}

#[tokio::test]
async fn run_tool_loop_web_preferred_degrades_without_evidence() {
    let db = Database::open_in_memory().expect("database");
    let mut request = request();
    request.web_enabled = true;
    request.message = "explain the public history of this institution".to_string();
    let accepted = RunIntake::start(&db, request).expect("accepted");
    let context = super::run_context::RunContextAssembler::assemble(
        &db,
        None,
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .expect("context");
    let calls = AtomicU32::new(0);
    let sink = RecordingSink::default();

    let result =
        super::run_tool_loop::collect_web_evidence_for_run(&db, &accepted, &context, &sink, |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            async {
                Ok(
                    crate::ai_runtime::web_evidence_broker::WebEvidenceBrokerOutput {
                        items: Vec::new(),
                        usage: Default::default(),
                    },
                )
            }
        })
        .await
        .expect("web_preferred may degrade");

    assert!(result.evidence_ids.is_empty());
    assert!(result.prompt_addendum.contains("No verified web evidence"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn offline_direct_run_never_calls_web_collector() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let context = super::run_context::RunContextAssembler::assemble(
        &db,
        None,
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .expect("context");
    let calls = AtomicU32::new(0);
    let sink = RecordingSink::default();

    let result =
        super::run_tool_loop::collect_web_evidence_for_run(&db, &accepted, &context, &sink, |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            async {
                Ok(
                    crate::ai_runtime::web_evidence_broker::WebEvidenceBrokerOutput {
                        items: Vec::new(),
                        usage: Default::default(),
                    },
                )
            }
        })
        .await
        .expect("offline direct run skips web");

    assert!(result.evidence_ids.is_empty());
    assert!(result.prompt_addendum.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        crate::ai_runtime::tool_audit::count_by_run(&db, &accepted.run_id).expect("audit count"),
        0
    );
}

#[tokio::test]
async fn direct_run_does_not_scan_or_dispatch_tools() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let context = super::run_context::RunContextAssembler::assemble(
        &db,
        None,
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .expect("context");
    let calls = AtomicU32::new(0);
    let sink = RecordingSink::default();

    let result =
        super::run_tool_loop::collect_web_evidence_for_run(&db, &accepted, &context, &sink, |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err(AppError::msg("collector must not be touched")) }
        })
        .await
        .expect("direct run skips the tool loop");

    assert!(result.evidence_ids.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn run_tool_loop_registers_bounded_web_evidence_before_provider_dispatch() {
    let db = Database::open_in_memory().expect("database");
    let mut request = request();
    request.web_enabled = true;
    request.message = "search current public facts".to_string();
    let accepted = RunIntake::start(&db, request).expect("accepted");
    let context = super::run_context::RunContextAssembler::assemble(
        &db,
        None,
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .expect("context");
    let sink = RecordingSink::default();
    let output = crate::ai_runtime::web_evidence_broker::WebEvidenceBrokerOutput {
        items: vec![crate::ai_runtime::web_evidence_broker::WebEvidenceItem {
            url: "https://example.com/current".to_string(),
            canonical_url: "https://example.com/current".to_string(),
            title: "Current public source".to_string(),
            domain: "example.com".to_string(),
            snippet: "search snippet".to_string(),
            fetched_excerpt: Some("bounded page evidence".to_string()),
            provider_id: "test.web".to_string(),
            provider_kind: "native".to_string(),
            cost_class: "free".to_string(),
            raw_result_hash: "test-result-hash".to_string(),
            extraction_method: "test_fetch".to_string(),
            trust_level: "external_untrusted".to_string(),
            retrieval_reason: "search".to_string(),
            search_backend: crate::ai_runtime::WebSearchBackend::Provider,
            source_rank: crate::ai_runtime::WebSourceRank::Unknown,
            freshness_label: None,
            failure_reason: None,
            conflict_group: None,
            conflict_note: None,
        }],
        usage: Default::default(),
    };

    let result = super::run_tool_loop::collect_web_evidence_for_run(
        &db,
        &accepted,
        &context,
        &sink,
        |_| async { Ok(output) },
    )
    .await
    .expect("web evidence is registered");

    assert_eq!(result.evidence_ids.len(), 1);
    assert!(result.prompt_addendum.contains("bounded page evidence"));
    assert_eq!(
        crate::ai_runtime::tool_audit::count_by_run(&db, &accepted.run_id).expect("audit count"),
        1
    );
    db.with_read_conn(|conn| {
        let excerpt: String = conn.query_row(
            "SELECT bounded_excerpt FROM session_evidence WHERE id = ?1",
            [result.evidence_ids[0]],
            |row| row.get(0),
        )?;
        assert_eq!(excerpt, "bounded page evidence");
        Ok(())
    })
    .expect("ledger evidence");
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert!(replay
        .events
        .iter()
        .any(|event| { serde_json::to_value(event).expect("event")["type"] == "tool_started" }));
    assert!(replay.events.iter().any(|event| {
        serde_json::to_value(event).expect("event")["type"] == "evidence_registered"
    }));
    assert!(replay
        .events
        .iter()
        .any(|event| { serde_json::to_value(event).expect("event")["type"] == "tool_completed" }));
}
