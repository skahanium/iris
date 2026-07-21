use std::cell::Cell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};

use super::agent_tool_loop::{ToolLoopExecutor, ToolLoopProvider};
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
use crate::ai_types::{
    EndpointFamily, MessageRole, ProviderConfig, ToolCall, ToolCallResult, ToolSpec,
};
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

struct MakeSqliteReadonlyProvider<'a> {
    db: &'a Database,
}

impl DirectAnswerProvider for MakeSqliteReadonlyProvider<'_> {
    fn answer(&self, _run_id: &str, _message: &str) -> AppResult<String> {
        for _ in 0..2 {
            self.db.with_conn(|conn| {
                conn.execute_batch("PRAGMA query_only=ON")
                    .map_err(Into::into)
            })?;
        }
        Ok("已经验证但无法持久化的回答".to_string())
    }
}

#[derive(Default)]
struct RecordingSink {
    events: std::sync::Mutex<Vec<serde_json::Value>>,
    transient_events: std::sync::Mutex<Vec<serde_json::Value>>,
}

impl RunEventSink for RecordingSink {
    fn emit(&self, event: &super::run_contract::AssistantRunEvent) -> AppResult<()> {
        self.events
            .lock()
            .expect("recording sink lock")
            .push(serde_json::to_value(event)?);
        Ok(())
    }

    fn emit_transient_content(
        &self,
        event: &super::run_contract::AssistantRunEvent,
    ) -> AppResult<()> {
        self.transient_events
            .lock()
            .expect("transient recording sink lock")
            .push(serde_json::to_value(event)?);
        Ok(())
    }
}

struct SelectiveFailingSink {
    fail_type: &'static str,
    events: std::sync::Mutex<Vec<serde_json::Value>>,
}

impl RunEventSink for SelectiveFailingSink {
    fn emit(&self, event: &super::run_contract::AssistantRunEvent) -> AppResult<()> {
        let event = serde_json::to_value(event)?;
        if event["type"] == self.fail_type {
            return Err(AppError::msg("test_event_delivery_failed"));
        }
        self.events.lock().expect("failing sink lock").push(event);
        Ok(())
    }

    fn emit_transient_content(
        &self,
        event: &super::run_contract::AssistantRunEvent,
    ) -> AppResult<()> {
        let event = serde_json::to_value(event)?;
        if event["type"] == self.fail_type {
            return Err(AppError::msg("test_event_delivery_failed"));
        }
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
        _messages: &'a [crate::ai_runtime::LlmMessage],
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

struct MetaAnalysisStreamingProvider;

struct NormalAnswerStreamingProvider;

struct MetaAnalysisToolLoopProvider;

struct ScriptedToolLoopProvider {
    responses: std::sync::Mutex<VecDeque<crate::ai_runtime::model_gateway::GatewayResponse>>,
}

struct SuccessfulToolLoopExecutor {
    calls: AtomicU32,
    evidence_ids: Vec<i64>,
}

struct UnusedToolLoopExecutor;

impl StreamingDirectAnswerProvider for MetaAnalysisStreamingProvider {
    fn answer_streaming<'a>(
        &'a self,
        _run_id: &'a str,
        _messages: &'a [crate::ai_runtime::LlmMessage],
        _observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<
        Box<
            dyn Future<Output = AppResult<crate::ai_runtime::model_gateway::GatewayResponse>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async {
            let meta = format!(
                "The user is asking for current sports information. {}",
                "I should inspect the system instructions before answering. ".repeat(12)
            );
            Ok(crate::ai_runtime::model_gateway::GatewayResponse {
                content: Some(format!(
                    "{meta}\n\nThe system prompt requires verified evidence before a final response.\n\n这是基于联网证据的最终答复。"
                )),
                tool_calls: vec![],
                usage: Default::default(),
                finish_reason: "stop".to_string(),
                reasoning_content: None,
            })
        })
    }
}

