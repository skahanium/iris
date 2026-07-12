use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};

use super::run_contract::{
    AssistantRunStartRequest, RunEventPayload, RunEventType, RunState, SafeRunErrorCode,
    SecurityDomain,
};
use super::run_engine::{
    direct_gateway_request, AgentRunStreamObserver, DirectAnswerProvider, RunEngine, RunEventSink,
    StreamingDirectAnswerProvider,
};
use super::run_intake::RunIntake;
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
    fail: bool,
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
            if self.fail {
                return Err(AppError::msg("provider transport error"));
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
fn run_stream_observer_persists_each_delta_before_emitting_it() {
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

    observer
        .observe(
            &StreamEvent {
                request_id: accepted.run_id.clone(),
                event_type: StreamEventType::Token,
                data: StreamEventData::Token {
                    token: "持久化后可见".to_string(),
                },
                surface: StreamSurface::VisibleAnswer,
                classified: false,
            },
            0,
        )
        .expect("stream delta");

    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run exists");
    assert_eq!(replay.run.state, RunState::Running);
    assert_eq!(replay.run.state_version, event_state_version(&running));
    assert_eq!(replay.events.len(), 4);
    assert_eq!(
        serde_json::to_value(replay.events.last().expect("delta event")).expect("serialize delta")
            ["payload"]["delta"],
        "持久化后可见"
    );
    assert_eq!(sink.events.lock().expect("sink lock").len(), 1);
}

#[tokio::test]
async fn streaming_direct_engine_persists_deltas_and_one_terminal_message() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let provider = MockStreamingProvider {
        calls: AtomicU32::new(0),
        fail: false,
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
        fail: true,
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

#[test]
fn direct_gateway_request_is_a_tool_free_user_message_only_stream() {
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

    assert_eq!(request.messages.len(), 1);
    assert!(matches!(request.messages[0].role, MessageRole::User));
    assert_eq!(request.messages[0].content.text_content(), "只回答这条消息");
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
