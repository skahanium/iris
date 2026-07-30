use std::cell::Cell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};

use super::agent_capacity_eval::EvaluationTelemetryTap;
use super::agent_tool_loop::{ToolLoopExecutor, ToolLoopProvider};
use super::conversation_memory::ConversationMemory;
use super::domain_executor::{AuthorizedDomainMaterial, DomainExecutor, DomainMaterialRole};
use super::frozen_change_plan::{FrozenChangePlan, FrozenChangePlanInput};
use super::normal_session_repository::NormalSessionRepository;
use super::policy_decision_engine::RunPolicyDecision;
use super::run_context::RunContextAssembler;
use super::run_contract::CapabilityId;
use super::run_contract::{
    AssistantRunStartRequest, Effect, ExplicitAction, ExplicitTarget, RunEventPayload,
    RunEventType, RunRecoveryKind, RunState, SafeRunErrorCode, SecurityDomain,
};
use super::run_engine::{
    apply_model_turn_budget, direct_gateway_request, AgentRunStreamObserver, DirectAnswerProvider,
    RunEngine, RunEventSink, StreamingDirectAnswerProvider,
};
use super::run_intake::RunIntake;
use crate::ai_runtime::agent_evidence_repository::{
    AgentEvidenceRepository, LocalEvidenceInput, MaterialRole, WebEvidenceInput,
};
use crate::ai_runtime::agent_run_repository::{
    AgentRunRepository, AppendRunCheckpointInput, AppendRunEventInput, DurableApplyCheckpoint,
    DurableApplyCheckpointStage,
};
use crate::ai_runtime::model_gateway::{
    StreamEvent, StreamEventData, StreamEventObserver, StreamEventType, StreamSurface,
};
use crate::ai_types::{
    EndpointFamily, MessageRole, ProviderConfig, ToolCall, ToolCallResult, ToolSpec,
};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

#[test]
fn child_turn_limits_are_written_into_the_gateway_request() {
    let mut request = direct_gateway_request(
        ProviderConfig {
            name: "budget-provider".into(),
            base_url: "https://api.example.com".into(),
            api_key: None,
            model: "budget-model".into(),
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
        },
        "bounded child prompt",
        8_192,
    );

    apply_model_turn_budget(
        &mut request,
        crate::ai_runtime::agent_tool_loop::AgentModelTurnBudget {
            input_token_budget: Some(2_000),
            max_output_tokens: Some(1_024),
        },
    );

    assert_eq!(request.input_token_budget, Some(2_000));
    assert_eq!(request.max_tokens, Some(1_024));
}

struct MockProvider {
    calls: Cell<u32>,
    response: Option<String>,
}

struct MockStreamingProvider {
    calls: AtomicU32,
    failure: Option<&'static str>,
}

struct FixedContentStreamingProvider {
    content: String,
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
    presentation_events: std::sync::Mutex<Vec<serde_json::Value>>,
}

impl RunEventSink for RecordingSink {
    fn emit(&self, event: &super::run_contract::AssistantRunEvent) -> AppResult<()> {
        self.events
            .lock()
            .expect("recording sink lock")
            .push(serde_json::to_value(event)?);
        Ok(())
    }

    fn emit_presentation(
        &self,
        _run_id: &str,
        payload: super::run_contract::RunPresentationPayload,
    ) -> AppResult<()> {
        self.presentation_events
            .lock()
            .expect("presentation recording sink lock")
            .push(serde_json::to_value(payload)?);
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

    fn emit_presentation(
        &self,
        _run_id: &str,
        _payload: super::run_contract::RunPresentationPayload,
    ) -> AppResult<()> {
        Err(AppError::msg("test_presentation_delivery_failed"))
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
                        replace_visible: false,
                    },
                    surface: StreamSurface::VisibleAnswerSanitized,
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
                continuation: None,
            })
        })
    }
}

