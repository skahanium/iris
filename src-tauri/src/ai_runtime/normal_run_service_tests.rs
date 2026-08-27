use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::agent_capacity_eval::{
    spawn_llm_protocol_double, EvaluationTelemetryTap, HttpResponseScript,
};
use super::agent_evidence_repository::AgentEvidenceRepository;
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
    AssistantRunAccepted, AssistantRunControlRequest, AssistantRunEvent, AssistantRunStartRequest,
    AssistantSessionRef, AssistantTurnDraft, CapabilityId, ContextMode, Effort, FreshFactDomain,
    FreshFactPolicy, RunControlAction, RunEventPayload, RunEventType, RunPresentationPayload,
    RunState, SecurityDomain,
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

fn web_tool_loop_request() -> AssistantRunStartRequest {
    let mut request = direct_request();
    request.client_request_id = "headless-normal-web-tool-loop".into();
    request.turn.message = "请联网核实 synthetic 的最新状态".into();
    request.web_enabled = true;
    request
}

fn direct_required_web_request() -> AssistantRunStartRequest {
    let mut request = direct_request();
    request.client_request_id = "headless-normal-web-direct-required".into();
    request.turn.message =
        "Please search online and verify when the first iPhone was announced.".into();
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
fn required_web_query_sanitizes_the_authorized_subject_before_disclosure() {
    let query = required_web_query_from_authorized_material(
        "这是什么时候召开的会议？",
        &[],
        ["  中国共产党\u{0000}\n第十八次全国代表大会\t  ".to_string()],
    );

    assert_eq!(
        query,
        "中国共产党 第十八次全国代表大会 这是什么时候召开的会议？"
    );
    assert!(!query.chars().any(char::is_control));
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
            web_fetch_mapping_json: None,
        },
    )
    .expect("headless MCP registry setup");
}