impl StreamingDirectAnswerProvider for NormalAnswerStreamingProvider {
    fn answer_streaming<'a>(
        &'a self,
        run_id: &'a str,
        _messages: &'a [crate::ai_runtime::LlmMessage],
        observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<
        Box<
            dyn Future<Output = AppResult<crate::ai_runtime::model_gateway::GatewayResponse>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let answer = "用户可以在设置中启用兼容模型。".to_string();
            observer.observe(
                &StreamEvent {
                    request_id: run_id.to_string(),
                    event_type: StreamEventType::Token,
                    data: StreamEventData::Token {
                        token: answer.clone(),
                    },
                    surface: StreamSurface::VisibleAnswerSanitized,
                    classified: false,
                },
                0,
            )?;
            Ok(crate::ai_runtime::model_gateway::GatewayResponse {
                content: Some(answer),
                tool_calls: vec![],
                usage: Default::default(),
                finish_reason: "stop".to_string(),
                reasoning_content: None,
            })
        })
    }
}

impl ToolLoopProvider for MetaAnalysisToolLoopProvider {
    fn answer_turn<'a>(
        &'a self,
        _run_id: &'a str,
        _messages: &'a [crate::ai_runtime::LlmMessage],
        _tools: &'a [ToolSpec],
        _observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<
        Box<
            dyn Future<Output = AppResult<crate::ai_runtime::model_gateway::GatewayResponse>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async {
            Ok(crate::ai_runtime::model_gateway::GatewayResponse {
                content: Some(
                    "The user asks for a current update.\n\nLooking at the system prompt, I should only use evidence.\n\n最终的工具循环答复。".to_string(),
                ),
                tool_calls: vec![],
                usage: Default::default(),
                finish_reason: "stop".to_string(),
                reasoning_content: None,
            })
        })
    }
}

impl ToolLoopProvider for ScriptedToolLoopProvider {
    fn answer_turn<'a>(
        &'a self,
        _run_id: &'a str,
        _messages: &'a [crate::ai_runtime::LlmMessage],
        _tools: &'a [ToolSpec],
        _observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<
        Box<
            dyn Future<Output = AppResult<crate::ai_runtime::model_gateway::GatewayResponse>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            self.responses
                .lock()
                .expect("scripted tool responses lock")
                .pop_front()
                .ok_or_else(|| AppError::msg("missing_scripted_tool_response"))
        })
    }
}

impl ToolLoopExecutor for UnusedToolLoopExecutor {
    fn execute<'a>(
        &'a self,
        _run_id: &'a str,
        _call: &'a ToolCall,
        _step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
        Box::pin(async { Err(AppError::msg("unused_tool_loop_executor")) })
    }
}

impl ToolLoopExecutor for SuccessfulToolLoopExecutor {
    fn execute<'a>(
        &'a self,
        _run_id: &'a str,
        call: &'a ToolCall,
        _step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let tool_name = call.function.name.clone();
        Box::pin(async move {
            Ok(ToolCallResult {
                tool_name,
                success: true,
                output: serde_json::json!({ "result": "ok" }),
                duration_ms: 1,
                tokens_used: None,
                error: None,
            })
        })
    }

    fn evidence_ids(&self) -> Vec<i64> {
        self.evidence_ids.clone()
    }
}

fn scripted_tool_loop_provider(final_content: String) -> ScriptedToolLoopProvider {
    ScriptedToolLoopProvider {
        responses: std::sync::Mutex::new(VecDeque::from([
            crate::ai_runtime::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![ToolCall {
                    id: "tool-call-1".to_string(),
                    call_type: "function".to_string(),
                    function: crate::ai_types::FunctionCall {
                        name: "test_tool".to_string(),
                        arguments: "{}".to_string(),
                    },
                }],
                usage: Default::default(),
                finish_reason: "tool_calls".to_string(),
                reasoning_content: None,
            },
            crate::ai_runtime::model_gateway::GatewayResponse {
                content: Some(final_content),
                tool_calls: vec![],
                usage: Default::default(),
                finish_reason: "stop".to_string(),
                reasoning_content: None,
            },
        ])),
    }
}