impl StreamingDirectAnswerProvider for FixedContentStreamingProvider {
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
            observer.observe(
                &StreamEvent {
                    request_id: run_id.to_string(),
                    event_type: StreamEventType::Token,
                    data: StreamEventData::Token {
                        token: self.content.clone(),
                        replace_visible: false,
                    },
                    surface: StreamSurface::VisibleAnswerSanitized,
                    classified: false,
                },
                0,
            )?;
            Ok(crate::ai_runtime::model_gateway::GatewayResponse {
                content: Some(self.content.clone()),
                tool_calls: Vec::new(),
                usage: crate::ai_types::TokenUsage {
                    prompt_tokens: 7,
                    completion_tokens: 5,
                    total_tokens: 12,
                    prompt_cache_hit_tokens: 0,
                    prompt_cache_miss_tokens: 7,
                },
                finish_reason: "stop".to_string(),
                reasoning_content: None,
                continuation: None,
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

struct StrictWebEvidenceExecutor {
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
                continuation: None,
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
                        replace_visible: false,
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
                continuation: None,
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
        _budget: crate::ai_runtime::agent_tool_loop::AgentModelTurnBudget,
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
                continuation: None,
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
        _budget: crate::ai_runtime::agent_tool_loop::AgentModelTurnBudget,
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

impl ToolLoopExecutor for StrictWebEvidenceExecutor {
    fn execute<'a>(
        &'a self,
        _run_id: &'a str,
        _call: &'a ToolCall,
        _step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
        Box::pin(async { Err(AppError::msg("unused_strict_web_executor")) })
    }

    fn evidence_ids(&self) -> Vec<i64> {
        self.evidence_ids.clone()
    }

    fn has_web_evidence(&self) -> bool {
        true
    }

    fn requires_web_evidence(&self) -> bool {
        true
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
                continuation: None,
            },
            crate::ai_runtime::model_gateway::GatewayResponse {
                content: Some(final_content),
                tool_calls: vec![],
                usage: Default::default(),
                finish_reason: "stop".to_string(),
                reasoning_content: None,
                continuation: None,
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
        external_tool_grants: Vec::new(),
        security_domain: SecurityDomain::Normal,
        classified_context_ref: None,
    }
}

fn standard_tool_loop_request() -> AssistantRunStartRequest {
    let mut request = request();
    request.turn.message = "根据本地项目笔记调用工具后回答".to_string();
    request
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
fn completed_runs_refresh_conversation_memory_without_changing_terminal_state() {
    let db = Database::open_in_memory().expect("database");
    let sink = RecordingSink::default();
    let first = RunIntake::start(&db, request()).expect("first accepted");
    let session = first.session.clone();
    let provider = MockProvider {
        calls: Cell::new(0),
        response: Some("已完成答复".to_string()),
    };
    RunEngine::execute_direct_with_sink(&db, &session, &first.run_id, &provider, &sink)
        .expect("first completed");

    for index in 2..=4 {
        let mut next = request();
        next.client_request_id = format!("memory-refresh-{index}");
        next.session = Some(session.clone());
        next.turn.message = format!("第 {index} 轮用户消息");
        let accepted = RunIntake::start(&db, next).expect("next accepted");
        RunEngine::execute_direct_with_sink(&db, &session, &accepted.run_id, &provider, &sink)
            .expect("next completed");
    }

    let normal = NormalSessionRepository::get(&db, &session.session_key)
        .expect("session lookup")
        .expect("session exists");
    let memory = ConversationMemory::latest_for_session(&db, normal.session_id)
        .expect("memory lookup")
        .expect("memory refreshed after the fourth completed turn");
    assert_eq!(memory.seq_end, 2);
}

#[test]
fn multi_turn_pressure_keeps_recent_context_bounded_and_memory_disjoint() {
    for turns in [1_u32, 20, 50, 100] {
        for repetition in 0..5 {
            let db = Database::open_in_memory().expect("database");
            let sink = RecordingSink::default();
            let provider = MockProvider {
                calls: Cell::new(0),
                response: Some("确定性压力答复".to_string()),
            };
            let mut session = None;
            let mut seen_runs = std::collections::HashSet::new();
            for turn in 1..=turns {
                let mut next = request();
                next.client_request_id = format!("pressure-{turns}-{repetition}-{turn}");
                next.session = session.clone();
                next.turn.message = format!("goal: 第 {turn} 轮压力测试");
                let accepted = RunIntake::start(&db, next).expect("accepted pressure turn");
                assert!(seen_runs.insert(accepted.run_id.clone()));
                session = Some(accepted.session.clone());
                RunEngine::execute_direct_with_sink(
                    &db,
                    &accepted.session,
                    &accepted.run_id,
                    &provider,
                    &sink,
                )
                .expect("completed pressure turn");
                let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
                    .expect("replay")
                    .expect("run");
                assert_eq!(replay.run.state, RunState::Completed);
                assert_eq!(terminal_event_count(&replay.events), 1);
            }

            let session = session.expect("pressure session");
            let mut pending = request();
            pending.client_request_id = format!("pressure-context-{turns}-{repetition}");
            pending.session = Some(session.clone());
            let pending = RunIntake::start(&db, pending).expect("accepted context probe");
            let context =
                RunContextAssembler::assemble(&db, None, &session.session_key, &pending.run_id)
                    .expect("assemble bounded pressure context");
            assert_eq!(context.recent_messages.len(), (turns * 2).min(6) as usize);
            if turns > 3 {
                let memory = context.conversation_memory.expect("long history memory");
                assert!(
                    memory.seq_end < context.recent_messages.first().expect("recent").seq,
                    "summary and recent history must remain disjoint"
                );
            }
        }
    }
}

#[tokio::test]
async fn strict_web_multi_turn_pressure_keeps_run_local_sources_without_repair_calls() {
    for repetition in 0..20 {
        let db = Database::open_in_memory().expect("database");
        let sink = RecordingSink::default();
        let mut session = None;

        for turn in 1..=30_i64 {
            let mut next = request();
            next.client_request_id = format!("strict-web-pressure-{repetition}-{turn}");
            next.session = session.clone();
            next.turn.message = format!("第 {turn} 轮严格联网问题");
            let accepted = RunIntake::start(&db, next).expect("accepted strict-web pressure turn");
            session = Some(accepted.session.clone());
            let evidence = AgentEvidenceRepository::register_web(
                &db,
                WebEvidenceInput {
                    session_id: 1,
                    run_id: accepted.run_id.clone(),
                    message_seq_first: turn * 2 - 1,
                    material_role: MaterialRole::Reference,
                    title: format!("第 {turn} 轮来源"),
                    url: format!("https://example.test/pressure/{turn}"),
                    normalized_url: format!("https://example.test/pressure/{turn}"),
                    domain: "example.test".to_string(),
                    retrieved_at: "2026-07-27T00:00:00Z".to_string(),
                    provider_id: "test-web".to_string(),
                    provider_kind: "https".to_string(),
                    raw_result_hash: format!("pressure-source-{turn}"),
                    extraction_method: "test".to_string(),
                    bounded_excerpt: format!("第 {turn} 轮当前证据。"),
                    retrieval_reason: Some("pressure".to_string()),
                    score: None,
                    source_rank: Some(1),
                    conflict_group: None,
                    failure_reason: None,
                },
            )
            .expect("register strict-web pressure evidence");
            let responses = if turn % 5 == 0 {
                vec![crate::ai_runtime::model_gateway::GatewayResponse {
                    content: Some(format!("第 {turn} 轮结论。")),
                    tool_calls: vec![],
                    usage: Default::default(),
                    finish_reason: "stop".to_string(),
                    reasoning_content: None,
                    continuation: None,
                }]
            } else {
                let marker = match turn % 3 {
                    0 => "[W1]",
                    1 => "[1]",
                    _ => "[¹]",
                };
                vec![crate::ai_runtime::model_gateway::GatewayResponse {
                    content: Some(format!("第 {turn} 轮结论。{marker}")),
                    tool_calls: vec![],
                    usage: Default::default(),
                    finish_reason: "stop".to_string(),
                    reasoning_content: None,
                    continuation: None,
                }]
            };
            let provider = ScriptedToolLoopProvider {
                responses: std::sync::Mutex::new(VecDeque::from(responses)),
            };
            RunEngine::execute_tool_loop_with_sink(
                &db,
                &accepted.session,
                &accepted.run_id,
                vec![crate::ai_runtime::LlmMessage {
                    role: MessageRole::User,
                    content: format!("第 {turn} 轮严格联网问题").into(),
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_content: None,
                }],
                vec![],
                &[evidence.evidence_id],
                None,
                &provider,
                &StrictWebEvidenceExecutor {
                    evidence_ids: vec![evidence.evidence_id],
                },
                &sink,
            )
            .await
            .expect("strict-web pressure turn completes");

            let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
                .expect("strict-web pressure replay")
                .expect("strict-web pressure run");
            assert_eq!(replay.run.state, RunState::Completed);
            assert_eq!(terminal_event_count(&replay.events), 1);
            let (content, citation_map): (String, String) = db
                .with_read_conn(|conn| {
                    conn.query_row(
                        "SELECT content, citation_map_json FROM session_messages
                     WHERE session_id = 1 AND turn_id = ?1 AND role = 'assistant'",
                        [replay.run.turn_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(Into::into)
                })
                .expect("strict-web pressure persisted answer");
            assert!(citation_map.contains("\"index\":1"));
            assert!(!citation_map.contains("\"index\":2"));
            if turn % 5 == 0 {
                assert_eq!(content, format!("第 {turn} 轮结论。"));
                assert!(citation_map.contains("\"mode\":\"source_group_fallback\""));
            } else {
                assert!(content.ends_with("[W1]"));
            }
        }
    }
}

#[test]
fn long_conversation_cancel_retract_and_resume_keeps_context_and_terminals_consistent() {
    let db = Database::open_in_memory().expect("database");
    let sink = RecordingSink::default();
    let provider = MockProvider {
        calls: Cell::new(0),
        response: Some("确定性恢复答复".to_string()),
    };
    let mut session = None;
    for turn in 1..=8 {
        let mut next = request();
        next.client_request_id = format!("mixed-lifecycle-{turn}");
        next.session = session.clone();
        next.turn.message = format!("保留的第 {turn} 轮");
        let accepted = RunIntake::start(&db, next).expect("accept retained turn");
        session = Some(accepted.session.clone());
        RunEngine::execute_direct_with_sink(
            &db,
            &accepted.session,
            &accepted.run_id,
            &provider,
            &sink,
        )
        .expect("complete retained turn");
        let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
            .expect("replay retained turn")
            .expect("retained turn exists");
        assert_eq!(replay.run.state, RunState::Completed);
        assert_eq!(terminal_event_count(&replay.events), 1);
    }

    let session = session.expect("long conversation session");
    let mut cancelled_request = request();
    cancelled_request.client_request_id = "mixed-lifecycle-cancel".to_string();
    cancelled_request.session = Some(session.clone());
    cancelled_request.turn.message = "必须撤回的取消问题".to_string();
    let cancelled = RunIntake::start(&db, cancelled_request).expect("accept cancelled turn");
    RunIntake::control(
        &db,
        super::run_contract::AssistantRunControlRequest {
            session: cancelled.session.clone(),
            run_id: cancelled.run_id.clone(),
            expected_state_version: 0,
            action: super::run_contract::RunControlAction::Cancel,
        },
    )
    .expect("cancel long-conversation turn");
    let cancelled_replay = RunIntake::get(&db, &cancelled.session, &cancelled.run_id)
        .expect("replay cancelled turn")
        .expect("cancelled turn exists");
    assert_eq!(cancelled_replay.run.state, RunState::Cancelled);
    assert_eq!(terminal_event_count(&cancelled_replay.events), 1);

    let mut resumed_request = request();
    resumed_request.client_request_id = "mixed-lifecycle-resume-before-retract".to_string();
    resumed_request.session = Some(session.clone());
    resumed_request.turn.message = "撤回前的后续问题".to_string();
    let resumed = RunIntake::start(&db, resumed_request).expect("accept resumed turn");
    RunEngine::execute_direct_with_sink(&db, &resumed.session, &resumed.run_id, &provider, &sink)
        .expect("complete resumed turn");

    assert_eq!(
        NormalSessionRepository::retract(&db, &session.session_key, 17)
            .expect("retract cancelled suffix"),
        3
    );
    let mut after_retract_request = request();
    after_retract_request.client_request_id = "mixed-lifecycle-after-retract".to_string();
    after_retract_request.session = Some(session.clone());
    after_retract_request.turn.message = "撤回后的后续问题".to_string();
    let after_retract =
        RunIntake::start(&db, after_retract_request).expect("accept post-retract turn");
    let context =
        RunContextAssembler::assemble(&db, None, &session.session_key, &after_retract.run_id)
            .expect("assemble post-retract context");
    assert!(context.recent_messages.iter().all(
        |message| !message.content.contains("必须撤回") && !message.content.contains("撤回前")
    ));
    let memory = context
        .conversation_memory
        .expect("long retained history memory");
    assert!(memory.seq_end < context.recent_messages.first().expect("recent").seq);
    RunEngine::execute_direct_with_sink(
        &db,
        &after_retract.session,
        &after_retract.run_id,
        &provider,
        &sink,
    )
    .expect("complete post-retract turn");
    let replay = RunIntake::get(&db, &after_retract.session, &after_retract.run_id)
        .expect("replay post-retract")
        .expect("post-retract turn exists");
    assert_eq!(replay.run.state, RunState::Completed);
    assert_eq!(terminal_event_count(&replay.events), 1);
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
                stage_code: None,
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
                stage_code: None,
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
fn startup_recovery_terminalizes_an_interrupted_tool_loop_without_replaying_it() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE agent_runs SET effort = 'tool_loop' WHERE run_id = ?1",
            [&accepted.run_id],
        )?;
        Ok(())
    })
    .expect("make tool-loop fixture");

    RunEngine::recover_interrupted_runs(&db).expect("recover interrupted tool loop");
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Failed);
    assert!(replay.run.recovery.is_none());
}

fn durable_apply_interrupted_after_consumed_confirmation() -> (
    Database,
    super::run_contract::AssistantRunAccepted,
    std::path::PathBuf,
) {
    durable_apply_interrupted_after_consumed_confirmation_with_expiry(i64::MAX)
}

fn durable_apply_interrupted_after_consumed_confirmation_with_expiry(
    expires_at_unix_ms: i64,
) -> (
    Database,
    super::run_contract::AssistantRunAccepted,
    std::path::PathBuf,
) {
    let vault =
        std::env::temp_dir().join(format!("iris-durable-recovery-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(vault.join("notes")).expect("create recovery vault");
    std::fs::write(vault.join("notes/a.md"), "base").expect("write base note");
    let base_hash = crate::cas::hash::content_hash_str("base");
    let expected_hash = crate::cas::hash::content_hash_str("after");
    let db = Database::open_in_memory().expect("database");
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('vault_path', ?1)",
            [serde_json::to_string(vault.to_string_lossy().as_ref())?],
        )?;
        Ok(())
    })
    .expect("persist vault setting");
    let mut durable = request();
    durable.client_request_id = format!("durable-recovery-{}", uuid::Uuid::new_v4());
    durable.turn.message = "将已确认的修改应用到笔记".into();
    durable
        .turn
        .explicit_references
        .push(crate::ai_types::ContextReferenceWire {
            id: "target-note".into(),
            kind: crate::ai_types::ContextReferenceKind::Note,
            file_path: Some("notes/a.md".into()),
            content_hash: Some(base_hash.clone()),
            utf8_range: None,
            editor_range: None,
            excerpt: String::new(),
            heading_path: None,
            anchor: None,
            stale: false,
            invalid_reason: None,
        });
    durable.explicit_action = Some(ExplicitAction {
        effect: Effect::Apply,
        target: Some(ExplicitTarget {
            reference_id: "target-note".into(),
            content_hash: base_hash.clone(),
        }),
        selection_snapshot: None,
    });
    let accepted = RunIntake::start(&db, durable).expect("accepted durable apply");
    let session_id = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT session_id FROM agent_runs WHERE run_id = ?1",
                [&accepted.run_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .expect("session id");
    let preparing = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: 0,
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Preparing,
                stage: "正在准备".into(),
                stage_code: None,
            },
        },
    )
    .expect("preparing");
    let running = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: preparing.state_version(),
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Running,
                stage: "正在生成变更预览".into(),
                stage_code: None,
            },
        },
    )
    .expect("running");
    let plan = FrozenChangePlan::freeze(FrozenChangePlanInput {
        confirmation_id: format!("confirmation-{}", accepted.run_id),
        run_id: accepted.run_id.clone(),
        session_id,
        request_id: accepted.run_id.clone(),
        tool_call_id: format!("tool-{}", accepted.run_id),
        vault_id: crate::cas::hash::content_hash_str(&vault.to_string_lossy()),
        relative_paths: vec!["notes/a.md".into()],
        operation: "replace_selection".into(),
        base_content_hashes: vec![("notes/a.md".into(), base_hash)],
        expected_post_content_hashes: vec![("notes/a.md".into(), expected_hash)],
        change: serde_json::json!({
            "target_path": "notes/a.md",
            "base_content_hash": crate::cas::hash::content_hash_str("base"),
            "range": { "start": 0, "end": 4 },
            "original_text": "base",
            "replacement": "after"
        }),
        affected_file_count: 1,
        rollback_summary: "可通过版本历史撤销".into(),
        expires_at_unix_ms,
    })
    .expect("frozen plan");
    AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: running.state_version(),
            event_type: RunEventType::ToolStarted,
            payload: RunEventPayload::ToolStarted {
                capability: plan.operation().to_string(),
                tool_call_id: plan.tool_call_id().to_string(),
            },
        },
    )
    .expect("started confirmed tool");
    let awaiting = AgentRunRepository::request_frozen_confirmation(
        &db,
        &plan,
        running.state_version(),
        "等待确认：更新 1 个目标",
    )
    .expect("await confirmation");
    AgentRunRepository::approve_frozen_confirmation(
        &db,
        &accepted.session.session_key,
        &accepted.run_id,
        plan.confirmation_id(),
        plan.plan_hash(),
        awaiting.state_version(),
        0,
    )
    .expect("consume confirmation");
    (db, accepted, vault)
}