fn install_headless_domain_bindings(
    state: &AppState,
    operations: &[crate::ai_runtime::run_contract::DomainOperation],
) {
    use crate::ai_runtime::mcp_external_tools::{
        review_discovered_tool, test_binding_hash, upsert_binding, DomainOutputMapping,
        McpCapabilityBindingInput,
    };

    let reviewed = review_discovered_tool(
        "domain",
        &serde_json::json!({
            "type": "object",
            "properties": {
                "location": {"type":"string"},
                "days": {"type":"integer"}, "topic": {"type":"string"},
                "limit": {"type":"integer"}, "instrument": {"type":"string"},
                "assetKind": {"type":"string"}, "title": {"type":"string"},
                "channel": {"type":"string"}, "competition": {"type":"string"},
                "participant": {"type":"string"}, "date": {"type":"string"}
            },
            "additionalProperties": false
        }),
        Some(true),
    )
    .expect("review domain fixture tool");
    let provider_id = "headless-contract-mcp";
    let provider = crate::ai_runtime::mcp_runtime_registry::list_web_evidence_providers(&state.db)
        .expect("list fixture providers")
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .expect("fixture provider");
    let provider_config_hash = provider.provider_config_hash.clone();
    let provider_launch_hash = crate::ai_runtime::mcp_host_runtime::frozen_provider_launch_hash(
        provider_id,
        &provider.transport_kind,
        &provider.transport_config_json,
        &provider.credential_refs_json,
    );
    let fields = [
        "location",
        "condition",
        "temperature",
        "units",
        "observationTime",
        "issueTime",
        "title",
        "publisher",
        "publishedAt",
        "topic",
        "instrument",
        "assetKind",
        "currency",
        "asOf",
        "delay",
        "value",
        "region",
        "channel",
        "date",
        "checkedAt",
        "competition",
        "participants",
        "startTime",
        "status",
        "score",
        "sourceUrl",
        "sourceTitle",
        "observedAt",
        "evidenceId",
    ]
    .into_iter()
    .map(|field| (field.to_string(), format!("$.{field}")))
    .collect();
    let output_mapping = DomainOutputMapping {
        records_path: "$.structuredContent.records".into(),
        fields,
    };
    for operation in operations {
        let binding_config_hash = test_binding_hash(
            provider_id,
            &provider_config_hash,
            &provider_launch_hash,
            "domain",
            &reviewed.input_schema,
            *operation,
            &output_mapping,
        );
        upsert_binding(
            &state.db,
            &McpCapabilityBindingInput {
                id: None,
                provider_id: provider_id.into(),
                mcp_tool_name: "domain".into(),
                input_schema: reviewed.input_schema.clone(),
                argument_mapping: serde_json::json!({}),
                domain_operation: Some(*operation),
                output_mapping: Some(output_mapping.clone()),
                risk_class: "read_only".into(),
                read_only: true,
                user_trusted: true,
                attested_binding_config_hash: binding_config_hash,
            },
            &reviewed,
            &provider_config_hash,
        )
        .expect("bind reviewed domain fixture tool");
    }
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
    let arguments = serde_json::to_string(&arguments).expect("serialize tool arguments");
    let payload = serde_json::json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "id": "domain-operation-call",
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
    crate::ai_runtime::domain_executor::DomainExecutionPlan,
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
    let domain_plan = context.domain_plan();
    let initial_evidence =
        RunContextAssembler::register_evidence(&state.db, &accepted.run_id, &context)
            .expect("initial evidence registration");
    (sink, accepted, context, domain_plan, initial_evidence)
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
async fn production_runtime_time_uses_frozen_surface_and_recovers() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let mut request = direct_request();
    request.client_request_id = "production-runtime-time".into();
    request.turn.message = "请调研当前时间，并使用 system_time_now 工具确认后汇总。".into();
    request.web_enabled = false;
    let (sink, accepted, context, domain_plan, initial_evidence) =
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
        context.messages_with_domain_plan(&domain_plan),
        tools,
        &initial_evidence,
        Some(&domain_plan),
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
async fn news_web_fallback_produces_validated_record_from_headless_mcp() {
    use crate::ai_runtime::fresh_domains::service::{FreshDomainRequest, FreshDomainService};
    use crate::ai_runtime::mcp_external_tools::DomainOperation;
    use crate::ai_runtime::retrieval_scope::RetrievalScope;
    use crate::ai_runtime::tool_dispatch::ToolDispatchContext;

    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    install_headless_contract_mcp(&state);
    let db = &state.db;
    let retrieval_scope = RetrievalScope::default();
    let available_tool_names: Vec<String> = vec!["news_lookup".into()];
    let cold_start_packets: Vec<crate::ai_runtime::ContextPacket> = Vec::new();
    let runtime_documents: Vec<crate::ai_runtime::RuntimeDocumentSnapshot> = Vec::new();
    let ctx = ToolDispatchContext {
        db: Some(db),
        selected_web_provider_id: None,
        note_path: None,
        file_id: None,
        run_id: None,
        write_target_path: None,
        document_policy: None,
        web_search_enabled: true,
        fresh_fact_policy: None,
        available_tool_names: &available_tool_names,
        max_web_fetches: 2,
        cold_start_packets: &cold_start_packets,
        retrieval_scope: &retrieval_scope,
        runtime_documents: &runtime_documents,
        app_handle: None,
        attachment_count: 0,
        skill_activation_plan: None,
    };
    let request = FreshDomainRequest {
        operation: DomainOperation::NewsSearch,
        args: serde_json::json!({ "operation": "news.search", "topic": "synthetic", "limit": 5 }),
        requested_at: chrono::DateTime::parse_from_rfc3339("2026-08-18T08:00:00Z")
            .expect("fixed time")
            .with_timezone(&chrono::Utc),
    };
    let records = FreshDomainService
        .execute(request, &ctx)
        .await
        .expect("news web fallback must produce validated records from the headless MCP");
    assert!(!records.is_empty(), "news fallback must not be empty");
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

/// HR-4 will allow a normal, sourced answer to complete without turning a
/// provider-neutral answer into a model-specific `submit_final_answer` call.
#[tokio::test]
#[should_panic(expected = "HR-4-target")]
async fn hr1_ordinary_research_reply_still_requires_structured_finalization() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    install_headless_contract_mcp(&state);
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(&tool_call_sse(
            "web_search",
            serde_json::json!({"query":"近期科技股下跌 原因"}),
        )),
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"近期科技股走势受多项公开因素影响，建议结合持仓期限判断。\"}}]}\n\ndata: [DONE]\n\n",
        ),
    ])
    .await
    .expect("local LLM boundary");
    install_test_routing(&state, &llm.base_url, "hr1-ordinary-research-model");

    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.client_request_id = "hr1-ordinary-research-finalization".into();
    request.turn.message = "为什么近期科技股下跌？".into();
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
        "HR-4-target: a normal sourced answer must not require a structured finalization tool"
    );
    assert_eq!(
        NormalSessionRepository::load_messages(&state.db, &accepted.session.session_key, 10)
            .expect("session messages")
            .into_iter()
            .filter(|message| message.role == "assistant")
            .count(),
        1,
        "HR-4-target: the normal answer must be persisted exactly once"
    );
    let calls = llm.finish().await.expect("LLM completion");
    assert_eq!(calls.len(), 2, "the real loop must reach a normal reply");
}