fn test_tool_spec() -> ToolSpec {
    ToolSpec {
        name: "test_tool".to_string(),
        description: "Return a bounded test result".to_string(),
        input_schema: serde_json::json!({ "type": "object" }),
        access_level: crate::ai_runtime::ToolAccessLevel::ReadProfile,
        requires_confirmation: false,
        max_results: None,
        capability_affinity: Vec::new(),
    }
}

fn request() -> AssistantRunStartRequest {
    AssistantRunStartRequest {
        client_request_id: "engine-client-request".to_string(),
        session: None,
        turn: super::run_contract::AssistantTurnDraft {
            message: "请给出最小直答".to_string(),
            content_parts: None,
            explicit_references: vec![],
            retrieval_scope: Default::default(),
            display_mentions: vec![],
        },
        explicit_action: None,
        web_enabled: false,
        model_override: None,
        security_domain: SecurityDomain::Normal,
        classified_context_ref: None,
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

    observer.bind_validated_content("稳定片段");
    observer.flush().expect("flush validated stream fragment");
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

#[test]
fn run_stream_observer_flushes_long_validated_content_in_safe_chunks() {
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

    let long_answer = "联网证据说明"
        .chars()
        .cycle()
        .take(4_500)
        .collect::<String>();
    observer.bind_validated_content(&long_answer);
    observer
        .flush()
        .expect("long validated content must flush in safe chunks");

    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run exists");
    let deltas: String = replay
        .events
        .iter()
        .filter_map(|event| {
            let value = serde_json::to_value(event).ok()?;
            (value["type"] == "content_delta")
                .then(|| value["payload"]["delta"].as_str().map(str::to_owned))
                .flatten()
        })
        .collect();
    assert_eq!(deltas, long_answer);
    assert!(
        replay
            .events
            .iter()
            .filter(|event| {
                serde_json::to_value(event)
                    .ok()
                    .is_some_and(|value| value["type"] == "content_delta")
            })
            .count()
            >= 3,
        "expected multiple content_delta events for a long answer"
    );
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
    let transient_events = sink.transient_events.lock().expect("transient sink lock");
    assert_eq!(transient_events.len(), 1);
    assert_eq!(transient_events[0]["seq"], 0);
    assert_eq!(transient_events[0]["payload"]["delta"], "流式片段");
    assert_eq!(
        serde_json::to_value(&replay.events[3]).expect("serialize delta")["payload"]["delta"],
        "流式最终答复"
    );
    assert_eq!(
        serde_json::to_value(replay.events.last().expect("completed"))
            .expect("serialize completed")["type"],
        "completed"
    );
}

#[tokio::test]
async fn streaming_direct_engine_persists_only_the_answer_after_meta_analysis() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let sink = RecordingSink::default();

    RunEngine::execute_direct_streaming_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        &MetaAnalysisStreamingProvider,
        &sink,
    )
    .await
    .expect("streaming direct run");

    let persisted: String = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT m.content
                 FROM agent_runs r
                 JOIN session_messages m ON m.session_id = r.session_id
                 WHERE r.run_id = ?1 AND m.role = 'assistant'",
                [&accepted.run_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .expect("persisted assistant message");
    assert_eq!(persisted, "这是基于联网证据的最终答复。");
}

#[tokio::test]
async fn streaming_direct_engine_persists_a_normal_answer_with_a_common_chinese_opener() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let sink = RecordingSink::default();

    RunEngine::execute_direct_streaming_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        &NormalAnswerStreamingProvider,
        &sink,
    )
    .await
    .expect("streaming direct run");

    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run exists");
    assert_eq!(replay.run.state, RunState::Completed);
    assert!(replay.run.final_message_id.is_some());
    assert!(replay.events.iter().any(|event| {
        serde_json::to_value(event).expect("serialize event")["payload"]["delta"]
            == "用户可以在设置中启用兼容模型。"
    }));

    let persisted: String = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT m.content
                 FROM agent_runs r
                 JOIN session_messages m ON m.session_id = r.session_id
                 WHERE r.run_id = ?1 AND m.role = 'assistant'",
                [&accepted.run_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .expect("persisted assistant message");
    assert_eq!(persisted, "用户可以在设置中启用兼容模型。");
}