#[test]
fn startup_recovery_offers_resume_only_when_consumed_target_is_still_at_base_hash() {
    let (db, accepted, vault) = durable_apply_interrupted_after_consumed_confirmation();

    assert_eq!(
        RunEngine::recover_interrupted_runs(&db).expect("recover durable apply"),
        1
    );
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Paused);
    assert_eq!(replay.run.recovery, Some(RunRecoveryKind::ResumeAvailable));

    std::fs::remove_dir_all(vault).expect("remove recovery vault");
}

#[test]
fn startup_recovery_does_not_recheck_ttl_after_confirmation_was_consumed() {
    let (db, accepted, vault) =
        durable_apply_interrupted_after_consumed_confirmation_with_expiry(0);

    assert_eq!(
        RunEngine::recover_interrupted_runs(&db).expect("recover expired consumed plan"),
        1
    );
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Paused);
    assert_eq!(replay.run.recovery, Some(RunRecoveryKind::ResumeAvailable));

    std::fs::remove_dir_all(vault).expect("remove recovery vault");
}

#[test]
fn startup_recovery_completes_an_already_written_consumed_plan_without_replaying_it() {
    let (db, accepted, vault) = durable_apply_interrupted_after_consumed_confirmation();
    std::fs::write(vault.join("notes/a.md"), "after").expect("simulate committed write");

    assert_eq!(
        RunEngine::recover_interrupted_runs(&db).expect("recover written durable apply"),
        1
    );
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Completed);
    assert_eq!(
        AgentRunRepository::latest_durable_apply_checkpoint(&db, &accepted.run_id)
            .expect("checkpoint")
            .expect("completed checkpoint")
            .stage(),
        super::agent_run_repository::DurableApplyCheckpointStage::Completed
    );
    assert_eq!(
        std::fs::read_to_string(vault.join("notes/a.md")).expect("read recovered note"),
        "after"
    );
    let lifecycle = replay
        .events
        .iter()
        .filter_map(|event| {
            let event = serde_json::to_value(event).expect("serialize recovery event");
            matches!(
                event["type"].as_str(),
                Some("tool_started" | "confirmation_required" | "tool_completed" | "completed")
            )
            .then_some(event)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        lifecycle
            .iter()
            .map(|event| event["type"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        vec![
            "tool_started",
            "confirmation_required",
            "tool_completed",
            "completed"
        ]
    );
    let recovered_tool = &lifecycle[2]["payload"];
    assert_eq!(recovered_tool["capability"], "replace_selection");
    assert_eq!(
        recovered_tool["toolCallId"],
        format!("tool-{}", accepted.run_id)
    );
    assert_eq!(recovered_tool["summary"], "已恢复已确认的变更执行状态");
    assert_eq!(recovered_tool["success"], true);
    assert!(recovered_tool.get("arguments").is_none());
    assert!(recovered_tool.get("rawOutput").is_none());

    std::fs::remove_dir_all(vault).expect("remove recovery vault");
}

#[test]
fn startup_recovery_does_not_duplicate_an_already_recovered_tool_completion() {
    let (db, accepted, vault) = durable_apply_interrupted_after_consumed_confirmation();
    let state_version = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay before recovered completion")
        .expect("run")
        .run
        .state_version;
    AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version,
            event_type: RunEventType::ToolCompleted,
            payload: RunEventPayload::ToolCompleted {
                capability: "replace_selection".into(),
                tool_call_id: format!("tool-{}", accepted.run_id),
                summary: "已恢复已确认的变更执行状态".into(),
                duration_ms: None,
                success: Some(true),
                subagent_batch_report: None,
            },
        },
    )
    .expect("persist recovered completion before simulated crash");
    std::fs::write(vault.join("notes/a.md"), "after").expect("simulate committed write");

    assert_eq!(
        RunEngine::recover_interrupted_runs(&db).expect("resume interrupted recovery"),
        1
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
                serde_json::to_value(event).expect("serialize event")["type"] == "tool_completed"
            })
            .count(),
        1
    );

    std::fs::remove_dir_all(vault).expect("remove recovery vault");
}

