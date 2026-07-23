use std::sync::{Arc, Mutex};

use super::agent_run_repository::{AgentRunRepository, AppendRunEventInput};
use super::agent_tool_loop::ToolLoopExecutor;
use super::normal_run_service::{desktop_app_handle, execute_normal_run};
use super::normal_session_repository::NormalSessionRepository;
use super::run_context::RunContextAssembler;
use super::run_contract::{
    AssistantRunEvent, AssistantRunStartRequest, AssistantTurnDraft, RunEventPayload, RunEventType,
    RunState, SecurityDomain,
};
use super::run_engine::{RunEngine, RunEventSink};
use super::run_intake::RunIntake;
use super::run_tool_loop::NormalRunToolExecutor;
use super::tool_policy::ToolPolicyContext;
use super::{AutonomyLevel, ToolCall};
use crate::app::AppState;
use crate::error::AppResult;

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

fn direct_request() -> AssistantRunStartRequest {
    AssistantRunStartRequest {
        client_request_id: "headless-normal-direct".to_string(),
        session: None,
        turn: AssistantTurnDraft {
            message: "请概述当前信息".to_string(),
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
fn desktop_adapter_preserves_the_app_handle_as_present() {
    let app = tauri::test::mock_app();

    let adapted = desktop_app_handle(app.handle().clone());

    assert!(adapted.is_some());
}

#[tokio::test]
async fn headless_normal_direct_run_preserves_terminal_and_content_lifecycle() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let sink = RecordingSink::default();
    let accepted =
        RunIntake::start_with_sink(&state.db, direct_request(), &sink).expect("accepted run");

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;

    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("run snapshot")
        .expect("persisted run");
    assert_eq!(response.run.state, RunState::Failed);
    let event_types = sink
        .events
        .lock()
        .expect("recorded events")
        .iter()
        .map(|event| event["type"].as_str().expect("event type").to_string())
        .collect::<Vec<_>>();
    assert_eq!(event_types, ["accepted", "stage_changed", "failed"]);

    let messages =
        NormalSessionRepository::load_messages(&state.db, &accepted.session.session_key, 10)
            .expect("session messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[0].content, "请概述当前信息");
}

#[tokio::test]
async fn tool_loop_executor_runs_without_a_desktop_app_handle() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let sink = RecordingSink::default();
    let accepted = RunIntake::start(&state.db, direct_request()).expect("accepted run");
    let context = RunContextAssembler::assemble(
        &state.db,
        None,
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .expect("run context");
    let preparing_version =
        RunEngine::mark_preparing_with_sink(&state.db, &accepted.session, &accepted.run_id, &sink)
            .expect("preparing state");
    AgentRunRepository::append_event(
        &state.db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: preparing_version,
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Running,
                stage: "正在调用模型和工具".to_string(),
            },
        },
    )
    .expect("running state");
    let executor = NormalRunToolExecutor::new(
        &state,
        None,
        &accepted,
        &context,
        ToolPolicyContext {
            autonomy_level: AutonomyLevel::L2,
            web_search_enabled: false,
            allow_writes: false,
            allow_research: false,
            allow_skill_management: false,
        },
        &sink,
        None,
    );

    let result = executor
        .execute(
            &accepted.run_id,
            &ToolCall::new("headless-tool-call", "system_time_now", "{}"),
            1,
        )
        .await
        .expect("bounded tool result");

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["kind"], "system_time");
}
