use super::*;
use crate::ai_runtime::model_gateway::{
    GatewayResponse, StreamEvent, StreamEventData, StreamEventObserver, StreamEventType,
    StreamSurface,
};
use crate::ai_runtime::run_contract::{
    AssistantRunEvent, AssistantRunStartRequest, AssistantTurnDraft, SecurityDomain,
};
use crate::ai_runtime::run_intake::RunIntake;

fn accepted(db: &Database) -> crate::ai_runtime::run_contract::AssistantRunAccepted {
    RunIntake::start(
        db,
        AssistantRunStartRequest {
            client_request_id: "publication".into(),
            session: None,
            turn: AssistantTurnDraft {
                message: "根据本地项目笔记调用工具后回答".into(),
                content_parts: None,
                explicit_references: vec![],
                retrieval_scope: Default::default(),
                display_mentions: vec![],
            },
            explicit_action: None,
            web_enabled: false,
            model_override: None,
            external_tool_grants: vec![],
            security_domain: SecurityDomain::Normal,
            classified_context_ref: None,
        },
    )
    .unwrap()
}

struct Sink<'a> {
    db: &'a Database,
    events: Mutex<Vec<AssistantRunEvent>>,
    presentation: Mutex<Vec<RunPresentationPayload>>,
    premature: Mutex<u32>,
}
impl RunEventSink for Sink<'_> {
    fn emit(&self, event: &AssistantRunEvent) -> AppResult<()> {
        if matches!(event.payload(), RunEventPayload::ContentDelta { .. }) {
            let completed = self.db.with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT status = 'completed' FROM agent_runs WHERE run_id = ?1",
                    [event.run_id()],
                    |row| row.get::<_, bool>(0),
                )?)
            })?;
            if !completed {
                *self.premature.lock().unwrap() += 1;
            }
        }
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
    fn emit_presentation(&self, _: &str, payload: RunPresentationPayload) -> AppResult<()> {
        self.presentation.lock().unwrap().push(payload);
        Ok(())
    }
}
struct DraftProvider;
impl ToolLoopProvider for DraftProvider {
    fn answer_turn<'a>(
        &'a self,
        run_id: &'a str,
        _: &'a [crate::ai_runtime::LlmMessage],
        _: &'a [crate::ai_runtime::ToolSpec],
        _: AgentModelTurnBudget,
        observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<Box<dyn Future<Output = AppResult<GatewayResponse>> + Send + 'a>> {
        Box::pin(async move {
            for replacement in [false, true] {
                observer.on_tools_finished()?;
                observer.observe(
                    &StreamEvent {
                        request_id: run_id.into(),
                        event_type: StreamEventType::Token,
                        data: StreamEventData::Token {
                            token: "不能公布的候选文字".into(),
                            replace_visible: replacement,
                        },
                        surface: StreamSurface::VisibleAnswerSanitized,
                        classified: false,
                    },
                    0,
                )?;
                observer.on_tools_starting()?;
            }
            Ok(GatewayResponse {
                content: Some("经过确认的唯一答复。".into()),
                tool_calls: vec![],
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
                continuation: None,
            })
        })
    }
}

