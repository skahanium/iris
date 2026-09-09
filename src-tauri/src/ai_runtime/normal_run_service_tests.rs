use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::agent_capacity_eval::{
    spawn_llm_protocol_double, EvaluationTelemetryTap, HttpResponseScript,
};
use super::agent_evidence_repository::AgentEvidenceRepository;
use super::agent_run_repository::{AgentRunRepository, AppendRunEventInput};
use super::agent_tool_loop::ToolLoopExecutor;
use super::conversation_memory::ConversationMemory;
use super::mcp_runtime_registry::{upsert_web_evidence_provider, WebEvidenceProviderInput};
use super::model_gateway::ModelGateway;
use super::normal_run_service::{
    build_cached_skill_activation, execute_normal_run, execute_normal_run_with_eval_telemetry,
};
use super::normal_session_repository::NormalSessionRepository;
use super::run_context::RunContextAssembler;
use super::run_contract::{
    AssistantRunAccepted, AssistantRunEvent, AssistantRunStartRequest, AssistantSessionRef,
    AssistantTurnDraft, CapabilityId, ContextMode, Effort, FreshFactDomain, FreshFactPolicy,
    RunEventPayload, RunEventType, RunPresentationPayload, RunState, SecurityDomain,
    WebDecisionReason,
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
use crate::storage::db::Database;

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

struct AnswerCompleteDurabilityProbe<'a> {
    db: &'a Database,
    session: AssistantSessionRef,
    run_id: String,
    observed_durable_state: Mutex<Option<bool>>,
}