#[tokio::test]
async fn production_weather_without_provider_fails_closed_instead_of_fabricating() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"weather-call\",\"type\":\"function\",\"function\":{\"name\":\"weather_lookup\",\"arguments\":\"{\\\"operation\\\":\\\"weather.current\\\",\\\"location\\\":\\\"上海\\\"}\"}}]}}]}\n\ndata: [DONE]\n\n",
        ),
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"上海今天天气晴朗。\"}}]}\n\ndata: [DONE]\n\n",
        ),
    ])
    .await
    .expect("local LLM boundary");
    install_test_routing(&state, &llm.base_url, "headless-contract-model");

    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.client_request_id = "production-weather-no-provider".into();
    request.turn.message = "上海今天天气怎么样？".into();
    request.web_enabled = true;
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink)
        .expect("accepted weather run without provider");

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;

    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("run snapshot")
        .expect("weather run");
    assert_ne!(
        response.run.state,
        RunState::Completed,
        "weather without a structured provider must not complete with a fabricated answer"
    );
    let assistant_message_count: i64 = state
        .db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM session_messages WHERE session_id = ?1 AND role = 'assistant'",
                [1_i64],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .expect("assistant message query");
    assert_eq!(
        assistant_message_count, 0,
        "no fabricated assistant weather answer may be persisted"
    );
}