#[test]
fn direct_engine_never_persists_a_meta_analysis_prefix() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let provider = MockProvider {
        calls: Cell::new(0),
        response: Some(
            "The user is greeting me.\n\nI should reply politely in Chinese.\n\n你好！有什么我可以帮你的吗？"
                .to_string(),
        ),
    };

    RunEngine::execute_direct(&db, &accepted.session, &accepted.run_id, &provider)
        .expect("direct run");

    let persisted: String = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT m.content
                 FROM agent_runs r
                 JOIN session_messages m ON m.session_id = r.session_id
                 WHERE r.run_id = ?1 AND m.role = 'assistant'",
                [&accepted.run_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .expect("persisted assistant message");
    assert_eq!(persisted, "你好！有什么我可以帮你的吗？");
}

#[test]
fn direct_empty_output_has_a_distinct_terminal_code_and_no_assistant_body() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let provider = MockProvider {
        calls: Cell::new(0),
        response: Some("   \n".to_string()),
    };
    let sink = RecordingSink::default();

    let error = RunEngine::execute_direct_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        &provider,
        &sink,
    )
    .expect_err("empty output must fail safely");

    assert_eq!(error.to_string(), SafeRunErrorCode::EmptyOutput.as_str());
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Failed);
    assert_eq!(
        serde_json::to_value(replay.events.last().expect("failed")).expect("event")["payload"]
            ["code"],
        SafeRunErrorCode::EmptyOutput.as_str()
    );
    assert!(replay
        .events
        .iter()
        .all(|event| { serde_json::to_value(event).expect("event")["type"] != "content_delta" }));
    assert!(replay.run.final_message_id.is_none());
}

#[test]
fn direct_oversized_output_terminalizes_without_persisting_model_body() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let provider = MockProvider {
        calls: Cell::new(0),
        response: Some("x".repeat(32_001)),
    };
    let sink = RecordingSink::default();

    let error = RunEngine::execute_direct_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        &provider,
        &sink,
    )
    .expect_err("oversized output must fail safely");

    assert_eq!(error.to_string(), SafeRunErrorCode::OutputTooLong.as_str());
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Failed);
    assert!(replay.run.final_message_id.is_none());
    assert!(replay
        .events
        .iter()
        .all(|event| { serde_json::to_value(event).expect("event")["type"] != "content_delta" }));
}

#[test]
fn sqlite_finalize_failure_emits_an_ephemeral_safe_failure_without_model_body() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let sink = RecordingSink::default();

    let error = RunEngine::execute_direct_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        &MakeSqliteReadonlyProvider { db: &db },
        &sink,
    )
    .expect_err("read-only SQLite must be surfaced safely");

    assert_eq!(
        error.to_string(),
        SafeRunErrorCode::PersistenceFailed.as_str()
    );
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Running);
    assert!(replay.run.final_message_id.is_none());
    assert!(replay
        .events
        .iter()
        .all(|event| { serde_json::to_value(event).expect("event")["type"] != "content_delta" }));
    let emitted = sink.events.lock().expect("sink lock");
    let failure = emitted.last().expect("ephemeral safe failure");
    assert_eq!(failure["type"], "failed");
    assert_eq!(
        failure["payload"]["code"],
        SafeRunErrorCode::PersistenceFailed.as_str()
    );
}

#[tokio::test]
async fn invalid_evidence_never_leaves_stream_delta_or_assistant_body_in_sqlite() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let provider = MockStreamingProvider {
        calls: AtomicU32::new(0),
        failure: None,
    };
    let sink = RecordingSink::default();

    let error = RunEngine::execute_direct_streaming_with_prompt_and_evidence_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        "authorized material",
        &[i64::MAX],
        &provider,
        &sink,
    )
    .await
    .expect_err("foreign evidence must fail before body persistence");

    assert_eq!(
        error.to_string(),
        SafeRunErrorCode::EvidenceInvalid.as_str()
    );
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Failed);
    assert!(replay.run.final_message_id.is_none());
    assert!(replay
        .events
        .iter()
        .all(|event| { serde_json::to_value(event).expect("event")["type"] != "content_delta" }));
}