impl RunEventSink for AnswerCompleteDurabilityProbe<'_> {
    fn emit(&self, _event: &AssistantRunEvent) -> AppResult<()> {
        Ok(())
    }

    fn emit_presentation(&self, run_id: &str, payload: RunPresentationPayload) -> AppResult<()> {
        if matches!(payload, RunPresentationPayload::AnswerComplete) {
            assert_eq!(run_id, self.run_id);
            let durable = RunIntake::get(self.db, &self.session, &self.run_id)
                .expect("probe Run snapshot")
                .is_some_and(|response| {
                    response.run.state == RunState::Completed
                        && response.run.final_message_id.is_some()
                });
            *self.observed_durable_state.lock().expect("probe lock") = Some(durable);
        }
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

/// Explicitly opted-in live regression. Reuses the existing read-only profile
/// discovery, single-use approval binding and isolated credential hydration.
/// This entry never runs as part of ordinary regression tests.
#[tokio::test]
#[ignore = "requires a fresh approved two-route preflight and paid live services"]
async fn timeliness_live_natural_questions_campaign() {
    use super::agent_capacity_eval::{
        approve_live_profile, discover_live_profile_candidates_from_database,
        prepare_approved_live_pilot, restore_and_consume_live_preflight_campaign,
        LiveCampaignRunCap, LiveCostConfirmation, LivePilotCallProbe,
    };
    assert_eq!(
        std::env::var("IRIS_AGENT_EVAL_COST_CONFIRMATION").as_deref(),
        Ok("two-route-12-run-campaign")
    );
    let source = std::path::PathBuf::from(
        std::env::var_os("IRIS_AGENT_EVAL_SOURCE_DB").expect("source required"),
    );
    let data =
        std::path::PathBuf::from(std::env::var_os("IRIS_DATA_DIR").expect("data root required"));
    let config = std::path::PathBuf::from(
        std::env::var_os("IRIS_CONFIG_DIR").expect("config root required"),
    );
    let session_id = std::env::var("IRIS_AGENT_EVAL_SESSION").expect("session required");
    let profiles = std::env::var("IRIS_AGENT_EVAL_APPROVED_PROFILE").expect("profiles required");
    let profiles = profiles.split(',').collect::<Vec<_>>();
    assert_eq!(profiles.len(), 2);
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace");
    let output = workspace.join("target/agent-eval");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    let candidates =
        discover_live_profile_candidates_from_database(&source).expect("read-only discovery");
    let mut sessions = restore_and_consume_live_preflight_campaign(
        &output.join(format!("live-{session_id}.json")),
        &session_id,
        [profiles[0], profiles[1]],
        candidates,
        now,
        &source,
        &data,
        &config,
    )
    .expect("current approved session");
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut prepared = Vec::new();
    for (session, profile) in sessions.iter_mut().zip(&profiles) {
        let approval =
            approve_live_profile(session, Some(profile), now).expect("approve selected route");
        prepared.push(
            prepare_approved_live_pilot(
                session,
                Some(approval.token()),
                Some(LiveCostConfirmation::TwoRouteCampaign),
                now,
                &LivePilotCallProbe::default(),
            )
            .expect("isolated route"),
        );
    }
    let cases = [
        ("近期有什么好看的电影正在热映或者即将上映吗?", true, false),
        ("近期有什么好看的电影正在热映或者即将上映吗?", true, false),
        (
            "我在上海，只看当前正在热映的电影，不要即将上映的。",
            true,
            true,
        ),
        ("那下个月即将上映的呢？", true, true),
        (
            "近期 NBA 有什么值得关注的动态，尤其是交易或转会方面？",
            true,
            false,
        ),
        ("近期有什么好看的电影正在热映或者即将上映吗?", false, false),
    ];
    let mut report = Vec::new();
    let mut models = 0_u32;
    let mut network = 0_u32;
    let focus = std::env::var("IRIS_AGENT_EVAL_TIMELINESS_FOCUS").unwrap_or_default();
    assert!(matches!(focus.as_str(), "" | "boundaries" | "continuity"));
    for (route, prepared) in prepared.iter().enumerate() {
        let state = prepared.state();
        let mut previous = None;
        for (case, (question, web_enabled, follow_up)) in cases.iter().enumerate() {
            let boundary_case = case == 0 || case == 5;
            if (focus == "boundaries" && !boundary_case) || (focus == "continuity" && boundary_case)
            {
                continue;
            }
            assert!(models < 96, "campaign model budget exhausted");
            let mut request = direct_request();
            request.client_request_id = format!("timeliness-live-{route}-{case}");
            request.turn.message = (*question).into();
            request.web_enabled = *web_enabled;
            request.session = if *follow_up { previous.clone() } else { None };
            let sink = RecordingSink::default();
            let accepted =
                RunIntake::start_with_sink(&state.db, request, &sink).expect("production intake");
            previous = Some(accepted.session.clone());
            let telemetry = EvaluationTelemetryTap::default();
            let started = std::time::Instant::now();
            super::normal_run_service::execute_normal_run_with_eval_telemetry_cap(
                Arc::clone(state),
                accepted.clone(),
                None,
                &sink,
                &telemetry,
                LiveCampaignRunCap {
                    max_model_turns: (96 - models).min(8),
                    max_tool_calls: 24,
                    max_network_tool_calls: (72 - network).min(6),
                },
            )
            .await;
            let elapsed = started.elapsed().as_millis();
            let usage = telemetry.snapshot();
            network += usage.web_tool_calls();
            let run = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
                .expect("snapshot")
                .expect("run");
            let messages = NormalSessionRepository::load_messages(
                &state.db,
                &accepted.session.session_key,
                20,
            )
            .expect("messages");
            let final_message = messages.iter().rev().find(|message| {
                message.role == "assistant"
                    && message.run_id.as_deref() == Some(accepted.run_id.as_str())
            });
            let answer = final_message
                .map(|message| message.content.as_str())
                .unwrap_or_default();
            let links = AgentEvidenceRepository::list_current_run_web_citation_links(
                &state.db,
                &accepted.run_id,
            )
            .expect("sources");
            let diagnostic = state
                .db
                .with_read_conn(|conn| {
                    let json: String = conn.query_row(
                        "SELECT provider_route_summary_json FROM agent_runs WHERE run_id = ?1",
                        [&accepted.run_id],
                        |row| row.get(0),
                    )?;
                    Ok(serde_json::from_str::<serde_json::Value>(&json)?["toolLoop"].clone())
                })
                .expect("safe diagnostic");
            let model_turns = diagnostic["modelTurns"]
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(usage.model_turns())
                .max(usage.model_turns());
            models += model_turns;
            let excerpts = state.db.with_read_conn(|conn| {
                let mut statement = conn.prepare("SELECT e.url, e.retrieved_at, substr(e.bounded_excerpt, 1, 2000) FROM agent_run_evidence r JOIN session_evidence e ON e.id = r.evidence_id WHERE r.run_id = ?1 AND e.source_type = 'web'")?;
                let rows = statement.query_map([&accepted.run_id], |row| Ok(serde_json::json!({
                    "url":row.get::<_, String>(0)?, "retrievedAt":row.get::<_, Option<String>>(1)?, "excerpt":row.get::<_, Option<String>>(2)?,
                })))?.collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            }).expect("bounded public evidence");
            report.push(serde_json::json!({ "route":route + 1, "case":case + 1, "prompt":question,
                "webEnabled":web_enabled, "state":run.run.state, "answer":answer,
                "sources":links.iter().map(|link| serde_json::json!({"label":link.label,"title":link.title,"url":link.url})).collect::<Vec<_>>(),
                "selectedSources":final_message.map(|message| &message.web_citations),
                "citationBinding":final_message.and_then(|message| message.citation_binding.as_ref()),
                "elapsedMs":elapsed, "modelTurns":model_turns, "completedModelTurns":usage.model_turns(), "webToolCalls":usage.web_tool_calls(), "excerpts":excerpts,
                "tokens":(usage.total_tokens() > 0).then_some(usage.total_tokens()), "diagnostic":diagnostic, "quality":"pending_review",
                "failureCodes":run.events.iter().filter_map(|event| match event.payload() { RunEventPayload::Failed { code, .. } => Some(code.as_str()), _ => None }).collect::<Vec<_>>() }));
            // Only the fixed public prompts, visible answers, public source metadata
            // and safe numeric diagnostics are persisted; never raw model packets.
            std::fs::write(
                output.join(format!("timeliness-{session_id}.json")),
                serde_json::to_vec_pretty(&report).expect("serialize report"),
            )
            .expect("write report");
            println!(
                "timeliness_live route={} case={} state={:?} sources={} elapsed_ms={elapsed}",
                route + 1,
                case + 1,
                run.run.state,
                links.len()
            );
            if !web_enabled {
                assert_eq!(usage.web_tool_calls(), 0, "offline must never dispatch Web");
            }
            assert!(models <= 96 && network <= 72, "campaign cap");
        }
    }
    assert_eq!(
        report.len(),
        match focus.as_str() {
            "boundaries" => 4,
            "continuity" => 8,
            _ => 12,
        }
    );
}

#[test]
fn timeliness_observation_intake_does_not_confuse_runtime_or_supplied_text_with_external_facts() {
    for (message, reason) in [
        (
            "近期有什么好看的电影正在热映或者即将上映吗?",
            WebDecisionReason::VolatileExternalFact,
        ),
        (
            "今天几号，近期有什么正在热映的电影？",
            WebDecisionReason::VolatileExternalFact,
        ),
        (
            "What is today's date and the current weather in Tokyo?",
            WebDecisionReason::VolatileExternalFact,
        ),
        (
            "即将上映的影片有哪些？",
            WebDecisionReason::VolatileExternalFact,
        ),
        ("法国总统是谁？", WebDecisionReason::VolatileExternalFact),
        (
            "为什么你不核实当前版本是否发布？",
            WebDecisionReason::VolatileExternalFact,
        ),
        (
            "翻译这句话：近期电影即将上映。",
            WebDecisionReason::DefaultOnline,
        ),
        (
            "总结提供的材料：今天的新闻和股价。",
            WebDecisionReason::DefaultOnline,
        ),
        ("今天几号？", WebDecisionReason::TrustedRuntimeFact),
    ] {
        let mut request = direct_request();
        request.web_enabled = true;
        request.turn.message = message.into();
        assert_eq!(
            RunIntake::resolve_envelope(&request)
                .expect("envelope")
                .web_reason,
            reason,
            "{message}"
        );
    }
}

#[tokio::test]
async fn timeliness_observation_original_movie_question_executes_before_a_model_can_skip_search() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    install_headless_contract_mcp_with_mode(&state, "search-fetch");
    let llm = spawn_llm_protocol_double(vec![HttpResponseScript::sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"根据公开资料整理影片范围。[W1]\"}}]}\n\ndata: [DONE]\n\n",
    )]).await.expect("model boundary");
    install_test_routing(
        &state,
        &llm.base_url,
        "iris-test-verified-tools-implicit-movie",
    );
    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.web_enabled = true;
    request.turn.message = "近期有什么好看的电影正在热映或者即将上映吗?".into();
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink).expect("accept");
    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;
    let calls = llm.finish().await.expect("model completed");
    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("snapshot")
        .expect("run");
    assert_eq!(response.run.state, RunState::Completed);
    let executed = response
        .events
        .iter()
        .filter_map(|event| match event.payload() {
            RunEventPayload::ToolStarted { capability, .. } => Some(capability.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(executed, ["web_search", "web_fetch"], "the original question must cause real search and fetch even when the model only returns text");
    assert!(
        !AgentEvidenceRepository::list_current_run_registered(&state.db, &accepted.run_id)
            .expect("evidence")
            .is_empty()
    );
    let search_health = super::mcp_runtime_registry::web_evidence_provider_health(
        &state.db,
        "headless-contract-mcp",
        "web.search",
    )
    .expect("search health")
    .expect("one real search");
    assert_eq!(
        search_health.success_count, 1,
        "the fetch must not hide another search"
    );
    let first_messages = calls[0].body["messages"].as_array().expect("messages");
    assert!(
        first_messages.iter().any(|message| message["content"]
            .as_str()
            .is_some_and(|content| content.contains("mainland China"))),
        "the production prompt must declare the geographic default"
    );
    let bootstrap = first_messages
        .iter()
        .filter_map(|message| message["content"].as_str())
        .filter_map(|content| serde_json::from_str::<serde_json::Value>(content).ok())
        .find(|value| value["kind"] == "host_web_bootstrap")
        .expect("Host observation");
    assert!(
        bootstrap["query"]
            .as_str()
            .is_some_and(|query| query.contains("中国大陆")),
        "the first actual search must carry the default scope, not only the prose prompt"
    );
    assert!(first_messages.iter().any(|message| message["content"]
        .as_str()
        .is_some_and(|content| content.contains("host_web_bootstrap"))));
    assert!(
        !first_messages
            .iter()
            .any(|message| message["role"] == "tool"),
        "Host observations must not impersonate model calls"
    );
}

#[tokio::test]
async fn timeliness_observation_explicit_url_fetches_without_an_unnecessary_search() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    install_headless_contract_mcp_with_mode(&state, "search-fetch");
    let llm = spawn_llm_protocol_double(vec![HttpResponseScript::sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"指定页面的正文说明了测试状态。[W1]\"}}]}\n\ndata: [DONE]\n\n",
    )]).await.expect("model boundary");
    install_test_routing(
        &state,
        &llm.base_url,
        "iris-test-verified-tools-explicit-url",
    );
    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.web_enabled = true;
    request.turn.message = "请阅读[这个页面](https://source.invalid/contract)，并概述内容。".into();
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink).expect("accept");
    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;
    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("snapshot")
        .expect("run");
    assert_eq!(response.run.state, RunState::Completed);
    let executed = response
        .events
        .iter()
        .filter_map(|event| match event.payload() {
            RunEventPayload::ToolStarted { capability, .. } => Some(capability.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(executed, ["web_fetch"]);
    assert!(
        super::mcp_runtime_registry::web_evidence_provider_health(
            &state.db,
            "headless-contract-mcp",
            "web.search"
        )
        .expect("search health")
        .is_none(),
        "web_fetch must not dispatch a hidden search inside the Broker"
    );
    assert_eq!(llm.finish().await.expect("model completed").len(), 1);
}

#[tokio::test]
async fn timeliness_fetches_a_public_https_url_without_prior_discovery() {
    let directory = tempfile::tempdir().expect("temp");
    let state = AppState::new(directory.path().join("data")).expect("state");
    install_headless_contract_mcp_with_mode(&state, "search-fetch");
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse("data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"fetch-unseen\",\"type\":\"function\",\"function\":{\"name\":\"web_fetch\",\"arguments\":\"{\\\"urls\\\":[\\\"https://unseen.invalid/page\\\"]}\"}}]}}]}\n\ndata: [DONE]\n\n"),
        HttpResponseScript::sse("data: {\"choices\":[{\"delta\":{\"content\":\"依据已经读取的资料，测试状态可以确认。\"}}]}\n\ndata: [DONE]\n\n"),
    ]).await.expect("model");
    install_test_routing(&state, &llm.base_url, "iris-test-verified-tools-url-repair");
    let sink = RecordingSink::default();
    let accepted =
        RunIntake::start_with_sink(&state.db, web_tool_loop_request(), &sink).expect("accept");
    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;
    let run = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("snapshot")
        .expect("run");
    assert_eq!(
        run.run.state,
        RunState::Completed,
        "a public URL is dispatched through the safe fetch boundary"
    );
    let calls = llm.finish().await.expect("two model turns");
    assert!(!calls[1]
        .body
        .to_string()
        .contains("web_url_not_public_https"));
    assert_eq!(
        run.events
            .iter()
            .filter(|event| matches!(event.payload(), RunEventPayload::ToolStarted { .. }))
            .count(),
        3
    );
    let messages =
        NormalSessionRepository::load_messages(&state.db, &accepted.session.session_key, 10)
            .expect("messages");
    assert!(messages
        .last()
        .expect("final answer")
        .content
        .contains("测试状态可以确认"));
}

