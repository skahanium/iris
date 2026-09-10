use super::*;
use crate::ai_runtime::agent_capacity_eval::{spawn_llm_protocol_double, HttpResponseScript};
use crate::ai_runtime::model_gateway::{LlmMessage, MessageRole, StreamEvent, StreamEventObserver};
use crate::ai_runtime::run_contract::{
    AssistantRunStartRequest, AssistantTurnDraft, SecurityDomain,
};
use crate::ai_runtime::run_intake::RunIntake;
use crate::llm::config::{ResolvedLlmConfig, ResolvedModelPool};

struct Observer;
impl StreamEventObserver for Observer {
    fn observe(&mut self, _: &StreamEvent, _: u32) -> AppResult<()> {
        Ok(())
    }
}

fn request() -> AssistantRunStartRequest {
    AssistantRunStartRequest {
        client_request_id: "continuation-regression".into(),
        session: None,
        turn: AssistantTurnDraft {
            message: "但听说还有一次复仇者联盟4的重映版?".into(),
            content_parts: None,
            explicit_references: vec![],
            retrieval_scope: Default::default(),
            display_mentions: vec![],
        },
        explicit_action: None,
        web_enabled: true,
        model_override: None,
        external_tool_grants: vec![],
        security_domain: SecurityDomain::Normal,
        classified_context_ref: None,
    }
}

fn requirements() -> crate::ai_runtime::provider_router::ProviderRequirements {
    crate::ai_runtime::provider_router::ProviderRequirements {
        endpoint_family: None,
        streaming: true,
        tools: true,
        vision: false,
        reasoning: false,
        min_input_budget_tokens: 1,
        min_output_budget_tokens: 1,
        security_domain: crate::ai_runtime::provider_router::SecurityDomain::External,
    }
}

fn route(url: &str, provider_id: &str) -> DirectProviderRoute {
    crate::ai_runtime::circuit_breaker::reset_for_tests(
        &crate::ai_runtime::circuit_breaker::llm_circuit_key(provider_id, "test-model"),
    );
    let resolved = ResolvedLlmConfig {
        provider_id: provider_id.into(),
        model: "test-model".into(),
        base_url: url.into(),
        thinking: false,
        reasoning: crate::ai_types::ResolvedReasoningRequest::disabled(),
        input_budget: 8192,
        output_budget: 256,
        endpoint_family: crate::ai_types::EndpointFamily::OpenAiCompatibleChatCompletions,
        supports_streaming: true,
        supports_tools: true,
        supports_vision: false,
        supports_reasoning: false,
    };
    DirectProviderRoute::from_secret_free_route(ResolvedModelPool {
        resolved,
        failover_candidates: vec![],
    })
    .unwrap()
}

fn messages() -> Vec<LlmMessage> {
    vec![
        LlmMessage {
            role: MessageRole::User,
            content: "synthetic follow-up".into(),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        },
        LlmMessage {
            role: MessageRole::Assistant,
            content: "".into(),
            tool_call_id: None,
            tool_calls: Some(vec![crate::ai_types::ToolCall::new(
                "observed-call",
                "web_search",
                r#"{"query":"synthetic"}"#,
            )]),
            reasoning_content: None,
        },
        LlmMessage {
            role: MessageRole::Tool,
            content: r#"{"results":[{"title":"synthetic result"}]}"#.into(),
            tool_call_id: Some("observed-call".into()),
            tool_calls: None,
            reasoning_content: None,
        },
    ]
}

fn diagnostics(db: &Database, run_id: &str) -> serde_json::Value {
    db.with_read_conn(|conn| {
        let raw: String = conn.query_row(
            "SELECT provider_route_summary_json FROM agent_runs WHERE run_id = ?1",
            [run_id],
            |row| row.get(0),
        )?;
        Ok(serde_json::from_str(&raw)?)
    })
    .unwrap()
}