#[tokio::test]
async fn transient_delivery_failure_terminalizes_once_without_persisting_model_body() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let provider = MockStreamingProvider {
        calls: AtomicU32::new(0),
        failure: None,
    };
    let sink = SelectiveFailingSink {
        fail_type: "content_delta",
        events: std::sync::Mutex::new(Vec::new()),
    };

    let error = RunEngine::execute_direct_streaming_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        &provider,
        &sink,
    )
    .await
    .expect_err("flush delivery failure must be classified");

    assert_eq!(
        error.to_string(),
        SafeRunErrorCode::EventDeliveryFailed.as_str()
    );
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Failed);
    assert_eq!(
        replay
            .events
            .iter()
            .filter(|event| {
                matches!(
                    serde_json::to_value(event).expect("event")["type"].as_str(),
                    Some("failed" | "completed" | "cancelled")
                )
            })
            .count(),
        1
    );
    let persisted_deltas = replay
        .events
        .iter()
        .filter_map(|event| {
            let event = serde_json::to_value(event).expect("event");
            (event["type"] == "content_delta").then(|| event["payload"]["delta"].clone())
        })
        .collect::<Vec<_>>();
    assert!(persisted_deltas.is_empty());
}

#[tokio::test]
async fn completed_emit_failure_never_appends_a_second_terminal_event() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let provider = MockStreamingProvider {
        calls: AtomicU32::new(0),
        failure: None,
    };
    let sink = SelectiveFailingSink {
        fail_type: "completed",
        events: std::sync::Mutex::new(Vec::new()),
    };

    let error = RunEngine::execute_direct_streaming_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        &provider,
        &sink,
    )
    .await
    .expect_err("completed emit failure is surfaced safely");
    assert_eq!(
        error.to_string(),
        SafeRunErrorCode::EventDeliveryFailed.as_str()
    );

    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Completed);
    assert_eq!(
        replay
            .events
            .iter()
            .filter(|event| {
                matches!(
                    serde_json::to_value(event).expect("event")["type"].as_str(),
                    Some("failed" | "completed" | "cancelled")
                )
            })
            .count(),
        1
    );
}

#[tokio::test]
async fn tool_loop_engine_never_persists_a_meta_analysis_prefix() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let sink = RecordingSink::default();

    RunEngine::execute_tool_loop_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        vec![crate::ai_runtime::LlmMessage {
            role: MessageRole::User,
            content: "请回答".into(),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        }],
        vec![],
        &[],
        None,
        &MetaAnalysisToolLoopProvider,
        &UnusedToolLoopExecutor,
        &sink,
    )
    .await
    .expect("tool loop run");

    let persisted: String = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT m.content
                 FROM agent_runs r
                 JOIN session_messages m ON m.session_id = r.session_id
                 WHERE r.run_id = ?1 AND m.role = 'assistant'",
                [&accepted.run_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .expect("persisted assistant message");
    assert_eq!(persisted, "最终的工具循环答复。");
}

#[tokio::test]
async fn tool_success_followed_by_oversized_output_has_one_precise_safe_terminal() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let provider = scripted_tool_loop_provider("过长".repeat(16_001));
    let executor = SuccessfulToolLoopExecutor {
        calls: AtomicU32::new(0),
        evidence_ids: vec![],
    };
    let sink = RecordingSink::default();

    let error = RunEngine::execute_tool_loop_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        vec![crate::ai_runtime::LlmMessage {
            role: MessageRole::User,
            content: "请调用工具后回答".into(),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        }],
        vec![test_tool_spec()],
        &[],
        None,
        &provider,
        &executor,
        &sink,
    )
    .await
    .expect_err("oversized tool-loop output must fail");

    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(error.to_string(), SafeRunErrorCode::OutputTooLong.as_str());
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Failed);
    assert!(replay.run.final_message_id.is_none());
    assert_eq!(terminal_event_count(&replay.events), 1);
    assert!(replay
        .events
        .iter()
        .all(|event| { serde_json::to_value(event).expect("event")["type"] != "content_delta" }));
}