#[tokio::test]
async fn direct_candidate_is_private_and_every_answer_event_follows_commit() {
    let db = Database::open_in_memory().unwrap();
    let run = accepted(&db);
    let sink = Sink {
        db: &db,
        events: Mutex::new(vec![]),
        presentation: Mutex::new(vec![]),
        premature: Mutex::new(0),
    };
    RunEngine::execute_direct_streaming_with_sink(
        &db,
        &run.session,
        &run.run_id,
        &DraftProvider,
        &sink,
    )
    .await
    .unwrap();
    assert_eq!(
        *sink.premature.lock().unwrap(),
        0,
        "no durable answer event may escape before final transaction"
    );
    let presentation = sink.presentation.lock().unwrap();
    assert!(
        !presentation
            .iter()
            .any(|p| matches!(p, RunPresentationPayload::AnswerReset)),
        "normal answer must never reset"
    );
    assert!(!serde_json::to_string(&*presentation)
        .unwrap()
        .contains("候选文字"));
    let replay = RunIntake::get(&db, &run.session, &run.run_id)
        .unwrap()
        .unwrap();
    let body: String = replay
        .events
        .iter()
        .filter_map(|e| match e.payload() {
            RunEventPayload::ContentDelta { delta } => Some(delta.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(body, "经过确认的唯一答复。");
}

#[tokio::test]
async fn final_transaction_failure_publishes_neither_draft_nor_final_body() {
    let db = Database::open_in_memory().unwrap();
    let run = accepted(&db);
    db.with_conn(|c| {c.execute_batch("CREATE TRIGGER reject_answer BEFORE INSERT ON session_messages WHEN NEW.role = 'assistant' BEGIN SELECT RAISE(ABORT, 'synthetic'); END;")?; Ok(())}).unwrap();
    let sink = Sink {
        db: &db,
        events: Mutex::new(vec![]),
        presentation: Mutex::new(vec![]),
        premature: Mutex::new(0),
    };
    assert!(RunEngine::execute_direct_streaming_with_sink(
        &db,
        &run.session,
        &run.run_id,
        &DraftProvider,
        &sink
    )
    .await
    .is_err());
    assert!(!sink.presentation.lock().unwrap().iter().any(|p| matches!(
        p,
        RunPresentationPayload::AnswerDelta { .. }
            | RunPresentationPayload::AnswerReset
            | RunPresentationPayload::AnswerComplete
    )));
    let replay = RunIntake::get(&db, &run.session, &run.run_id)
        .unwrap()
        .unwrap();
    assert!(!replay.events.iter().any(|e| matches!(
        e.payload(),
        RunEventPayload::ContentDelta { .. } | RunEventPayload::Completed { .. }
    )));
}

#[tokio::test]
async fn completed_event_failure_rolls_back_body_message_and_publication_metrics() {
    let db = Database::open_in_memory().unwrap();
    let run = accepted(&db);
    db.with_conn(|c| {c.execute_batch("CREATE TRIGGER reject_completed BEFORE INSERT ON agent_run_events WHEN NEW.event_type = 'completed' BEGIN SELECT RAISE(ABORT, 'synthetic'); END;")?; Ok(())}).unwrap();
    let sink = Sink {
        db: &db,
        events: Mutex::new(vec![]),
        presentation: Mutex::new(vec![]),
        premature: Mutex::new(0),
    };
    assert!(RunEngine::execute_direct_streaming_with_sink(
        &db,
        &run.session,
        &run.run_id,
        &DraftProvider,
        &sink
    )
    .await
    .is_err());
    let replay = RunIntake::get(&db, &run.session, &run.run_id)
        .unwrap()
        .unwrap();
    assert!(replay.run.final_message_id.is_none());
    assert!(!replay.events.iter().any(|e| matches!(
        e.payload(),
        RunEventPayload::ContentDelta { .. } | RunEventPayload::Completed { .. }
    )));
    assert!(!sink.events.lock().unwrap().iter().any(|e| matches!(
        e.payload(),
        RunEventPayload::ContentDelta { .. } | RunEventPayload::Completed { .. }
    )));
}

#[tokio::test]
async fn second_commit_cannot_replace_the_published_answer() {
    let db = Database::open_in_memory().unwrap();
    let run = accepted(&db);
    let sink = Sink {
        db: &db,
        events: Mutex::new(vec![]),
        presentation: Mutex::new(vec![]),
        premature: Mutex::new(0),
    };
    RunEngine::execute_direct_streaming_with_sink(
        &db,
        &run.session,
        &run.run_id,
        &DraftProvider,
        &sink,
    )
    .await
    .unwrap();
    let before = RunIntake::get(&db, &run.session, &run.run_id)
        .unwrap()
        .unwrap();
    assert!(AgentRunRepository::finalize_with_events(
        &db,
        FinalizeRunInput {
            run_id: run.run_id.clone(),
            state_version: before.run.state_version,
            content: "不同的第二份正文".into(),
            evidence_ids: vec![],
            citation_map: serde_json::json!({}),
            source_summary: vec![]
        }
    )
    .is_err());
    db.with_read_conn(|conn| {
        let rejected: i64 = conn.query_row("SELECT COALESCE(json_extract(provider_route_summary_json, '$.publication.rejectedFinalizations'), 0) FROM agent_runs WHERE run_id = ?1", [&run.run_id], |row| row.get(0))?;
        assert_eq!(rejected, 1);
        Ok(())
    }).unwrap();
    let after = RunIntake::get(&db, &run.session, &run.run_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_value(before.events).unwrap(),
        serde_json::to_value(after.events).unwrap()
    );
}

struct ResearchProvider(std::sync::atomic::AtomicU32);
impl ToolLoopProvider for ResearchProvider {
    fn answer_turn<'a>(
        &'a self,
        run_id: &'a str,
        messages: &'a [crate::ai_runtime::LlmMessage],
        tools: &'a [crate::ai_runtime::ToolSpec],
        budget: AgentModelTurnBudget,
        observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<Box<dyn Future<Output = AppResult<GatewayResponse>> + Send + 'a>> {
        Box::pin(async move {
            let turn = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let mut response = DraftProvider
                .answer_turn(run_id, messages, tools, budget, observer)
                .await?;
            if turn < 2 {
                response.tool_calls = vec![crate::ai_types::ToolCall::new(
                    format!("call-{turn}"),
                    "test_tool",
                    format!("{{\"round\":{turn}}}"),
                )];
                response.finish_reason = "tool_calls".into();
            }
            Ok(response)
        })
    }
}
struct ResearchExecutor;
impl ToolLoopExecutor for ResearchExecutor {
    fn execute<'a>(
        &'a self,
        _: &'a str,
        call: &'a crate::ai_types::ToolCall,
        _: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<crate::ai_types::ToolCallResult>> + Send + 'a>> {
        Box::pin(async move {
            Ok(crate::ai_types::ToolCallResult {
                tool_name: call.function.name.clone(),
                success: true,
                output: serde_json::json!({"observation":call.function.arguments}),
                duration_ms: 1,
                tokens_used: None,
                error: None,
            })
        })
    }
}
#[tokio::test]
async fn multiple_research_rounds_publish_only_the_committed_final_version() {
    let db = Database::open_in_memory().unwrap();
    let run = accepted(&db);
    let sink = Sink {
        db: &db,
        events: Mutex::new(vec![]),
        presentation: Mutex::new(vec![]),
        premature: Mutex::new(0),
    };
    let provider = ResearchProvider(std::sync::atomic::AtomicU32::new(0));
    RunEngine::execute_tool_loop_with_sink(&db,&run.session,&run.run_id,vec![],vec![crate::ai_types::ToolSpec {name:"test_tool".into(),description:"Synthetic read".into(),input_schema:serde_json::json!({"type":"object","properties":{"round":{"type":"integer"}},"required":["round"]}),access_level:crate::ai_runtime::ToolAccessLevel::ReadProfile,requires_confirmation:false,max_results:None,capability_affinity:vec![]}],&[],None,&provider,&ResearchExecutor,&sink).await.unwrap();
    assert_eq!(provider.0.load(std::sync::atomic::Ordering::SeqCst), 3);
    assert_eq!(*sink.premature.lock().unwrap(), 0);
    assert!(!sink
        .presentation
        .lock()
        .unwrap()
        .iter()
        .any(|event| matches!(
            event,
            RunPresentationPayload::AnswerDelta { .. } | RunPresentationPayload::AnswerReset
        )));
    let replay = RunIntake::get(&db, &run.session, &run.run_id)
        .unwrap()
        .unwrap();
    let body: String = replay
        .events
        .iter()
        .filter_map(|event| match event.payload() {
            RunEventPayload::ContentDelta { delta } => Some(delta.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(body, "经过确认的唯一答复。");
}

struct CancelBeforeCommit<'a> {
    db: &'a Database,
    session: crate::ai_runtime::run_contract::AssistantSessionRef,
}
impl ToolLoopProvider for CancelBeforeCommit<'_> {
    fn answer_turn<'a>(
        &'a self,
        run_id: &'a str,
        messages: &'a [crate::ai_runtime::LlmMessage],
        tools: &'a [crate::ai_runtime::ToolSpec],
        budget: AgentModelTurnBudget,
        observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<Box<dyn Future<Output = AppResult<GatewayResponse>> + Send + 'a>> {
        Box::pin(async move {
            let response = DraftProvider
                .answer_turn(run_id, messages, tools, budget, observer)
                .await?;
            let snapshot = RunIntake::get(self.db, &self.session, run_id)?.unwrap();
            RunIntake::control(
                self.db,
                crate::ai_runtime::run_contract::AssistantRunControlRequest {
                    session: self.session.clone(),
                    run_id: run_id.into(),
                    expected_state_version: snapshot.run.state_version,
                    action: crate::ai_runtime::run_contract::RunControlAction::Cancel,
                },
            )?;
            Ok(response)
        })
    }
}

#[tokio::test]
async fn cancellation_after_candidate_but_before_commit_cannot_publish_it() {
    let db = Database::open_in_memory().unwrap();
    let run = accepted(&db);
    let sink = Sink {
        db: &db,
        events: Mutex::new(vec![]),
        presentation: Mutex::new(vec![]),
        premature: Mutex::new(0),
    };
    let provider = CancelBeforeCommit {
        db: &db,
        session: run.session.clone(),
    };
    let _ = RunEngine::execute_direct_streaming_with_sink(
        &db,
        &run.session,
        &run.run_id,
        &provider,
        &sink,
    )
    .await;
    let replay = RunIntake::get(&db, &run.session, &run.run_id)
        .unwrap()
        .unwrap();
    assert_eq!(replay.run.state, RunState::Cancelled);
    assert!(replay.run.final_message_id.is_none());
    assert!(!replay.events.iter().any(|event| matches!(
        event.payload(),
        RunEventPayload::ContentDelta { .. } | RunEventPayload::Completed { .. }
    )));
    assert!(!sink
        .presentation
        .lock()
        .unwrap()
        .iter()
        .any(|event| matches!(
            event,
            RunPresentationPayload::AnswerDelta { .. }
                | RunPresentationPayload::AnswerReset
                | RunPresentationPayload::AnswerComplete
        )));
}

#[test]
fn committed_chunks_preserve_escaped_unicode_within_event_budget() {
    let source = "\"\\\n\t👨‍👩‍👧‍👦e\u{301}中".repeat(1200);
    let mut remaining = source.clone();
    let mut restored = String::new();
    while !remaining.is_empty() {
        let delta =
            crate::ai_runtime::agent_run_repository::take_safe_content_delta_chunk(&mut remaining)
                .unwrap();
        assert!(!delta.is_empty());
        let payload = RunEventPayload::ContentDelta {
            delta: delta.clone(),
        };
        assert!(serde_json::to_string(&payload).unwrap().chars().count() <= 2000);
        restored.push_str(&delta);
    }
    assert_eq!(restored, source);
}

#[test]
fn generic_event_append_cannot_publish_a_candidate_outside_finalization() {
    let db = Database::open_in_memory().unwrap();
    let run = accepted(&db);
    let result = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: run.run_id.clone(),
            state_version: run.state_version,
            event_type: RunEventType::ContentDelta,
            payload: RunEventPayload::ContentDelta {
                delta: "未确认的候选".into(),
            },
        },
    );
    assert_eq!(
        result.unwrap_err().to_string(),
        "agent_run_finalization_required"
    );
    let replay = RunIntake::get(&db, &run.session, &run.run_id)
        .unwrap()
        .unwrap();
    assert_eq!(replay.events.len(), 1);
}