#[tokio::test]
async fn legacy_domain_operations_remain_readable_for_recovery_after_new_intake_retires_them() {
    use crate::ai_runtime::run_contract::DomainOperation;

    let cases = vec![
        (
            DomainOperation::WeatherCurrent,
            "上海今天天气怎么样？",
            "weather_lookup",
            serde_json::json!({"operation":"weather.current","location":"上海"}),
        ),
        (
            DomainOperation::WeatherForecast,
            "上海未来一周天气",
            "weather_lookup",
            serde_json::json!({"operation":"weather.forecast","location":"上海","days":3}),
        ),
        (
            DomainOperation::NewsSearch,
            "最新 synthetic 新闻",
            "news_lookup",
            serde_json::json!({"operation":"news.search","topic":"synthetic"}),
        ),
        (
            DomainOperation::FinanceQuote,
            "AAPL 股票当前股价",
            "finance_lookup",
            serde_json::json!({"operation":"finance.quote","instrument":"AAPL"}),
        ),
        (
            DomainOperation::FinanceMetrics,
            "AAPL 股票市盈率指标",
            "finance_lookup",
            serde_json::json!({"operation":"finance.metrics","instrument":"AAPL"}),
        ),
        (
            DomainOperation::FinanceNews,
            "AAPL 股票新闻",
            "finance_lookup",
            serde_json::json!({"operation":"finance.news","instrument":"AAPL"}),
        ),
        (
            DomainOperation::EntertainmentNowPlaying,
            "上海正在上映什么电影",
            "entertainment_lookup",
            serde_json::json!({"operation":"entertainment.now_playing","location":"上海"}),
        ),
        (
            DomainOperation::EntertainmentUpcoming,
            "有什么电影即将上映",
            "entertainment_lookup",
            serde_json::json!({"operation":"entertainment.upcoming"}),
        ),
        (
            DomainOperation::EntertainmentStreaming,
            "现在能看什么流媒体电影",
            "entertainment_lookup",
            serde_json::json!({"operation":"entertainment.streaming"}),
        ),
        (
            DomainOperation::SportsSchedule,
            "湖人下一场赛程",
            "sports_lookup",
            serde_json::json!({"operation":"sports.schedule","participant":"湖人"}),
        ),
        (
            DomainOperation::SportsScore,
            "湖人比分",
            "sports_lookup",
            serde_json::json!({"operation":"sports.score","participant":"湖人"}),
        ),
    ];

    for (index, (operation, message, tool_name, arguments)) in cases.into_iter().enumerate() {
        let directory = tempfile::tempdir().expect("temporary app directory");
        let state = AppState::new(directory.path().join("data")).expect("application state");
        install_headless_contract_mcp_with_mode(&state, "domain-dto");
        install_headless_domain_bindings(&state, &[operation]);
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
        let tool_call = tool_call_sse(tool_name, arguments);
        let final_submission = tool_call_sse(
            "submit_final_answer",
            serde_json::json!({
                "blocks": [{
                    "markdown": "结构化领域结果已核实。",
                    "sources": ["W1", "E1003"]
                }]
            }),
        );
        let llm = spawn_llm_protocol_double(vec![
            HttpResponseScript::sse(&tool_call),
            HttpResponseScript::sse(&final_submission),
        ])
        .await
        .expect("local LLM boundary");
        install_test_routing(&state, &llm.base_url, "iris-test-verified-tools-domain");

        let sink = RecordingSink::default();
        let mut request = direct_request();
        request.client_request_id = format!("production-domain-operation-{index}");
        request.turn.message = message.into();
        request.web_enabled = true;
        let mut legacy_envelope =
            RunIntake::resolve_envelope(&request).expect("classify new production Run");
        assert_eq!(legacy_envelope.fresh_fact, Default::default());
        let accepted = RunIntake::start_with_sink(&state.db, request, &sink)
            .expect("accept new production Run");
        legacy_envelope.fresh_fact = FreshFactPolicy {
            domain: match operation {
                DomainOperation::WeatherCurrent | DomainOperation::WeatherForecast => {
                    FreshFactDomain::Weather
                }
                DomainOperation::NewsSearch => FreshFactDomain::News,
                DomainOperation::FinanceQuote
                | DomainOperation::FinanceMetrics
                | DomainOperation::FinanceNews => FreshFactDomain::Finance,
                DomainOperation::EntertainmentNowPlaying
                | DomainOperation::EntertainmentUpcoming
                | DomainOperation::EntertainmentStreaming => FreshFactDomain::Entertainment,
                DomainOperation::SportsSchedule | DomainOperation::SportsScore => {
                    FreshFactDomain::Sports
                }
            },
            operation: Some(operation),
            location_requirement: if operation == DomainOperation::EntertainmentNowPlaying {
                crate::ai_runtime::run_contract::LocationRequirement::City
            } else {
                crate::ai_runtime::run_contract::LocationRequirement::None
            },
            ..FreshFactPolicy::default()
        };
        legacy_envelope.freshness = crate::ai_runtime::run_contract::Freshness::WebRequired;
        legacy_envelope.verification_requirement =
            crate::ai_runtime::run_contract::VerificationRequirement::CurrentRunWeb;
        legacy_envelope
            .required_capabilities
            .push(CapabilityId::new("web.domain.read"));
        let legacy_envelope_json =
            serde_json::to_string(&legacy_envelope).expect("serialize legacy envelope");
        state
            .db
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE agent_runs SET envelope_json = ?1 WHERE run_id = ?2",
                    rusqlite::params![legacy_envelope_json, accepted.run_id],
                )?;
                crate::ai_runtime::mcp_external_tools::freeze_domain_run_grants(
                    conn,
                    &accepted.run_id,
                    &[operation],
                    None,
                )
            })
            .expect("install legacy domain policy and frozen grant");
        let snapshots =
            crate::ai_runtime::mcp_external_tools::load_run_snapshots(&state.db, &accepted.run_id)
                .expect("load frozen snapshots");
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].domain_operation, Some(operation));

        tokio::time::timeout(
            Duration::from_secs(15),
            execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink),
        )
        .await
        .unwrap_or_else(|_| panic!("{operation:?} production Run timed out"));

        let initial_response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
            .expect("completed snapshot")
            .expect("completed Run");
        if initial_response.run.state == RunState::AwaitingInput {
            assert_eq!(operation, DomainOperation::EntertainmentNowPlaying);
            let mut values = std::collections::BTreeMap::new();
            values.insert("city".to_string(), "上海".to_string());
            RunIntake::control_with_sink(
                &state.db,
                AssistantRunControlRequest {
                    session: accepted.session.clone(),
                    run_id: accepted.run_id.clone(),
                    expected_state_version: initial_response.run.state_version,
                    action: RunControlAction::SubmitInput {
                        input_id: format!("location-{}", accepted.run_id),
                        values,
                    },
                },
                &sink,
            )
            .expect("resume historical city-bound Run with persisted input");
            tokio::time::timeout(
                Duration::from_secs(15),
                execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink),
            )
            .await
            .unwrap_or_else(|_| panic!("{operation:?} resumed production Run timed out"));
        }

        let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
            .expect("completed snapshot")
            .expect("completed Run");
        assert_eq!(
            response.run.state,
            RunState::Completed,
            "{operation:?}: events={:?}, audit={:?}, evidence={:?}, provenance={:?}",
            response.events.iter().map(AssistantRunEvent::payload).collect::<Vec<_>>(),
            crate::ai_runtime::tool_audit::query_by_run(&state.db, &accepted.run_id)
                .expect("tool audit"),
            crate::ai_runtime::agent_evidence_repository::AgentEvidenceRepository::list_current_run_registered(
                &state.db,
                &accepted.run_id,
            )
            .expect("current Run evidence"),
            crate::ai_runtime::agent_evidence_repository::AgentEvidenceRepository::provenance_policy(
                &state.db,
                &accepted.run_id,
                true,
            )
            .expect("provenance policy")
        );
        let calls = tokio::time::timeout(Duration::from_secs(2), llm.finish())
            .await
            .expect("LLM completion timed out")
            .expect("LLM completion");
        assert_eq!(calls.len(), 2, "{operation:?} must use a tool continuation");
        let tool = calls[0].body["tools"]
            .as_array()
            .expect("model tool surface")
            .iter()
            .find(|tool| tool["function"]["name"] == tool_name)
            .expect("only matching domain tool is exposed");
        assert_eq!(
            tool["function"]["parameters"]["properties"]["operation"]["enum"],
            serde_json::json!([operation.as_str()])
        );
        assert!(
            !calls[1].body.to_string().contains("provider-supplied-id"),
            "Iris must replace provider-supplied evidence IDs"
        );
        assert!(response.run.final_message_id.is_some());

        execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;
        let recovered = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
            .expect("recovered snapshot")
            .expect("recovered Run");
        assert_eq!(recovered.run.state, RunState::Completed);
        assert_eq!(
            recovered.run.final_message_id,
            response.run.final_message_id
        );
    }
}

