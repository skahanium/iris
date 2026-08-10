use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::agent_capacity_eval::{
    spawn_llm_protocol_double, EvaluationTelemetryTap, HttpResponseScript,
};
use super::agent_run_repository::{AgentRunRepository, AppendRunEventInput};
use super::agent_tool_loop::ToolLoopExecutor;
use super::mcp_runtime_registry::{upsert_web_evidence_provider, WebEvidenceProviderInput};
use super::model_gateway::ModelGateway;
use super::normal_run_service::{
    build_cached_skill_activation, execute_normal_run, execute_normal_run_with_eval_telemetry,
    required_web_query_from_authorized_material, required_web_query_from_user_history,
    strict_follow_up_capabilities,
};
use super::normal_session_repository::NormalSessionRepository;
use super::run_context::RunContextAssembler;
use super::run_contract::{
    AssistantRunEvent, AssistantRunStartRequest, AssistantTurnDraft, CapabilityId, ContextMode,
    RunEventPayload, RunEventType, RunState, SecurityDomain,
};
use super::run_engine::{ModelGatewayStreamingDirectAnswerProvider, RunEngine, RunEventSink};
use super::run_intake::{looks_like_local_vault_dependency, RunIntake};
use super::run_tool_loop::NormalRunToolExecutor;
use super::tool_executor::ToolRegistry;
use super::ToolCall;
use crate::ai_types::{EndpointFamily, ProviderConfig};
use crate::app::AppState;
use crate::error::AppResult;
use crate::llm::config::{LlmRoutingConfig, ModelReference, ProviderOverride};

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
        external_tool_grants: Vec::new(),
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

#[test]
fn required_web_query_avoids_polluting_a_new_question_with_retry_chatter() {
    let history = vec![
        "你再试试?".to_string(),
        "这不会是 OpenAI 自导自演的吧？".to_string(),
    ];

    assert_eq!(
        required_web_query_from_user_history(
            "为什么我总觉得 OpenAI 和 Anthropic 的负责人表演欲望都严重过头？",
            &history,
        ),
        "为什么我总觉得 OpenAI 和 Anthropic 的负责人表演欲望都严重过头？"
    );
}

#[test]
fn required_web_query_uses_last_substantive_turn_for_a_retry_instruction() {
    let history = vec![
        "你再试试?".to_string(),
        "详细讲一下 OpenAI AI 智能体越狱事件".to_string(),
    ];

    assert_eq!(
        required_web_query_from_user_history("你再试试?", &history),
        "详细讲一下 OpenAI AI 智能体越狱事件\n你再试试?"
    );
}

#[test]
fn required_web_query_uses_explicitly_authorized_material_to_resolve_a_deictic_question() {
    let query = required_web_query_from_authorized_material(
        "这是什么时候召开的会议？",
        &[],
        ["中国共产党第十八次全国代表大会".to_string()],
    );

    assert!(query.contains("中国共产党第十八次全国代表大会"));
    assert!(query.contains("这是什么时候召开的会议"));
}

#[test]
fn required_web_query_never_uses_automatic_local_retrieval_without_explicit_authorization() {
    let query = required_web_query_from_authorized_material(
        "这是什么时候召开的会议？",
        &[],
        std::iter::empty::<String>(),
    );

    assert_eq!(query, "这是什么时候召开的会议？");
}

#[test]
fn strict_web_follow_up_surface_keeps_only_vault_and_explicit_external_reads() {
    let capabilities = strict_follow_up_capabilities(&[
        CapabilityId::new("runtime.read"),
        CapabilityId::new("vault.read"),
        CapabilityId::new("web.search"),
        CapabilityId::new("external.read"),
        CapabilityId::new("note.apply_patch"),
    ]);
    let names = capabilities
        .iter()
        .map(CapabilityId::as_str)
        .collect::<Vec<_>>();

    assert_eq!(names, ["vault.read", "external.read"]);
}

