//! Desktop adapter integration and confirmed recovery tests.

use std::cell::Cell;
use std::sync::Arc;
use std::time::Duration;

use super::{
    assistant_run_control, assistant_session_retract, dispatch_normal_run_service,
    evaluate_normal_run_policy, execute_confirmed_change_with_sink,
    historical_source_summary_for_run, historical_web_citations_for_run,
};
#[cfg(not(windows))]
use crate::ai_runtime::agent_capacity_eval::{spawn_llm_protocol_double, HttpResponseScript};
use crate::ai_runtime::agent_evidence_repository::{
    AgentEvidenceRepository, MaterialRole, WebEvidenceInput,
};
use crate::ai_runtime::agent_run_repository::{
    AgentRunRepository, AppendRunCheckpointInput, AppendRunEventInput, DurableApplyCheckpoint,
    DurableApplyCheckpointStage,
};
use crate::ai_runtime::frozen_change_plan::{
    FrozenChangeOperationInput, FrozenChangePlan, FrozenChangePlanInput, FrozenChangeSetInput,
};
#[cfg(not(windows))]
use crate::ai_runtime::mcp_external_tools::{
    review_discovered_tool, upsert_binding, McpCapabilityBindingInput, McpCapabilityBindingSummary,
};
#[cfg(not(windows))]
use crate::ai_runtime::mcp_host_runtime::{
    discover_provider_tools_without_recording_with_config_hash, McpHostRuntimeOptions,
    DEFAULT_STDIO_SESSION_IDLE_TIMEOUT,
};
#[cfg(not(windows))]
use crate::ai_runtime::mcp_runtime_registry::{
    upsert_web_evidence_provider, WebEvidenceProviderInput,
};
#[cfg(not(windows))]
use crate::ai_runtime::run_contract::ExternalToolGrantRef;
use crate::ai_runtime::run_contract::{
    AssistantRunAccepted, AssistantRunControlRequest, AssistantRunEvent, AssistantRunStartRequest,
    AssistantTurnDraft, Effect, ExplicitAction, ExplicitTarget, RunControlAction, RunEventPayload,
    RunEventType, RunRecoveryKind, RunState, SecurityDomain,
};
use crate::ai_runtime::run_engine::RunEngine;
use crate::ai_runtime::run_engine::RunEventSink;
use crate::ai_runtime::run_intake::RunIntake;
use crate::ai_types::{ContextReferenceKind, ContextReferenceWire};
use crate::app::AppState;
use crate::error::AppResult;
#[cfg(not(windows))]
use crate::llm::config::{LlmRoutingConfig, ModelReference, ProviderOverride};
use tauri::webview::InvokeRequest;

struct NoopSink;

impl RunEventSink for NoopSink {
    fn emit(&self, _event: &AssistantRunEvent) -> AppResult<()> {
        Ok(())
    }
}

struct InflightProbeSink {
    run_id: String,
    saw_inflight: std::sync::Mutex<bool>,
}

impl RunEventSink for InflightProbeSink {
    fn emit(&self, _event: &AssistantRunEvent) -> AppResult<()> {
        if crate::ai_runtime::run_inflight::is_marked(&self.run_id) {
            *self.saw_inflight.lock().expect("inflight probe") = true;
        }
        Ok(())
    }
}

#[test]
fn modern_explicit_empty_evidence_selection_never_rehydrates_run_sources() {
    let db = crate::storage::db::Database::open_in_memory().expect("database");
    let session =
        crate::ai_runtime::normal_session_repository::NormalSessionRepository::create(&db)
            .expect("session");
    crate::ai_runtime::agent_run_repository::AgentRunRepository::accept(
        &db,
        crate::ai_runtime::agent_run_repository::AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key,
            client_request_id: "projection-client".into(),
            run_id: "projection-run".into(),
            turn_id: "projection-turn".into(),
            message: "核实当前事实".into(),
            content_parts: None,
            explicit_references: Vec::new(),
            context_scope: Default::default(),
            display_mentions: Vec::new(),
            explicit_action: None,
            envelope: crate::ai_runtime::run_contract::ExecutionEnvelope {
                effect: Effect::Answer,
                context: crate::ai_runtime::run_contract::ContextMode::None,
                freshness: crate::ai_runtime::run_contract::Freshness::WebRequired,
                web_reason:
                    crate::ai_runtime::run_contract::WebDecisionReason::VolatileExternalFact,
                verification_requirement:
                    crate::ai_runtime::run_contract::VerificationRequirement::CurrentRunWeb,
                effort: crate::ai_runtime::run_contract::Effort::ToolLoop,
                security_domain: SecurityDomain::Normal,
                risk: crate::ai_runtime::run_contract::RiskClass::ReadOnly,
                modalities: Vec::new(),
                material_needs: Vec::new(),
                required_capabilities: vec![crate::ai_runtime::run_contract::CapabilityId::new(
                    "web.search",
                )],
                explicit_constraints: Vec::new(),
                fresh_fact: Default::default(),
            },
        },
    )
    .expect("accepted run");
    let registered = AgentEvidenceRepository::register_web(
        &db,
        WebEvidenceInput {
            session_id: session.session_id,
            run_id: "projection-run".into(),
            message_seq_first: 1,
            material_role: MaterialRole::Lookup,
            title: "已抓取正文".into(),
            url: "https://example.test/article".into(),
            normalized_url: "https://example.test/article".into(),
            domain: "example.test".into(),
            retrieved_at: "2026-09-01T00:00:00Z".into(),
            provider_id: "test-web".into(),
            provider_kind: "mcp".into(),
            raw_result_hash: "projection-body-hash".into(),
            extraction_method: "mcp_fetch_raw_content".into(),
            bounded_excerpt: "足够长的已抓取正文".into(),
            retrieval_reason: Some("web_fetch".into()),
            score: None,
            source_rank: None,
            conflict_group: None,
            failure_reason: None,
        },
    )
    .expect("web evidence");

    assert!(
        historical_web_citations_for_run(&db, Some("projection-run"), Some(&[]), vec![],)
            .is_empty()
    );
    assert!(historical_source_summary_for_run(
        &db,
        Some("projection-run"),
        Some(&[]),
        vec![crate::ai_runtime::provenance::SourceSummaryEntry {
            category: "web".into(),
            count: 1,
        }],
    )
    .is_empty());
    assert_eq!(
        historical_web_citations_for_run(
            &db,
            Some("projection-run"),
            Some(&[registered.evidence_id]),
            vec![],
        )
        .len(),
        1
    );
    assert_eq!(
        historical_source_summary_for_run(
            &db,
            Some("projection-run"),
            Some(&[registered.evidence_id]),
            vec![],
        ),
        vec![crate::ai_runtime::provenance::SourceSummaryEntry {
            category: "web".into(),
            count: 1,
        }]
    );
    assert_eq!(
        historical_web_citations_for_run(&db, Some("projection-run"), None, vec![]).len(),
        1
    );
}