#[test]
fn startup_recovery_requires_manual_review_when_consumed_target_diverged() {
    let (db, accepted, vault) = durable_apply_interrupted_after_consumed_confirmation();
    std::fs::write(vault.join("notes/a.md"), "third-party").expect("simulate divergence");

    RunEngine::recover_interrupted_runs(&db).expect("recover diverged durable apply");
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Paused);
    assert_eq!(
        replay.run.recovery,
        Some(RunRecoveryKind::ManualReviewRequired)
    );

    std::fs::remove_dir_all(vault).expect("remove recovery vault");
}

#[test]
fn startup_recovery_requires_manual_review_for_mixed_multi_target_hashes() {
    let (db, accepted, vault) = durable_apply_interrupted_after_consumed_confirmation();
    std::fs::write(vault.join("notes/b.md"), "post-b").expect("write mixed second target");
    let (confirmation_id, session_id): (String, i64) = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT c.confirmation_id, r.session_id
                 FROM agent_run_confirmations c
                 JOIN agent_runs r ON r.run_id = c.run_id
                 WHERE c.run_id = ?1",
                [&accepted.run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(Into::into)
        })
        .expect("recovery identities");
    let base_a = crate::cas::hash::content_hash_str("base");
    let base_b = crate::cas::hash::content_hash_str("base-b");
    let expected_a = crate::cas::hash::content_hash_str("after-a");
    let expected_b = crate::cas::hash::content_hash_str("post-b");
    let mixed_plan = FrozenChangePlan::freeze(FrozenChangePlanInput {
        confirmation_id,
        run_id: accepted.run_id.clone(),
        session_id,
        request_id: accepted.run_id.clone(),
        tool_call_id: format!("tool-mixed-{}", accepted.run_id),
        vault_id: crate::cas::hash::content_hash_str(&vault.to_string_lossy()),
        relative_paths: vec!["notes/a.md".into(), "notes/b.md".into()],
        operation: "replace_selection".into(),
        base_content_hashes: vec![
            ("notes/a.md".into(), base_a.clone()),
            ("notes/b.md".into(), base_b.clone()),
        ],
        expected_post_content_hashes: vec![
            ("notes/a.md".into(), expected_a.clone()),
            ("notes/b.md".into(), expected_b.clone()),
        ],
        change: serde_json::json!({
            "target_path": "notes/a.md",
            "new_path": "notes/b.md",
            "base_content_hash": base_a,
            "range": { "start": 0, "end": 4 },
            "original_text": "base",
            "replacement": "after-a"
        }),
        affected_file_count: 2,
        rollback_summary: "可通过版本历史撤销".into(),
        expires_at_unix_ms: i64::MAX,
    })
    .expect("mixed frozen plan");
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE agent_run_confirmations
             SET plan_hash = ?1, plan_json = ?2
             WHERE run_id = ?3",
            rusqlite::params![
                mixed_plan.plan_hash(),
                mixed_plan.persisted_plan_json()?,
                accepted.run_id,
            ],
        )?;
        conn.execute(
            "DELETE FROM agent_run_steps WHERE run_id = ?1 AND kind = 'durable_apply'",
            [&accepted.run_id],
        )?;
        Ok(())
    })
    .expect("replace consumed plan fixture");
    let state_version = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay before recovery")
        .expect("run")
        .run
        .state_version;
    AgentRunRepository::append_checkpoint_step(
        &db,
        AppendRunCheckpointInput {
            run_id: accepted.run_id.clone(),
            state_version,
            checkpoint: DurableApplyCheckpoint::new(
                mixed_plan.confirmation_id(),
                mixed_plan.plan_hash(),
                DurableApplyCheckpointStage::Approved,
                vec![base_a, base_b],
                vec![expected_a, expected_b],
                Vec::new(),
            )
            .expect("mixed checkpoint"),
        },
    )
    .expect("persist mixed checkpoint");

    RunEngine::recover_interrupted_runs(&db).expect("recover mixed targets");
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Paused);
    assert_eq!(
        replay.run.recovery,
        Some(RunRecoveryKind::ManualReviewRequired)
    );
    assert_eq!(
        std::fs::read_to_string(vault.join("notes/a.md")).expect("read first target"),
        "base"
    );
    assert_eq!(
        std::fs::read_to_string(vault.join("notes/b.md")).expect("read second target"),
        "post-b"
    );

    std::fs::remove_dir_all(vault).expect("remove recovery vault");
}

#[test]
fn startup_recovery_leaves_a_pending_confirmation_awaiting_user_input() {
    let (db, accepted, vault) = durable_apply_interrupted_after_consumed_confirmation();
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE agent_run_confirmations
             SET status = 'pending', consumed_at = NULL
             WHERE run_id = ?1",
            [&accepted.run_id],
        )?;
        conn.execute(
            "DELETE FROM agent_run_steps WHERE run_id = ?1 AND kind = 'durable_apply'",
            [&accepted.run_id],
        )?;
        conn.execute(
            "UPDATE agent_runs
             SET status = 'awaiting_confirmation', state_version = 3
             WHERE run_id = ?1",
            [&accepted.run_id],
        )?;
        Ok(())
    })
    .expect("simulate pending-confirmation restart");

    assert_eq!(
        RunEngine::recover_interrupted_runs(&db).expect("recover pending confirmation"),
        0
    );
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::AwaitingConfirmation);
    assert!(replay.run.pending_confirmation.is_some());

    std::fs::remove_dir_all(vault).expect("remove recovery vault");
}