fn install_headless_contract_mcp(state: &AppState) {
    let (command, args) = if cfg!(windows) {
        let fixture = format!(
            "{}\\tests\\fixtures\\agent-capacity-mcp-stdio.ps1",
            env!("CARGO_MANIFEST_DIR")
        );
        (
            "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
            vec![
                "-NoProfile".to_string(),
                "-NonInteractive".to_string(),
                "-ExecutionPolicy".to_string(),
                "Bypass".to_string(),
                "-File".to_string(),
                fixture,
                "search-only".to_string(),
            ],
        )
    } else {
        let fixture = format!(
            "{}/tests/fixtures/agent-capacity-mcp-stdio.sh",
            env!("CARGO_MANIFEST_DIR")
        );
        ("/bin/sh", vec![fixture, "search-only".to_string()])
    };
    upsert_web_evidence_provider(
        &state.db,
        &WebEvidenceProviderInput {
            id: "headless-contract-mcp".into(),
            name: "Headless contract MCP".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json: serde_json::json!({
                "command": command,
                "args": args,
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
    assert!(matches!(
        response.events.last().map(AssistantRunEvent::payload),
        Some(RunEventPayload::Failed {
            code: super::run_contract::SafeRunErrorCode::WebVerificationRequired,
            ..
        })
    ));
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
async fn normal_run_injects_cached_confirmed_skill_after_source_file_is_removed() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let vault = directory.path().join("vault");
    std::fs::create_dir_all(&vault).expect("vault directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    state.set_vault(vault.clone()).expect("activate vault");
    let skill_path = vault.join(".iris/skills/run-skill/SKILL.md");
    let skill_target = std::path::PathBuf::from("run-skill/SKILL.md");
    let skill = crate::ai_runtime::skills::write_confirmed_skill_content(
        &vault,
        &skill_target,
        crate::ai_runtime::skills::SkillScope::Vault,
        "---\nname: run-skill\ndescription: run-skill applies a confirmed response style\n---\n\nAlways include the marker SKILL-RUN-CACHED.",
    )
    .expect("write confirmed skill");
    state
        .upsert_cached_skill_for_vault(&vault, skill)
        .expect("cache confirmed skill");
    std::fs::remove_file(skill_path).expect("remove source after caching");

    let llm = spawn_llm_protocol_double(vec![HttpResponseScript::sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"已答复\"}}]}\n\ndata: [DONE]\n\n",
    )])
    .await
    .expect("local LLM boundary");
    let mut routing = LlmRoutingConfig::default();
    routing.providers.clear();
    routing.providers.insert(
        "custom".into(),
        ProviderOverride {
            base_url: Some(llm.base_url.clone()),
            enabled_models: Some(vec!["cached-skill-model".into()]),
            ..Default::default()
        },
    );
    routing.default_model = Some(ModelReference {
        provider_id: "custom".into(),
        model_id: "cached-skill-model".into(),
    });
    crate::llm::config::save(&state.db, &routing).expect("normal service route setup");
    state.set_test_streaming_client(reqwest::Client::new());
    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.client_request_id = "cached-skill-production-run".into();
    request.turn.message = "Rewrite this sentence using run-skill: Hello.".into();
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink).expect("accepted run");

    execute_normal_run(
        Arc::clone(&state),
        accepted.clone(),
        Some(vault),
        None,
        &sink,
    )
    .await;

    let captures = tokio::time::timeout(Duration::from_secs(2), llm.finish())
        .await
        .expect("cached Skill run must reach the model boundary")
        .expect("LLM completion");
    let system_prompt = captures[0].body["messages"][0]["content"]
        .as_str()
        .expect("system prompt text");
    assert!(system_prompt.contains("## Activated Skills"));
    assert!(system_prompt.contains("SKILL-RUN-CACHED"));
    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("run snapshot")
        .expect("persisted run");
    assert_eq!(response.run.state, RunState::Completed);
}

#[test]
fn production_vault_set_keeps_new_vault_skill_available_to_normal_activation() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let vault = directory.path().join("vault");
    std::fs::create_dir_all(&vault).expect("vault directory");
    crate::ai_runtime::skills::write_confirmed_skill_content(
        &vault,
        &std::path::PathBuf::from("vault-command-skill/SKILL.md"),
        crate::ai_runtime::skills::SkillScope::Vault,
        "---\nname: vault-command-skill\ndescription: Apply the production vault command Skill\n---\n\nUse the production vault command instructions.",
    )
    .expect("write confirmed Skill before vault activation");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    // Exercise the production `vault_set` state-transition order without
    // starting a platform watcher in the headless test runtime.
    state
        .set_vault(vault.clone())
        .expect("set production vault");
    let active_vault = state.vault_path().expect("canonical active vault");
    assert_eq!(
        state
            .cached_skills_for_vault(&active_vault)
            .expect("read new vault registry")
            .expect("new vault registry")
            .len(),
        1,
        "set_vault must install the lexical registry before transient cleanup"
    );
    state.clear_ai_state();
    assert_eq!(
        state
            .cached_skills_for_vault(&active_vault)
            .expect("read registry after cleanup")
            .expect("registry after cleanup")
            .len(),
        1,
        "transient vault cleanup must preserve the new registry"
    );

    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.client_request_id = "vault-set-skill-activation".into();
    request.turn.message = "请使用 vault-command-skill".into();
    assert!(
        !looks_like_local_vault_dependency(&request.turn.message),
        "a Skill identifier is not itself a request to retrieve vault material"
    );
    assert_ne!(
        RunIntake::resolve_envelope(&request)
            .expect("skill request envelope")
            .context,
        ContextMode::ImplicitVault,
        "a Skill identifier is not itself a request to retrieve vault material"
    );
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink).expect("accepted run");
    let context = RunContextAssembler::assemble(
        &state.db,
        Some(&active_vault),
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .expect("run context");

    let activation = build_cached_skill_activation(&state, Some(&active_vault), &context, &[])
        .expect("activation");

    assert_eq!(
        activation
            .plan
            .expect("vault_set must leave the lexical registry available")
            .activated_skills[0]
            .name,
        "vault-command-skill"
    );
}

#[test]
fn vault_source_request_still_enters_the_fail_closed_local_retrieval_boundary() {
    let mut request = direct_request();
    request.turn.message = "请根据 vault 的笔记回答。".into();

    assert_eq!(
        RunIntake::resolve_envelope(&request)
            .expect("vault material envelope")
            .context,
        ContextMode::ImplicitVault,
        "source-reading language must remain distinct from a Skill identifier"
    );
}

#[test]
fn normal_run_skill_activation_reads_prepared_query_vector_without_embedding_work() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let vault = directory.path().join("vault");
    std::fs::create_dir_all(&vault).expect("vault directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    state.set_vault(vault.clone()).expect("activate vault");
    for (name, description) in [
        ("alpha-general", "assistant"),
        ("beta-general", "assistant"),
        ("z-release-readiness", "assistant"),
    ] {
        let entry = crate::ai_runtime::skills::write_confirmed_skill_content(
            &vault,
            &std::path::PathBuf::from(format!("{name}/SKILL.md")),
            crate::ai_runtime::skills::SkillScope::Vault,
            &format!(
                "---\nname: {name}\ndescription: {description}\n---\n\nUse {name} instructions."
            ),
        )
        .expect("write confirmed Skill");
        state
            .upsert_cached_skill_for_vault(&vault, entry)
            .expect("cache Skill");
    }
    let scheduler = state.embedding_scheduler();
    scheduler.reset_for_vault();
    state
        .db
        .with_conn(|conn| {
            for (name, axis) in [
                ("alpha-general", 0_usize),
                ("beta-general", 1_usize),
                ("z-release-readiness", 2_usize),
            ] {
                let mut vector = vec![0.0_f32; crate::embedding::engine::EMBEDDING_DIMENSION];
                vector[axis] = 1.0;
                conn.execute(
                    "UPDATE skill_activation_index
                     SET embedding_json = ?1,
                         embedding_model = ?2,
                         embedding_dimensions = ?3
                     WHERE skill_name = ?4 AND scope = 'Vault'",
                    rusqlite::params![
                        serde_json::to_string(&vector)?,
                        crate::embedding::engine::EMBEDDING_MODEL_FINGERPRINT,
                        crate::embedding::engine::EMBEDDING_DIMENSION as i64,
                        name,
                    ],
                )?;
            }
            Ok(())
        })
        .expect("seed activation vectors");
    state
        .refresh_skills_for_vault(&vault)
        .expect("replace the in-memory activation index");
    let mut query_vector = vec![0.0_f32; crate::embedding::engine::EMBEDDING_DIMENSION];
    query_vector[2] = 1.0;
    scheduler.cache_skill_activation_query_for_test("发版前看看能不能上线", query_vector);
    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.client_request_id = "prepared-skill-query".into();
    request.turn.message = "发版前看看能不能上线".into();
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink).expect("accepted run");
    let context = RunContextAssembler::assemble(
        &state.db,
        Some(&vault),
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .expect("run context");
    state
        .db
        .with_conn(|conn| {
            conn.execute("DROP TABLE skill_activation_index", [])?;
            Ok(())
        })
        .expect("remove persisted index after the Run context is ready");

    let activation =
        build_cached_skill_activation(&state, Some(&vault), &context, &[]).expect("activation");

    assert_eq!(
        activation
            .plan
            .expect("prepared vector should activate Skills")
            .activated_skills[0]
            .name,
        "z-release-readiness"
    );
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
                stage_code: None,
            },
        },
    )
    .expect("running state");
    let executor = NormalRunToolExecutor::new(
        &state,
        None,
        &accepted,
        &context,
        vec![CapabilityId::new("runtime.read")],
        super::run_contract::RunBudgetPolicy::for_envelope(&context.envelope),
        &sink,
        Vec::new(),
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
    let mut research_request = web_tool_loop_request();
    research_request.turn.message =
        "Investigate and compare multiple sources about synthetic evidence.".into();
    let accepted = RunIntake::start_with_sink(&state.db, research_request, &sink)
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
            "data: {\"choices\":[{\"delta\":{\"content\":\"联网证据已核实。[W1]\"}}]}\n\ndata: [DONE]\n\n",
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
    let capabilities = vec![CapabilityId::new("web.search")];
    let tools = ToolRegistry::new().tools_for_authorized_capabilities(&capabilities, true);
    assert!(tools.iter().any(|tool| tool.name == "web_search"));
    let provider_snapshot =
        super::mcp_runtime_registry::resolve_selected_web_search_provider(&state.db)
            .expect("freeze selected MCP provider before the model tool loop");
    let executor = NormalRunToolExecutor::new(
        &state,
        None,
        &accepted,
        &context,
        capabilities,
        super::run_contract::RunBudgetPolicy::for_envelope(&context.envelope),
        &sink,
        vec![provider_snapshot],
    );

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

#[tokio::test]
async fn execute_normal_run_uses_real_service_policy_route_executor_and_engine() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    install_headless_contract_mcp(&state);
    let llm = spawn_llm_protocol_double(vec![HttpResponseScript::sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"服务链路已核实。[W1]\"}}]}\n\ndata: [DONE]\n\n",
    )])
    .await
    .expect("local LLM boundary");
    let mut routing = LlmRoutingConfig::default();
    routing.providers.clear();
    routing.providers.insert(
        "custom".into(),
        ProviderOverride {
            base_url: Some(llm.base_url.clone()),
            enabled_models: Some(vec!["headless-contract-model".into()]),
            ..Default::default()
        },
    );
    routing.default_model = Some(ModelReference {
        provider_id: "custom".into(),
        model_id: "headless-contract-model".into(),
    });
    crate::llm::config::save(&state.db, &routing).expect("normal service route setup");
    state.set_test_streaming_client(reqwest::Client::new());

    let sink = RecordingSink::default();
    let accepted = RunIntake::start_with_sink(&state.db, web_tool_loop_request(), &sink)
        .expect("accepted web tool-loop run");
    tokio::time::timeout(
        Duration::from_secs(10),
        execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink),
    )
    .await
    .expect("strict service path must finish within its bounded evidence budget");

    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("run snapshot")
        .expect("completed run");
    assert_eq!(
        response.run.state,
        RunState::Completed,
        "terminal events: {:?}",
        response
            .events
            .iter()
            .map(AssistantRunEvent::payload)
            .collect::<Vec<_>>()
    );

    let calls = tokio::time::timeout(Duration::from_secs(2), llm.finish())
        .await
        .expect("strict service path must reach the one model turn")
        .expect("LLM double completion");
    assert_eq!(
        calls.len(),
        1,
        "strict Web service path uses one model turn"
    );
    let system_prompt = calls[0].body["messages"]
        .as_array()
        .expect("provider messages")
        .iter()
        .filter_map(|message| message["content"].as_str())
        .find(|content| content.contains("WebEvidenceData"))
        .expect("web evidence system prompt");
    assert!(
        system_prompt.contains("Keep source mechanics out of visible prose"),
        "uncalibrated routes must keep source-group mechanics out of visible prose"
    );
    assert!(
        !system_prompt.contains("CurrentRunVerifiedWebEvidence"),
        "model-facing evidence data must not expose the old lifecycle heading"
    );
    assert!(
        !system_prompt.contains("source-group disclosure"),
        "model-facing evidence data must not expose source-group protocol labels"
    );
    assert!(
        !system_prompt.contains("Cite its [Wn] labels"),
        "uncalibrated routes must not request model-authored precise citations"
    );
    let tool_names = calls[0].body["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| tool["function"]["name"].as_str())
        .collect::<Vec<_>>();
    assert!(
        tool_names.is_empty(),
        "a strict Web-only Run must not expose unrelated tools to the model: {tool_names:?}"
    );
    assert!(response
        .events
        .iter()
        .any(|event| matches!(event.payload(), RunEventPayload::EvidenceRegistered { .. })));
}

