//! Production-entry dual-path V04: `execute_web_tool` → broker → recorded native
//! transport + contract MCP. Not V05 live evidence.

use serde_json::{json, Value};
use std::sync::Mutex;

use super::agent_run_repository::{AgentRunRepository, AppendRunEventInput};
use super::agent_tool_loop::ToolLoopExecutor;
use super::mcp_runtime_registry::{self, WebEvidenceProviderInput, WebSearchRouteConfig};
use super::native_search_adapter::{
    install_recorded_native_search_transport, NativeSearchHttpResult,
};
use super::native_search_subrequest::NativeSearchEndpointRef;
use super::run_context::RunContextAssembler;
use super::run_contract::{
    AssistantRunEvent, AssistantRunStartRequest, AssistantTurnDraft, CapabilityId, RunBudgetPolicy,
    RunEventPayload, RunEventType, RunState, SecurityDomain,
};
use super::run_engine::{RunEngine, RunEventSink};
use super::run_intake::RunIntake;
use super::run_tool_loop::NormalRunToolExecutor;
use crate::ai_runtime::ToolCall;
use crate::ai_types::EndpointFamily;
use crate::app::AppState;
use crate::error::AppResult;
use crate::storage::db::Database;

const SHARED_HTTPS: &str = "https://source.invalid/contract";
const FAKE_TOKEN: &str = "v04-test-token";

#[derive(Default)]
struct RecordingSink {
    events: Mutex<Vec<Value>>,
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

fn request() -> AssistantRunStartRequest {
    AssistantRunStartRequest {
        client_request_id: "v04-client".into(),
        session: None,
        turn: AssistantTurnDraft {
            message: "请联网核实".into(),
            content_parts: None,
            explicit_references: vec![],
            retrieval_scope: Default::default(),
            display_mentions: vec![],
        },
        explicit_action: None,
        web_enabled: true,
        model_override: None,
        external_tool_grants: Vec::new(),
        security_domain: SecurityDomain::Normal,
        classified_context_ref: None,
    }
}

fn begin(
    state: &AppState,
    accepted: &super::run_contract::AssistantRunAccepted,
    sink: &RecordingSink,
) {
    let version =
        RunEngine::mark_preparing_with_sink(&state.db, &accepted.session, &accepted.run_id, sink)
            .unwrap();
    AgentRunRepository::append_event(
        &state.db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: version,
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Running,
                stage: "v04 fixture".into(),
                stage_code: None,
            },
        },
    )
    .unwrap();
}

fn write_setting(db: &Database, key: &str, value: &Value) {
    let json = serde_json::to_string(value).expect("setting json");
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, json],
        )?;
        Ok(())
    })
    .expect("write setting");
}

fn upsert_mcp(db: &Database, id: &str, mode: &str) {
    mcp_runtime_registry::upsert_web_evidence_provider(
        db,
        &WebEvidenceProviderInput {
            id: id.into(),
            name: id.into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json:
                super::mcp_stdio_test_support::contract_mcp_stdio_transport_config(mode, "1")
                    .to_string(),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: Some(
                r#"{"tool":"search","queryArg":"query","maxResultsArg":"max_results"}"#.into(),
            ),
            web_fetch_mapping_json: None,
        },
    )
    .expect("upsert mcp");
}

fn freeze_route(
    db: &Database,
    ids: &[&str],
) -> Vec<mcp_runtime_registry::WebEvidenceProviderMappingSummary> {
    mcp_runtime_registry::save_web_search_route_config(
        db,
        &WebSearchRouteConfig {
            candidate_provider_ids: ids.iter().map(|id| (*id).to_string()).collect(),
        },
    )
    .expect("save route");
    mcp_runtime_registry::resolve_web_search_provider_route(db).expect("freeze route")
}

fn minimax_endpoint() -> NativeSearchEndpointRef {
    let mut endpoint = NativeSearchEndpointRef::new(
        "MiniMax-M3",
        EndpointFamily::OpenAiCompatibleChatCompletions,
    );
    endpoint.api_base = Some("https://api.minimaxi.com/v1".into());
    endpoint.credential_service = Some("iris.llm.minimax".into());
    endpoint
}

fn glm_endpoint() -> NativeSearchEndpointRef {
    NativeSearchEndpointRef::new("glm-4.5", EndpointFamily::OpenAiCompatibleChatCompletions)
}

fn recorded_minimax_body() -> Value {
    json!({
        "id": "resp-v04",
        "object": "response",
        "status": "completed",
        "model": "MiniMax-M3",
        "usage": { "input_tokens": 11, "output_tokens": 7 },
        "output": [
            {
                "type": "web_search_call",
                "status": "completed",
                "action": { "type": "search", "query": "contract" }
            },
            {
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "summary",
                    "annotations": [{
                        "type": "url_citation",
                        "title": "Contract",
                        "url": SHARED_HTTPS
                    }]
                }]
            }
        ]
    })
}