#[tokio::test]
async fn production_service_dispatch_receives_a_present_desktop_app_handle() {
    let app = tauri::test::mock_app();
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let accepted = RunIntake::start(
        &state.db,
        AssistantRunStartRequest {
            client_request_id: "desktop-service-dispatch".to_string(),
            session: None,
            turn: AssistantTurnDraft {
                message: "请回答".to_string(),
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
        },
    )
    .expect("accepted run");
    let observed_present = Cell::new(false);

    dispatch_normal_run_service(
        Arc::clone(&state),
        accepted,
        None,
        app.handle().clone(),
        &NoopSink,
        |_, _, _, app_handle: Option<tauri::AppHandle<tauri::test::MockRuntime>>, _| {
            observed_present.set(app_handle.is_some());
            std::future::ready(())
        },
    )
    .await;

    assert!(observed_present.get());
}

#[cfg(not(windows))]
async fn install_production_external_binding(state: &AppState) -> McpCapabilityBindingSummary {
    let fixture = format!(
        "{}/tests/fixtures/agent-capacity-mcp-stdio.sh",
        env!("CARGO_MANIFEST_DIR")
    );
    upsert_web_evidence_provider(
        &state.db,
        &WebEvidenceProviderInput {
            id: "assistant-start-external".into(),
            name: "Assistant Start External".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json: serde_json::json!({
                "command": "/bin/sh",
                "args": [fixture, "search-only", "2"]
            })
            .to_string(),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: Some(r#"{"tool":"search","queryArg":"query"}"#.into()),
            web_fetch_mapping_json: None,
        },
    )
    .expect("external MCP provider");
    let (discovery, provider_config_hash) =
        discover_provider_tools_without_recording_with_config_hash(
            &state.db,
            "assistant-start-external",
            McpHostRuntimeOptions {
                request_timeout: Duration::from_secs(5),
                max_stdout_line_bytes: 64 * 1024,
                max_stderr_bytes: 8 * 1024,
                cwd: None,
                stdio_session_pool: true,
                stdio_session_idle_timeout: DEFAULT_STDIO_SESSION_IDLE_TIMEOUT,
            },
        )
        .await
        .expect("real stdio MCP discovery");
    let discovered = discovery
        .tools
        .into_iter()
        .find(|tool| tool.name == "search")
        .expect("search tool");
    let reviewed = review_discovered_tool(
        &discovered.name,
        &discovered.input_schema,
        discovered.read_only_hint,
    )
    .expect("read-only review");
    let input = McpCapabilityBindingInput {
        id: None,
        provider_id: "assistant-start-external".into(),
        mcp_tool_name: discovered.name,
        input_schema: reviewed.input_schema.clone(),
        argument_mapping: serde_json::json!({}),
        risk_class: "read_only".into(),
        read_only: true,
        user_trusted: true,
        attested_binding_config_hash: String::new(),
        domain_operation: None,
        output_mapping: None,
    };
    let attestation = crate::ai_runtime::mcp_external_tools::attest_reviewed_tool(
        &state.db,
        &input.provider_id,
        &reviewed,
        &provider_config_hash,
        &input.argument_mapping,
    )
    .expect("binding attestation");
    let input = McpCapabilityBindingInput {
        attested_binding_config_hash: attestation.binding_config_hash,
        ..input
    };
    upsert_binding(&state.db, &input, &reviewed, &provider_config_hash).expect("trusted binding")
}

#[cfg(not(windows))]
fn configure_test_llm(state: &AppState, base_url: String, model_id: &str) {
    let mut routing = LlmRoutingConfig::default();
    routing.providers.clear();
    routing.providers.insert(
        "custom".into(),
        ProviderOverride {
            base_url: Some(base_url),
            enabled_models: Some(vec![model_id.into()]),
            ..Default::default()
        },
    );
    routing.default_model = Some(ModelReference {
        provider_id: "custom".into(),
        model_id: model_id.into(),
    });
    crate::llm::config::save(&state.db, &routing).expect("normal service route");
    state.set_test_streaming_client(reqwest::Client::new());
}

#[cfg(not(windows))]
async fn wait_for_terminal(state: &AppState, accepted: &AssistantRunAccepted) -> RunState {
    let mut last = None;
    for _ in 0..500 {
        let current = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
            .expect("poll run")
            .expect("accepted run");
        if current.run.state.is_terminal() {
            return current.run.state;
        }
        last = Some(current.run.state);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "assistant run did not reach a terminal state; last observed {:?}",
        last
    );
}

#[cfg(not(windows))]
async fn drive_accepted_run(state: &Arc<AppState>, accepted: &AssistantRunAccepted) {
    crate::ai_runtime::normal_run_service::execute_normal_run(
        Arc::clone(state),
        accepted.clone(),
        None,
        None,
        &NoopSink,
    )
    .await;
}

#[cfg(not(windows))]
#[tokio::test]
async fn assistant_run_start_reaches_frozen_stdio_external_tool_and_enforces_exact_run_grant_evidence(
) {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let binding = install_production_external_binding(&state).await;
    let first_tool_packet = serde_json::json!({
        "choices":[{
            "delta":{
                "tool_calls":[{
                    "index":0,
                    "id":"assistant-start-external-call",
                    "type":"function",
                    "function":{
                        "name":binding.exposed_name,
                        "arguments":"{\"query\":\"synthetic\"}"
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    });
    let first_tool_sse = format!("data: {first_tool_packet}\n\ndata: [DONE]\n\n");
    let final_submission_packet = serde_json::json!({
        "choices":[{
            "delta":{
                "tool_calls":[{
                    "index":0,
                    "id":"assistant-start-final",
                    "type":"function",
                    "function":{
                        "name":"submit_final_answer",
                        "arguments":"{\"blocks\":[{\"markdown\":\"外部工具事实已核实。\",\"sources\":[\"E1\"]}]}"
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    });
    let final_submission_sse = format!("data: {final_submission_packet}\n\ndata: [DONE]\n\n");
    let llm = spawn_llm_protocol_double(vec![
        HttpResponseScript::sse(&first_tool_sse),
        HttpResponseScript::sse(&final_submission_sse),
    ])
    .await
    .expect("local LLM boundary");
    configure_test_llm(
        &state,
        llm.base_url.clone(),
        "iris-test-verified-tools-assistant-external",
    );
    let granted_request = AssistantRunStartRequest {
        client_request_id: "assistant-start-external-granted".into(),
        session: None,
        turn: AssistantTurnDraft {
            message: "请用我授权的外部只读工具核实 synthetic 事实".into(),
            content_parts: None,
            explicit_references: Vec::new(),
            retrieval_scope: Default::default(),
            display_mentions: Vec::new(),
        },
        explicit_action: None,
        web_enabled: false,
        model_override: None,
        external_tool_grants: vec![ExternalToolGrantRef {
            binding_id: binding.id.clone(),
            binding_config_hash: binding.binding_config_hash.clone(),
        }],
        security_domain: SecurityDomain::Normal,
        classified_context_ref: None,
    };
    let granted = RunIntake::start(&state.db, granted_request.clone()).expect("granted intake");
    drive_accepted_run(&state, &granted).await;
    assert_eq!(
        wait_for_terminal(&state, &granted).await,
        RunState::Completed
    );
    let calls = llm.finish().await.expect("LLM completion");
    assert_eq!(calls.len(), 2);
    let granted_tools = calls[0].body["tools"]
        .as_array()
        .expect("granted model tool surface");
    assert!(granted_tools
        .iter()
        .any(|tool| tool["function"]["name"] == binding.exposed_name));
    assert!(!granted_tools
        .iter()
        .any(|tool| tool["function"]["name"] == "web_search"));
    let external_evidence_count = state
        .db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM agent_run_evidence
                 WHERE run_id = ?1 AND registration_source = 'external_tool'",
                [&granted.run_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(Into::into)
        })
        .expect("external evidence count");
    assert_eq!(external_evidence_count, 1);

    let mut ungranted_request = granted_request.clone();
    ungranted_request.client_request_id = "assistant-start-external-ungranted".into();
    ungranted_request.external_tool_grants.clear();
    let ungranted = RunIntake::start(&state.db, ungranted_request).expect("ungranted intake");
    drive_accepted_run(&state, &ungranted).await;
    assert_eq!(
        wait_for_terminal(&state, &ungranted).await,
        RunState::Failed
    );
    let ungranted_tools =
        crate::ai_runtime::tool_executor::ToolRegistry::for_run(&state.db, &ungranted.run_id)
            .expect("ungranted registry")
            .tools_for_authorized_capabilities(
                &[crate::ai_runtime::run_contract::CapabilityId::new(
                    "external.read",
                )],
                true,
            );
    assert!(ungranted_tools
        .iter()
        .any(|tool| tool.name == binding.exposed_name));

    let bypass_llm = spawn_llm_protocol_double(vec![HttpResponseScript::sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"未经工具核实的事实。\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
    )])
    .await
    .expect("bypass LLM boundary");
    configure_test_llm(
        &state,
        bypass_llm.base_url.clone(),
        "iris-test-verified-tools-assistant-external-bypass",
    );
    let mut bypass_request = granted_request;
    bypass_request.client_request_id = "assistant-start-external-bypass".into();
    let bypass = RunIntake::start(&state.db, bypass_request).expect("bypass intake");
    drive_accepted_run(&state, &bypass).await;
    assert_eq!(wait_for_terminal(&state, &bypass).await, RunState::Failed);
    let bypass_calls = bypass_llm.finish().await.expect("bypass LLM completion");
    assert_eq!(bypass_calls.len(), 1);
    assert!(bypass_calls[0].body["tools"]
        .as_array()
        .expect("bypass tool surface")
        .iter()
        .any(|tool| tool["function"]["name"] == binding.exposed_name));
    let bypass_evidence_count = state
        .db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM agent_run_evidence
                 WHERE run_id = ?1 AND registration_source = 'external_tool'",
                [&bypass.run_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(Into::into)
        })
        .expect("bypass evidence count");
    assert_eq!(bypass_evidence_count, 0);
}

fn durable_apply_fixture() -> (
    tempfile::TempDir,
    Arc<AppState>,
    AssistantRunAccepted,
    FrozenChangePlan,
) {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let vault = directory.path().join("vault");
    std::fs::create_dir_all(&vault).expect("vault directory");
    std::fs::write(vault.join("note.md"), "base").expect("base note");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    state.set_vault(vault.clone()).expect("activate vault");
    let base_hash = crate::cas::hash::content_hash_str("base");
    let accepted = RunIntake::start(
        &state.db,
        AssistantRunStartRequest {
            client_request_id: format!("durable-command-{}", uuid::Uuid::new_v4()),
            session: None,
            turn: AssistantTurnDraft {
                message: "将已确认的修改应用到笔记".into(),
                content_parts: None,
                explicit_references: vec![ContextReferenceWire {
                    id: "target-note".into(),
                    kind: ContextReferenceKind::Note,
                    file_path: Some("note.md".into()),
                    content_hash: Some(base_hash.clone()),
                    utf8_range: None,
                    editor_range: None,
                    excerpt: String::new(),
                    heading_path: None,
                    anchor: None,
                    stale: false,
                    invalid_reason: None,
                }],
                retrieval_scope: Default::default(),
                display_mentions: vec![],
            },
            explicit_action: Some(ExplicitAction {
                effect: Effect::Apply,
                target: Some(ExplicitTarget {
                    reference_id: "target-note".into(),
                    content_hash: base_hash.clone(),
                }),
                selection_snapshot: None,
            }),
            web_enabled: false,
            model_override: None,
            external_tool_grants: Vec::new(),
            security_domain: SecurityDomain::Normal,
            classified_context_ref: None,
        },
    )
    .expect("accepted durable apply");
    let active_vault = state.vault_path().expect("canonical active vault");
    let policy = evaluate_normal_run_policy(&state.db, &accepted).expect("current policy decision");
    AgentRunRepository::persist_authorization_snapshot(
        &state.db,
        &accepted.session.session_key,
        &accepted.run_id,
        &policy.allowed_capabilities,
    )
    .expect("immutable authorization snapshot");
    let session_id = state
        .db
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
        &state.db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: accepted.state_version,
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
        &state.db,
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
    let tool_call_id = format!("tool-{}", accepted.run_id);
    let started = AgentRunRepository::append_event(
        &state.db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: running.state_version(),
            event_type: RunEventType::ToolStarted,
            payload: RunEventPayload::ToolStarted {
                capability: "replace_selection".into(),
                tool_call_id: tool_call_id.clone(),
            },
        },
    )
    .expect("tool started");
    let plan = FrozenChangePlan::freeze(FrozenChangePlanInput {
        confirmation_id: format!("confirmation-{}", accepted.run_id),
        run_id: accepted.run_id.clone(),
        session_id,
        request_id: accepted.run_id.clone(),
        tool_call_id,
        vault_id: crate::cas::hash::content_hash_str(&active_vault.to_string_lossy()),
        relative_paths: vec!["note.md".into()],
        operation: "replace_selection".into(),
        base_content_hashes: vec![("note.md".into(), base_hash.clone())],
        expected_post_content_hashes: vec![(
            "note.md".into(),
            crate::cas::hash::content_hash_str("after"),
        )],
        change: serde_json::json!({
            "target_path": "note.md",
            "base_content_hash": base_hash,
            "range": { "start": 0, "end": 4 },
            "original_text": "base",
            "replacement": "after"
        }),
        affected_file_count: 1,
        rollback_summary: "可通过版本历史撤销".into(),
        expires_at_unix_ms: i64::MAX,
    })
    .expect("frozen plan");
    let awaiting = AgentRunRepository::request_frozen_confirmation(
        &state.db,
        &plan,
        started.state_version(),
        "等待确认：更新 1 个目标",
    )
    .expect("await confirmation");
    AgentRunRepository::approve_frozen_confirmation(
        &state.db,
        &accepted.session.session_key,
        &accepted.run_id,
        plan.confirmation_id(),
        plan.plan_hash(),
        awaiting.state_version(),
        0,
    )
    .expect("consume confirmation");
    (directory, state, accepted, plan)
}

fn tamper_consumed_confirmation(state: &AppState, original: &FrozenChangePlan) -> FrozenChangePlan {
    let original_operation = &original.operations()[0];
    let tampered = FrozenChangePlan::freeze(FrozenChangePlanInput {
        confirmation_id: original.confirmation_id().into(),
        run_id: original.run_id().into(),
        session_id: original.session_id(),
        request_id: original.run_id().into(),
        tool_call_id: original_operation.tool_call_id().into(),
        vault_id: original.vault_id().into(),
        relative_paths: original.relative_paths().to_vec(),
        operation: original_operation.operation().into(),
        base_content_hashes: original_operation.base_content_hashes().to_vec(),
        expected_post_content_hashes: vec![(
            "note.md".into(),
            crate::cas::hash::content_hash_str("tampered"),
        )],
        change: serde_json::json!({
            "target_path": "note.md",
            "base_content_hash": crate::cas::hash::content_hash_str("base"),
            "range": { "start": 0, "end": 4 },
            "original_text": "base",
            "replacement": "tampered"
        }),
        affected_file_count: 1,
        rollback_summary: "可通过版本历史撤销".into(),
        expires_at_unix_ms: i64::MAX,
    })
    .expect("tampered frozen plan");
    state
        .db
        .with_conn(|conn| {
            conn.execute(
                "UPDATE agent_run_confirmations
                 SET plan_hash = ?1, plan_json = ?2
                 WHERE confirmation_id = ?3 AND run_id = ?4 AND status = 'consumed'",
                rusqlite::params![
                    tampered.plan_hash(),
                    tampered.persisted_plan_json()?,
                    tampered.confirmation_id(),
                    tampered.run_id(),
                ],
            )?;
            Ok(())
        })
        .expect("tamper consumed confirmation row");
    tampered
}

fn install_two_note_change_set(
    directory: &tempfile::TempDir,
    state: &Arc<AppState>,
    accepted: &AssistantRunAccepted,
    original: &FrozenChangePlan,
    second_note_content: &str,
) -> FrozenChangePlan {
    std::fs::write(
        directory.path().join("vault/note-2.md"),
        second_note_content,
    )
    .expect("second note content");
    let chained = FrozenChangePlan::freeze_set(FrozenChangeSetInput {
        confirmation_id: original.confirmation_id().to_string(),
        run_id: accepted.run_id.clone(),
        session_id: original.session_id(),
        request_id: accepted.run_id.clone(),
        vault_id: original.vault_id().to_string(),
        operations: vec![
            FrozenChangeOperationInput {
                tool_call_id: original.operations()[0].tool_call_id().to_string(),
                operation: "replace_selection".to_string(),
                relative_paths: vec!["note.md".to_string()],
                base_content_hashes: vec![(
                    ("note.md").to_string(),
                    crate::cas::hash::content_hash_str("base"),
                )],
                expected_post_content_hashes: vec![(
                    "note.md".to_string(),
                    crate::cas::hash::content_hash_str("after"),
                )],
                change: serde_json::json!({
                    "target_path": "note.md",
                    "base_content_hash": crate::cas::hash::content_hash_str("base"),
                    "range": { "start": 0, "end": 4 },
                    "original_text": "base",
                    "replacement": "after"
                }),
                rollback_summary: "可通过版本历史撤销".to_string(),
            },
            FrozenChangeOperationInput {
                tool_call_id: format!("{}-second", original.operations()[0].tool_call_id()),
                operation: "replace_selection".to_string(),
                relative_paths: vec!["note-2.md".to_string()],
                base_content_hashes: vec![(
                    "note-2.md".to_string(),
                    crate::cas::hash::content_hash_str("base2"),
                )],
                expected_post_content_hashes: vec![(
                    "note-2.md".to_string(),
                    crate::cas::hash::content_hash_str("final2"),
                )],
                change: serde_json::json!({
                    "target_path": "note-2.md",
                    "base_content_hash": crate::cas::hash::content_hash_str("base2"),
                    "range": { "start": 0, "end": 5 },
                    "original_text": "base2",
                    "replacement": "final2"
                }),
                rollback_summary: "可通过版本历史撤销".to_string(),
            },
        ],
        expires_at_unix_ms: i64::MAX,
    })
    .expect("freeze ordered change set");
    state
        .db
        .with_conn(|conn| {
            conn.execute(
                "UPDATE agent_run_confirmations
                 SET plan_hash = ?1, plan_json = ?2
                 WHERE confirmation_id = ?3 AND run_id = ?4 AND status = 'consumed'",
                rusqlite::params![
                    chained.plan_hash(),
                    chained.persisted_plan_json()?,
                    chained.confirmation_id(),
                    accepted.run_id,
                ],
            )?;
            conn.execute(
                "DELETE FROM agent_run_steps WHERE run_id = ?1 AND kind = 'durable_apply'",
                [&accepted.run_id],
            )?;
            Ok(())
        })
        .expect("replace test-only consumed plan");
    let running = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("running replay")
        .expect("run");
    AgentRunRepository::append_event(
        &state.db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: running.run.state_version,
            event_type: RunEventType::ToolStarted,
            payload: RunEventPayload::ToolStarted {
                capability: "replace_selection".into(),
                tool_call_id: chained.operations()[1].tool_call_id().to_string(),
            },
        },
    )
    .expect("start second frozen tool");
    AgentRunRepository::append_checkpoint_step(
        &state.db,
        AppendRunCheckpointInput {
            run_id: accepted.run_id.clone(),
            state_version: running.run.state_version,
            checkpoint: DurableApplyCheckpoint::new_change_set(
                chained.confirmation_id(),
                chained.plan_hash(),
                DurableApplyCheckpointStage::Approved,
                chained
                    .all_base_content_hashes()
                    .iter()
                    .map(|(_, hash)| hash.clone())
                    .collect(),
                chained
                    .all_expected_post_content_hashes()
                    .iter()
                    .map(|(_, hash)| hash.clone())
                    .collect(),
                0,
                2,
                Vec::new(),
            )
            .expect("approved set checkpoint"),
        },
    )
    .expect("persist set checkpoint");
    chained
}

fn assert_zero_write_and_dispatch(
    state: &AppState,
    accepted: &AssistantRunAccepted,
    directory: &tempfile::TempDir,
) {
    assert_eq!(
        std::fs::read_to_string(directory.path().join("vault/note.md"))
            .expect("read unchanged note"),
        "base"
    );
    assert_eq!(
        crate::ai_runtime::tool_audit::count_by_run(&state.db, &accepted.run_id)
            .expect("dispatch audit count"),
        0
    );
}

#[tokio::test]
async fn approved_confirmation_tamper_before_async_executor_fails_closed_without_write() {
    let (directory, state, accepted, plan) = durable_apply_fixture();
    tamper_consumed_confirmation(&state, &plan);

    execute_confirmed_change_with_sink(
        Arc::clone(&state),
        accepted.session.clone(),
        accepted.run_id.clone(),
        plan.confirmation_id().into(),
        state.vault_path().ok(),
        &NoopSink,
    )
    .await;

    let replay = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Failed);
    assert_zero_write_and_dispatch(&state, &accepted, &directory);
}

#[tokio::test]
async fn confirmed_change_worker_marks_inflight_while_emitting_failure() {
    let (_directory, state, accepted, plan) = durable_apply_fixture();
    tamper_consumed_confirmation(&state, &plan);
    let sink = InflightProbeSink {
        run_id: accepted.run_id.clone(),
        saw_inflight: std::sync::Mutex::new(false),
    };
    execute_confirmed_change_with_sink(
        Arc::clone(&state),
        accepted.session.clone(),
        accepted.run_id.clone(),
        plan.confirmation_id().into(),
        state.vault_path().ok(),
        &sink,
    )
    .await;
    assert!(
        *sink.saw_inflight.lock().expect("inflight probe"),
        "confirmation worker must hold InflightGuard for the whole execute"
    );
}

#[tokio::test]
async fn approved_confirmation_does_not_write_after_vault_switch() {
    let (directory, state, accepted, plan) = durable_apply_fixture();
    let original = directory.path().join("vault/note.md");
    let other = directory.path().join("other-vault");
    std::fs::create_dir_all(&other).expect("switched vault");
    state.set_vault(other).expect("switch vault");
    execute_confirmed_change_with_sink(
        Arc::clone(&state),
        accepted.session.clone(),
        accepted.run_id.clone(),
        plan.confirmation_id().into(),
        state.vault_path().ok(),
        &NoopSink,
    )
    .await;
    assert_eq!(
        std::fs::read_to_string(&original).expect("original vault note"),
        "base"
    );
    let replay = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Failed);
}

#[tokio::test]
async fn tamper_after_startup_classification_before_resume_executor_fails_closed() {
    let (directory, state, accepted, plan) = durable_apply_fixture();
    RunEngine::recover_interrupted_runs(&state.db).expect("startup classification");
    let paused = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("paused replay")
        .expect("run");
    assert_eq!(paused.run.state, RunState::Paused);
    assert_eq!(
        paused.run.recovery,
        Some(RunRecoveryKind::ResumeAvailable),
        "startup recovery must classify the intact fixture as resumable"
    );
    RunIntake::control(
        &state.db,
        AssistantRunControlRequest {
            session: accepted.session.clone(),
            run_id: accepted.run_id.clone(),
            expected_state_version: paused.run.state_version,
            action: RunControlAction::Resume,
        },
    )
    .expect("resume classified run");
    tamper_consumed_confirmation(&state, &plan);

    execute_confirmed_change_with_sink(
        Arc::clone(&state),
        accepted.session.clone(),
        accepted.run_id.clone(),
        plan.confirmation_id().into(),
        state.vault_path().ok(),
        &NoopSink,
    )
    .await;

    let replay = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Failed);
    assert_zero_write_and_dispatch(&state, &accepted, &directory);
}

#[tokio::test]
async fn resume_from_dispatching_checkpoint_replays_the_exact_plan_once() {
    let (directory, state, accepted, plan) = durable_apply_fixture();
    let running = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("running replay")
        .expect("run");
    AgentRunRepository::append_checkpoint_step(
        &state.db,
        AppendRunCheckpointInput {
            run_id: accepted.run_id.clone(),
            state_version: running.run.state_version,
            checkpoint: DurableApplyCheckpoint::new(
                plan.confirmation_id(),
                plan.plan_hash(),
                DurableApplyCheckpointStage::Dispatching,
                plan.all_base_content_hashes()
                    .iter()
                    .map(|(_, hash)| hash.clone())
                    .collect(),
                plan.all_expected_post_content_hashes()
                    .iter()
                    .map(|(_, hash)| hash.clone())
                    .collect(),
                Vec::new(),
            )
            .expect("dispatching checkpoint"),
        },
    )
    .expect("simulate crash after dispatching checkpoint");
    RunEngine::recover_interrupted_runs(&state.db).expect("startup classification");
    let paused = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("paused replay")
        .expect("run");
    assert_eq!(paused.run.recovery, Some(RunRecoveryKind::ResumeAvailable));
    RunIntake::control(
        &state.db,
        AssistantRunControlRequest {
            session: accepted.session.clone(),
            run_id: accepted.run_id.clone(),
            expected_state_version: paused.run.state_version,
            action: RunControlAction::Resume,
        },
    )
    .expect("resume dispatching checkpoint");

    execute_confirmed_change_with_sink(
        Arc::clone(&state),
        accepted.session.clone(),
        accepted.run_id.clone(),
        plan.confirmation_id().into(),
        state.vault_path().ok(),
        &NoopSink,
    )
    .await;

    let completed = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("completed replay")
        .expect("run");
    assert_eq!(completed.run.state, RunState::Completed);
    assert_eq!(
        std::fs::read_to_string(directory.path().join("vault/note.md")).expect("applied note"),
        "after"
    );
    assert_eq!(
        crate::ai_runtime::tool_audit::count_by_run(&state.db, &accepted.run_id)
            .expect("single dispatch audit"),
        1
    );
}

#[tokio::test]
async fn confirmed_change_set_applies_two_ordered_operations_once_and_completes_its_cursor() {
    let (directory, state, accepted, original) = durable_apply_fixture();
    std::fs::write(directory.path().join("vault/note-2.md"), "base2").expect("second base note");
    let chained = FrozenChangePlan::freeze_set(FrozenChangeSetInput {
        confirmation_id: original.confirmation_id().to_string(),
        run_id: accepted.run_id.clone(),
        session_id: original.session_id(),
        request_id: accepted.run_id.clone(),
        vault_id: original.vault_id().to_string(),
        operations: vec![
            FrozenChangeOperationInput {
                tool_call_id: original.operations()[0].tool_call_id().to_string(),
                operation: "replace_selection".to_string(),
                relative_paths: vec!["note.md".to_string()],
                base_content_hashes: vec![(
                    "note.md".to_string(),
                    crate::cas::hash::content_hash_str("base"),
                )],
                expected_post_content_hashes: vec![(
                    "note.md".to_string(),
                    crate::cas::hash::content_hash_str("after"),
                )],
                change: serde_json::json!({
                    "target_path": "note.md",
                    "base_content_hash": crate::cas::hash::content_hash_str("base"),
                    "range": { "start": 0, "end": 4 },
                    "original_text": "base",
                    "replacement": "after"
                }),
                rollback_summary: "可通过版本历史撤销".to_string(),
            },
            FrozenChangeOperationInput {
                tool_call_id: format!("{}-second", original.operations()[0].tool_call_id()),
                operation: "replace_selection".to_string(),
                relative_paths: vec!["note-2.md".to_string()],
                base_content_hashes: vec![(
                    "note-2.md".to_string(),
                    crate::cas::hash::content_hash_str("base2"),
                )],
                expected_post_content_hashes: vec![(
                    "note-2.md".to_string(),
                    crate::cas::hash::content_hash_str("final2"),
                )],
                change: serde_json::json!({
                    "target_path": "note-2.md",
                    "base_content_hash": crate::cas::hash::content_hash_str("base2"),
                    "range": { "start": 0, "end": 5 },
                    "original_text": "base2",
                    "replacement": "final2"
                }),
                rollback_summary: "可通过版本历史撤销".to_string(),
            },
        ],
        expires_at_unix_ms: i64::MAX,
    })
    .expect("freeze ordered change set");
    state
        .db
        .with_conn(|conn| {
            conn.execute(
                "UPDATE agent_run_confirmations
                 SET plan_hash = ?1, plan_json = ?2
                 WHERE confirmation_id = ?3 AND run_id = ?4 AND status = 'consumed'",
                rusqlite::params![
                    chained.plan_hash(),
                    chained.persisted_plan_json()?,
                    chained.confirmation_id(),
                    accepted.run_id,
                ],
            )?;
            conn.execute(
                "DELETE FROM agent_run_steps WHERE run_id = ?1 AND kind = 'durable_apply'",
                [&accepted.run_id],
            )?;
            Ok(())
        })
        .expect("replace test-only consumed plan");
    let running = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("running replay")
        .expect("run");
    AgentRunRepository::append_event(
        &state.db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: running.run.state_version,
            event_type: RunEventType::ToolStarted,
            payload: RunEventPayload::ToolStarted {
                capability: "replace_selection".into(),
                tool_call_id: chained.operations()[1].tool_call_id().to_string(),
            },
        },
    )
    .expect("start second frozen tool");
    AgentRunRepository::append_checkpoint_step(
        &state.db,
        AppendRunCheckpointInput {
            run_id: accepted.run_id.clone(),
            state_version: running.run.state_version,
            checkpoint: DurableApplyCheckpoint::new_change_set(
                chained.confirmation_id(),
                chained.plan_hash(),
                DurableApplyCheckpointStage::Approved,
                chained
                    .all_base_content_hashes()
                    .iter()
                    .map(|(_, hash)| hash.clone())
                    .collect(),
                chained
                    .all_expected_post_content_hashes()
                    .iter()
                    .map(|(_, hash)| hash.clone())
                    .collect(),
                0,
                2,
                Vec::new(),
            )
            .expect("approved set checkpoint"),
        },
    )
    .expect("persist set checkpoint");

    execute_confirmed_change_with_sink(
        Arc::clone(&state),
        accepted.session.clone(),
        accepted.run_id.clone(),
        chained.confirmation_id().to_string(),
        state.vault_path().ok(),
        &NoopSink,
    )
    .await;

    let completed = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("completed replay")
        .expect("run");
    assert_eq!(completed.run.state, RunState::Completed);
    assert_eq!(
        std::fs::read_to_string(directory.path().join("vault/note.md")).expect("applied note"),
        "after"
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("vault/note-2.md"))
            .expect("applied second note"),
        "final2"
    );
    let checkpoint =
        AgentRunRepository::latest_durable_apply_checkpoint(&state.db, &accepted.run_id)
            .expect("checkpoint")
            .expect("completed set checkpoint");
    assert_eq!(checkpoint.stage(), DurableApplyCheckpointStage::Completed);
    assert_eq!(checkpoint.next_operation_index(), 2);
    let assistant_content = state
        .db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT m.content FROM session_messages m
                 JOIN sessions s ON s.id = m.session_id
                 WHERE s.session_key = ?1 AND m.turn_id = ?2 AND m.role = 'assistant'",
                rusqlite::params![accepted.session.session_key, accepted.turn_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(Into::into)
        })
        .expect("completed assistant report");
    assert!(assistant_content.contains("已执行操作：replace_selection（note.md）"));
    assert!(assistant_content.contains("replace_selection（note-2.md）"));
    assert_eq!(
        crate::ai_runtime::tool_audit::count_by_run(&state.db, &accepted.run_id)
            .expect("two dispatch audits"),
        2
    );
}

#[tokio::test]
async fn confirmed_change_set_reports_partial_completion_when_second_target_drifted() {
    let (directory, state, accepted, original) = durable_apply_fixture();
    let chained = install_two_note_change_set(
        &directory,
        &state,
        &accepted,
        &original,
        "changed-by-someone-else",
    );

    execute_confirmed_change_with_sink(
        Arc::clone(&state),
        accepted.session.clone(),
        accepted.run_id.clone(),
        chained.confirmation_id().to_string(),
        state.vault_path().ok(),
        &NoopSink,
    )
    .await;

    let completed = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("completed partial report")
        .expect("run");
    assert_eq!(completed.run.state, RunState::Completed);
    assert_eq!(
        std::fs::read_to_string(directory.path().join("vault/note.md"))
            .expect("first target applied"),
        "after"
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("vault/note-2.md"))
            .expect("drifted target preserved"),
        "changed-by-someone-else"
    );
    let checkpoint =
        AgentRunRepository::latest_durable_apply_checkpoint(&state.db, &accepted.run_id)
            .expect("checkpoint")
            .expect("partial checkpoint");
    assert_eq!(checkpoint.stage(), DurableApplyCheckpointStage::Applied);
    assert_eq!(checkpoint.next_operation_index(), 1);
    let assistant_content = state
        .db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT m.content FROM session_messages m
                 JOIN sessions s ON s.id = m.session_id
                 WHERE s.session_key = ?1 AND m.turn_id = ?2 AND m.role = 'assistant'",
                rusqlite::params![accepted.session.session_key, accepted.turn_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(Into::into)
        })
        .expect("partial assistant report");
    assert!(assistant_content.contains("已执行 1/2 项变更"));
    assert_eq!(
        crate::ai_runtime::tool_audit::count_by_run(&state.db, &accepted.run_id)
            .expect("two audited operations"),
        2
    );
}

fn invoke_control(
    webview: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    request: AssistantRunControlRequest,
) -> Result<tauri::ipc::InvokeResponseBody, serde_json::Value> {
    tauri::test::get_ipc_response(
        webview,
        InvokeRequest {
            cmd: "assistant_run_control".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            // Windows/Android 的 wry workaround 使用 http://tauri.localhost，
            // 其余平台才是 tauri://localhost；用错 URL 会被判定为 remote origin 并触发 ACL 拒绝。
            url: if cfg!(any(windows, target_os = "android")) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .expect("invoke URL"),
            body: tauri::ipc::InvokeBody::Json(serde_json::json!({ "request": request })),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
    )
}

#[test]
fn production_resume_command_falls_back_safely_when_model_verification_is_unavailable() {
    let (directory, state, accepted, _plan) = durable_apply_fixture();
    let frozen_budget = AgentRunRepository::budget_policy_for_session(
        &state.db,
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .expect("read frozen budget")
    .expect("frozen budget");
    assert_eq!(frozen_budget.post_confirmation_max_model_turns, 2);
    assert_eq!(frozen_budget.post_confirmation_max_local_tool_calls, 4);
    RunEngine::recover_interrupted_runs(&state.db).expect("startup classification");
    let paused = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("paused replay")
        .expect("run");
    assert_eq!(
        paused.run.recovery,
        Some(RunRecoveryKind::ResumeAvailable),
        "startup recovery must classify the intact fixture as resumable"
    );
    let request = AssistantRunControlRequest {
        session: accepted.session.clone(),
        run_id: accepted.run_id.clone(),
        expected_state_version: paused.run.state_version,
        action: RunControlAction::Resume,
    };
    let app = tauri::test::mock_builder()
        .manage(Arc::clone(&state))
        .invoke_handler(tauri::generate_handler![assistant_run_control])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock application");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("mock webview");

    let control_result = invoke_control(&webview, request.clone());
    assert!(
        control_result.is_ok(),
        "invoke_control failed: {control_result:?}"
    );
    for _ in 0..100 {
        let completed = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
            .expect("poll replay")
            .is_some_and(|replay| replay.run.state == RunState::Completed);
        if completed {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let completed = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .expect("completed replay")
        .expect("run");
    assert_eq!(completed.run.state, RunState::Completed);
    assert_eq!(
        std::fs::read_to_string(directory.path().join("vault/note.md")).expect("applied note"),
        "after"
    );
    assert_eq!(
        AgentRunRepository::latest_durable_apply_checkpoint(&state.db, &accepted.run_id)
            .expect("latest checkpoint")
            .expect("completed checkpoint")
            .stage(),
        DurableApplyCheckpointStage::Completed
    );
    assert_eq!(
        crate::ai_runtime::tool_audit::count_by_run(&state.db, &accepted.run_id)
            .expect("single dispatch audit"),
        1
    );
    let post_confirmation_model_event_count = completed
        .events
        .iter()
        .filter(|event| {
            matches!(
                event.payload(),
                RunEventPayload::ProviderSwitched { .. }
                    | RunEventPayload::ReasoningSummary { .. }
                    | RunEventPayload::ContentDelta { .. }
            )
        })
        .count();
    assert_eq!(post_confirmation_model_event_count, 0);

    assert!(invoke_control(&webview, request).is_err());
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(
        crate::ai_runtime::tool_audit::count_by_run(&state.db, &accepted.run_id)
            .expect("dispatch count after repeated resume"),
        1
    );
}

fn review_checkpoint(
    state: &AppState,
    accepted: &AssistantRunAccepted,
    plan: &FrozenChangePlan,
    stage: DurableApplyCheckpointStage,
    next: usize,
) {
    let current = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .unwrap()
        .unwrap();
    state.db.with_conn(|conn| {
        conn.execute("UPDATE agent_run_confirmations SET plan_hash=?1, plan_json=?2 WHERE confirmation_id=?3",
            rusqlite::params![plan.plan_hash(), plan.persisted_plan_json()?, plan.confirmation_id()])?;
        conn.execute("DELETE FROM agent_run_steps WHERE run_id=?1 AND kind='durable_apply'", [&accepted.run_id])?;
        Ok(())
    }).unwrap();
    let stages = if stage == DurableApplyCheckpointStage::Applied {
        vec![
            (DurableApplyCheckpointStage::Approved, 0),
            (DurableApplyCheckpointStage::Dispatching, 0),
            (stage, next),
        ]
    } else {
        vec![(DurableApplyCheckpointStage::Approved, 0), (stage, next)]
    };
    for (stage, next) in stages {
        AgentRunRepository::append_checkpoint_step(
            &state.db,
            AppendRunCheckpointInput {
                run_id: accepted.run_id.clone(),
                state_version: current.run.state_version,
                checkpoint: DurableApplyCheckpoint::new_change_set(
                    plan.confirmation_id(),
                    plan.plan_hash(),
                    stage,
                    plan.all_base_content_hashes()
                        .into_iter()
                        .map(|(_, hash)| hash)
                        .collect(),
                    plan.all_expected_post_content_hashes()
                        .into_iter()
                        .map(|(_, hash)| hash)
                        .collect(),
                    next,
                    plan.operations().len(),
                    Vec::new(),
                )
                .unwrap(),
            },
        )
        .unwrap();
    }
}

#[test]
fn review_regression_c_recovery_compares_target_sets_without_reordering_plan() {
    let (directory, state, accepted, original) = durable_apply_fixture();
    let plan = install_two_note_change_set(&directory, &state, &accepted, &original, "base2");
    assert_eq!(plan.relative_paths(), ["note.md", "note-2.md"]);
    RunEngine::recover_interrupted_runs(&state.db).unwrap();
    let replay = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .unwrap()
        .unwrap();
    assert_eq!(replay.run.recovery, Some(RunRecoveryKind::ResumeAvailable));
    assert_eq!(plan.relative_paths(), ["note.md", "note-2.md"]);
}

#[tokio::test]
async fn review_regression_c_real_resume_rebuilds_current_prefix_context_and_writes_suffix_once() {
    for stage in [
        DurableApplyCheckpointStage::Dispatching,
        DurableApplyCheckpointStage::Applied,
    ] {
        let (directory, state, accepted, original) = durable_apply_fixture();
        let two = install_two_note_change_set(&directory, &state, &accepted, &original, "base2");
        let mut value: serde_json::Value =
            serde_json::from_str(&two.persisted_plan_json().unwrap()).unwrap();
        let second = &mut value["operations"][1];
        second["relativePaths"] = serde_json::json!(["note.md"]);
        second["baseContentHashes"] =
            serde_json::json!([["note.md", crate::cas::hash::content_hash_str("after")]]);
        second["expectedPostContentHashes"] =
            serde_json::json!([["note.md", crate::cas::hash::content_hash_str("final")]]);
        second["change"] = serde_json::json!({ "target_path":"note.md", "base_content_hash":crate::cas::hash::content_hash_str("after"),
            "range":{"start":0,"end":5}, "original_text":"after", "replacement":"final" });
        let plan = FrozenChangePlan::from_persisted_plan_json(&value.to_string()).unwrap();
        review_checkpoint(
            &state,
            &accepted,
            &plan,
            stage,
            if stage == DurableApplyCheckpointStage::Applied {
                1
            } else {
                0
            },
        );
        std::fs::write(directory.path().join("vault/note.md"), "after").unwrap();
        RunEngine::recover_interrupted_runs(&state.db).unwrap();
        let paused = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
            .unwrap()
            .unwrap();
        assert_eq!(paused.run.recovery, Some(RunRecoveryKind::ResumeAvailable));
        RunIntake::control(
            &state.db,
            AssistantRunControlRequest {
                session: accepted.session.clone(),
                run_id: accepted.run_id.clone(),
                expected_state_version: paused.run.state_version,
                action: RunControlAction::Resume,
            },
        )
        .unwrap();
        // This is the production worker, with no retained pre-write context.
        execute_confirmed_change_with_sink(
            Arc::clone(&state),
            accepted.session.clone(),
            accepted.run_id.clone(),
            plan.confirmation_id().into(),
            state.vault_path().ok(),
            &NoopSink,
        )
        .await;
        let complete = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
            .unwrap()
            .unwrap();
        assert_eq!(complete.run.state, RunState::Completed, "{stage:?}");
        let completed_calls = complete
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event.payload(),
                    RunEventPayload::ToolCompleted {
                        success: Some(true),
                        ..
                    }
                )
            })
            .count();
        assert_eq!(
            completed_calls, 2,
            "every recovered prefix and suffix call has a receipt: {stage:?}"
        );
        assert_eq!(
            std::fs::read_to_string(directory.path().join("vault/note.md")).unwrap(),
            "final"
        );
        assert_eq!(
            crate::version::version_list(&state, "note.md")
                .unwrap()
                .len(),
            1
        );
        execute_confirmed_change_with_sink(
            Arc::clone(&state),
            accepted.session.clone(),
            accepted.run_id.clone(),
            plan.confirmation_id().into(),
            state.vault_path().ok(),
            &NoopSink,
        )
        .await;
        assert_eq!(
            crate::version::version_list(&state, "note.md")
                .unwrap()
                .len(),
            1
        );
    }
}

#[tokio::test]
async fn review_regression_c_hash_override_cannot_reuse_selection_offsets() {
    let (directory, state, accepted, plan) = durable_apply_fixture();
    state.db.with_conn(|conn| {
        let raw: String = conn.query_row("SELECT explicit_references_json FROM session_messages WHERE turn_id=?1 AND role='user'", [&accepted.turn_id], |row| row.get(0))?;
        let mut refs: serde_json::Value = serde_json::from_str(&raw)?;
        refs[0]["kind"] = serde_json::json!("selection");
        refs[0]["utf8Range"] = serde_json::json!({"start":0,"end":4});
        conn.execute("UPDATE session_messages SET explicit_references_json=?1 WHERE turn_id=?2 AND role='user'", rusqlite::params![refs.to_string(), accepted.turn_id])?;
        Ok(())
    }).unwrap();
    std::fs::write(directory.path().join("vault/note.md"), "after").unwrap();
    let context =
        crate::ai_runtime::run_context::RunContextAssembler::assemble_with_expected_hashes(
            &state.db,
            state.vault_path().ok().as_deref(),
            &accepted.session.session_key,
            &accepted.run_id,
            &[(
                "note.md".into(),
                crate::cas::hash::content_hash_str("after"),
            )],
        );
    assert!(
        context.is_err(),
        "a new hash does not authorize old selection offsets"
    );
    let mapped =
        crate::ai_runtime::run_context::RunContextAssembler::assemble_at_confirmed_boundary(
            &state.db,
            state.vault_path().ok().as_deref(),
            &accepted.session.session_key,
            &accepted.run_id,
            &[(
                "note.md".into(),
                crate::cas::hash::content_hash_str("after"),
            )],
            plan.operations(),
        )
        .unwrap();
    assert_eq!(mapped.materials[0].content, "after");
    assert_eq!(mapped.materials[0].source_span_end, 5);
    let verification =
        crate::ai_runtime::normal_run_service::execute_post_confirmation_verification(
            Arc::clone(&state),
            accepted.clone(),
            state.vault_path().ok(),
            &plan,
            "已执行变更",
            &NoopSink,
        )
        .await;
    assert_eq!(
        verification.unwrap_err().to_string(),
        "post_confirmation_verification_scope_unavailable"
    );
}

fn invoke_session_retract(
    webview: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    session_key: &str,
    from_seq: i64,
) -> Result<tauri::ipc::InvokeResponseBody, serde_json::Value> {
    tauri::test::get_ipc_response(
        webview,
        InvokeRequest {
            cmd: "assistant_session_retract".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            // Windows/Android 的 wry workaround 使用 http://tauri.localhost，
            // 其余平台才是 tauri://localhost；用错 URL 会被判定为 remote origin 并触发 ACL 拒绝。
            url: if cfg!(any(windows, target_os = "android")) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .expect("invoke URL"),
            body: tauri::ipc::InvokeBody::Json(serde_json::json!({
                "request": {
                    "session": { "domain": "normal", "sessionKey": session_key },
                    "fromSeq": from_seq,
                }
            })),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
    )
}

// N23 evidence probe: the retract command reports a delete count and never
// crosses into RunIntake::start / spawn_normal_direct_run. RunIntake::start
// would insert an 'accepted' agent_runs row, an Accepted event and a user
// message; all three staying untouched pins that the retract path cannot
// jump onto the run-start path.
#[test]
fn n23_assistant_session_retract_returns_the_delete_count_and_never_starts_a_run() {
    let directory = tempfile::tempdir().expect("temporary app directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    let session =
        crate::ai_runtime::normal_session_repository::NormalSessionRepository::create(&state.db)
            .expect("session");
    state
        .db
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO agent_runs
                 (run_id, client_request_id, session_id, turn_id, status, state_version,
                  effect, effort, security_domain, risk, envelope_json, goal_summary,
                  created_at, updated_at)
                 VALUES (?1, ?1, ?2, ?1, 'completed', 0, 'answer', 'direct', 'normal', 'read_only',
                         '{}', '', '2020-01-01', '2020-01-01')",
                rusqlite::params![session.session_key, session.session_id],
            )?;
            conn.execute(
                "INSERT INTO session_messages (session_id, seq, role, content, turn_id, created_at)
                 VALUES (?1, 1, 'user', 'public fixture', ?2, '2020-01-01')",
                rusqlite::params![session.session_id, session.session_key],
            )?;
            conn.execute(
                "INSERT INTO session_messages (session_id, seq, role, content, turn_id, created_at)
                 VALUES (?1, 2, 'assistant', 'published answer', ?2, '2020-01-01')",
                rusqlite::params![session.session_id, session.session_key],
            )?;
            conn.execute(
                "INSERT INTO agent_run_events
                 (run_id, event_seq, state_version, event_type, payload_json, created_at)
                 VALUES (?1, 1, 0, 'completed', '{}', '2020-01-01')",
                rusqlite::params![session.session_key],
            )?;
            Ok(())
        })
        .expect("seed completed run with a published answer");

    let app = tauri::test::mock_builder()
        .manage(Arc::clone(&state))
        .invoke_handler(tauri::generate_handler![assistant_session_retract])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock application");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("mock webview");

    let response = invoke_session_retract(&webview, &session.session_key, 2)
        .expect("assistant_session_retract invoke");
    let deleted: u32 = response.deserialize().expect("delete-count payload");
    assert_eq!(
        deleted, 1,
        "retract reports its delete count, never a run acceptance"
    );

    state
        .db
        .with_read_conn(|conn| {
            let run_rows: i64 = conn.query_row(
                "SELECT COUNT(*) FROM agent_runs WHERE session_id = ?1",
                [session.session_id],
                |row| row.get(0),
            )?;
            assert_eq!(run_rows, 1, "retract must not accept or spawn a run");
            let completed: i64 = conn.query_row(
                "SELECT COUNT(*) FROM agent_runs WHERE session_id = ?1 AND status = 'completed'",
                [session.session_id],
                |row| row.get(0),
            )?;
            assert_eq!(completed, 1, "the terminal run keeps its status");
            let messages: i64 = conn.query_row(
                "SELECT COUNT(*) FROM session_messages WHERE session_id = ?1",
                [session.session_id],
                |row| row.get(0),
            )?;
            assert_eq!(
                messages, 1,
                "only the suffix disappears; RunIntake::start would append a user row"
            );
            let events: i64 = conn.query_row(
                "SELECT COUNT(*) FROM agent_run_events WHERE run_id = ?1",
                rusqlite::params![session.session_key],
                |row| row.get(0),
            )?;
            assert_eq!(events, 1, "no new run event is written");
            Ok(())
        })
        .expect("post-retract ledger");
}