#[tokio::test]
async fn timeliness_later_fetch_uses_run_labels_instead_of_restarting_at_w1() {
    let directory = tempfile::tempdir().expect("temp");
    let state = AppState::new(directory.path().join("data")).expect("state");
    install_headless_contract_mcp_with_mode(&state, "search-fetch");
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(&tool_call_sse("web_fetch", serde_json::json!({"urls":["https://source.invalid/b"]}))),
        HttpResponseScript::sse("data: {\"choices\":[{\"delta\":{\"content\":\"第二份来源的正文支持测试状态。[W2]\"}}]}\n\ndata: [DONE]\n\n"),
    ]).await.expect("model");
    install_test_routing(
        &state,
        &llm.base_url,
        "iris-test-verified-tools-citation-continuity",
    );
    let sink = RecordingSink::default();
    let mut request = web_tool_loop_request();
    request.turn.message = "比较 https://source.invalid/a 和 https://source.invalid/b".into();
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink).expect("accept");
    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;
    let calls = llm.finish().await.expect("model turns");
    let observed = calls[1].body["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .rev()
        .find(|message| message["role"] == "tool")
        .expect("fetch observation");
    let value: serde_json::Value =
        serde_json::from_str(observed["content"].as_str().expect("content")).expect("JSON");
    assert_eq!(
        value["output"]["results"][0]["citation_label"], "[W2]",
        "the second source must keep its Run-local label when fetched alone"
    );
}

#[tokio::test]
async fn timeliness_follow_up_maps_run_citations_and_persists_only_selected_sources() {
    let directory = tempfile::tempdir().expect("temp");
    let state = AppState::new(directory.path().join("data")).expect("state");
    install_headless_contract_mcp_with_mode(&state, "search-fetch");
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse("data: {\"choices\":[{\"delta\":{\"content\":\"上一条资料。[W1]\"}}]}\n\ndata: [DONE]\n\n"),
        HttpResponseScript::sse("data: {\"choices\":[{\"delta\":{\"content\":\"第二份当前资料支持这个答复。[W2]\"}}]}\n\ndata: [DONE]\n\n"),
    ]).await.expect("model");
    install_test_routing(
        &state,
        &llm.base_url,
        "iris-test-verified-tools-follow-up-citations",
    );
    let sink = RecordingSink::default();
    let mut first = web_tool_loop_request();
    first.turn.message = "读取 https://source.invalid/old".into();
    let previous = RunIntake::start_with_sink(&state.db, first, &sink).expect("first run");
    execute_normal_run(Arc::clone(&state), previous.clone(), None, None, &sink).await;
    let before =
        NormalSessionRepository::load_messages(&state.db, &previous.session.session_key, 10)
            .expect("history");
    let previous_answer = before.last().expect("first answer").content.clone();

    let mut next = web_tool_loop_request();
    next.client_request_id = "follow-up-run-local-citations".into();
    next.session = Some(previous.session.clone());
    next.turn.message = "近期有什么好看的电影正在热映或者即将上映吗?".into();
    let accepted = RunIntake::start_with_sink(&state.db, next, &sink).expect("follow-up");
    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;
    let sources =
        AgentEvidenceRepository::list_current_run_web_citation_links(&state.db, &accepted.run_id)
            .expect("sources");
    assert_eq!(sources.len(), 2);
    let messages =
        NormalSessionRepository::load_messages(&state.db, &accepted.session.session_key, 10)
            .expect("reloaded messages");
    let answer = messages.last().expect("answer");
    assert!(
        answer.content.contains(&format!("]({})", sources[1].url)),
        "current W2 must resolve independently of old session citations: {}",
        answer.content
    );
    assert!(!answer.content.contains(&sources[0].url));
    assert_eq!(
        answer.web_citations.len(),
        1,
        "unselected fetched bodies are not final sources"
    );
    assert_eq!(answer.web_citations[0].url, sources[1].url);
    assert_eq!(answer.web_citations[0].index, 2);
    assert_eq!(
        answer
            .evidence_refs
            .as_ref()
            .expect("modern selection")
            .len(),
        1
    );
    assert_eq!(
        messages
            .iter()
            .find(|message| message.role == "assistant"
                && message.run_id.as_deref() == Some(previous.run_id.as_str()))
            .expect("old answer")
            .content,
        previous_answer
    );
    assert_eq!(llm.finish().await.expect("two model answers").len(), 2);
}

#[tokio::test]
async fn timeliness_native_tool_argument_fragments_reach_one_real_dispatch() {
    let directory = tempfile::tempdir().expect("temp");
    let state = AppState::new(directory.path().join("data")).expect("state");
    install_headless_contract_mcp_with_mode(&state, "search-fetch");
    let chunks = [
        serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"fragmented-search","type":"function","function":{"name":"web_search","arguments":"{\"query\":\""}}]}}]}),
        serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"different current source\"}"}}]}}]}),
    ];
    let stream = format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        chunks[0], chunks[1]
    );
    let llm = spawn_llm_protocol_double(vec![HttpResponseScript::sse(&stream), HttpResponseScript::sse("data: {\"choices\":[{\"delta\":{\"content\":\"资料中的测试状态已经确认。[W1]\"}}]}\n\ndata: [DONE]\n\n")]).await.expect("model");
    install_test_routing(
        &state,
        &llm.base_url,
        "iris-test-verified-tools-native-fragments",
    );
    let sink = RecordingSink::default();
    let accepted =
        RunIntake::start_with_sink(&state.db, web_tool_loop_request(), &sink).expect("accept");
    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;
    let calls = llm.finish().await.expect("model calls");
    assert_eq!(calls.len(), 2);
    let run = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("get")
        .expect("run");
    assert_eq!(run.run.state, RunState::Completed);
    assert_eq!(
        run.events
            .iter()
            .filter(|event| matches!(event.payload(), RunEventPayload::ToolStarted { .. }))
            .count(),
        3,
        "Host search/fetch plus one reassembled model search"
    );
}

fn web_tool_loop_request() -> AssistantRunStartRequest {
    let mut request = direct_request();
    request.client_request_id = "headless-normal-web-tool-loop".into();
    request.turn.message = "请联网核实 synthetic 的最新状态".into();
    request.web_enabled = true;
    request
}

#[tokio::test]
async fn timeliness_offline_content_tool_protocol_retries_without_external_dispatch() {
    let directory = tempfile::tempdir().expect("temp");
    let state = AppState::new(directory.path().join("data")).expect("state");
    let raw = serde_json::json!({"choices":[{"delta":{"content":"我来帮你查一下最近的电影资讯。]<]minimax[>[<tool_call>\n{\"name\":\"web_search\",\"parameters\":{\"query\":\"current movies\"}}\n</tool_call>"}}]});
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(&format!("data: {raw}\n\ndata: [DONE]\n\n")),
        HttpResponseScript::sse("data: {\"choices\":[{\"delta\":{\"content\":\"联网已关闭，无法确认当前上映情况。\"}}]}\n\ndata: [DONE]\n\n"),
    ]).await.expect("model");
    install_test_routing(
        &state,
        &llm.base_url,
        "iris-test-verified-tools-offline-protocol",
    );
    let mut request = direct_request();
    request.turn.message = "近期有什么好看的电影正在热映或者即将上映吗?".into();
    let sink = RecordingSink::default();
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink).expect("accept");
    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;
    let messages =
        NormalSessionRepository::load_messages(&state.db, &accepted.session.session_key, 10)
            .expect("messages");
    assert_eq!(
        messages.last().expect("answer").content,
        "联网已关闭，无法确认当前上映情况。"
    );
    assert_eq!(llm.finish().await.expect("bounded provider retry").len(), 2);
    let run = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("get")
        .expect("run");
    assert!(!run
        .events
        .iter()
        .any(|event| matches!(event.payload(), RunEventPayload::ToolStarted { .. })));
}