fn recorded_text_only_body() -> Value {
    json!({
        "id": "resp-v04-text",
        "status": "completed",
        "output": [{
            "type": "message",
            "content": [{ "type": "output_text", "text": "no citations" }]
        }]
    })
}

fn ok_http(body: Value) -> NativeSearchHttpResult {
    NativeSearchHttpResult {
        status: 200,
        body,
        transport_failed: false,
    }
}

fn failed_http() -> NativeSearchHttpResult {
    NativeSearchHttpResult {
        status: 500,
        body: json!({ "error": "upstream" }),
        transport_failed: false,
    }
}

struct V04Harness {
    _budget: super::model_turn_ledger::BindGuard,
    _directory: tempfile::TempDir,
    state: std::sync::Arc<AppState>,
    accepted: super::run_contract::AssistantRunAccepted,
    context: super::run_context::RunContext,
    sink: RecordingSink,
}

impl V04Harness {
    fn new() -> Self {
        Self::with_request(request())
    }

    fn with_request(input: AssistantRunStartRequest) -> Self {
        let directory = tempfile::tempdir().expect("tempdir");
        let data_dir = directory.path().join("data");
        let config_dir = directory.path().join("config");
        std::fs::create_dir_all(&config_dir).expect("config dir");
        std::env::set_var("IRIS_DATA_DIR", &data_dir);
        std::env::set_var("IRIS_CONFIG_DIR", &config_dir);
        let state = AppState::new(data_dir).expect("app state");
        crate::credentials::set_api_key("iris.llm.minimax", FAKE_TOKEN).expect("fake token");
        write_setting(&state.db, "web_search_enabled", &json!(true));
        let accepted = RunIntake::start(&state.db, input).expect("accept");
        let context = RunContextAssembler::assemble(
            &state.db,
            None,
            &accepted.session.session_key,
            &accepted.run_id,
        )
        .expect("context");
        let sink = RecordingSink::default();
        super::agent_run_repository::AgentRunRepository::persist_authorization_snapshot(
            &state.db,
            &accepted.session.session_key,
            &accepted.run_id,
            &[CapabilityId::new("web.search")],
        )
        .unwrap();
        let budget = super::model_turn_ledger::BindGuard::persisted(
            &state.db,
            &accepted.run_id,
            super::model_turn_ledger::BudgetPhase::Main,
            &RunBudgetPolicy::for_envelope(&context.envelope),
        )
        .unwrap();
        begin(&state, &accepted, &sink);
        Self {
            _budget: budget,
            _directory: directory,
            state,
            accepted,
            context,
            sink,
        }
    }

    fn executor(
        &self,
        snapshots: Vec<mcp_runtime_registry::WebEvidenceProviderMappingSummary>,
        endpoint: Option<NativeSearchEndpointRef>,
    ) -> NormalRunToolExecutor<'_> {
        NormalRunToolExecutor::new(
            &self.state,
            None,
            &self.accepted,
            &self.context,
            vec![CapabilityId::new("web.search")],
            RunBudgetPolicy::for_envelope(&self.context.envelope),
            &self.sink,
            snapshots,
        )
        .with_allowed_tool_names(&["web_search".into(), "web_fetch".into()])
        .with_native_search_endpoint(endpoint)
    }
}

fn dispatch_search_ctx<'a>(
    harness: &'a V04Harness,
    retrieval_scope: &'a crate::ai_runtime::retrieval_scope::RetrievalScope,
    action: &'a super::web_evidence_broker::WebEvidenceBrokerInput,
) -> crate::ai_runtime::tool_dispatch::ToolDispatchContext<'a> {
    let mut ctx = crate::ai_runtime::tool_dispatch::ToolDispatchContext::for_tests(retrieval_scope);
    ctx.db = Some(&harness.state.db);
    ctx.run_id = Some(&harness.accepted.run_id);
    ctx.web_search_enabled = true;
    ctx.max_web_fetches = 5;
    ctx.web_action = Some(action);
    ctx
}

fn dispatch_search_action(
    harness: &V04Harness,
    endpoint: Option<NativeSearchEndpointRef>,
) -> super::web_evidence_broker::WebEvidenceBrokerInput {
    super::web_evidence_broker::WebEvidenceBrokerInput {
        query: "contract".into(),
        urls: Vec::new(),
        enabled: true,
        max_search_results: 5,
        max_fetches: 0,
        provider_snapshots: mcp_runtime_registry::resolve_web_search_provider_route(
            &harness.state.db,
        )
        .unwrap(),
        provider_selection_frozen: true,
        search_identity: super::dual_path_search::SearchActionIdentity {
            run_id: harness.accepted.run_id.clone(),
            input_revision: harness.accepted.turn_id.clone(),
            action_id: "v04-secondary-action".into(),
            tool_surface_version: "fixture-surface".into(),
            attempt: 1,
            ..Default::default()
        },
        search_deadline: Some(
            tokio::time::Instant::now()
                + super::native_search_adapter::web_search_call_deadline(endpoint.as_ref()),
        ),
        web_revocation_epoch: super::model_gateway::web_revocation_epoch(),
        native_endpoint: endpoint,
    }
}