#[tokio::test]
async fn normal_service_executes_depth_one_child_run_on_the_real_provider_route() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"parent-spawn\",\"type\":\"function\",\"function\":{\"name\":\"spawn_subagent\",\"arguments\":\"{\\\"task\\\":\\\"读取当前时间\\\",\\\"allowed_tools\\\":[\\\"system_time_now\\\",\\\"memory_write\\\",\\\"spawn_subagent\\\"]}\"}}]}}]}\n\ndata: [DONE]\n\n",
        ),
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"child-time\",\"type\":\"function\",\"function\":{\"name\":\"system_time_now\",\"arguments\":\"{}\"}}]}}]}\n\ndata: [DONE]\n\n",
        ),
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"子任务已读取当前时间。\"}}]}\n\ndata: [DONE]\n\n",
        ),
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"父级已整合子任务结果。\"}}]}\n\ndata: [DONE]\n\n",
        ),
    ])
    .await
    .expect("local LLM boundary");
    let mut routing = LlmRoutingConfig::default();
    routing.providers.clear();
    routing.providers.insert(
        "custom".into(),
        ProviderOverride {
            base_url: Some(llm.base_url.clone()),
            enabled_models: Some(vec!["child-run-model".into()]),
            ..Default::default()
        },
    );
    routing.default_model = Some(ModelReference {
        provider_id: "custom".into(),
        model_id: "child-run-model".into(),
    });
    crate::llm::config::save(&state.db, &routing).expect("normal service route setup");
    state.set_test_streaming_client(reqwest::Client::new());
    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.client_request_id = "normal-service-child-run".into();
    request.turn.message = "请委派一个子任务读取当前时间后汇总。".into();
    let accepted =
        RunIntake::start_with_sink(&state.db, request, &sink).expect("accepted child-run request");

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;

    let calls = llm.finish().await.expect("LLM double completion");
    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("run snapshot")
        .expect("completed run");
    assert_eq!(
        calls.len(),
        4,
        "parent and child must each complete their loop"
    );
    let child_tools = calls[1].body["tools"]
        .as_array()
        .expect("child tool surface");
    let child_tool_names = child_tools
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str())
        .collect::<Vec<_>>();
    assert!(child_tool_names.contains(&"system_time_now"));
    assert!(!child_tool_names.contains(&"memory_write"));
    assert!(!child_tool_names.contains(&"spawn_subagent"));
    assert_eq!(response.run.state, RunState::Completed);
    let child_depth = state
        .db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT subagent_depth FROM tool_audit WHERE run_id = ?1 AND tool_name = 'system_time_now'",
                [&accepted.run_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(Into::into)
        })
        .expect("child tool audit");
    assert_eq!(child_depth, 1);
}