#[tokio::test]
async fn timeliness_repair_rejected_proposals_are_not_searches_and_keep_safe_diagnostics() {
    let directory = tempfile::tempdir().expect("temp");
    let state = AppState::new(directory.path().join("data")).expect("state");
    install_headless_contract_mcp_with_mode(&state, "search-fetch");
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(&tool_call_sse_with_id("bad-args", "web_search", serde_json::json!({"query":42}))),
        HttpResponseScript::sse(&tool_call_sse_with_id("unknown", "private-sentinel-tool", serde_json::json!({}))),
        HttpResponseScript::sse("data: {\"choices\":[{\"delta\":{\"content\":\"已有资料说明了影片状态。[W1]\"}}]}\n\ndata: [DONE]\n\n"),
    ]).await.expect("model");
    install_test_routing(&state, &llm.base_url, "iris-test-verified-tools-repair");
    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.web_enabled = true;
    request.turn.message = "近期有什么好看的电影正在热映或者即将上映吗?".into();
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink).expect("accept");
    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;
    let calls = llm.finish().await.expect("model calls");
    let run = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("get")
        .expect("run");
    assert_eq!(run.run.state, RunState::Completed);
    assert_eq!(
        run.events
            .iter()
            .filter(|event| matches!(event.payload(), RunEventPayload::ToolStarted { .. }))
            .count(),
        2,
        "only Host search/fetch actually ran"
    );
    assert!(calls[1]
        .body
        .to_string()
        .contains("arguments_schema_mismatch"));
    let diagnostic: serde_json::Value = state
        .db
        .with_read_conn(|conn| {
            let value: String = conn.query_row(
                "SELECT provider_route_summary_json FROM agent_runs WHERE run_id = ?1",
                [&accepted.run_id],
                |row| row.get(0),
            )?;
            Ok(serde_json::from_str(&value)?)
        })
        .expect("diagnostic");
    let loop_data = &diagnostic["toolLoop"];
    assert_eq!(loop_data["rejectedProposals"], 2);
    assert_eq!(loop_data["repairRounds"], 2);
    assert_eq!(loop_data["dispatched"], 2);
    assert_eq!(loop_data["noProgressRounds"], 0);
    assert_eq!(
        loop_data["firstRejection"]["reason"],
        "arguments_schema_mismatch"
    );
    assert!(!loop_data.to_string().contains("private-sentinel-tool"));
    assert!(!loop_data.to_string().contains("近期"));
}

#[tokio::test]
async fn timeliness_failed_bootstrap_keeps_one_tool_enabled_research_opportunity() {
    let directory = tempfile::tempdir().expect("temp");
    let state = AppState::new(directory.path().join("data")).expect("state");
    // No configured Web service: an actual broker attempt must be distinct
    // from a rejected model proposal, and must not skip the repair turn.
    let answer = "data: {\"choices\":[{\"delta\":{\"content\":\"暂时没有可用资料。\"}}]}\n\ndata: [DONE]\n\n";
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(answer),
        HttpResponseScript::sse(answer),
    ])
    .await
    .expect("model");
    install_test_routing(
        &state,
        &llm.base_url,
        "iris-test-verified-tools-research-recovery",
    );
    let sink = RecordingSink::default();
    let request = web_tool_loop_request();
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink).expect("accept");
    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;
    let calls = tokio::time::timeout(Duration::from_secs(3), llm.finish())
        .await
        .expect("both model turns must run")
        .expect("calls");
    assert_eq!(calls.len(), 2);
    assert!(calls[1].body["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .any(|tool| tool["function"]["name"] == "web_search"));
    let run = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("get")
        .expect("run");
    assert_eq!(run.run.state, RunState::Completed);
    let messages =
        NormalSessionRepository::load_messages(&state.db, &accepted.session.session_key, 10)
            .expect("messages");
    let final_text = &messages.last().expect("answer").content;
    assert!(!final_text.contains("用药") && !final_text.contains("签证"));
    let diagnostic: serde_json::Value = state
        .db
        .with_read_conn(|conn| {
            let json: String = conn.query_row(
                "SELECT provider_route_summary_json FROM agent_runs WHERE run_id = ?1",
                [&accepted.run_id],
                |row| row.get(0),
            )?;
            Ok(serde_json::from_str(&json)?)
        })
        .expect("diagnostic");
    assert_eq!(
        diagnostic["toolLoop"]["exit"]["observationPerformed"], false,
        "no configured service is a capability block, not a performed search"
    );
}

fn direct_required_web_request() -> AssistantRunStartRequest {
    let mut request = direct_request();
    request.client_request_id = "headless-normal-web-direct-required".into();
    request.turn.message =
        "Please search online and verify when the first iPhone was announced.".into();
    request.web_enabled = true;
    request
}

fn install_headless_contract_mcp(state: &AppState) {
    install_headless_contract_mcp_with_mode(state, "search-only");
}

fn install_headless_contract_mcp_with_mode(state: &AppState, mode: &str) {
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
                mode.to_string(),
                "2".to_string(),
            ],
        )
    } else {
        let fixture = format!(
            "{}/tests/fixtures/agent-capacity-mcp-stdio.sh",
            env!("CARGO_MANIFEST_DIR")
        );
        ("/bin/sh", vec![fixture, mode.to_string(), "2".to_string()])
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
            web_fetch_mapping_json: matches!(mode, "search-fetch" | "fetch-rate-limit")
                .then(|| r#"{"tool":"fetch","urlArg":"url"}"#.into()),
        },
    )
    .expect("headless MCP registry setup");
}

fn install_test_routing(state: &AppState, base_url: &str, model_name: &str) {
    let mut routing = LlmRoutingConfig::default();
    routing.providers.clear();
    routing.providers.insert(
        "custom".into(),
        ProviderOverride {
            base_url: Some(base_url.to_string()),
            enabled_models: Some(vec![model_name.to_string()]),
            ..Default::default()
        },
    );
    routing.default_model = Some(ModelReference {
        provider_id: "custom".into(),
        model_id: model_name.to_string(),
    });
    crate::llm::config::save(&state.db, &routing).expect("normal service route setup");
    state.set_test_streaming_client(reqwest::Client::new());
}

fn tool_call_sse(tool_name: &str, arguments: serde_json::Value) -> String {
    tool_call_sse_with_id("domain-operation-call", tool_name, arguments)
}

fn tool_call_sse_with_id(
    tool_call_id: &str,
    tool_name: &str,
    arguments: serde_json::Value,
) -> String {
    let arguments = serde_json::to_string(&arguments).expect("serialize tool arguments");
    let payload = serde_json::json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "id": tool_call_id,
                    "type": "function",
                    "function": { "name": tool_name, "arguments": arguments }
                }]
            }
        }]
    });
    format!("data: {payload}\n\ndata: [DONE]\n\n")
}

#[allow(clippy::type_complexity)]
fn start_headless_tool_loop(
    state: &AppState,
    request: AssistantRunStartRequest,
) -> (
    RecordingSink,
    AssistantRunAccepted,
    crate::ai_runtime::run_context::RunContext,
    crate::ai_runtime::context_materials::ContextMaterialPlan,
    Vec<i64>,
) {
    let sink = RecordingSink::default();
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink)
        .expect("accepted headless tool-loop run");
    let context = RunContextAssembler::assemble(
        &state.db,
        None,
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .expect("run context");
    let material_plan = context.context_material_plan();
    let initial_evidence =
        RunContextAssembler::register_evidence(&state.db, &accepted.run_id, &context)
            .expect("initial evidence registration");
    (sink, accepted, context, material_plan, initial_evidence)
}