fn seed_page(db: &Database, url: &str, body: &str) {
    use crate::llm::fetch_web_page::{PageFetchCacheScope, PAGE_FETCH_CACHE_BROKER_VERSION};
    use sha2::{Digest, Sha256};
    let scope = PageFetchCacheScope::native(None, PAGE_FETCH_CACHE_BROKER_VERSION);
    let mut hash = Sha256::new();
    for part in [
        "default",
        &scope.provider_id,
        &scope.provider_kind,
        &scope.provider_config_hash,
        &scope.broker_version,
    ] {
        hash.update(part.as_bytes());
        hash.update(b"\0");
    }
    hash.update(url.as_bytes());
    let key = hex::encode(hash.finalize());
    db.with_conn(|conn| {
        conn.execute(
            "INSERT OR REPLACE INTO web_page_cache
            (url_hash,title,body_text,fetched_at,expires_at,provider_id,provider_kind,provider_config_hash,broker_version)
            VALUES (?1,'Fixture',?2,datetime('now'),datetime('now','+1 day'),?3,?4,?5,?6)",
            rusqlite::params![
                key,
                body,
                scope.provider_id,
                scope.provider_kind,
                scope.provider_config_hash,
                scope.broker_version
            ],
        )?;
        Ok(())
    })
    .unwrap();
}

fn search_call(id: &str) -> ToolCall {
    ToolCall::new(id, "web_search", json!({ "query": "contract" }).to_string())
}

fn install_native(
    run_id: &str,
    results: Vec<NativeSearchHttpResult>,
) -> super::native_search_adapter::RecordedNativeSearchTransportGuard {
    install_recorded_native_search_transport(run_id, results, FAKE_TOKEN)
}

fn assert_no_degradation(payload: &Value) {
    let encoded = payload.to_string();
    assert!(!encoded.contains("能力降级"), "{encoded}");
}

#[tokio::test]
async fn v04_native_plus_single_mcp_merges_shared_https_url() {
    let harness = V04Harness::new();
    upsert_mcp(&harness.state.db, "v04-mcp", "search-only");
    let snapshots = freeze_route(&harness.state.db, &["v04-mcp"]);
    let recorded = install_native(
        &harness.accepted.run_id,
        vec![ok_http(recorded_minimax_body())],
    );
    let executor = harness.executor(snapshots, Some(minimax_endpoint()));
    let result = executor
        .execute(&harness.accepted.run_id, &search_call("v04-both"), 1)
        .await
        .expect("execute");
    assert!(
        result.success,
        "{:?} output={} calls={}",
        result.error,
        result.output,
        recorded.call_count()
    );
    let dual = &result.output["dualPath"];
    assert_eq!(dual["native"]["attempted"], true);
    assert_eq!(dual["native"]["succeeded"], true);
    assert_eq!(dual["native"]["hasRetrievalCredentials"], true);
    assert_eq!(dual["mcp"]["attempted"], true);
    assert_eq!(dual["mcp"]["succeeded"], true);
    assert_eq!(dual["bothAvailableRoutesAttempted"], true);
    assert_eq!(dual["usage"]["nativePromptTokens"], 11);
    assert_eq!(dual["usage"]["nativeCompletionTokens"], 7);
    let merged = dual["candidates"]
        .as_array()
        .expect("candidates")
        .iter()
        .find(|candidate| candidate["url"] == SHARED_HTTPS)
        .expect("shared url");
    let channels = merged["channels"].as_array().expect("channels");
    assert!(channels.iter().any(|channel| channel == "native"));
    assert!(channels.iter().any(|channel| channel == "mcp"));
    assert_no_degradation(&result.output);
    assert!(recorded.authorization_present());
    assert!(recorded
        .captured_urls()
        .iter()
        .all(|url| url.starts_with("https://")));
    let events = super::boundary_events::query_by_run(&harness.state.db, &harness.accepted.run_id)
        .expect("c26");
    let witness = events
        .iter()
        .find(|event| {
            event.payload.get("kind").and_then(Value::as_str) == Some("native_search_subrequest")
        })
        .expect("native witness");
    assert_eq!(witness.run_id, harness.accepted.run_id);
    // C26 correlates to the persisted tool-call id (SearchActionIdentity.action_id),
    // not the catalog tool name.
    assert_eq!(witness.call_id, "v04-both");
    assert_eq!(witness.payload["isNetworkToolDispatch"], false);
    assert_eq!(witness.payload["hasRetrievalCredentials"], true);
    let encoded = witness.payload.to_string();
    assert!(!encoded.contains("https://"));
    assert!(!encoded.contains(FAKE_TOKEN));
    assert!(!encoded.contains("sk-"));
}