#[test]
fn startup_recovery_completes_a_rejected_confirmation_as_not_modified() {
    let (db, accepted, vault) = durable_apply_interrupted_after_consumed_confirmation();
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE agent_run_confirmations
             SET status = 'rejected', consumed_at = NULL
             WHERE run_id = ?1",
            [&accepted.run_id],
        )?;
        conn.execute(
            "DELETE FROM agent_run_steps WHERE run_id = ?1 AND kind = 'durable_apply'",
            [&accepted.run_id],
        )?;
        Ok(())
    })
    .expect("simulate rejected confirmation before terminalization");

    assert_eq!(
        RunEngine::recover_interrupted_runs(&db).expect("recover rejected confirmation"),
        1
    );
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Completed);
    let message: String = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT content FROM session_messages
                 WHERE session_id = 1 AND role = 'assistant'",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .expect("fixed rejection message");
    assert_eq!(message, "已取消该变更，未作任何修改。");
    assert_eq!(
        std::fs::read_to_string(vault.join("notes/a.md")).expect("read untouched note"),
        "base"
    );

    std::fs::remove_dir_all(vault).expect("remove recovery vault");
}

#[test]
fn startup_recovery_fails_durable_apply_without_a_consumed_confirmation() {
    let (db, accepted, vault) = durable_apply_interrupted_after_consumed_confirmation();
    db.with_conn(|conn| {
        conn.execute(
            "DELETE FROM agent_run_confirmations WHERE run_id = ?1",
            [&accepted.run_id],
        )?;
        conn.execute(
            "DELETE FROM agent_run_steps WHERE run_id = ?1 AND kind = 'durable_apply'",
            [&accepted.run_id],
        )?;
        Ok(())
    })
    .expect("simulate interruption before confirmation consumption");

    RunEngine::recover_interrupted_runs(&db).expect("recover unconfirmed durable apply");
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Failed);

    std::fs::remove_dir_all(vault).expect("remove recovery vault");
}

#[test]
fn paused_recovery_kind_is_replayed_from_the_durable_event() {
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
                stage_code: None,
            },
        },
    )
    .expect("preparing");
    let running = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: preparing.state_version(),
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Running,
                stage: "正在处理".into(),
                stage_code: None,
            },
        },
    )
    .expect("running");
    AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: running.state_version(),
            event_type: RunEventType::Paused,
            payload: RunEventPayload::Paused {
                reason: "恢复前需要确认".into(),
                recovery: Some(RunRecoveryKind::ResumeAvailable),
            },
        },
    )
    .expect("paused");

    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.recovery, Some(RunRecoveryKind::ResumeAvailable));
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
                stage_code: None,
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
                stage_code: None,
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
                        replace_visible: false,
                    },
                    surface: StreamSurface::VisibleAnswerSanitized,
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
fn evaluation_stream_tap_observes_first_visible_token_without_persisting_measurements() {
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
                stage_code: None,
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
                stage_code: None,
            },
        },
    )
    .expect("running");
    let sink = RecordingSink::default();
    let telemetry = EvaluationTelemetryTap::default();
    let mut observer = AgentRunStreamObserver::new_with_eval_telemetry(
        &db,
        &accepted.run_id,
        event_state_version(&running),
        &sink,
        false,
        telemetry.clone(),
    );
    let before = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("before replay")
        .expect("run");

    observer
        .observe(
            &StreamEvent {
                request_id: "raw-request-id".to_string(),
                event_type: StreamEventType::Token,
                data: StreamEventData::Token {
                    token: "visible but never measured as text".to_string(),
                    replace_visible: false,
                },
                surface: StreamSurface::VisibleAnswerSanitized,
                classified: false,
            },
            0,
        )
        .expect("observe");

    let after = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("after replay")
        .expect("run");
    assert!(telemetry.snapshot().first_visible_token_ms().is_some());
    assert_eq!(after.events.len(), before.events.len());
}

#[tokio::test]
async fn evaluation_direct_run_records_real_successful_final_output_and_gateway_usage() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let sink = RecordingSink::default();
    let telemetry = EvaluationTelemetryTap::default();

    RunEngine::execute_direct_streaming_with_eval_telemetry(
        &db,
        &accepted.session,
        &accepted.run_id,
        &FixedContentStreamingProvider {
            content: "bounded answer".to_string(),
        },
        &sink,
        &telemetry,
    )
    .await
    .expect("successful evaluation run");

    let snapshot = telemetry.snapshot();
    assert_eq!(snapshot.model_turns(), 1);
    assert_eq!(snapshot.total_tokens(), 12);
    assert_eq!(snapshot.final_output_successes(), 1);
    assert_eq!(snapshot.final_output_rejections(), 0);
    assert_eq!(snapshot.output_budget_reached(), 0);
}

#[tokio::test]
async fn evaluation_direct_run_records_real_oversized_final_output_rejection() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let sink = RecordingSink::default();
    let telemetry = EvaluationTelemetryTap::default();

    let error = RunEngine::execute_direct_streaming_with_eval_telemetry(
        &db,
        &accepted.session,
        &accepted.run_id,
        &FixedContentStreamingProvider {
            content: "x".repeat(32_001),
        },
        &sink,
        &telemetry,
    )
    .await
    .expect_err("oversized final output");

    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    let snapshot = telemetry.snapshot();
    assert_eq!(error.to_string(), SafeRunErrorCode::OutputTooLong.as_str());
    assert_eq!(replay.run.state, RunState::Failed);
    assert_eq!(snapshot.final_output_successes(), 0);
    assert_eq!(snapshot.final_output_rejections(), 1);
    assert_eq!(snapshot.output_budget_reached(), 1);
}

#[test]
fn tool_loop_observer_streams_answer_deltas_after_tools_finish() {
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
                stage: "正在准备工具执行".to_string(),
                stage_code: None,
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
                stage: "正在调用模型和工具".to_string(),
                stage_code: None,
            },
        },
    )
    .expect("running");
    let sink = RecordingSink::default();
    let mut observer = AgentRunStreamObserver::new_with_deferred_deltas(
        &db,
        &accepted.run_id,
        event_state_version(&running),
        &sink,
        true,
    );

    observer
        .observe(
            &StreamEvent {
                request_id: accepted.run_id.clone(),
                event_type: StreamEventType::Token,
                data: StreamEventData::Token {
                    token: "工具前预写\n".to_string(),
                    replace_visible: false,
                },
                surface: StreamSurface::VisibleAnswerSanitized,
                classified: false,
            },
            0,
        )
        .expect("deferred token");
    assert!(
        sink.presentation_events
            .lock()
            .expect("presentation lock")
            .is_empty(),
        "tool-turn tokens must stay deferred"
    );

    observer
        .on_tools_starting()
        .expect("drop deferred provisional text before tools");
    observer
        .on_tools_finished()
        .expect("unlock final answer streaming");
    assert!(
        sink.presentation_events
            .lock()
            .expect("presentation lock")
            .is_empty(),
        "unlocking stream must not invent AnswerDelta without tokens"
    );
    let durable = sink.events.lock().expect("sink lock").clone();
    assert!(
        durable.is_empty(),
        "tools_finished must not emit 正在生成答复 before the tool loop is done: {durable:?}"
    );

    for (token_index, token) in ["第一段\n", "第二段\n"].into_iter().enumerate() {
        observer
            .observe(
                &StreamEvent {
                    request_id: accepted.run_id.clone(),
                    event_type: StreamEventType::Token,
                    data: StreamEventData::Token {
                        token: token.to_string(),
                        replace_visible: false,
                    },
                    surface: StreamSurface::VisibleAnswerSanitized,
                    classified: false,
                },
                token_index as u32 + 1,
            )
            .expect("stream final-turn token");
    }

    let presentation = sink
        .presentation_events
        .lock()
        .expect("presentation lock")
        .clone();
    let answer_deltas = presentation
        .iter()
        .filter(|event| event["kind"] == "answer_delta")
        .collect::<Vec<_>>();
    assert!(
        answer_deltas.len() >= 2,
        "final turn after tools must emit multiple AnswerDelta events, got {presentation:?}"
    );
    assert_eq!(answer_deltas[0]["delta"], "第一段\n");
    assert_eq!(answer_deltas[1]["delta"], "第二段\n");
    assert!(
        sink.events
            .lock()
            .expect("sink lock")
            .iter()
            .all(|event| event["payload"]["stage"] != "正在生成答复"),
        "answer streaming after an intermediate tools_finished must not invent 正在生成答复"
    );

    observer
        .emit_generating_answer_stage_if_needed()
        .expect("final answer stage after tool loop");
    let durable = sink.events.lock().expect("sink lock").clone();
    assert_eq!(durable.len(), 1);
    assert_eq!(durable[0]["payload"]["stage"], "正在生成答复");
}