#[tokio::test]
async fn headless_normal_direct_run_preserves_terminal_and_content_lifecycle() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.turn.message = "请核实当前这项法规是否已经生效".into();
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink).expect("accepted run");

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
    assert_eq!(messages[0].content, "请核实当前这项法规是否已经生效");
}

#[tokio::test]
async fn direct_streaming_does_not_emit_answer_complete_before_durable_finalization() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let llm = spawn_llm_protocol_double(vec![HttpResponseScript::sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"普通直答\"}}]}\n\ndata: [DONE]\n\n",
    )])
    .await
    .expect("local LLM boundary");
    let mut routing = LlmRoutingConfig::default();
    routing.providers.clear();
    routing.providers.insert(
        "custom".into(),
        ProviderOverride {
            base_url: Some(llm.base_url.clone()),
            enabled_models: Some(vec!["headless-direct-model".into()]),
            ..Default::default()
        },
    );
    routing.default_model = Some(ModelReference {
        provider_id: "custom".into(),
        model_id: "headless-direct-model".into(),
    });
    crate::llm::config::save(&state.db, &routing).expect("normal service route setup");
    state.set_test_streaming_client(reqwest::Client::new());

    let mut request = direct_request();
    request.turn.message = "hello".into();
    let accepted = RunIntake::start(&state.db, request).expect("accepted run");
    let probe = AnswerCompleteDurabilityProbe {
        db: &state.db,
        session: accepted.session.clone(),
        run_id: accepted.run_id.clone(),
        observed_durable_state: Mutex::new(None),
    };
    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &probe).await;

    assert_eq!(
        *probe
            .observed_durable_state
            .lock()
            .expect("probe lock"),
        Some(true),
        "AnswerComplete must be emitted only after the assistant message and Completed state are durable"
    );
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
    )
    .with_allowed_tool_names(&["system_time_now".to_string()]);

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
    install_headless_contract_mcp_with_mode(&state, "search-fetch");
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
    let material_plan = context.context_material_plan();
    let initial_evidence =
        RunContextAssembler::register_evidence(&state.db, &accepted.run_id, &context)
            .expect("initial evidence registration");
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"headless-web-call\",\"type\":\"function\",\"function\":{\"name\":\"web_search\",\"arguments\":\"{\\\"query\\\":\\\"synthetic\\\"}\"}}]}}]}\n\ndata: [DONE]\n\n",
        ),
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"headless-fetch-call\",\"type\":\"function\",\"function\":{\"name\":\"web_fetch\",\"arguments\":\"{\\\"urls\\\":[\\\"https://source.invalid/contract\\\"]}\"}}]}}]}\n\ndata: [DONE]\n\n",
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
    assert!(tools.iter().any(|tool| tool.name == "web_fetch"));
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
    )
    .with_allowed_tool_names(
        &tools
            .iter()
            .map(|tool| tool.name.clone())
            .collect::<Vec<_>>(),
    );

    RunEngine::execute_tool_loop_with_sink(
        &state.db,
        &accepted.session,
        &accepted.run_id,
        context.messages_with_context_material_plan(&material_plan),
        tools,
        &initial_evidence,
        Some(&material_plan),
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
    let (web_evidence_count, extraction_method, bounded_excerpt) = state
        .db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*), MIN(extraction_method), MIN(bounded_excerpt)
                 FROM session_evidence
                 WHERE origin_run_id = ?1 AND source_type = 'web'",
                [&accepted.run_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(Into::into)
        })
        .expect("evidence ledger query");
    let last_tool_observation = |call_index: usize| {
        let content = calls[call_index].body["messages"]
            .as_array()
            .expect("model messages")
            .iter()
            .rev()
            .find(|message| message["role"] == "tool")
            .and_then(|message| message["content"].as_str())
            .expect("tool observation");
        serde_json::from_str::<serde_json::Value>(content).expect("tool observation JSON")
    };
    let discovery = last_tool_observation(1);
    let fetched = last_tool_observation(2);

    assert_eq!(calls.len(), 3, "LLM must search, fetch, and synthesize");
    assert_eq!(response.run.state, RunState::Completed);
    assert_eq!(discovery["output"]["evidenceIds"], serde_json::json!([]));
    assert_eq!(discovery["output"]["requiresFetchForCitation"], true);
    assert_eq!(discovery["output"]["observationDepth"], "search_snippet");
    assert!(
        fetched["output"]["evidenceIds"]
            .as_array()
            .is_some_and(|ids| ids.len() == 1),
        "only the selected fetched body becomes evidence"
    );
    assert_eq!(fetched["output"]["requiresFetchForCitation"], false);
    assert_eq!(fetched["output"]["observationDepth"], "fetched_body");
    assert_eq!(web_evidence_count, 1);
    assert_eq!(extraction_method, "mcp_fetch_raw_content");
    assert!(bounded_excerpt.contains("fetch-result"));
    assert!(!bounded_excerpt.contains("snippet: deterministic"));
    assert!(response
        .events
        .iter()
        .any(|event| matches!(event.payload(), RunEventPayload::EvidenceRegistered { .. })));
}

#[tokio::test]
async fn production_runtime_time_uses_frozen_surface_and_recovers() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let mut request = direct_request();
    request.client_request_id = "production-runtime-time".into();
    request.turn.message = "请调研当前时间，并使用 system_time_now 工具确认后汇总。".into();
    request.web_enabled = false;
    let (sink, accepted, context, material_plan, initial_evidence) =
        start_headless_tool_loop(&state, request);
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"runtime-time-call\",\"type\":\"function\",\"function\":{\"name\":\"system_time_now\",\"arguments\":\"{}\"}}]}}]}\n\ndata: [DONE]\n\n",
        ),
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"当前时间是 2026-08-18 08:00:00。\"}}]}\n\ndata: [DONE]\n\n",
        ),
    ])
    .await
    .expect("local LLM boundary");
    let gateway = ModelGateway::new(reqwest::Client::new(), Vec::new());
    let provider = ModelGatewayStreamingDirectAnswerProvider::new(
        &gateway,
        ProviderConfig {
            name: "headless-runtime-model".into(),
            base_url: llm.base_url.clone(),
            api_key: None,
            model: "runtime-model".into(),
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
        },
        256,
    )
    .expect("model gateway provider");
    let capabilities = vec![CapabilityId::new("runtime.read")];
    let tools = ToolRegistry::new().tools_for_authorized_capabilities(&capabilities, true);
    assert!(tools.iter().any(|tool| tool.name == "system_time_now"));
    let executor = NormalRunToolExecutor::new(
        &state,
        None,
        &accepted,
        &context,
        capabilities,
        super::run_contract::RunBudgetPolicy::for_envelope(&context.envelope),
        &sink,
        Vec::new(),
    )
    .with_allowed_tool_names(
        &tools
            .iter()
            .map(|tool| tool.name.clone())
            .collect::<Vec<_>>(),
    );

    RunEngine::execute_tool_loop_with_sink(
        &state.db,
        &accepted.session,
        &accepted.run_id,
        context.messages_with_context_material_plan(&material_plan),
        tools,
        &initial_evidence,
        Some(&material_plan),
        &provider,
        &executor,
        &sink,
    )
    .await
    .expect("production runtime tool-loop chain");

    let calls = llm.finish().await.expect("LLM double completion");
    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("run snapshot")
        .expect("completed run");
    assert_eq!(
        calls.len(),
        2,
        "runtime tool call must complete a real continuation"
    );
    assert_eq!(response.run.state, RunState::Completed);
    let runtime_tool_audit = state
        .db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM tool_audit WHERE run_id = ?1 AND tool_name = 'system_time_now'",
                [&accepted.run_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(Into::into)
        })
        .expect("tool audit query");
    assert_eq!(runtime_tool_audit, 1, "system_time_now must be dispatched");
}