#[tokio::test]
async fn v04_unadapted_catalog_model_keeps_mcp_and_skips_native() {
    let harness = V04Harness::new();
    upsert_mcp(&harness.state.db, "v04-mcp-only", "search-only");
    let snapshots = freeze_route(&harness.state.db, &["v04-mcp-only"]);
    let recorded = install_native(
        &harness.accepted.run_id,
        vec![ok_http(recorded_minimax_body())],
    );
    let executor = harness.executor(snapshots, Some(glm_endpoint()));
    let result = executor
        .execute(&harness.accepted.run_id, &search_call("v04-mcp-only"), 1)
        .await
        .expect("execute");
    assert!(result.success, "{:?}", result.error);
    let dual = &result.output["dualPath"];
    assert_eq!(dual["native"]["supported"], false);
    assert_eq!(dual["native"]["attempted"], false);
    assert_eq!(dual["mcp"]["succeeded"], true);
    assert_eq!(dual["bothAvailableRoutesAttempted"], false);
    assert_eq!(recorded.call_count(), 0);
    assert_no_degradation(&result.output);
    let encoded = result.output.to_string();
    assert!(
        !encoded.contains("能力降级") && dual["native"]["unsupportedReason"] == "adapter_absent",
        "{encoded}"
    );
}

#[tokio::test]
async fn v04_mcp_primary_backup_is_not_dual_path() {
    let harness = V04Harness::new();
    upsert_mcp(&harness.state.db, "v04-mcp-primary", "search-empty");
    upsert_mcp(&harness.state.db, "v04-mcp-backup", "search-only");
    let snapshots = freeze_route(&harness.state.db, &["v04-mcp-primary", "v04-mcp-backup"]);
    let executor = harness.executor(snapshots, None);
    let result = executor
        .execute(&harness.accepted.run_id, &search_call("v04-failover"), 1)
        .await
        .expect("execute");
    assert!(result.success, "{:?}", result.error);
    let dual = &result.output["dualPath"];
    assert_eq!(dual["native"]["attempted"], false);
    assert_eq!(dual["mcp"]["succeeded"], true);
    assert_eq!(dual["mcpInternalProviderAttempts"], 2);
    assert_eq!(dual["bothAvailableRoutesAttempted"], false);
}

#[tokio::test]
async fn v04_alternating_route_entries_freeze_library_order() {
    let harness = V04Harness::new();
    upsert_mcp(&harness.state.db, "v04-route-a", "search-only");
    upsert_mcp(&harness.state.db, "v04-route-b", "search-only");
    mcp_runtime_registry::save_web_search_route_config(
        &harness.state.db,
        &WebSearchRouteConfig {
            candidate_provider_ids: vec!["v04-route-a".into(), "v04-route-b".into()],
        },
    )
    .expect("set first");
    mcp_runtime_registry::promote_web_search_route_primary(&harness.state.db, "v04-route-b")
        .expect("promote");
    mcp_runtime_registry::save_web_search_route_config(
        &harness.state.db,
        &WebSearchRouteConfig {
            candidate_provider_ids: vec!["v04-route-a".into(), "v04-route-b".into()],
        },
    )
    .expect("set after promote");
    let snapshots = mcp_runtime_registry::resolve_web_search_provider_route(&harness.state.db)
        .expect("freeze after alternating writes");
    let authority =
        mcp_runtime_registry::get_web_search_route_config(&harness.state.db).expect("authority");
    assert_eq!(
        snapshots
            .iter()
            .map(|snapshot| snapshot.id.as_str())
            .collect::<Vec<_>>(),
        authority
            .candidate_provider_ids
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    let executor = harness.executor(snapshots, None);
    let result = executor
        .execute(&harness.accepted.run_id, &search_call("v04-route"), 1)
        .await
        .expect("execute");
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["dualPath"]["mcp"]["succeeded"], true);
}