#[tokio::test]
async fn tool_bound_turn_retries_same_model_once_without_replaying_tools() {
    let server = spawn_llm_protocol_double(vec![
        HttpResponseScript::raw(500, r#"{"error":{"message":"synthetic transient"}}"#),
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"recovered\"}}]}\n\ndata: [DONE]\n\n",
        ),
    ])
    .await
    .unwrap();
    let db = Database::open_in_memory().unwrap();
    let accepted = RunIntake::start(&db, request()).unwrap();
    let provider = FailoverStreamingProvider::new(
        route(&server.base_url, "custom-continuation-retry-same-model"),
        requirements(),
        &db,
        &accepted.session,
        &super::super::NoopRunEventSink,
    )
    .with_test_streaming_client(reqwest::Client::new());
    provider.on_tool_call_dispatched(&accepted.run_id).unwrap();
    let response = provider
        .answer_turn(
            &accepted.run_id,
            &messages(),
            &[],
            AgentModelTurnBudget::default(),
            &mut Observer,
        )
        .await;
    assert!(
        response.is_ok(),
        "tool binding must not block the same-model retry"
    );
    let calls = server.finish().await.unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0].body, calls[1].body,
        "retry must reuse the same observed tool result"
    );
    let tool_results = calls[1].body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "tool")
        .count();
    assert_eq!(tool_results, 1);
    let attempts = diagnostics(&db, &accepted.run_id);
    assert_eq!(attempts["attempts"][0]["decision"], "retry_same_provider");
    assert_eq!(attempts["attempts"][0]["httpStatus"], 500);
}

#[tokio::test]
async fn request_rejection_records_status_without_provider_body_or_retry() {
    let server = spawn_llm_protocol_double(vec![HttpResponseScript::raw(
        422,
        r#"{"error":{"message":"private-synthetic-error"}}"#,
    )])
    .await
    .unwrap();
    let db = Database::open_in_memory().unwrap();
    let accepted = RunIntake::start(&db, request()).unwrap();
    let provider = FailoverStreamingProvider::new(
        route(&server.base_url, "custom-continuation-request-rejected"),
        requirements(),
        &db,
        &accepted.session,
        &super::super::NoopRunEventSink,
    )
    .with_test_streaming_client(reqwest::Client::new());
    provider.on_tool_call_dispatched(&accepted.run_id).unwrap();
    assert!(provider
        .answer_turn(
            &accepted.run_id,
            &messages(),
            &[],
            AgentModelTurnBudget::default(),
            &mut Observer
        )
        .await
        .is_err());
    assert_eq!(server.finish().await.unwrap().len(), 1);
    let value = diagnostics(&db, &accepted.run_id);
    assert_eq!(value["attempts"][0]["errorCategory"], "request_rejected");
    assert_eq!(value["attempts"][0]["httpStatus"], 422);
    assert!(!value.to_string().contains("private-synthetic-error"));
}

