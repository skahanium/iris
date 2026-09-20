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
    _directory: tempfile::TempDir,
    state: std::sync::Arc<AppState>,
    accepted: super::run_contract::AssistantRunAccepted,
    context: super::run_context::RunContext,
    sink: RecordingSink,
}

impl V04Harness {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("tempdir");
        let data_dir = directory.path().join("data");
        let config_dir = directory.path().join("config");
        std::fs::create_dir_all(&config_dir).expect("config dir");
        std::env::set_var("IRIS_DATA_DIR", &data_dir);
        std::env::set_var("IRIS_CONFIG_DIR", &config_dir);
        let state = AppState::new(data_dir).expect("app state");
        crate::credentials::set_api_key("iris.llm.minimax", FAKE_TOKEN).expect("fake token");
        write_setting(&state.db, "web_search_enabled", &json!(true));
        let accepted = RunIntake::start(&state.db, request()).expect("accept");
        let context = RunContextAssembler::assemble(
            &state.db,
            None,
            &accepted.session.session_key,
            &accepted.run_id,
        )
        .expect("context");
        let sink = RecordingSink::default();
        begin(&state, &accepted, &sink);
        Self {
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
        .with_allowed_tool_names(&["web_search".into()])
        .with_native_search_endpoint(endpoint)
    }
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
    assert_eq!(witness.call_id, "web_search");
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