#[tokio::test]
async fn v04_mid_run_web_search_switch_off_denies_new_search() {
    let harness = V04Harness::new();
    upsert_mcp(&harness.state.db, "v04-k09", "search-only");
    let snapshots = freeze_route(&harness.state.db, &["v04-k09"]);
    let recorded = install_native(
        &harness.accepted.run_id,
        vec![
            ok_http(recorded_minimax_body()),
            ok_http(recorded_minimax_body()),
        ],
    );
    let executor = harness.executor(snapshots, Some(minimax_endpoint()));
    let first = executor
        .execute(&harness.accepted.run_id, &search_call("v04-k09-first"), 1)
        .await
        .expect("first search");
    assert!(
        first.success,
        "{:?} output={} calls={}",
        first.error,
        first.output,
        recorded.call_count()
    );
    assert_eq!(first.output["dualPath"]["native"]["succeeded"], true);
    let calls_after_first = recorded.call_count();
    assert!(calls_after_first >= 1);
    write_setting(&harness.state.db, "web_search_enabled", &json!(false));
    let second = executor
        .execute(&harness.accepted.run_id, &search_call("v04-k09-second"), 2)
        .await
        .expect("second search");
    assert!(!second.success, "closed switch must deny a new search");
    assert_eq!(
        second.error.as_deref(),
        Some("agent_run_permission_denied"),
        "{:?}",
        second.output
    );
    assert!(
        second.output.get("dualPath").is_none(),
        "denied search must not emit a dual-path observation: {}",
        second.output
    );
    assert_eq!(recorded.call_count(), calls_after_first);
    assert_eq!(first.output["dualPath"]["native"]["succeeded"], true);
}

#[tokio::test]
async fn v04_native_protocol_insufficient_keeps_mcp() {
    let harness = V04Harness::new();
    upsert_mcp(&harness.state.db, "v04-keep-mcp", "search-only");
    let snapshots = freeze_route(&harness.state.db, &["v04-keep-mcp"]);
    let _recorded = install_native(
        &harness.accepted.run_id,
        vec![
            ok_http(recorded_text_only_body()),
            ok_http(recorded_text_only_body()),
        ],
    );
    let executor = harness.executor(snapshots, Some(minimax_endpoint()));
    let result = executor
        .execute(
            &harness.accepted.run_id,
            &search_call("v04-native-short"),
            1,
        )
        .await
        .expect("execute");
    assert!(result.success, "{:?}", result.error);
    let dual = &result.output["dualPath"];
    assert_eq!(dual["native"]["attempted"], true);
    assert_eq!(dual["native"]["succeeded"], false);
    assert_eq!(dual["mcp"]["succeeded"], true);
}

#[tokio::test]
async fn v04_mcp_failure_keeps_native_credentials() {
    let harness = V04Harness::new();
    upsert_mcp(&harness.state.db, "v04-keep-native", "search-empty");
    let snapshots = freeze_route(&harness.state.db, &["v04-keep-native"]);
    let _recorded = install_native(
        &harness.accepted.run_id,
        vec![ok_http(recorded_minimax_body())],
    );
    let executor = harness.executor(snapshots, Some(minimax_endpoint()));
    let result = executor
        .execute(&harness.accepted.run_id, &search_call("v04-mcp-short"), 1)
        .await
        .expect("execute");
    assert!(result.success, "{:?}", result.error);
    let dual = &result.output["dualPath"];
    assert_eq!(dual["native"]["succeeded"], true);
    assert_eq!(dual["native"]["hasRetrievalCredentials"], true);
    assert_eq!(dual["mcp"]["succeeded"], false);
}

#[tokio::test]
async fn v04_both_routes_failed_is_not_forged_success() {
    let harness = V04Harness::new();
    upsert_mcp(&harness.state.db, "v04-both-fail", "search-empty");
    let snapshots = freeze_route(&harness.state.db, &["v04-both-fail"]);
    let recorded = install_native(&harness.accepted.run_id, vec![failed_http()]);
    let executor = harness.executor(snapshots, Some(minimax_endpoint()));
    let result = executor
        .execute(&harness.accepted.run_id, &search_call("v04-both-fail"), 1)
        .await
        .expect("execute");
    assert!(!result.success);
    assert!(recorded.call_count() >= 1);
    let events = super::boundary_events::query_by_run(&harness.state.db, &harness.accepted.run_id)
        .expect("c26");
    let witness = events
        .iter()
        .find(|event| {
            event.payload.get("kind").and_then(Value::as_str) == Some("native_search_subrequest")
        })
        .expect("native witness");
    assert_eq!(witness.payload["hasRetrievalCredentials"], false);
    assert_ne!(witness.payload["statusClass"], "2xx");
}

#[tokio::test]
async fn v04_dispatch_tool_web_search_attempts_native_when_endpoint_is_frozen() {
    let harness = V04Harness::new();
    upsert_mcp(&harness.state.db, "v04-dispatch-native", "search-only");
    let _snapshots = freeze_route(&harness.state.db, &["v04-dispatch-native"]);
    let recorded = install_native(
        &harness.accepted.run_id,
        vec![ok_http(recorded_minimax_body())],
    );
    let retrieval_scope = crate::ai_runtime::retrieval_scope::RetrievalScope::default();
    let action = dispatch_search_action(&harness, Some(minimax_endpoint()));
    let ctx = dispatch_search_ctx(&harness, &retrieval_scope, &action);
    let result = crate::ai_runtime::tool_dispatch::dispatch_tool(
        harness.state.as_ref(),
        &ctx,
        "web_search",
        &json!({ "query": "contract" }),
    )
    .await;
    assert!(
        result.success,
        "{:?} output={}",
        result.error, result.output
    );
    let dual = &result.output["dualPath"];
    assert_eq!(dual["native"]["attempted"], true);
    assert_eq!(dual["mcp"]["attempted"], true);
    assert!(recorded.call_count() >= 1);
    assert_no_degradation(&result.output);
}