#[tokio::test]
async fn tool_success_followed_by_invalid_evidence_never_persists_output() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let provider = scripted_tool_loop_provider("工具后的回答".to_string());
    let executor = SuccessfulToolLoopExecutor {
        calls: AtomicU32::new(0),
        evidence_ids: vec![i64::MAX],
    };
    let sink = RecordingSink::default();

    let error = RunEngine::execute_tool_loop_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        vec![crate::ai_runtime::LlmMessage {
            role: MessageRole::User,
            content: "请调用工具后回答".into(),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        }],
        vec![test_tool_spec()],
        &[],
        None,
        &provider,
        &executor,
        &sink,
    )
    .await
    .expect_err("foreign evidence must fail");

    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        error.to_string(),
        SafeRunErrorCode::EvidenceInvalid.as_str()
    );
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Failed);
    assert!(replay.run.final_message_id.is_none());
    assert_eq!(terminal_event_count(&replay.events), 1);
    assert!(replay
        .events
        .iter()
        .all(|event| { serde_json::to_value(event).expect("event")["type"] != "content_delta" }));
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
            api_key: Some(zeroize::Zeroizing::new("test-key".to_string())),
            model: "model".to_string(),
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

#[tokio::test]
async fn multimodal_direct_run_preserves_image_parts_for_the_selected_provider() {
    struct CapturingProvider {
        messages: std::sync::Mutex<Vec<crate::ai_runtime::LlmMessage>>,
    }

    impl StreamingDirectAnswerProvider for CapturingProvider {
        fn answer_streaming<'a>(
            &'a self,
            _run_id: &'a str,
            messages: &'a [crate::ai_runtime::LlmMessage],
            _observer: &'a mut dyn StreamEventObserver,
        ) -> Pin<
            Box<
                dyn Future<Output = AppResult<crate::ai_runtime::model_gateway::GatewayResponse>>
                    + Send
                    + 'a,
            >,
        > {
            self.messages
                .lock()
                .expect("capture lock")
                .extend_from_slice(messages);
            Box::pin(async {
                Ok(crate::ai_runtime::model_gateway::GatewayResponse {
                    content: Some("已分析图片".into()),
                    tool_calls: Vec::new(),
                    usage: Default::default(),
                    finish_reason: "stop".into(),
                    reasoning_content: None,
                })
            })
        }
    }

    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let plan = DomainExecutor::plan(
        &super::run_contract::ExecutionEnvelope {
            effect: super::run_contract::Effect::Answer,
            context: super::run_contract::ContextMode::None,
            freshness: super::run_contract::Freshness::Offline,
            web_reason: super::run_contract::WebDecisionReason::LegacyUnknown,
            effort: super::run_contract::Effort::Direct,
            security_domain: SecurityDomain::Normal,
            risk: super::run_contract::RiskClass::ReadOnly,
            modalities: vec![super::run_contract::Modality::Image],
            material_needs: Vec::new(),
            required_capabilities: vec![CapabilityId::new("model.vision")],
            explicit_constraints: Vec::new(),
        },
        "描述图片",
        &[],
        &[],
    );
    let provider = CapturingProvider {
        messages: std::sync::Mutex::new(Vec::new()),
    };
    let sink = RecordingSink::default();
    let messages = vec![crate::ai_runtime::LlmMessage {
        role: MessageRole::User,
        content: crate::ai_types::MessageContent::Parts(vec![
            crate::ai_types::ContentPart::Text {
                text: "描述图片".into(),
            },
            crate::ai_types::ContentPart::ImageUrl {
                image_url: crate::ai_types::ImageUrlPayload {
                    url: "data:image/png;base64,AA==".into(),
                    detail: Some("low".into()),
                },
            },
        ]),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    }];

    RunEngine::execute_direct_streaming_with_messages_evidence_and_domain_plan_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        &messages,
        &[],
        &plan,
        &provider,
        &sink,
    )
    .await
    .expect("multimodal direct run");

    let captured = provider.messages.lock().expect("capture lock");
    assert!(matches!(
        captured[0].content,
        crate::ai_types::MessageContent::Parts(ref parts)
            if parts.iter().any(|part| matches!(part, crate::ai_types::ContentPart::ImageUrl { .. }))
    ));
}