#[tokio::test]
async fn production_location_like_request_does_not_pause_a_new_run_for_structured_input() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let llm = spawn_llm_protocol_double(vec![HttpResponseScript::sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"无法获取天气。\"}}]}\n\ndata: [DONE]\n\n",
    )])
    .await
    .expect("local LLM boundary");
    install_test_routing(&state, &llm.base_url, "headless-contract-model");
    install_headless_contract_mcp(&state);

    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.client_request_id = "production-missing-city-input".into();
    request.turn.message = "今天天气怎么样？".into();
    request.web_enabled = true;
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink)
        .expect("accepted weather run without city");

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;
    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("run snapshot")
        .expect("completed run");
    assert!(
        response.run.state.is_terminal(),
        "new Run must terminate rather than reserve a city-specific structured input"
    );
    assert!(response.run.pending_input.is_none());
}

#[tokio::test]
async fn ordinary_missing_context_does_not_reserve_a_structured_input_run() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let llm = spawn_llm_protocol_double(vec![HttpResponseScript::sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"不应在补充信息前调用模型。\"}}]}\n\ndata: [DONE]\n\n",
    )])
    .await
    .expect("local LLM boundary");
    install_test_routing(&state, &llm.base_url, "hr1-ordinary-missing-context-model");
    install_headless_contract_mcp(&state);
    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.client_request_id = "hr1-ordinary-missing-context".into();
    request.turn.message = "附近电影院今晚有什么场次？".into();
    request.web_enabled = true;
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink)
        .expect("accepted ordinary missing-context Run");

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;

    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("run snapshot")
        .expect("ordinary missing-context Run");
    assert!(
        response.run.state.is_terminal(),
        "ordinary missing context must not strand an active structured-input Run"
    );
    assert!(
        response.run.pending_input.is_none(),
        "ordinary clarification must not reserve an active Run input"
    );
}

#[tokio::test]
async fn ordinary_clarification_completes_and_next_run_receives_conversation_context() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    install_headless_contract_mcp(&state);
    let llm = spawn_llm_protocol_double(vec![HttpResponseScript::sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"为了查询附近电影院今晚的场次，请告诉我所在的城市或地区？\"}}]}\n\ndata: [DONE]\n\n",
    )])
    .await
    .expect("local LLM boundary");
    install_test_routing(
        &state,
        &llm.base_url,
        "iris-test-verified-tools-hr4-natural-clarification",
    );

    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.client_request_id = "hr4-natural-clarification".into();
    request.turn.message = "附近电影院今晚有什么场次？".into();
    request.web_enabled = true;
    let accepted =
        RunIntake::start_with_sink(&state.db, request, &sink).expect("accepted clarification Run");

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;

    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("clarification snapshot")
        .expect("clarification Run");
    assert_eq!(response.run.state, RunState::Completed);
    assert!(response.run.pending_input.is_none());
    let messages =
        NormalSessionRepository::load_messages(&state.db, &accepted.session.session_key, 10)
            .expect("persisted clarification messages");
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.role == "assistant")
            .count(),
        1
    );
    assert!(messages.iter().any(|message| {
        message.role == "assistant" && message.content.contains("城市或地区")
    }));
    assert_eq!(llm.finish().await.expect("LLM completion").len(), 1);

    let mut follow_up_request = direct_request();
    follow_up_request.client_request_id = "hr4-natural-clarification-follow-up".into();
    follow_up_request.session = Some(accepted.session.clone());
    follow_up_request.turn.message = "深圳".into();
    let follow_up = RunIntake::start(&state.db, follow_up_request).expect("accept follow-up Run");
    let follow_up_context = RunContextAssembler::assemble(
        &state.db,
        None,
        &follow_up.session.session_key,
        &follow_up.run_id,
    )
    .expect("assemble follow-up conversation context");
    let follow_up_messages = follow_up_context
        .messages_with_context_material_plan(&follow_up_context.context_material_plan());
    assert!(follow_up_messages.iter().any(|message| {
        message
            .content
            .text_content()
            .contains("附近电影院今晚有什么场次")
    }));
    assert!(follow_up_messages.iter().any(|message| {
        message
            .content
            .text_content()
            .contains("请告诉我所在的城市或地区")
    }));
}

#[tokio::test]
async fn legacy_current_fact_run_is_terminalized_without_provider_replay() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let llm = spawn_llm_protocol_double(vec![HttpResponseScript::sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"must not be requested\"}}]}\n\ndata: [DONE]\n\n",
    )])
    .await
    .expect("callable local Provider boundary");
    install_test_routing(&state, &llm.base_url, "legacy-run-must-not-replay-model");
    let sink = RecordingSink::default();
    let accepted = RunIntake::start_with_sink(&state.db, direct_request(), &sink)
        .expect("accept legacy compatibility fixture");
    let mut envelope = RunIntake::resolve_envelope(&direct_request())
        .expect("build historical compatibility envelope");
    envelope.fresh_fact = FreshFactPolicy {
        domain: FreshFactDomain::Weather,
        ..FreshFactPolicy::default()
    };
    state
        .db
        .with_conn(|conn| {
            conn.execute(
                "UPDATE agent_runs SET envelope_json = ?1 WHERE run_id = ?2",
                rusqlite::params![serde_json::to_string(&envelope)?, accepted.run_id],
            )?;
            Ok(())
        })
        .expect("install historical envelope");

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;

    assert_eq!(
        llm.request_count(),
        0,
        "a retired historical current-fact Run must terminalize before it reaches a callable Provider"
    );

    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("terminal snapshot")
        .expect("terminal Run");
    assert_eq!(response.run.state, RunState::Failed);
    assert!(response.run.final_message_id.is_none());
    assert!(matches!(
        response.events.last().map(AssistantRunEvent::payload),
        Some(RunEventPayload::Failed { code, .. })
            if *code == super::run_contract::SafeRunErrorCode::FinalizationProtocolInvalid
    ));
}

#[tokio::test]
async fn ordinary_research_reply_repairs_missing_run_local_citation_before_completion() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    install_headless_contract_mcp_with_mode(&state, "search-fetch");
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(&tool_call_sse_with_id(
            "ordinary-research-search",
            "web_search",
            serde_json::json!({"query":"近期科技股下跌 原因"}),
        )),
        HttpResponseScript::sse(&tool_call_sse_with_id(
            "ordinary-research-fetch",
            "web_fetch",
            serde_json::json!({
                "urls":["https://source.invalid/contract"]
            }),
        )),
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"近期科技股走势受多项公开因素影响，建议结合持仓期限判断。\"}}]}\n\ndata: [DONE]\n\n",
        ),
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"近期科技股走势受多项公开因素影响，建议结合持仓期限判断。[W1]\"}}]}\n\ndata: [DONE]\n\n",
        ),
    ])
    .await
    .expect("local LLM boundary");
    install_test_routing(
        &state,
        &llm.base_url,
        "iris-test-verified-tools-hr4-ordinary-research",
    );

    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.client_request_id = "hr1-ordinary-research-finalization".into();
    request.turn.message = "请联网核实为什么近期科技股下跌？".into();
    request.web_enabled = true;
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink)
        .expect("accepted ordinary research Run");

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;

    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("run snapshot")
        .expect("ordinary research Run");
    assert_eq!(
        response.run.state,
        RunState::Completed,
        "a normal sourced answer must not require a structured finalization tool; events={:?}; model_requests={}",
        response
            .events
            .iter()
            .map(AssistantRunEvent::payload)
            .collect::<Vec<_>>(),
        llm.request_count()
    );
    assert_eq!(
        NormalSessionRepository::load_messages(&state.db, &accepted.session.session_key, 10)
            .expect("session messages")
            .into_iter()
            .filter(|message| message.role == "assistant")
            .count(),
        1,
        "the normal answer must be persisted exactly once"
    );
    let calls = llm.finish().await.expect("LLM completion");
    assert_eq!(
        calls.len(),
        4,
        "the real loop searches, fetches, and repairs the source binding once"
    );
    let tool_names = calls[0].body["tools"]
        .as_array()
        .expect("tool surface")
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str())
        .collect::<Vec<_>>();
    assert!(tool_names.contains(&"web_search"));
    assert!(
        !tool_names.contains(&"submit_final_answer"),
        "ordinary WebRequired answers must not require a structured finalization tool"
    );
    let citation_map: String = state
        .db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT citation_map_json FROM session_messages
                 WHERE session_id = 1 AND role = 'assistant'",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .expect("precise citation map");
    assert!(!citation_map.contains("source_group_fallback"));
    assert!(citation_map.contains("https://"));
}