#[tokio::test]
async fn v04_dispatch_tool_web_search_skips_native_when_endpoint_is_absent() {
    let harness = V04Harness::new();
    upsert_mcp(&harness.state.db, "v04-dispatch-absent", "search-only");
    let _snapshots = freeze_route(&harness.state.db, &["v04-dispatch-absent"]);
    let recorded = install_native(
        &harness.accepted.run_id,
        vec![ok_http(recorded_minimax_body())],
    );
    let retrieval_scope = crate::ai_runtime::retrieval_scope::RetrievalScope::default();
    let action = dispatch_search_action(&harness, None);
    let ctx = dispatch_search_ctx(&harness, &retrieval_scope, &action);
    let result = crate::ai_runtime::tool_dispatch::dispatch_tool(
        harness.state.as_ref(),
        &ctx,
        "web_search",
        &json!({ "query": "contract" }),
    )
    .await;
    assert!(
        result.success,
        "{:?} output={}",
        result.error, result.output
    );
    let dual = &result.output["dualPath"];
    assert_eq!(dual["native"]["attempted"], false);
    assert_eq!(dual["mcp"]["attempted"], true);
    assert_eq!(recorded.call_count(), 0);
    assert_no_degradation(&result.output);
}

#[tokio::test]
async fn v04_web_fetch_keeps_successful_body_when_one_url_fails() {
    let harness = V04Harness::new();
    let ok_url = "https://example.com/v04-fetch-ok";
    // Public HTTPS with an empty cached body: K13 empty, not a private-host
    // reject, and no live TLS. The reading fixture's mixed batch uses startChar
    // and therefore never fetches the missing URL.
    let fail_url = "https://example.com/v04-fetch-empty";
    const BODY: &str = "FETCH_BODY_NOT_SNIPPET unique-v04-body";
    seed_page(&harness.state.db, ok_url, BODY);
    seed_page(&harness.state.db, fail_url, "   ");
    let executor = harness.executor(Vec::new(), None);
    let result = executor
        .execute(
            &harness.accepted.run_id,
            &ToolCall::new(
                "v04-fetch-partial",
                "web_fetch",
                json!({ "urls": [ok_url, fail_url] }).to_string(),
            ),
            1,
        )
        .await
        .expect("execute");
    assert!(
        result.success,
        "partial fetch must not fail the whole batch: {:?} output={}",
        result.error, result.output
    );
    assert_eq!(result.output["count"], 1);
    assert_eq!(result.output["failedUrls"], json!([fail_url]));
    assert_eq!(result.output["observationDepth"], "fetched_body");
    let results = result.output["results"].as_array().expect("results");
    let success = results
        .iter()
        .find(|item| item["canonicalUrl"] == ok_url)
        .expect("successful page");
    let excerpt = success["excerpt"].as_str().expect("excerpt");
    assert_eq!(excerpt, BODY);
    assert_ne!(excerpt, "Fixture");
    let method = success
        .pointer("/web/extraction_method")
        .and_then(Value::as_str)
        .unwrap_or("");
    assert_ne!(method, "search_snippet", "{success}");
    let failed = results
        .iter()
        .find(|item| item["canonicalUrl"] == fail_url)
        .expect("failed url status");
    assert_eq!(failed["status"], "snapshot_unavailable");
    assert!(failed.get("excerpt").and_then(Value::as_str).is_none());
}

#[tokio::test]
async fn review_regression_d_production_native_in_flight_cancel_retains_mcp_and_unknown_attempt() {
    let harness = V04Harness::new();
    upsert_mcp(&harness.state.db, "review-cancel", "search-only");
    let snapshots = freeze_route(&harness.state.db, &["review-cancel"]);
    let recorded = install_native(
        &harness.accepted.run_id,
        vec![ok_http(recorded_minimax_body())],
    );
    recorded.block_response();
    let executor = harness.executor(snapshots, Some(minimax_endpoint()));
    let call = search_call("review-cancel-native");
    let work = executor.execute(&harness.accepted.run_id, &call, 1);
    tokio::pin!(work);
    let result = tokio::select! {
        result = &mut work => result,
        _ = recorded.wait_until_dispatched() => {
            super::model_gateway::request_abort(&harness.accepted.run_id);
            work.await
        },
    };
    super::model_gateway::clear_abort(&harness.accepted.run_id);
    let result = result.expect("tool result records completed MCP evidence");
    assert_ne!(result.output["dualPath"]["native"]["succeeded"], true);
    assert_eq!(recorded.call_count(), 1, "{}", result.output);
    assert_eq!(super::model_turn_ledger::used(&harness.accepted.run_id), 1);
    let (snapshot, _) = super::agent_run_repository::AgentRunRepository::model_budget_snapshot(
        &harness.state.db,
        &harness.accepted.run_id,
    )
    .unwrap();
    let snapshot = snapshot.unwrap();
    assert_eq!(snapshot["attempts"][0]["usage_known"], false);
    assert_eq!(snapshot["attempts"][0]["dispatched"], true);
}