#[test]
fn tool_loop_observer_defers_generating_stage_until_after_later_tool_rounds() {
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
                stage: "正在准备工具执行".to_string(),
                stage_code: None,
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
                stage: "正在调用模型和工具".to_string(),
                stage_code: None,
            },
        },
    )
    .expect("running");
    let sink = RecordingSink::default();
    let mut observer = AgentRunStreamObserver::new_with_deferred_deltas(
        &db,
        &accepted.run_id,
        event_state_version(&running),
        &sink,
        true,
    );

    observer.on_tools_finished().expect("finish search tools");
    observer.on_tools_starting().expect("start read_note round");
    observer
        .on_tools_finished()
        .expect("finish read_note tools");
    assert!(
        sink.events
            .lock()
            .expect("sink lock")
            .iter()
            .all(|event| event["payload"]["stage"] != "正在生成答复"),
        "generating stage must stay after every tool round, including read_note"
    );
    assert!(!observer.emitted_generating_answer_stage());

    observer
        .emit_generating_answer_stage_if_needed()
        .expect("emit after tool loop returns");
    let durable = sink.events.lock().expect("sink lock").clone();
    assert_eq!(durable.len(), 1);
    assert_eq!(durable[0]["payload"]["stage"], "正在生成答复");
    assert!(observer.emitted_generating_answer_stage());
}

#[test]
fn tool_loop_observer_resets_provisional_answer_when_tools_restart() {
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
                stage: "正在准备工具执行".to_string(),
                stage_code: None,
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
                stage: "正在调用模型和工具".to_string(),
                stage_code: None,
            },
        },
    )
    .expect("running");
    let sink = RecordingSink::default();
    let mut observer = AgentRunStreamObserver::new_with_deferred_deltas(
        &db,
        &accepted.run_id,
        event_state_version(&running),
        &sink,
        true,
    );

    observer.on_tools_finished().expect("finish first tools");
    observer
        .observe(
            &StreamEvent {
                request_id: accepted.run_id.clone(),
                event_type: StreamEventType::Token,
                data: StreamEventData::Token {
                    token: "半成品答复\n".to_string(),
                    replace_visible: false,
                },
                surface: StreamSurface::VisibleAnswerSanitized,
                classified: false,
            },
            0,
        )
        .expect("stream provisional answer");
    assert_eq!(
        sink.presentation_events
            .lock()
            .expect("presentation lock")
            .len(),
        1
    );

    observer
        .on_tools_starting()
        .expect("re-defer before next tools");
    let presentation = sink
        .presentation_events
        .lock()
        .expect("presentation lock")
        .clone();
    assert_eq!(presentation.len(), 2);
    assert_eq!(presentation[0]["kind"], "answer_delta");
    assert_eq!(presentation[0]["delta"], "半成品答复\n");
    assert_eq!(presentation[1]["kind"], "answer_reset");

    observer
        .observe(
            &StreamEvent {
                request_id: accepted.run_id.clone(),
                event_type: StreamEventType::Token,
                data: StreamEventData::Token {
                    token: "不应再流出\n".to_string(),
                    replace_visible: false,
                },
                surface: StreamSurface::VisibleAnswerSanitized,
                classified: false,
            },
            1,
        )
        .expect("deferred token after tools restart");
    assert_eq!(
        sink.presentation_events
            .lock()
            .expect("presentation lock")
            .len(),
        2,
        "tokens during a later tool round must stay deferred"
    );
}