#[tokio::test]
async fn news_web_fallback_is_unavailable_when_web_is_disabled() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let sink = RecordingSink::default();
    let mut request = direct_request();
    request.client_request_id = "news-web-disabled".into();
    request.turn.message = "最新 synthetic 新闻".into();
    let accepted =
        RunIntake::start_with_sink(&state.db, request, &sink).expect("accept offline news Run");

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;

    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("offline news snapshot")
        .expect("offline news Run");
    assert_eq!(response.run.state, RunState::Failed);
    assert!(matches!(
        response.events.last().map(AssistantRunEvent::payload),
        Some(RunEventPayload::Failed {
            code: super::run_contract::SafeRunErrorCode::WebVerificationRequired,
            ..
        })
    ));
}

#[tokio::test]
async fn news_web_fallback_engine_chain_passes_through_evidence_ledger() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    install_headless_contract_mcp(&state);
    let mut request = web_tool_loop_request();
    request.client_request_id = "production-news-web-fallback".into();
    request.turn.message = "请调研最新 synthetic 新闻。".into();
    let (sink, accepted, context, domain_plan, initial_evidence) =
        start_headless_tool_loop(&state, request);
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"news-call\",\"type\":\"function\",\"function\":{\"name\":\"news_lookup\",\"arguments\":\"{\\\"operation\\\":\\\"news.search\\\",\\\"topic\\\":\\\"synthetic\\\",\\\"limit\\\":5}\"}}]}}]}\n\ndata: [DONE]\n\n",
        ),
        HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"最新 synthetic 新闻已核实。[W1]\"}}]}\n\ndata: [DONE]\n\n",
        ),
    ])
    .await
    .expect("local LLM boundary");
    let gateway = ModelGateway::new(reqwest::Client::new(), Vec::new());
    let provider = ModelGatewayStreamingDirectAnswerProvider::new(
        &gateway,
        ProviderConfig {
            name: "headless-news-model".into(),
            base_url: llm.base_url.clone(),
            api_key: None,
            model: "news-model".into(),
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
        },
        256,
    )
    .expect("model gateway provider");
    let capabilities = vec![
        CapabilityId::new("web.search"),
        CapabilityId::new("web.domain.read"),
    ];
    let tools = ToolRegistry::new().tools_for_authorized_capabilities(&capabilities, true);
    assert!(tools.iter().any(|tool| tool.name == "news_lookup"));
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
        context.messages_with_domain_plan(&domain_plan),
        tools,
        &initial_evidence,
        Some(&domain_plan),
        &provider,
        &executor,
        &sink,
    )
    .await
    .expect("production news web-fallback chain");

    let calls = llm.finish().await.expect("LLM double completion");
    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("run snapshot")
        .expect("completed run");
    assert_eq!(
        calls.len(),
        2,
        "news lookup must complete a real continuation"
    );
    assert_eq!(response.run.state, RunState::Completed);
    let news_evidence_count = state
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
    assert!(
        news_evidence_count >= 1,
        "news web-fallback must enter the evidence ledger"
    );
}