#[tokio::test]
async fn review_regression_d_production_revocation_before_retry_does_not_revive_after_reenable() {
    let harness = V04Harness::new();
    upsert_mcp(&harness.state.db, "review-revoke", "search-only");
    let snapshots = freeze_route(&harness.state.db, &["review-revoke"]);
    let recorded = install_native(
        &harness.accepted.run_id,
        vec![
            ok_http(recorded_text_only_body()),
            ok_http(recorded_minimax_body()),
        ],
    );
    recorded.before_response(super::model_gateway::notify_web_revoked);
    let executor = harness.executor(snapshots, Some(minimax_endpoint()));
    let result = executor
        .execute(
            &harness.accepted.run_id,
            &search_call("review-revoked-native"),
            1,
        )
        .await
        .unwrap();
    assert_eq!(recorded.call_count(), 1, "{}", result.output);
    assert_ne!(result.output["dualPath"]["native"]["succeeded"], true);
    assert_eq!(result.output["dualPath"]["mcp"]["succeeded"], true);
}

#[tokio::test]
async fn review_regression_d_adapter_frozen_prompt_limit_rejects_before_post() {
    let harness = V04Harness::new();
    upsert_mcp(&harness.state.db, "budget-limit-fixture", "search-only");
    let policy = AgentRunRepository::budget_policy_for_session(
        &harness.state.db,
        &harness.accepted.session.session_key,
        &harness.accepted.run_id,
    )
    .unwrap()
    .unwrap();
    let recorded = install_native(
        &harness.accepted.run_id,
        vec![ok_http(recorded_minimax_body())],
    );
    let action = dispatch_search_action(&harness, Some(minimax_endpoint()));
    let control = super::native_search_adapter::SearchExecutionControl {
        db: &harness.state.db,
        run_id: harness.accepted.run_id.clone(),
        deadline: action.search_deadline.unwrap(),
        revocation_epoch: action.web_revocation_epoch,
    };
    let oversized_query = "法".repeat(policy.max_prompt_tokens as usize * 4);
    let result = super::native_search_adapter::execute_production_route(
        &control,
        action.native_endpoint.as_ref(),
        &action.search_identity,
        &oversized_query,
    )
    .await;
    assert_eq!(recorded.call_count(), 0);
    assert_eq!(result.dispatched_attempts, 0);
    assert_eq!(super::model_turn_ledger::used(&harness.accepted.run_id), 0);
    assert_eq!(
        result.failure,
        Some(super::dual_path_search::RouteFailureClass::ProtocolOrResultInsufficient)
    );
}

#[tokio::test]
async fn review_regression_d_adapter_unknown_prompt_reserves_frozen_limit() {
    let harness = V04Harness::new();
    upsert_mcp(&harness.state.db, "budget-unknown-fixture", "search-only");
    let policy = AgentRunRepository::budget_policy_for_session(
        &harness.state.db,
        &harness.accepted.session.session_key,
        &harness.accepted.run_id,
    )
    .unwrap()
    .unwrap();
    let mut body = recorded_minimax_body();
    body["usage"] = json!({"output_tokens":7});
    let recorded = install_native(&harness.accepted.run_id, vec![ok_http(body)]);
    let action = dispatch_search_action(&harness, Some(minimax_endpoint()));
    let control = super::native_search_adapter::SearchExecutionControl {
        db: &harness.state.db,
        run_id: harness.accepted.run_id.clone(),
        deadline: action.search_deadline.unwrap(),
        revocation_epoch: action.web_revocation_epoch,
    };
    let result = super::native_search_adapter::execute_production_route(
        &control,
        action.native_endpoint.as_ref(),
        &action.search_identity,
        &action.query,
    )
    .await;
    assert_eq!(recorded.call_count(), 1);
    let (snapshot, _) =
        AgentRunRepository::model_budget_snapshot(&harness.state.db, &harness.accepted.run_id)
            .unwrap();
    let snapshot = snapshot.unwrap();
    assert_eq!(
        snapshot["attempts"][0]["reserved_prompt"],
        policy.max_prompt_tokens
    );
    assert_eq!(result.completion_tokens, Some(7));
    assert!(result.usage_unknown);
}