#[test]
fn run_stream_observer_replays_only_safe_reasoning_summaries_after_turn_done() {
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
                stage_code: None,
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
                stage: "正在生成答复".into(),
                stage_code: None,
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
                event_type: StreamEventType::ReasoningSummary,
                data: StreamEventData::ReasoningSummary {
                    summary_id: "summary-1".into(),
                    text: "正在核对资料；sk-test-123456789012 不应进入历史。".into(),
                },
                surface: StreamSurface::InternalCandidate,
                classified: false,
            },
            0,
        )
        .expect("transient summary");
    assert_eq!(
        sink.presentation_events
            .lock()
            .expect("presentation sink lock")
            .len(),
        1
    );
    observer
        .observe(
            &StreamEvent {
                request_id: accepted.run_id.clone(),
                event_type: StreamEventType::ReasoningSummary,
                data: StreamEventData::ReasoningSummary {
                    summary_id: "summary-2".into(),
                    text: r#"{"query":"private search text","limit":5}"#.into(),
                },
                surface: StreamSurface::InternalCandidate,
                classified: false,
            },
            0,
        )
        .expect("structured summary is generalized");
    observer
        .observe(
            &StreamEvent {
                request_id: accepted.run_id.clone(),
                event_type: StreamEventType::ReasoningSummary,
                data: StreamEventData::ReasoningSummary {
                    summary_id: "summary-\u{0001}".into(),
                    text: "安全\u{0001}".repeat(500),
                },
                surface: StreamSurface::InternalCandidate,
                classified: false,
            },
            0,
        )
        .expect("escaped controls must still fit the durable event budget");

    observer
        .observe(
            &StreamEvent {
                request_id: accepted.run_id.clone(),
                event_type: StreamEventType::Done,
                data: StreamEventData::Done { usage: None },
                surface: StreamSurface::InternalCandidate,
                classified: false,
            },
            0,
        )
        .expect("persist final summary");

    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run exists");
    let persisted = replay
        .events
        .iter()
        .map(|event| serde_json::to_value(event).expect("serialize summary"))
        .filter(|event| event["type"] == "reasoning_summary")
        .collect::<Vec<_>>();
    assert_eq!(persisted.len(), 3);
    assert_eq!(persisted[0]["payload"]["summaryId"], "summary-1");
    assert!(persisted[0]["payload"]["text"]
        .as_str()
        .expect("summary text")
        .contains("正在核对资料"));
    assert_eq!(persisted[1]["payload"]["summaryId"], "summary-2");
    assert_eq!(persisted[1]["payload"]["text"], "已完成必要的推理准备。");
    assert_eq!(persisted[2]["payload"]["summaryId"], "summary-_");
    let control_payload = serde_json::to_string(&persisted[2]["payload"])
        .expect("serialize normalized control summary");
    assert!(control_payload.chars().count() <= 2_000);
    assert!(!control_payload.contains("\\u0001"));
    let serialized = serde_json::to_string(&persisted).expect("serialize persisted summaries");
    assert!(!serialized.contains("sk-test-123456789012"));
    assert!(!serialized.contains("private search text"));
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
                stage_code: None,
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
                stage_code: None,
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
    let presentation_events = sink
        .presentation_events
        .lock()
        .expect("presentation sink lock");
    assert_eq!(presentation_events.len(), 4);
    assert_eq!(presentation_events[0]["kind"], "answer_delta");
    assert_eq!(presentation_events[0]["delta"], "流式片段");
    assert_eq!(presentation_events[1]["kind"], "answer_reset");
    assert_eq!(presentation_events[2]["kind"], "answer_delta");
    assert_eq!(presentation_events[2]["delta"], "流式最终答复");
    assert_eq!(presentation_events[3]["kind"], "answer_complete");
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
async fn presentation_delivery_failure_never_invalidates_the_durable_answer() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let provider = MockStreamingProvider {
        calls: AtomicU32::new(0),
        failure: None,
    };
    let sink = SelectiveFailingSink {
        fail_type: "never",
        events: std::sync::Mutex::new(Vec::new()),
    };

    RunEngine::execute_direct_streaming_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        &provider,
        &sink,
    )
    .await
    .expect("presentation delivery failures are best effort");
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
    let persisted_deltas = replay
        .events
        .iter()
        .filter_map(|event| {
            let event = serde_json::to_value(event).expect("event");
            (event["type"] == "content_delta").then(|| event["payload"]["delta"].clone())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        persisted_deltas,
        [serde_json::Value::String("流式最终答复".into())]
    );
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
    let accepted = RunIntake::start(&db, standard_tool_loop_request()).expect("accepted");
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
    let accepted = RunIntake::start(&db, standard_tool_loop_request()).expect("accepted");
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
async fn strict_web_answer_without_current_run_marker_completes_with_a_source_group() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let evidence = AgentEvidenceRepository::register_web(
        &db,
        WebEvidenceInput {
            session_id: 1,
            run_id: accepted.run_id.clone(),
            message_seq_first: 1,
            material_role: MaterialRole::Reference,
            title: "当前轮官方来源".to_string(),
            url: "https://example.test/current-run".to_string(),
            normalized_url: "https://example.test/current-run".to_string(),
            domain: "example.test".to_string(),
            retrieved_at: "2026-07-27T00:00:00Z".to_string(),
            provider_id: "test-web".to_string(),
            provider_kind: "https".to_string(),
            raw_result_hash: "current-run-source".to_string(),
            extraction_method: "test".to_string(),
            bounded_excerpt: "当前轮证据摘录。".to_string(),
            retrieval_reason: Some("test".to_string()),
            score: None,
            source_rank: Some(1),
            conflict_group: None,
            failure_reason: None,
        },
    )
    .expect("register current-run web evidence");
    let provider = ScriptedToolLoopProvider {
        responses: std::sync::Mutex::new(VecDeque::from([
            crate::ai_runtime::model_gateway::GatewayResponse {
                content: Some("缺少本轮引用的答复。".to_string()),
                tool_calls: vec![],
                usage: Default::default(),
                finish_reason: "stop".to_string(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
    };
    let executor = StrictWebEvidenceExecutor {
        evidence_ids: vec![evidence.evidence_id],
    };
    let sink = RecordingSink::default();

    RunEngine::execute_tool_loop_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        vec![crate::ai_runtime::LlmMessage {
            role: MessageRole::User,
            content: "请给出当前事实".into(),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        }],
        vec![],
        &[evidence.evidence_id],
        None,
        &provider,
        &executor,
        &sink,
    )
    .await
    .expect("missing current-run marker must degrade to the verified source group");
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Completed);
    assert_eq!(terminal_event_count(&replay.events), 1);
    let citation_map: String = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT citation_map_json FROM session_messages
                 WHERE session_id = 1 AND role = 'assistant'",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .expect("persisted source-group answer");
    assert!(citation_map.contains("\"mode\":\"source_group_fallback\""));
}

#[tokio::test]
async fn strict_web_answer_persists_canonical_current_run_marker_and_citation_map() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let evidence = AgentEvidenceRepository::register_web(
        &db,
        WebEvidenceInput {
            session_id: 1,
            run_id: accepted.run_id.clone(),
            message_seq_first: 1,
            material_role: MaterialRole::Reference,
            title: "当前轮官方来源".to_string(),
            url: "https://example.test/current-run".to_string(),
            normalized_url: "https://example.test/current-run".to_string(),
            domain: "example.test".to_string(),
            retrieved_at: "2026-07-27T00:00:00Z".to_string(),
            provider_id: "test-web".to_string(),
            provider_kind: "https".to_string(),
            raw_result_hash: "current-run-source".to_string(),
            extraction_method: "test".to_string(),
            bounded_excerpt: "当前轮证据摘录。".to_string(),
            retrieval_reason: Some("test".to_string()),
            score: None,
            source_rank: Some(1),
            conflict_group: None,
            failure_reason: None,
        },
    )
    .expect("register current-run web evidence");
    let provider = ScriptedToolLoopProvider {
        responses: std::sync::Mutex::new(VecDeque::from([
            crate::ai_runtime::model_gateway::GatewayResponse {
                content: Some("已由当前轮证据核验。[W1]".to_string()),
                tool_calls: vec![],
                usage: Default::default(),
                finish_reason: "stop".to_string(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
    };
    let executor = StrictWebEvidenceExecutor {
        evidence_ids: vec![evidence.evidence_id],
    };
    let sink = RecordingSink::default();

    RunEngine::execute_tool_loop_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        vec![crate::ai_runtime::LlmMessage {
            role: MessageRole::User,
            content: "请给出当前事实".into(),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        }],
        vec![],
        &[evidence.evidence_id],
        None,
        &provider,
        &executor,
        &sink,
    )
    .await
    .expect("valid current-run citation completes");

    let (content, citation_map): (String, String) = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT content, citation_map_json FROM session_messages
                 WHERE session_id = ?1 AND role = 'assistant'",
                [1],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(Into::into)
        })
        .expect("persisted strict-web answer");
    assert_eq!(content, "已由当前轮证据核验。[W1]");
    assert!(citation_map.contains("https://example.test/current-run"));
}

#[tokio::test]
async fn strict_web_missing_marker_completes_with_a_verified_source_group() {
    let db = Database::open_in_memory().expect("database");
    let accepted = RunIntake::start(&db, request()).expect("accepted");
    let evidence = AgentEvidenceRepository::register_web(
        &db,
        WebEvidenceInput {
            session_id: 1,
            run_id: accepted.run_id.clone(),
            message_seq_first: 1,
            material_role: MaterialRole::Reference,
            title: "当前轮来源".to_string(),
            url: "https://example.test/repair".to_string(),
            normalized_url: "https://example.test/repair".to_string(),
            domain: "example.test".to_string(),
            retrieved_at: "2026-07-27T00:00:00Z".to_string(),
            provider_id: "test-web".to_string(),
            provider_kind: "https".to_string(),
            raw_result_hash: "repair-source".to_string(),
            extraction_method: "test".to_string(),
            bounded_excerpt: "当前轮证据摘录。".to_string(),
            retrieval_reason: Some("test".to_string()),
            score: None,
            source_rank: Some(1),
            conflict_group: None,
            failure_reason: None,
        },
    )
    .expect("register evidence");
    let provider = ScriptedToolLoopProvider {
        responses: std::sync::Mutex::new(VecDeque::from([
            crate::ai_runtime::model_gateway::GatewayResponse {
                content: Some("结论来自当前轮证据。".to_string()),
                tool_calls: vec![],
                usage: Default::default(),
                finish_reason: "stop".to_string(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
    };
    let sink = RecordingSink::default();

    RunEngine::execute_tool_loop_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        vec![crate::ai_runtime::LlmMessage {
            role: MessageRole::User,
            content: "请给出当前事实".into(),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        }],
        vec![],
        &[evidence.evidence_id],
        None,
        &provider,
        &StrictWebEvidenceExecutor {
            evidence_ids: vec![evidence.evidence_id],
        },
        &sink,
    )
    .await
    .expect("source-group fallback completes");

    let (content, citation_map): (String, String) = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT content, citation_map_json FROM session_messages WHERE session_id = 1 AND role = 'assistant'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(Into::into)
        })
        .expect("source-group answer persisted");
    assert_eq!(content, "结论来自当前轮证据。");
    assert!(citation_map.contains("\"mode\":\"source_group_fallback\""));
}

#[tokio::test]
async fn strict_web_follow_up_persists_run_local_citation_map() {
    let db = Database::open_in_memory().expect("database");
    let first = RunIntake::start(&db, request()).expect("first accepted");
    let first_evidence = AgentEvidenceRepository::register_web(
        &db,
        WebEvidenceInput {
            session_id: 1,
            run_id: first.run_id.clone(),
            message_seq_first: 1,
            material_role: MaterialRole::Reference,
            title: "首轮来源".to_string(),
            url: "https://example.test/first".to_string(),
            normalized_url: "https://example.test/first".to_string(),
            domain: "example.test".to_string(),
            retrieved_at: "2026-07-27T00:00:00Z".to_string(),
            provider_id: "test-web".to_string(),
            provider_kind: "https".to_string(),
            raw_result_hash: "first-source".to_string(),
            extraction_method: "test".to_string(),
            bounded_excerpt: "首轮证据。".to_string(),
            retrieval_reason: Some("test".to_string()),
            score: None,
            source_rank: Some(1),
            conflict_group: None,
            failure_reason: None,
        },
    )
    .expect("register first evidence");
    let sink = RecordingSink::default();
    let first_provider = ScriptedToolLoopProvider {
        responses: std::sync::Mutex::new(VecDeque::from([
            crate::ai_runtime::model_gateway::GatewayResponse {
                content: Some("首轮回答。[W1]".to_string()),
                tool_calls: vec![],
                usage: Default::default(),
                finish_reason: "stop".to_string(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
    };
    RunEngine::execute_tool_loop_with_sink(
        &db,
        &first.session,
        &first.run_id,
        vec![crate::ai_runtime::LlmMessage {
            role: MessageRole::User,
            content: "首轮问题".into(),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        }],
        vec![],
        &[first_evidence.evidence_id],
        None,
        &first_provider,
        &StrictWebEvidenceExecutor {
            evidence_ids: vec![first_evidence.evidence_id],
        },
        &sink,
    )
    .await
    .expect("first strict web answer completes");

    let mut follow_up_request = request();
    follow_up_request.client_request_id = "strict-web-follow-up".to_string();
    follow_up_request.session = Some(first.session.clone());
    follow_up_request.turn.message = "第二轮问题".to_string();
    let follow_up = RunIntake::start(&db, follow_up_request).expect("follow-up accepted");
    let follow_up_evidence = AgentEvidenceRepository::register_web(
        &db,
        WebEvidenceInput {
            session_id: 1,
            run_id: follow_up.run_id.clone(),
            message_seq_first: 3,
            material_role: MaterialRole::Reference,
            title: "第二轮来源".to_string(),
            url: "https://example.test/follow-up".to_string(),
            normalized_url: "https://example.test/follow-up".to_string(),
            domain: "example.test".to_string(),
            retrieved_at: "2026-07-27T00:01:00Z".to_string(),
            provider_id: "test-web".to_string(),
            provider_kind: "https".to_string(),
            raw_result_hash: "follow-up-source".to_string(),
            extraction_method: "test".to_string(),
            bounded_excerpt: "第二轮证据。".to_string(),
            retrieval_reason: Some("test".to_string()),
            score: None,
            source_rank: Some(1),
            conflict_group: None,
            failure_reason: None,
        },
    )
    .expect("register follow-up evidence");
    let follow_up_provider = ScriptedToolLoopProvider {
        responses: std::sync::Mutex::new(VecDeque::from([
            crate::ai_runtime::model_gateway::GatewayResponse {
                content: Some("第二轮回答。[W1]".to_string()),
                tool_calls: vec![],
                usage: Default::default(),
                finish_reason: "stop".to_string(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
    };
    let follow_up_context =
        RunContextAssembler::assemble(&db, None, &follow_up.session.session_key, &follow_up.run_id)
            .expect("assemble follow-up context with prior strict-web answer");
    let follow_up_messages =
        follow_up_context.messages_with_domain_plan(&follow_up_context.domain_plan());
    let historical_answer = follow_up_messages
        .iter()
        .find(|message| message.content.text_content().contains("首轮回答"))
        .expect("follow-up includes prior assistant answer");
    assert!(historical_answer
        .content
        .text_content()
        .contains("[历史来源 1]"));
    assert!(!historical_answer.content.text_content().contains("[W1]"));
    assert!(!historical_answer
        .content
        .text_content()
        .contains("https://"));
    RunEngine::execute_tool_loop_with_sink(
        &db,
        &follow_up.session,
        &follow_up.run_id,
        follow_up_messages,
        vec![],
        &[follow_up_evidence.evidence_id],
        None,
        &follow_up_provider,
        &StrictWebEvidenceExecutor {
            evidence_ids: vec![follow_up_evidence.evidence_id],
        },
        &sink,
    )
    .await
    .expect("follow-up strict web answer completes");

    let (content, citation_map): (String, String) = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT content, citation_map_json FROM session_messages
                 WHERE session_id = 1 AND role = 'assistant' ORDER BY seq DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(Into::into)
        })
        .expect("follow-up persisted");
    assert_eq!(content, "第二轮回答。[W1]");
    assert!(citation_map.contains("\"index\":1"));
    assert!(!citation_map.contains("\"index\":2"));
}

#[tokio::test]
async fn source_group_strict_web_turn_does_not_block_the_next_turn_in_the_same_session() {
    let db = Database::open_in_memory().expect("database");
    let failed = RunIntake::start(&db, request()).expect("accepted strict-web turn");
    let evidence = AgentEvidenceRepository::register_web(
        &db,
        WebEvidenceInput {
            session_id: 1,
            run_id: failed.run_id.clone(),
            message_seq_first: 1,
            material_role: MaterialRole::Reference,
            title: "当前轮来源".to_string(),
            url: "https://example.test/failed-turn".to_string(),
            normalized_url: "https://example.test/failed-turn".to_string(),
            domain: "example.test".to_string(),
            retrieved_at: "2026-07-27T00:00:00Z".to_string(),
            provider_id: "test-web".to_string(),
            provider_kind: "https".to_string(),
            raw_result_hash: "failed-turn-source".to_string(),
            extraction_method: "test".to_string(),
            bounded_excerpt: "当前轮证据摘录。".to_string(),
            retrieval_reason: Some("test".to_string()),
            score: None,
            source_rank: Some(1),
            conflict_group: None,
            failure_reason: None,
        },
    )
    .expect("register strict-web evidence");
    let invalid_provider = ScriptedToolLoopProvider {
        responses: std::sync::Mutex::new(VecDeque::from([
            crate::ai_runtime::model_gateway::GatewayResponse {
                content: Some("遗漏当前轮引用。".to_string()),
                tool_calls: vec![],
                usage: Default::default(),
                finish_reason: "stop".to_string(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
    };
    let sink = RecordingSink::default();
    RunEngine::execute_tool_loop_with_sink(
        &db,
        &failed.session,
        &failed.run_id,
        vec![crate::ai_runtime::LlmMessage {
            role: MessageRole::User,
            content: "严格联网问题".into(),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        }],
        vec![],
        &[evidence.evidence_id],
        None,
        &invalid_provider,
        &StrictWebEvidenceExecutor {
            evidence_ids: vec![evidence.evidence_id],
        },
        &sink,
    )
    .await
    .expect("strict-web source group completes");

    let mut retry = request();
    retry.client_request_id = "turn-after-strict-web-failure".to_string();
    retry.session = Some(failed.session.clone());
    retry.turn.message = "请继续处理另一个问题".to_string();
    let retry = RunIntake::start(&db, retry).expect("accept next turn");
    let provider = MockProvider {
        calls: Cell::new(0),
        response: Some("后续轮次正常完成。".to_string()),
    };
    RunEngine::execute_direct_with_sink(&db, &retry.session, &retry.run_id, &provider, &sink)
        .expect("next turn completes");
    assert_eq!(
        RunIntake::get(&db, &retry.session, &retry.run_id)
            .expect("next replay")
            .expect("next run")
            .run
            .state,
        RunState::Completed
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
                    continuation: None,
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
            verification_requirement: super::run_contract::VerificationRequirement::None,
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
                        replace_visible: false,
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
                continuation: None,
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
            verification_requirement: super::run_contract::VerificationRequirement::None,
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

#[test]
fn tool_loop_limit_keeps_a_dedicated_safe_code_and_message() {
    assert_eq!(
        super::run_engine::classify_tool_loop_failure(&AppError::msg("agent_run_tool_loop_limit",)),
        SafeRunErrorCode::ToolLoopLimit,
    );
    assert_eq!(
        SafeRunErrorCode::ToolLoopLimit.as_str(),
        "agent_run_tool_loop_limit"
    );
    let failed = RunEventPayload::Failed {
        code: SafeRunErrorCode::ToolLoopLimit,
        message: "模型调用工具次数过多，请基于已附资料缩小问题后重试".into(),
    };
    let encoded = serde_json::to_value(&failed).expect("serialize failed payload");
    assert_eq!(encoded["code"], "agent_run_tool_loop_limit");
    assert_eq!(
        encoded["message"],
        "模型调用工具次数过多，请基于已附资料缩小问题后重试"
    );
}