#[tokio::test]
async fn high_stakes_current_fact_keeps_structured_finalization_tool() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    install_headless_contract_mcp(&state);
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(&tool_call_sse_with_id(
            "high-stakes-search",
            "web_search",
            serde_json::json!({"query":"当前法律建议"}),
        )),
        HttpResponseScript::sse(&tool_call_sse(
            "submit_final_answer",
            serde_json::json!({
                "blocks": [{
                    "markdown": "当前法律资料已按来源要求提交。",
                    "sources": ["W1"]
                }]
            }),
        )),
    ])
    .await
    .expect("local LLM boundary");
    install_test_routing(
        &state,
        &llm.base_url,
        "iris-test-verified-tools-hr4-high-stakes",
    );

    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.client_request_id = "hr4-high-stakes-finalization".into();
    request.turn.message = "请给我当前法律建议。".into();
    request.web_enabled = true;
    let envelope = RunIntake::resolve_envelope(&request).expect("high-stakes envelope");
    assert_eq!(
        envelope.web_reason,
        WebDecisionReason::HighStakesCurrentFact
    );
    let accepted =
        RunIntake::start_with_sink(&state.db, request, &sink).expect("accepted high-stakes Run");

    execute_normal_run(Arc::clone(&state), accepted, None, None, &sink).await;

    let calls = llm.finish().await.expect("LLM completion");
    let tool_names = calls[0].body["tools"]
        .as_array()
        .expect("tool surface")
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str())
        .collect::<Vec<_>>();
    assert!(tool_names.contains(&"web_search"));
    assert!(
        tool_names.contains(&"submit_final_answer"),
        "high-stakes current facts retain the existing strict terminal contract"
    );
}

#[tokio::test]
async fn news_question_still_answers_when_web_is_disabled() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let llm = spawn_llm_protocol_double(vec![HttpResponseScript::sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"以下是未核验的公开信息整理。\"}}]}\n\ndata: [DONE]\n\n",
    )])
    .await
    .expect("local LLM boundary");
    install_test_routing(&state, &llm.base_url, "iris-test-news-offline");
    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.client_request_id = "news-web-disabled".into();
    request.turn.message = "最新 synthetic 新闻".into();
    request.web_enabled = false;
    let accepted =
        RunIntake::start_with_sink(&state.db, request, &sink).expect("accept offline news Run");

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;

    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("offline news snapshot")
        .expect("offline news Run");
    assert_eq!(response.run.state, RunState::Completed);
    assert!(!matches!(
        response.events.last().map(AssistantRunEvent::payload),
        Some(RunEventPayload::Failed {
            code: super::run_contract::SafeRunErrorCode::WebVerificationRequired,
            ..
        })
    ));
    let assistant_messages =
        NormalSessionRepository::load_messages(&state.db, &accepted.session.session_key, 10)
            .expect("session messages")
            .into_iter()
            .filter(|message| message.role == "assistant")
            .collect::<Vec<_>>();
    assert_eq!(assistant_messages.len(), 1);
    assert!(assistant_messages[0]
        .content
        .contains("未核验的公开信息整理"));
}

#[tokio::test]
async fn production_news_uses_run_local_citation_with_high_ledger_ids_and_recovers() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    state
        .db
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO sqlite_sequence(name, seq) VALUES ('session_evidence', 1000)",
                [],
            )?;
            Ok(())
        })
        .expect("advance evidence ledger sequence");
    install_headless_contract_mcp_with_mode(&state, "search-fetch");
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(&tool_call_sse_with_id(
            "news-search",
            "web_search",
            serde_json::json!({"query":"最新 synthetic 新闻"}),
        )),
        HttpResponseScript::sse(&tool_call_sse_with_id(
            "news-fetch",
            "web_fetch",
            serde_json::json!({
                "urls":[
                    "https://source.invalid/contract",
                    "https://source-2.invalid/2"
                ]
            }),
        )),
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"最新 synthetic 新闻已按当前公开资料核实。\"}}]}\n\ndata: [DONE]\n\n",
        ),
    ])
    .await
    .expect("local LLM boundary");
    install_test_routing(
        &state,
        &llm.base_url,
        "iris-test-verified-tools-news-fallback",
    );

    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.client_request_id = "production-news-web-fallback".into();
    request.turn.message = "最新 synthetic 新闻".into();
    request.web_enabled = true;
    assert_eq!(
        RunIntake::resolve_envelope(&request)
            .expect("classify news fallback Run")
            .fresh_fact,
        Default::default()
    );
    let accepted =
        RunIntake::start_with_sink(&state.db, request, &sink).expect("accept news fallback Run");
    assert_eq!(
        AgentRunRepository::budget_policy_for_session(
            &state.db,
            &accepted.session.session_key,
            &accepted.run_id,
        )
        .expect("news Run budget")
        .expect("persisted news Run budget")
        .max_model_turns,
        8,
        "a tool-enabled current-fact Run must keep the Standard loop budget"
    );
    assert!(
        crate::ai_runtime::mcp_external_tools::load_run_snapshots(&state.db, &accepted.run_id)
            .expect("load news fallback snapshots")
            .is_empty(),
        "News fallback must not borrow a structured binding"
    );

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;

    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("news fallback snapshot")
        .expect("news fallback Run");
    assert_eq!(
        llm.request_count(),
        3,
        "preferred news Run searches and fetches then completes without a citation repair; events={:?}",
        response
            .events
            .iter()
            .map(AssistantRunEvent::payload)
            .collect::<Vec<_>>()
    );
    let calls = llm.finish().await.expect("LLM completion");
    assert_eq!(response.run.state, RunState::Completed);
    let names = calls[0].body["tools"]
        .as_array()
        .expect("model tool surface")
        .iter()
        .map(|tool| tool["function"]["name"].as_str().expect("tool name"))
        .collect::<Vec<_>>();
    assert!(!names.contains(&"news_lookup"));
    assert!(
        names.contains(&"web_search"),
        "preferred news Run still exposes generic Web tools"
    );
    assert!(!names.contains(&"submit_final_answer"));
    assert_eq!(calls.len(), 3);
    let current_evidence =
        crate::ai_runtime::agent_evidence_repository::AgentEvidenceRepository::list_current_run_registered(
            &state.db,
            &accepted.run_id,
        )
        .expect("current Run evidence");
    assert!(
        current_evidence
            .iter()
            .all(|evidence| evidence.evidence_id > 1000),
        "Run-local W1 must not depend on a matching global ledger ID"
    );

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;
    assert_eq!(
        RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
            .expect("recovery snapshot")
            .expect("recovered Run")
            .run
            .final_message_id,
        response.run.final_message_id
    );
}

#[tokio::test]
async fn recent_movie_research_uses_generic_web_evidence_without_city_or_domain_tools() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    state
        .db
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO sqlite_sequence(name, seq) VALUES ('session_evidence', 2000)",
                [],
            )?;
            Ok(())
        })
        .expect("advance evidence ledger sequence");
    install_headless_contract_mcp_with_mode(&state, "search-fetch");
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(&tool_call_sse_with_id(
            "movie-search",
            "web_search",
            serde_json::json!({"query":"近期有什么好看的电影上映"}),
        )),
        HttpResponseScript::sse(&tool_call_sse_with_id(
            "movie-fetch",
            "web_fetch",
            serde_json::json!({
                "urls":[
                    "https://source.invalid/contract",
                    "https://source-2.invalid/2"
                ]
            }),
        )),
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"近期上映影片已经按当前公开资料整理。\"}}]}\n\ndata: [DONE]\n\n",
        ),
    ])
    .await
    .expect("local LLM boundary");
    install_test_routing(
        &state,
        &llm.base_url,
        "iris-test-verified-tools-movie-fallback",
    );

    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.client_request_id = "production-movie-web-fallback".into();
    request.turn.message = "近期有什么好看的电影上映？".into();
    request.web_enabled = true;
    let envelope = RunIntake::resolve_envelope(&request).expect("classify movie research Run");
    assert_eq!(envelope.fresh_fact, Default::default());
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink)
        .expect("accept broad movie research Run");

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;

    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("movie research snapshot")
        .expect("movie research Run");
    assert_eq!(
        response.run.state,
        RunState::Completed,
        "ordinary current research must complete naturally; events={:?}; evidence={:?}",
        response
            .events
            .iter()
            .map(AssistantRunEvent::payload)
            .collect::<Vec<_>>(),
        AgentEvidenceRepository::list_current_run_registered(&state.db, &accepted.run_id)
            .expect("diagnostic movie evidence")
    );
    assert!(response.run.pending_input.is_none());
    let evidence =
        AgentEvidenceRepository::list_current_run_registered(&state.db, &accepted.run_id)
            .expect("movie research evidence");
    assert!(evidence.iter().all(|item| item.evidence_id > 2000));
    assert_eq!(llm.finish().await.expect("LLM completion").len(), 3);
}