#[tokio::test]
async fn review_regression_d_adapter_child_uses_frozen_child_prompt_limit() {
    let mut input = request();
    input.turn.message = "请委派子任务并联网核实".into();
    let harness = V04Harness::with_request(input);
    upsert_mcp(&harness.state.db, "budget-child-fixture", "search-only");
    let policy = AgentRunRepository::budget_policy_for_session(
        &harness.state.db,
        &harness.accepted.session.session_key,
        &harness.accepted.run_id,
    )
    .unwrap()
    .unwrap();
    assert!(policy.child_input_tokens_per_turn > 0);
    assert!(policy.child_input_tokens_per_turn < policy.max_prompt_tokens);
    let scope = super::agent_tool_loop::scoped_child_provider_run_id(
        &harness.accepted.run_id,
        "fixture-child",
    );
    let _child = super::model_turn_ledger::BindGuard::new(&scope, policy.child_max_model_turns);
    let mut body = recorded_minimax_body();
    body["usage"] = json!({"output_tokens":7});
    let recorded = install_native(&harness.accepted.run_id, vec![ok_http(body)]);
    let mut action = dispatch_search_action(&harness, Some(minimax_endpoint()));
    action.search_identity.child_run_id = Some(scope.clone());
    let control = super::native_search_adapter::SearchExecutionControl {
        db: &harness.state.db,
        run_id: harness.accepted.run_id.clone(),
        deadline: action.search_deadline.unwrap(),
        revocation_epoch: action.web_revocation_epoch,
    };
    let query = "法".repeat(policy.child_input_tokens_per_turn as usize + 1);
    let result = super::model_turn_ledger::with_scope(
        &scope,
        super::native_search_adapter::execute_production_route(
            &control,
            action.native_endpoint.as_ref(),
            &action.search_identity,
            &query,
        ),
    )
    .await;
    assert_eq!(recorded.call_count(), 0);
    assert!(result.failure.is_some());
    let result = super::model_turn_ledger::with_scope(
        &scope,
        super::native_search_adapter::execute_production_route(
            &control,
            action.native_endpoint.as_ref(),
            &action.search_identity,
            "contract",
        ),
    )
    .await;
    assert!(result.has_retrieval_credentials);
    assert_eq!(recorded.call_count(), 1);
    let (snapshot, _) =
        AgentRunRepository::model_budget_snapshot(&harness.state.db, &harness.accepted.run_id)
            .unwrap();
    assert_eq!(
        snapshot.unwrap()["attempts"][0]["reserved_prompt"],
        policy.child_input_tokens_per_turn
    );
}

#[tokio::test]
async fn review_regression_d_production_native_deadline_keeps_completed_mcp() {
    let harness = V04Harness::new();
    upsert_mcp(&harness.state.db, "deadline-mcp-fixture", "search-only");
    let _snapshots = freeze_route(&harness.state.db, &["deadline-mcp-fixture"]);
    let recorded = install_native(
        &harness.accepted.run_id,
        vec![ok_http(recorded_minimax_body())],
    );
    recorded.block_response();
    let mut action = dispatch_search_action(&harness, Some(minimax_endpoint()));
    // The recorded transport controls only the deadline wait; production
    // dispatch, authorization, budget settlement and partial-result handling run.
    let frozen_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    action.search_deadline = Some(frozen_deadline);
    let retrieval_scope = Default::default();
    let ctx = dispatch_search_ctx(&harness, &retrieval_scope, &action);
    let args = json!({"query":"contract"});
    let work =
        crate::ai_runtime::tool_dispatch::dispatch_tool(&harness.state, &ctx, "web_search", &args);
    tokio::pin!(work);
    tokio::select! {
        result = &mut work => panic!("native must enter before deadline: {:?}", result.error),
        _ = recorded.wait_until_dispatched() => {},
    }
    assert_eq!(recorded.captured_deadlines(), [frozen_deadline]);
    recorded.elapse_deadline();
    let result = work.await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(recorded.call_count(), 1);
    assert_eq!(recorded.captured_deadlines(), [frozen_deadline]);
    assert_eq!(result.output["dualPath"]["mcp"]["succeeded"], true);
    assert_eq!(result.output["dualPath"]["native"]["succeeded"], false);
    assert_eq!(
        result.output["dualPath"]["native"]["failed"],
        "temporary_failure"
    );
    assert_eq!(result.output["dualPath"]["usage"]["nativeAttempts"], 1);
    assert_eq!(
        result.output["dualPath"]["usage"]["nativeUsageUnknown"],
        true
    );
    assert!(result.output["count"].as_u64().unwrap() > 0);
}