fn event_state_version(event: &super::run_contract::AssistantRunEvent) -> u64 {
    serde_json::to_value(event).expect("serialize event")["stateVersion"]
        .as_u64()
        .expect("state version")
}

fn terminal_event_count(events: &[super::run_contract::AssistantRunEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                serde_json::to_value(event).expect("terminal event")["type"].as_str(),
                Some("failed" | "completed" | "cancelled")
            )
        })
        .count()
}

struct LeakingStreamingProvider;

impl StreamingDirectAnswerProvider for LeakingStreamingProvider {
    fn answer_streaming<'a>(
        &'a self,
        run_id: &'a str,
        _messages: &'a [crate::ai_runtime::LlmMessage],
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
            web_reason: super::run_contract::WebDecisionReason::LegacyUnknown,
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

    assert_eq!(
        error.to_string(),
        SafeRunErrorCode::EvidenceInvalid.as_str()
    );
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run exists");
    assert_eq!(replay.run.state, RunState::Failed);
    assert!(replay.run.final_message_id.is_none());
    assert!(replay.events.iter().all(|event| {
        serde_json::to_value(event).expect("serialize event")["type"] != "content_delta"
    }));
}

#[test]
fn web_evidence_failure_classification_never_uses_model_provider_codes() {
    assert_eq!(
        super::run_tool_loop::classify_web_evidence_failure(&AppError::msg("deadline exceeded")),
        SafeRunErrorCode::WebProviderTimeout,
    );
    assert_eq!(
        super::run_tool_loop::classify_web_evidence_failure(&AppError::msg(
            "mcp_search_parse_empty:unrecognized_schema",
        )),
        SafeRunErrorCode::WebEvidenceInvalid,
    );
    assert_eq!(
        super::run_tool_loop::classify_web_evidence_failure(&AppError::msg(
            "web_search_failed: connection reset",
        )),
        SafeRunErrorCode::WebProviderFailed,
    );
    assert_eq!(
        super::run_tool_loop::classify_web_evidence_failure(&AppError::msg(
            "agent_run_web_provider_auth_failed",
        )),
        SafeRunErrorCode::WebProviderAuthFailed,
    );
    assert_eq!(
        super::run_tool_loop::web_evidence_failure_reason(&AppError::msg(
            "output_too_large: MCP HTTP response exceeded configured cap",
        )),
        super::run_contract::WebEvidenceFailureReason::ProviderOutputTooLarge,
    );
}

#[test]
fn web_failure_retryability_is_limited_to_known_transient_conditions() {
    for deterministic in [
        "web_search_provider_missing",
        "provider_disabled: circuit_open",
        "unauthorized: invalid api key",
        "agent_run_web_provider_auth_failed",
        "policy denied",
        "mcp_search_parse_empty:unrecognized_schema",
        "output too large",
    ] {
        assert!(
            !super::run_tool_loop::web_evidence_failure_is_retryable(
                &AppError::msg(deterministic,)
            ),
            "{deterministic}"
        );
    }
    for transient in ["deadline exceeded", "connection reset by peer"] {
        assert!(
            super::run_tool_loop::web_evidence_failure_is_retryable(&AppError::msg(transient)),
            "{transient}"
        );
    }
}

#[test]
fn tool_loop_web_failures_keep_their_web_safe_codes() {
    assert_eq!(
        super::run_engine::classify_tool_loop_failure(&AppError::msg(
            "agent_run_web_provider_timeout",
        )),
        SafeRunErrorCode::WebProviderTimeout,
    );
    assert_eq!(
        super::run_engine::classify_tool_loop_failure(&AppError::msg(
            "agent_run_web_provider_failed",
        )),
        SafeRunErrorCode::WebProviderFailed,
    );
    assert_eq!(
        super::run_engine::classify_tool_loop_failure(&AppError::msg(
            "agent_run_web_evidence_invalid",
        )),
        SafeRunErrorCode::WebEvidenceInvalid,
    );
}