#[tokio::test]
async fn strict_current_fact_repairs_out_of_run_w8_then_completes_with_limitation() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    install_headless_contract_mcp(&state);
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(&tool_call_sse(
            "web_search",
            serde_json::json!({"query":"当前法律建议"}),
        )),
        HttpResponseScript::sse(&tool_call_sse(
            "submit_final_answer",
            serde_json::json!({
                "blocks": [{
                    "markdown": "这段内容引用了不属于当前 Run 的来源。",
                    "sources": ["W8"]
                }]
            }),
        )),
        HttpResponseScript::sse(&tool_call_sse(
            "submit_final_answer",
            serde_json::json!({
                "blocks": [{
                    "markdown": "修复后仍引用了不属于当前 Run 的来源。",
                    "sources": ["W8"]
                }]
            }),
        )),
    ])
    .await
    .expect("local LLM boundary");
    install_test_routing(
        &state,
        &llm.base_url,
        "iris-test-verified-tools-invalid-web-source",
    );

    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.client_request_id = "production-invalid-web-source".into();
    request.turn.message = "请给我当前法律建议。".into();
    request.web_enabled = true;
    let accepted =
        RunIntake::start_with_sink(&state.db, request, &sink).expect("accept invalid-source Run");

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;

    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("invalid-source snapshot")
        .expect("invalid-source Run");
    assert_eq!(response.run.state, RunState::Completed);
    assert!(matches!(
        response.events.last().map(AssistantRunEvent::payload),
        Some(RunEventPayload::Completed {
            source_summary,
            ..
        }) if source_summary.is_empty()
    ));
    let assistant_messages =
        NormalSessionRepository::load_messages(&state.db, &accepted.session.session_key, 10)
            .expect("session messages")
            .into_iter()
            .filter(|message| message.role == "assistant")
            .collect::<Vec<_>>();
    assert_eq!(assistant_messages.len(), 1);
    assert!(
        assistant_messages[0]
            .content
            .starts_with(super::agent_tool_loop::EVIDENCE_LIMITED_RESPONSE_PREFIX),
        "strict current-fact limitation must stay Host-authored: {}",
        assistant_messages[0].content
    );
    assert!(
        assistant_messages[0].content.contains("未核实线索"),
        "invalid web sources remain unverified leads: {}",
        assistant_messages[0].content
    );
    assert!(
        assistant_messages[0].content.contains("无法据此确认"),
        "strict degradation must forbid applicability conclusions: {}",
        assistant_messages[0].content
    );
    assert!(assistant_messages[0]
        .content
        .contains("https://source.invalid/contract"));
    assert!(assistant_messages[0]
        .content
        .contains("https://source-2.invalid/2"));
    assert_eq!(llm.finish().await.expect("LLM completion").len(), 3);
}

#[tokio::test]
async fn strict_web_run_fails_closed_when_no_tool_capable_model_is_available() {
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
        .expect("terminal run");
    assert_eq!(
        response.run.state,
        RunState::Failed,
        "terminal events: {:?}",
        response
            .events
            .iter()
            .map(AssistantRunEvent::payload)
            .collect::<Vec<_>>()
    );

    assert!(
        !response
            .events
            .iter()
            .any(|event| matches!(event.payload(), RunEventPayload::EvidenceRegistered { .. })),
        "通用循环在模型不具备工具能力时不得绕过它预取联网证据"
    );
    assert!(matches!(
        response.events.last().map(AssistantRunEvent::payload),
        Some(RunEventPayload::Failed {
            code: crate::ai_runtime::run_contract::SafeRunErrorCode::NoCapableModel,
            ..
        })
    ));
}

#[tokio::test]
async fn required_web_run_fails_closed_when_the_selected_model_lacks_tool_support() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    install_headless_contract_mcp(&state);
    let llm = spawn_llm_protocol_double(vec![HttpResponseScript::sse(&tool_call_sse(
        "submit_final_answer",
        serde_json::json!({
            "blocks": [{
                "markdown": "第一代 iPhone 于 2007 年发布。",
                "sources": ["W1"]
            }]
        }),
    ))])
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
    let request = direct_required_web_request();
    assert_eq!(
        RunIntake::resolve_envelope(&request)
            .expect("strict Web envelope")
            .effort,
        Effort::ToolLoop
    );
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink)
        .expect("accepted direct required Web run");
    assert_eq!(
        accepted.state,
        RunState::Accepted,
        "the strict Web fixture must start from an accepted Run"
    );

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;

    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("run snapshot")
        .expect("terminal run");
    assert_eq!(response.run.state, RunState::Failed);
    assert!(matches!(
        response.events.last().map(AssistantRunEvent::payload),
        Some(RunEventPayload::Failed {
            code: crate::ai_runtime::run_contract::SafeRunErrorCode::NoCapableModel,
            ..
        })
    ));
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
            enabled_models: Some(vec!["iris-test-verified-tools-child-run".into()]),
            ..Default::default()
        },
    );
    routing.default_model = Some(ModelReference {
        provider_id: "custom".into(),
        model_id: "iris-test-verified-tools-child-run".into(),
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
        response.events.last().map(|event| event.payload()),
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

#[tokio::test]
async fn long_direct_conversation_uses_one_no_tool_compaction_then_answers_from_the_updated_summary(
) {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let session = NormalSessionRepository::create(&state.db).expect("session");
    state
        .db
        .with_conn(|conn| {
            for seq in 1..=26_i64 {
                conn.execute(
                    "INSERT INTO session_messages (session_id, seq, role, content, created_at, turn_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        session.session_id,
                        seq,
                        if seq % 2 == 0 { "assistant" } else { "user" },
                        format!("history-{seq}"),
                        format!("2026-09-05T00:06:{seq:02}Z"),
                        format!("history-turn-{}", (seq + 1) / 2),
                    ],
                )?;
            }
            Ok(())
        })
        .expect("seed history");
    ConversationMemory::refresh_for_session(&state.db, session.session_id, Default::default())
        .expect("fallback memory");

    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"{\\\"goal_summary\\\":\\\"current-direct-goal\\\",\\\"preference_summary\\\":\\\"latest correction\\\",\\\"decision_summary\\\":\\\"confirmed\\\",\\\"open_threads_summary\\\":\\\"next\\\"}\"}}]}\n\ndata: [DONE]\n\n",
        ),
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"来自更新摘要的正常答复\"}}]}\n\ndata: [DONE]\n\n",
        ),
    ])
    .await
    .expect("local LLM boundary");
    install_test_routing(&state, &llm.base_url, "long-direct-model");
    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.client_request_id = "long-direct-memory-run".into();
    request.session = Some(AssistantSessionRef {
        domain: SecurityDomain::Normal,
        session_key: session.session_key.clone(),
    });
    request.turn.message = "请承接此前目标并回答。".into();
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink).expect("accepted run");
    RunContextAssembler::assemble(
        &state.db,
        None,
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .expect("long Direct context must assemble before provider dispatch");

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;

    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("run snapshot")
        .expect("run");
    assert_eq!(
        response.run.state,
        RunState::Completed,
        "terminal payload: {:?}",
        response.events.last().map(|event| event.payload()),
    );

    let calls = tokio::time::timeout(Duration::from_secs(2), llm.finish())
        .await
        .expect("LLM double must complete")
        .expect("LLM double completion");
    assert_eq!(calls.len(), 2, "one compaction turn plus one answer turn");
    let second_messages = calls[1].body["messages"]
        .as_array()
        .expect("second request messages");
    assert!(second_messages.iter().any(|message| {
        message
            .get("content")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|content| content.contains("current-direct-goal"))
    }));
    let budget = AgentRunRepository::budget_policy_for_session(
        &state.db,
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .expect("budget")
    .expect("policy");
    assert!(budget.is_direct_memory_compaction_shape());
}