#[tokio::test]
async fn tool_bound_retry_exhaustion_stops_after_two_requests() {
    let server = spawn_llm_protocol_double(vec![
        HttpResponseScript::raw(500, "{}"),
        HttpResponseScript::raw(500, "{}"),
    ])
    .await
    .unwrap();
    let db = Database::open_in_memory().unwrap();
    let accepted = RunIntake::start(&db, request()).unwrap();
    let provider = FailoverStreamingProvider::new(
        route(&server.base_url, "custom-continuation-retry-exhaustion"),
        requirements(),
        &db,
        &accepted.session,
        &super::super::NoopRunEventSink,
    )
    .with_test_streaming_client(reqwest::Client::new());
    provider.on_tool_call_dispatched(&accepted.run_id).unwrap();
    assert!(provider
        .answer_turn(
            &accepted.run_id,
            &messages(),
            &[],
            AgentModelTurnBudget::default(),
            &mut Observer
        )
        .await
        .is_err());
    assert_eq!(server.finish().await.unwrap().len(), 2);
    let value = diagnostics(&db, &accepted.run_id);
    assert_eq!(value["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(value["attempts"][1]["decision"], "terminal");
}

#[tokio::test]
async fn tool_bound_visible_output_is_not_retried() {
    struct VisibleObserver;
    impl StreamEventObserver for VisibleObserver {
        fn observe(&mut self, _: &StreamEvent, _: u32) -> AppResult<()> {
            Ok(())
        }
        fn has_visible_content(&self) -> bool {
            true
        }
    }
    let server = spawn_llm_protocol_double(vec![HttpResponseScript::raw(500, "{}")])
        .await
        .unwrap();
    let db = Database::open_in_memory().unwrap();
    let accepted = RunIntake::start(&db, request()).unwrap();
    let provider = FailoverStreamingProvider::new(
        route(&server.base_url, "custom-continuation-visible-output"),
        requirements(),
        &db,
        &accepted.session,
        &super::super::NoopRunEventSink,
    )
    .with_test_streaming_client(reqwest::Client::new());
    provider.on_tool_call_dispatched(&accepted.run_id).unwrap();
    assert!(provider
        .answer_turn(
            &accepted.run_id,
            &messages(),
            &[],
            AgentModelTurnBudget::default(),
            &mut VisibleObserver
        )
        .await
        .is_err());
    assert_eq!(server.finish().await.unwrap().len(), 1);
    let value = diagnostics(&db, &accepted.run_id);
    assert_eq!(value["attempts"][0]["decision"], "terminal");
    assert_eq!(value["attempts"][0]["hadVisibleOutput"], true);
}

async fn report_reference_result(label: &str, response: reqwest::Response) -> bool {
    let status = response.status().as_u16();
    let text = response.text().await.unwrap();
    let mut content = String::new();
    let mut has_error = false;
    let mut numeric_code = None;
    let packets = serde_json::from_str::<serde_json::Value>(&text)
        .map(|value| vec![value])
        .unwrap_or_else(|_| {
            text.lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
                .collect()
        });
    for packet in packets {
        has_error |= packet.get("error").is_some_and(|error| !error.is_null());
        numeric_code = packet["base_resp"]["status_code"].as_u64().or(numeric_code);
        if let Some(value) = packet["choices"][0]["message"]["content"]
            .as_str()
            .or_else(|| packet["choices"][0]["delta"]["content"].as_str())
        {
            content.push_str(value);
        }
    }
    eprintln!("official_result label={label} status={status} has_error={has_error} numeric_code={numeric_code:?} content_bytes={} synthetic_fact_present={}", content.len(), content.contains("24"));
    (200..300).contains(&status)
        && !has_error
        && numeric_code.is_none_or(|code| code == 0)
        && content.contains("24")
}

/// An isolated production Run or a synthetic official-example parity check.
/// Both reuse the existing single-use approved profile.
/// No history, raw responses, private reasoning or credentials are printed.
#[tokio::test]
#[ignore = "requires an approved live profile; one default-budget Run or five reference HTTP requests"]
async fn live_follow_up_continuation_probe() {
    use crate::ai_runtime::agent_capacity_eval::*;
    let mode = std::env::var("IRIS_CONTINUATION_PROBE").unwrap();
    assert!(matches!(
        mode.as_str(),
        "production-run" | "reference-parity"
    ));
    let source = std::path::PathBuf::from(std::env::var_os("IRIS_AGENT_EVAL_SOURCE_DB").unwrap());
    let data = std::path::PathBuf::from(std::env::var_os("IRIS_DATA_DIR").unwrap());
    let config = std::path::PathBuf::from(std::env::var_os("IRIS_CONFIG_DIR").unwrap());
    let id = std::env::var("IRIS_AGENT_EVAL_SESSION").unwrap();
    let profile = std::env::var("IRIS_AGENT_EVAL_APPROVED_PROFILE").unwrap();
    let candidates = filter_live_profile_candidates_by_model_allowlist(
        discover_live_profile_candidates_from_database(&source).unwrap(),
        Some("MiniMax-M3"),
    )
    .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target/agent-eval")
        .join(format!("live-{id}.json"));
    let mut session = restore_and_consume_live_preflight_session(
        &path, &id, &profile, candidates, now, &source, &data, &config,
    )
    .unwrap();
    let approval = approve_live_profile(&mut session, Some(&profile), now).unwrap();
    let _ = rustls::crypto::ring::default_provider().install_default();
    let prepared = prepare_approved_live_pilot(
        &mut session,
        Some(approval.token()),
        Some(LiveCostConfirmation::InteractionMatrixPilot),
        now,
        &LivePilotCallProbe::default(),
    )
    .unwrap();
    let state = prepared.state();
    if mode == "reference-parity" {
        let pool = crate::llm::config::resolve_model_pool_for_requirements_without_secret(
            &state.db,
            crate::llm::config::ModelPoolRequirements {
                context_tokens: 1024,
                has_images: false,
                needs_tools: true,
                needs_reasoning: false,
            },
        )
        .unwrap();
        let dispatch = DirectProviderRoute::from_secret_free_route(pool)
            .unwrap()
            .hydrate_selected_streaming_dispatch(requirements(), 0)
            .unwrap();
        // Official OpenAI-compatible example: preserve the complete response_message.
        // This direct HTTP branch deliberately bypasses Iris serialization and SSE parsing.
        let client = crate::network::cert_pinning::https_client_builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap();
        let url = crate::llm::providers::chat_completions_url(&dispatch.provider.base_url);
        let tools = serde_json::json!([{"type":"function","function":{"name":"get_weather","description":"Get weather of the supplied city.","parameters":{"type":"object","properties":{"location":{"type":"string"}},"required":["location"]}}}]);
        let user = serde_json::json!({"role":"user","content":"This is a synthetic API test. Use get_weather for San Francisco once; then answer briefly using the supplied tool result."});
        let mut body = serde_json::json!({"model":dispatch.provider.model,"messages":[user.clone()],"tools":tools,"max_tokens":4000,"reasoning_split":true,"thinking":{"type":"adaptive"},"stream":false});
        let first = client
            .post(&url)
            .bearer_auth(dispatch.provider.api_key.as_ref().unwrap().as_str())
            .json(&body)
            .send()
            .await
            .unwrap();
        eprintln!("official_probe initial_status={}", first.status().as_u16());
        assert!(
            first.status().is_success(),
            "reference initial request rejected"
        );
        let first: serde_json::Value = first.json().await.unwrap();
        let assistant = first["choices"][0]["message"].clone();
        let calls: Vec<crate::ai_types::ToolCall> =
            serde_json::from_value(assistant["tool_calls"].clone())
                .expect("reference requires native tool proposal");
        assert!(!calls.is_empty());
        let mut raw_messages = vec![user.clone(), assistant.clone()];
        let mut normalized_messages = vec![
            LlmMessage {
                role: MessageRole::User,
                content: user["content"].as_str().unwrap().into(),
                ..Default::default()
            },
            LlmMessage {
                role: MessageRole::Assistant,
                content: assistant["content"].as_str().unwrap_or_default().into(),
                tool_calls: Some(calls.clone()),
                reasoning_content: assistant
                    .get("reasoning_details")
                    .map(serde_json::Value::to_string),
                tool_call_id: None,
            },
        ];
        for call in &calls {
            raw_messages.push(serde_json::json!({"role":"tool","tool_call_id":call.id,"content":"Synthetic weather: 24 C, sunny."}));
            normalized_messages.push(LlmMessage {
                role: MessageRole::Tool,
                content: "Synthetic weather: 24 C, sunny.".into(),
                tool_call_id: Some(call.id.clone()),
                ..Default::default()
            });
        }
        body["messages"] = serde_json::json!(raw_messages);
        let reference = client
            .post(&url)
            .bearer_auth(dispatch.provider.api_key.as_ref().unwrap().as_str())
            .json(&body)
            .send()
            .await
            .unwrap();
        let mut results = vec![report_reference_result("full_message_nonstream", reference).await];
        let gateway_request = crate::ai_runtime::model_gateway::GatewayRequest {
            provider: dispatch.provider.clone(),
            messages: normalized_messages,
            tools: serde_json::from_value(tools).unwrap(),
            max_tokens: Some(4000),
            input_token_budget: None,
            temperature: None,
            stream: false,
            thinking: dispatch.thinking,
            reasoning: dispatch.reasoning,
            continuation: None,
            skip_stub_ids: vec![],
        };
        let mut iris_body =
            crate::ai_runtime::model_gateway::build_chat_completions_body(&gateway_request);
        iris_body["stream"] = serde_json::json!(false);
        let differences = [
            "content",
            "tool_calls",
            "reasoning_details",
            "reasoning_content",
            "role",
        ]
        .into_iter()
        .filter(|key| body["messages"][1].get(key) != iris_body["messages"][1].get(key))
        .collect::<Vec<_>>();
        eprintln!(
            "official_probe normalized_message_differences={:?}",
            differences
        );
        let normalized = client
            .post(&url)
            .bearer_auth(dispatch.provider.api_key.as_ref().unwrap().as_str())
            .json(&iris_body)
            .send()
            .await
            .unwrap();
        results.push(report_reference_result("iris_normalized_nonstream", normalized).await);
        body["stream"] = serde_json::json!(true);
        iris_body["stream"] = serde_json::json!(true);
        let reference = client
            .post(&url)
            .bearer_auth(dispatch.provider.api_key.as_ref().unwrap().as_str())
            .json(&body)
            .send()
            .await
            .unwrap();
        results.push(report_reference_result("full_message_stream", reference).await);
        let normalized = client
            .post(&url)
            .bearer_auth(dispatch.provider.api_key.as_ref().unwrap().as_str())
            .header("Accept", "text/event-stream")
            .header("Accept-Encoding", "identity")
            .json(&iris_body)
            .send()
            .await
            .unwrap();
        results.push(report_reference_result("iris_normalized_stream", normalized).await);
        assert!(
            results.into_iter().all(|valid| valid),
            "one or more continuation variants failed; inspect the safe metrics above"
        );
        return;
    }
    // Optional user-authorized incident replay. Read the old Run without mutating it,
    // and copy only its public conversation history into the isolated pilot database.
    let mut replay_request = request();
    if let Ok(original_run) = std::env::var("IRIS_PUBLICATION_REPLAY_RUN") {
        use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
        let source_conn = rusqlite::Connection::open_with_flags(
            &source,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let (source_session, before_seq, message): (i64, i64, String) = source_conn.query_row(
            "SELECT m.session_id, m.seq, m.content FROM session_messages m JOIN agent_runs r ON r.session_id = m.session_id AND r.turn_id = m.turn_id WHERE r.run_id = ?1 AND r.security_domain = 'normal' AND m.role = 'user'",
            [&original_run], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).expect("original public user turn must exist");
        let session = NormalSessionRepository::create(&state.db).unwrap();
        let mut stmt = source_conn.prepare("SELECT m.role, m.content FROM session_messages m WHERE m.session_id = ?1 AND m.seq < ?2 AND m.role IN ('user','assistant') AND (m.turn_id IS NULL OR EXISTS (SELECT 1 FROM agent_runs r WHERE r.session_id = m.session_id AND r.turn_id = m.turn_id AND r.status = 'completed')) ORDER BY m.seq").unwrap();
        let history = stmt
            .query_map(rusqlite::params![source_session, before_seq], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        state.db.with_conn(|conn| {
            for (index, (role, content)) in history.iter().enumerate() {
                conn.execute("INSERT INTO session_messages (session_id, seq, role, content, content_hash, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", rusqlite::params![session.session_id, index + 1, role, content, crate::cas::hash::content_hash_str(content), chrono::Utc::now().to_rfc3339()])?;
            }
            Ok(())
        }).unwrap();
        replay_request.session = Some(crate::ai_runtime::run_contract::AssistantSessionRef {
            domain: SecurityDomain::Normal,
            session_key: session.session_key,
        });
        replay_request.turn.message = message;
        eprintln!("publication_replay history_messages={}", history.len());
    }
    let accepted = RunIntake::start(&state.db, replay_request).unwrap();

    let context_ready = crate::ai_runtime::run_context::RunContextAssembler::assemble(
        &state.db,
        None,
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .is_ok();
    let registry_ready =
        crate::ai_runtime::tool_executor::ToolRegistry::for_run(&state.db, &accepted.run_id)
            .is_ok();
    let route_ready = crate::llm::config::resolve_model_pool_for_requirements_without_secret(
        &state.db,
        crate::llm::config::ModelPoolRequirements {
            context_tokens: 1024,
            has_images: false,
            needs_tools: true,
            needs_reasoning: false,
        },
    )
    .is_ok();
    eprintln!("publication_replay context_ready={context_ready} registry_ready={registry_ready} route_ready={route_ready}");
    struct PublicationAudit<'a> {
        db: &'a Database,
        invalid: std::sync::atomic::AtomicU32,
    }
    impl RunEventSink for PublicationAudit<'_> {
        fn emit(
            &self,
            event: &crate::ai_runtime::run_contract::AssistantRunEvent,
        ) -> AppResult<()> {
            if matches!(event.payload(), RunEventPayload::ContentDelta { .. }) {
                let committed = self.db.with_read_conn(|conn| {
                    Ok(conn.query_row(
                        "SELECT status = 'completed' FROM agent_runs WHERE run_id = ?1",
                        [event.run_id()],
                        |row| row.get::<_, bool>(0),
                    )?)
                })?;
                if !committed {
                    self.invalid
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
            }
            Ok(())
        }
        fn emit_presentation(&self, _: &str, event: RunPresentationPayload) -> AppResult<()> {
            if matches!(
                event,
                RunPresentationPayload::AnswerDelta { .. } | RunPresentationPayload::AnswerReset
            ) {
                self.invalid
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            Ok(())
        }
    }
    let audit = PublicationAudit {
        db: &state.db,
        invalid: std::sync::atomic::AtomicU32::new(0),
    };
    crate::ai_runtime::normal_run_service::execute_normal_run_with_eval_telemetry_cap(
        std::sync::Arc::clone(state),
        accepted.clone(),
        None,
        &audit,
        &EvaluationTelemetryTap::default(),
        LiveCampaignRunCap {
            max_model_turns: 8,
            max_tool_calls: 24,
            max_network_tool_calls: 6,
        },
    )
    .await;
    assert_eq!(
        audit.invalid.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "candidate or reset escaped publication gate"
    );
    let snapshot = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .unwrap()
        .unwrap();
    for event in &snapshot.events {
        if let RunEventPayload::Failed { code, .. } = event.payload() {
            eprintln!("publication_replay failure_code={code:?}");
        }
    }
    eprintln!(
        "continuation_probe state={:?} diagnostic={}",
        snapshot.run.state,
        diagnostics(&state.db, &accepted.run_id)
    );
    assert_eq!(snapshot.run.state, crate::ai_runtime::run_contract::RunState::Completed, "production continuation did not complete; completion alone still requires separate answer quality review");
}