#[tokio::test]
async fn production_news_web_fallback_accepts_w1_with_high_ledger_ids_and_recovers() {
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
    install_headless_contract_mcp(&state);
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(&tool_call_sse(
            "web_search",
            serde_json::json!({"query":"最新 synthetic 新闻"}),
        )),
        HttpResponseScript::sse(&tool_call_sse(
            "submit_final_answer",
            serde_json::json!({
                "blocks": [{
                    "markdown": "最新 synthetic 新闻已核实。",
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
    assert!(
        crate::ai_runtime::mcp_external_tools::load_run_snapshots(&state.db, &accepted.run_id)
            .expect("load news fallback snapshots")
            .is_empty(),
        "News fallback must not borrow a structured binding"
    );

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;

    let calls = llm.finish().await.expect("LLM completion");
    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("news fallback snapshot")
        .expect("news fallback Run");
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
        "HR-3 的严格联网任务也必须从通用循环暴露 Web 工具"
    );
    assert!(names.contains(&"submit_final_answer"));
    assert_eq!(calls.len(), 2);
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
async fn webpreferred_movie_research_uses_generic_web_evidence_without_city_or_domain_tools() {
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
    install_headless_contract_mcp(&state);
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(&tool_call_sse(
            "web_search",
            serde_json::json!({"query":"近期有什么好看的电影上映"}),
        )),
        HttpResponseScript::sse(&tool_call_sse(
            "submit_final_answer",
            serde_json::json!({
                "blocks": [{
                    "markdown": "近期上映影片已经按当前公开资料核实。",
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
        RunState::Failed,
        "ordinary WebPreferred finalization remains the explicit HR-4 boundary; events={:?}; evidence={:?}",
        response
            .events
            .iter()
            .map(AssistantRunEvent::payload)
            .collect::<Vec<_>>(),
        AgentEvidenceRepository::list_current_run_registered(&state.db, &accepted.run_id)
            .expect("diagnostic movie evidence")
    );
    assert!(matches!(
        response.events.last().map(AssistantRunEvent::payload),
        Some(RunEventPayload::Failed {
            code: crate::ai_runtime::run_contract::SafeRunErrorCode::FinalizationProtocolInvalid,
            ..
        })
    ));
    assert!(response.run.pending_input.is_none());
    let evidence =
        AgentEvidenceRepository::list_current_run_registered(&state.db, &accepted.run_id)
            .expect("movie research evidence");
    assert!(evidence.iter().all(|item| item.evidence_id > 2000));
    assert_eq!(llm.finish().await.expect("LLM completion").len(), 2);
}

#[tokio::test]
async fn web_fallback_rejects_an_out_of_run_w8_without_persisting_an_answer() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    install_headless_contract_mcp(&state);
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(&tool_call_sse(
            "web_search",
            serde_json::json!({"query":"最新 synthetic 新闻"}),
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
    request.turn.message = "最新 synthetic 新闻".into();
    request.web_enabled = true;
    let accepted =
        RunIntake::start_with_sink(&state.db, request, &sink).expect("accept invalid-source Run");

    execute_normal_run(Arc::clone(&state), accepted.clone(), None, None, &sink).await;

    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("invalid-source snapshot")
        .expect("invalid-source Run");
    assert_eq!(response.run.state, RunState::Failed);
    assert!(matches!(
        response.events.last().map(AssistantRunEvent::payload),
        Some(RunEventPayload::Failed {
            code: super::run_contract::SafeRunErrorCode::FinalizationProtocolInvalid,
            ..
        })
    ));
    let assistant_count =
        NormalSessionRepository::load_messages(&state.db, &accepted.session.session_key, 10)
            .expect("session messages")
            .into_iter()
            .filter(|message| message.role == "assistant")
            .count();
    assert_eq!(assistant_count, 0);
    assert_eq!(llm.finish().await.expect("LLM completion").len(), 2);
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