#[tokio::test]
async fn evaluation_headless_entry_observes_the_real_normal_service_direct_path() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let llm = spawn_llm_protocol_double(vec![HttpResponseScript::sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"真实无头链路答复\"}}]}\n\ndata: [DONE]\n\n",
    )])
    .await
    .expect("local LLM boundary");
    let mut routing = LlmRoutingConfig::default();
    routing.providers.clear();
    routing.providers.insert(
        "custom".into(),
        ProviderOverride {
            base_url: Some(llm.base_url.clone()),
            enabled_models: Some(vec!["headless-contract-model".into()]),
            ..Default::default()
        },
    );
    routing.default_model = Some(ModelReference {
        provider_id: "custom".into(),
        model_id: "headless-contract-model".into(),
    });
    crate::llm::config::save(&state.db, &routing).expect("normal service route setup");
    state.set_test_streaming_client(reqwest::Client::new());
    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.turn.message = "hello".into();
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink).expect("accepted run");
    let telemetry = EvaluationTelemetryTap::default();

    execute_normal_run_with_eval_telemetry(
        Arc::clone(&state),
        accepted.clone(),
        None,
        &sink,
        &telemetry,
    )
    .await;

    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("run snapshot")
        .expect("completed run");
    assert_eq!(
        response.run.state,
        RunState::Completed,
        "terminal payload: {:?}",
        response.events.last().map(|event| event.payload())
    );
    let calls = tokio::time::timeout(std::time::Duration::from_secs(2), llm.finish())
        .await
        .expect("LLM double must be called")
        .expect("LLM double completion");
    let snapshot = telemetry.snapshot();
    assert_eq!(calls.len(), 1);
    assert_eq!(snapshot.model_turns(), 1);
    assert_eq!(snapshot.final_output_successes(), 1);
}
