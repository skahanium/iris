use std::sync::{Arc, Mutex};

use super::agent_capacity_eval::{spawn_llm_protocol_double, HttpResponseScript};
use super::agent_run_repository::{AgentRunRepository, AppendRunEventInput};
use super::agent_tool_loop::ToolLoopExecutor;
use super::mcp_runtime_registry::{upsert_web_evidence_provider, WebEvidenceProviderInput};
use super::model_gateway::ModelGateway;
use super::normal_run_service::execute_normal_run;
use super::normal_session_repository::NormalSessionRepository;
use super::run_context::RunContextAssembler;
use super::run_contract::{
    AssistantRunEvent, AssistantRunStartRequest, AssistantTurnDraft, RunEventPayload, RunEventType,
    RunState, SecurityDomain,
};
use super::run_engine::{ModelGatewayStreamingDirectAnswerProvider, RunEngine, RunEventSink};
use super::run_intake::RunIntake;
use super::run_tool_loop::NormalRunToolExecutor;
use super::tool_executor::ToolRegistry;
use super::tool_policy::ToolPolicyContext;
use super::{AutonomyLevel, ToolCall};
use crate::ai_types::{EndpointFamily, ProviderConfig};
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

fn web_tool_loop_request() -> AssistantRunStartRequest {
    let mut request = direct_request();
    request.client_request_id = "headless-normal-web-tool-loop".into();
    request.turn.message = "请联网核实 synthetic 的最新状态".into();
    request.web_enabled = true;
    request
}

fn install_headless_contract_mcp(state: &AppState) {
    let fixture = format!(
        "{}/tests/fixtures/agent-capacity-mcp-stdio.sh",
        env!("CARGO_MANIFEST_DIR")
    );
    upsert_web_evidence_provider(
        &state.db,
        &WebEvidenceProviderInput {
            id: "headless-contract-mcp".into(),
            name: "Headless contract MCP".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json: serde_json::json!({
                "command": "/bin/sh",
                "args": [fixture, "search-only"],
            })
            .to_string(),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: Some(r#"{"tool":"search","queryArg":"query"}"#.into()),
            web_fetch_mapping_json: None,
        },
    )
    .expect("headless MCP registry setup");
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

#[tokio::test]
async fn headless_tool_loop_runs_real_executor_mcp_broker_evidence_ledger_and_terminalization() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    install_headless_contract_mcp(&state);
    let sink = RecordingSink::default();
    let accepted = RunIntake::start_with_sink(&state.db, web_tool_loop_request(), &sink)
        .expect("accepted web tool-loop run");
    let context = RunContextAssembler::assemble(
        &state.db,
        None,
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .expect("run context");
    let domain_plan = context.domain_plan();
    let initial_evidence =
        RunContextAssembler::register_evidence(&state.db, &accepted.run_id, &context)
            .expect("initial evidence registration");
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"headless-web-call\",\"type\":\"function\",\"function\":{\"name\":\"web_search\",\"arguments\":\"{\\\"query\\\":\\\"synthetic\\\"}\"}}]}}]}\n\ndata: [DONE]\n\n",
        ),
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"联网证据已核实。\"}}]}\n\ndata: [DONE]\n\n",
        ),
    ])
    .await
    .expect("local LLM boundary");
    let gateway = ModelGateway::new(reqwest::Client::new(), Vec::new());
    let provider = ModelGatewayStreamingDirectAnswerProvider::new(
        &gateway,
        ProviderConfig {
            name: "headless-contract-model".into(),
            base_url: llm.base_url.clone(),
            api_key: None,
            model: "contract-model".into(),
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
        },
        256,
    )
    .expect("model gateway provider");
    let policy = ToolPolicyContext {
        autonomy_level: AutonomyLevel::L2,
        web_search_enabled: true,
        allow_writes: false,
        allow_research: true,
        allow_skill_management: false,
    };
    let tools = ToolRegistry::new().tools_for_policy_surface(&policy, true);
    assert!(tools.iter().any(|tool| tool.name == "web_search"));
    let executor =
        NormalRunToolExecutor::new(&state, None, &accepted, &context, policy, &sink, None);

    RunEngine::execute_tool_loop_with_sink(
        &state.db,
        &accepted.session,
        &accepted.run_id,
        context.messages_with_domain_plan(&domain_plan),
        tools,
        &initial_evidence,
        Some(&domain_plan),
        &provider,
        &executor,
        &sink,
    )
    .await
    .expect("headless production tool-loop chain");
    let calls = llm.finish().await.expect("LLM double completion");
    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("run snapshot")
        .expect("completed run");
    let web_evidence_count = state
        .db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM session_evidence WHERE origin_run_id = ?1 AND source_type = 'web'",
                [&accepted.run_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(Into::into)
        })
        .expect("evidence ledger query");

    assert_eq!(calls.len(), 2, "LLM must complete a real tool continuation");
    assert_eq!(response.run.state, RunState::Completed);
    assert!(
        web_evidence_count >= 1,
        "web result must enter the evidence ledger"
    );
    assert!(response
        .events
        .iter()
        .any(|event| matches!(event.payload(), RunEventPayload::EvidenceRegistered { .. })));
}
