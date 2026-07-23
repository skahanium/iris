use std::time::Duration;

use crate::storage::db::Database;

use super::agent_capacity_eval::{
    evaluate_case, spawn_llm_protocol_double, AnswerObservation, CaseManifest, CheckStatus,
    CitationObservation, EvidenceGroup, FactSupportObservation, HttpResponseScript,
    ImplicitVaultExpectation, LlmProtocolDouble, McpCapabilityContract, McpOperation,
    McpTransportContract, McpTransportFailureContract, ObservedSource, ProtocolContractOutcome,
    ProtocolValidationLevel, SourceKind, WebAnswerContamination, WebState,
};
use super::mcp_host_runtime::{
    call_required_capability, probe_provider_stdio_tools, McpHostRuntimeOptions, McpStdioDiscovery,
    McpToolDefinition,
};
use super::mcp_runtime_registry::{upsert_web_evidence_provider, WebEvidenceProviderInput};
use super::model_gateway::{GatewayRequest, LlmFunctionDef, LlmToolDef, ModelGateway};
use super::provider_router::{
    CandidateAvailability, CandidateHealth, ProviderCandidate, ProviderFailure,
    ProviderRequirements, ProviderRouter, SecurityDomain,
};
use super::run_engine::{FailoverStreamingToolLoopProvider, RunEventSink};
use super::run_intake::RunIntake;
use super::{
    EndpointFamily, LlmMessage, MessageRole, ProviderConfig, ReasoningAdapter, ReasoningControl,
    ReasoningMode, ReasoningVisibility, ResolvedReasoningRequest, ToolCall,
};
use crate::ai_runtime::agent_tool_loop::ToolLoopProvider;
use crate::ai_runtime::direct_provider_route::DirectProviderRoute;
use crate::ai_runtime::model_gateway::{StreamEvent, StreamEventObserver};
use crate::ai_runtime::provider_router::ProviderRequirements as RuntimeProviderRequirements;
use crate::ai_runtime::run_contract::{
    AssistantRunStartRequest, AssistantTurnDraft, RunEventPayload,
    SecurityDomain as RunSecurityDomain,
};
use crate::llm::config::{ResolvedLlmConfig, ResolvedModelPool};

fn manifest_fixture() -> CaseManifest {
    CaseManifest::parse(include_str!(
        "../../../docs/eval/fixtures/agent-answer-v1.json"
    ))
    .expect("versioned evaluation fixture must parse")
}

fn observation_for(case: &CaseManifest) -> AnswerObservation {
    AnswerObservation {
        case_id: case.id.clone(),
        sources: case
            .required_sources
            .iter()
            .map(|source| ObservedSource {
                id: source.id.clone(),
                kind: source.kind,
                authorization_scope_id: None,
            })
            .collect(),
        fact_supports: case
            .required_facts
            .iter()
            .filter_map(|fact| {
                fact.allowed_sources
                    .first()
                    .map(|source_id| FactSupportObservation {
                        fact_id: fact.id.clone(),
                        source_ids: vec![source_id.clone()],
                    })
            })
            .collect(),
        contradicted_fact_ids: Vec::new(),
        citations: case
            .required_facts
            .iter()
            .filter_map(|fact| {
                fact.allowed_sources
                    .first()
                    .map(|source_id| CitationObservation {
                        fact_id: fact.id.clone(),
                        source_id: source_id.clone(),
                    })
            })
            .collect(),
        tool_calls: Vec::new(),
        disclosures: case.disclosure_constraints.clone(),
        degraded: false,
        clarification_requested: false,
        web_answer_contamination: WebAnswerContamination::ConfirmedAbsent,
        safety_violations: Vec::new(),
    }
}

fn stdio_options(request_timeout: Duration) -> McpHostRuntimeOptions {
    McpHostRuntimeOptions {
        request_timeout,
        max_stdout_line_bytes: 32 * 1024,
        max_stderr_bytes: 2 * 1024,
        cwd: None,
        stdio_session_pool: false,
        stdio_session_idle_timeout: Duration::from_secs(1),
    }
}

fn install_contract_stdio_provider(db: &Database, provider_id: &str, mode: &str, with_fetch: bool) {
    let fixture = format!(
        "{}/tests/fixtures/agent-capacity-mcp-stdio.sh",
        env!("CARGO_MANIFEST_DIR")
    );
    upsert_web_evidence_provider(
        db,
        &WebEvidenceProviderInput {
            id: provider_id.into(),
            name: "Agent capacity contract MCP".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json: serde_json::json!({
                "command": "/bin/sh",
                "args": [fixture, mode],
            })
            .to_string(),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: Some(r#"{"tool":"search","queryArg":"query"}"#.into()),
            web_fetch_mapping_json: with_fetch.then(|| r#"{"tool":"fetch","urlArg":"url"}"#.into()),
        },
    )
    .expect("contract MCP provider is valid");
}

struct CapacityNoopSink;

impl RunEventSink for CapacityNoopSink {
    fn emit(&self, _event: &super::run_contract::AssistantRunEvent) -> crate::error::AppResult<()> {
        Ok(())
    }
}

struct CapacityNoopStreamObserver;

impl StreamEventObserver for CapacityNoopStreamObserver {
    fn observe(&mut self, _event: &StreamEvent, _token_index: u32) -> crate::error::AppResult<()> {
        Ok(())
    }
}

fn retry_run_request() -> AssistantRunStartRequest {
    AssistantRunStartRequest {
        client_request_id: "agent-capacity-retry".into(),
        session: None,
        turn: AssistantTurnDraft {
            message: "verify retry boundary".into(),
            content_parts: None,
            explicit_references: Vec::new(),
            retrieval_scope: Default::default(),
            display_mentions: Vec::new(),
        },
        explicit_action: None,
        web_enabled: false,
        model_override: None,
        security_domain: RunSecurityDomain::Normal,
        classified_context_ref: None,
    }
}

fn retry_candidate(provider_id: &str, base_url: &str) -> ResolvedLlmConfig {
    ResolvedLlmConfig {
        provider_id: provider_id.into(),
        model: "contract-model".into(),
        base_url: base_url.into(),
        thinking: false,
        reasoning: ResolvedReasoningRequest::disabled(),
        input_budget: 4_096,
        output_budget: 256,
        endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
        supports_streaming: true,
        supports_tools: true,
        supports_vision: false,
        supports_reasoning: false,
    }
}

fn retry_requirements() -> RuntimeProviderRequirements {
    RuntimeProviderRequirements {
        endpoint_family: None,
        streaming: true,
        tools: true,
        vision: false,
        reasoning: false,
        min_input_budget_tokens: 1,
        min_output_budget_tokens: 1,
        security_domain: SecurityDomain::External,
    }
}

#[test]
fn manifest_parser_rejects_non_whitelisted_raw_answer_field() {
    let raw = include_str!("../../../docs/eval/fixtures/agent-answer-v1.json");
    let mut value: serde_json::Value = serde_json::from_str(raw).unwrap();
    value["rawAnswer"] = serde_json::json!("must not persist");

    let error = CaseManifest::parse(&value.to_string()).unwrap_err();

    assert_eq!(error.reason_code(), "manifest_schema_invalid");
    assert!(!error.to_string().contains("must not persist"));
}

#[test]
fn manifest_parser_rejects_path_shaped_identifiers() {
    let raw = include_str!("../../../docs/eval/fixtures/agent-answer-v1.json");
    let mut value: serde_json::Value = serde_json::from_str(raw).unwrap();
    value["requiredSources"][0]["id"] = serde_json::json!("private/folder/note.md");

    let error = CaseManifest::parse(&value.to_string()).unwrap_err();

    assert_eq!(error.reason_code(), "manifest_identifier_unsafe");
    assert!(!error.to_string().contains("private"));
}

#[test]
fn manifest_case_id_is_a_closed_low_information_ordinal() {
    let raw = include_str!("../../../docs/eval/fixtures/agent-answer-v1.json");
    for id in [
        "3mJr7AoUXx2Wqd",
        "ordinarySecretWithoutSpaces",
        "case-01",
        "case-001",
    ] {
        let mut value: serde_json::Value = serde_json::from_str(raw).unwrap();
        value["id"] = serde_json::json!(id);

        let error = CaseManifest::parse(&value.to_string()).unwrap_err();

        assert_eq!(error.reason_code(), "manifest_case_id_invalid", "{id}");
        assert!(!error.to_string().contains(id));
    }

    let case = manifest_fixture();
    let verdict = evaluate_case(&case, &observation_for(&case)).unwrap();
    let serialized = serde_json::to_value(verdict).unwrap();
    assert_eq!(serialized["caseId"], 1);
    assert!(serialized.to_string().contains("caseId"));
    assert!(!serialized.to_string().contains(&case.id));
}

#[tokio::test]
async fn pretransport_mcp_failures_are_classified_but_not_transport_verified() {
    let db = Database::open_in_memory().unwrap();
    upsert_web_evidence_provider(
        &db,
        &WebEvidenceProviderInput {
            id: "invalid-provider".into(),
            name: "Disabled contract MCP".into(),
            kind: "mcp".into(),
            enabled: false,
            transport_kind: "stdio".into(),
            transport_config_json: r#"{"command":"/bin/sh"}"#.into(),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: Some(r#"{"tool":"search","queryArg":"query"}"#.into()),
            web_fetch_mapping_json: None,
        },
    )
    .unwrap();

    for provider_id in ["missing-provider", "invalid-provider"] {
        let probe =
            probe_provider_stdio_tools(&db, provider_id, stdio_options(Duration::from_millis(120)))
                .await;
        let failure = McpTransportFailureContract::from_probe(probe)
            .expect("pretransport provider failures must be classified");

        assert_eq!(
            failure.validation_level(),
            ProtocolValidationLevel::FailureClassifiedOnly,
            "{provider_id} must not earn a transport proof"
        );
    }
}

#[test]
fn parses_versioned_manifest_without_raw_answer_or_endpoint_fields() {
    let case = manifest_fixture();

    assert_eq!(case.schema_version, "agent-answer-v1");
    assert_eq!(case.id, "case-1");
    assert_eq!(case.evidence_group, EvidenceGroup::Hybrid);
    assert_eq!(case.web_state, WebState::Online);
    assert_eq!(
        case.local_authorization.implicit_vault,
        ImplicitVaultExpectation::Allowed
    );
    assert_eq!(case.required_sources.len(), 2);

    let serialized = serde_json::to_value(case).expect("manifest remains serializable");
    for forbidden in ["prompt", "answer", "path", "url", "apiKey", "endpoint"] {
        assert!(
            serialized.get(forbidden).is_none(),
            "strict manifest whitelist must exclude {forbidden}"
        );
    }
}

#[test]
fn missing_required_source_fails_evidence_verdict() {
    let case = manifest_fixture();
    let mut observation = observation_for(&case);
    observation
        .sources
        .retain(|source| source.kind != SourceKind::Web);

    let verdict = evaluate_case(&case, &observation).unwrap();

    assert_eq!(verdict.required_evidence().status(), CheckStatus::Fail);
    assert_eq!(
        verdict.required_evidence().reason_code().as_str(),
        "required_source_missing"
    );
    assert!(!verdict.overall_pass());
}

#[test]
fn required_source_id_with_wrong_kind_does_not_satisfy_evidence() {
    let case = manifest_fixture();
    let mut observation = observation_for(&case);
    let web = observation
        .sources
        .iter_mut()
        .find(|source| source.id == "web-authority")
        .unwrap();
    web.kind = SourceKind::Local;

    let error = evaluate_case(&case, &observation).unwrap_err();

    assert_eq!(error.reason_code(), "observation_source_kind_mismatch");
}

#[test]
fn offline_web_case_passes_only_with_explicit_degradation() {
    let mut case = manifest_fixture();
    case.evidence_group = EvidenceGroup::WebOnly;
    case.web_state = WebState::Offline;
    case.required_sources
        .retain(|source| source.kind == SourceKind::Web);
    case.required_facts.clear();
    case.disclosure_constraints = vec!["web_unavailable".into()];

    let observation = AnswerObservation {
        case_id: case.id.clone(),
        sources: Vec::new(),
        fact_supports: Vec::new(),
        contradicted_fact_ids: Vec::new(),
        citations: Vec::new(),
        tool_calls: Vec::new(),
        disclosures: vec!["web_unavailable".into()],
        degraded: true,
        clarification_requested: false,
        web_answer_contamination: WebAnswerContamination::ConfirmedAbsent,
        safety_violations: Vec::new(),
    };

    let verdict = evaluate_case(&case, &observation).unwrap();

    assert_eq!(verdict.required_evidence().status(), CheckStatus::Pass);
    assert_eq!(
        verdict.degradation_or_clarification().status(),
        CheckStatus::Pass
    );
    assert!(verdict.overall_pass());
}

#[test]
fn offline_web_dispatch_is_a_hard_safety_failure_even_when_policy_is_optional() {
    let mut case = manifest_fixture();
    case.evidence_group = EvidenceGroup::WebOnly;
    case.web_state = WebState::Offline;
    case.required_sources
        .retain(|source| source.kind == SourceKind::Web);
    case.required_facts.clear();
    case.disclosure_constraints = vec!["web_unavailable".into()];
    case.tool_policy.web_search = super::agent_capacity_eval::WebSearchPolicy::Optional;
    let observation = AnswerObservation {
        case_id: case.id.clone(),
        sources: Vec::new(),
        fact_supports: Vec::new(),
        contradicted_fact_ids: Vec::new(),
        citations: Vec::new(),
        tool_calls: vec!["web_search".into()],
        disclosures: vec!["web_unavailable".into()],
        degraded: true,
        clarification_requested: false,
        web_answer_contamination: WebAnswerContamination::ConfirmedAbsent,
        safety_violations: Vec::new(),
    };
    let verdict = evaluate_case(&case, &observation).unwrap();

    assert_eq!(verdict.authorization().status(), CheckStatus::Fail);
    assert_eq!(verdict.safety().status(), CheckStatus::Fail);
    assert!(!verdict.overall_pass());
}

#[test]
fn offline_required_web_policy_allows_explicit_degradation_without_dispatch() {
    let mut case = manifest_fixture();
    case.evidence_group = EvidenceGroup::Hybrid;
    case.web_state = WebState::Offline;
    case.tool_policy.web_search = super::agent_capacity_eval::WebSearchPolicy::Required;
    case.disclosure_constraints = vec!["web_unavailable".into()];
    let mut observation = observation_for(&case);
    observation
        .sources
        .retain(|source| source.kind == SourceKind::Local);
    observation.disclosures = vec!["web_unavailable".into()];
    observation.degraded = true;

    let verdict = evaluate_case(&case, &observation).unwrap();

    assert_eq!(
        verdict.degradation_or_clarification().status(),
        CheckStatus::Pass
    );
    assert_eq!(verdict.safety().status(), CheckStatus::Pass);
    assert!(verdict.overall_pass());
}

#[test]
fn scorer_rejects_mismatched_unknown_and_duplicate_observation_identifiers() {
    let case = manifest_fixture();

    let mut mismatched = observation_for(&case);
    mismatched.case_id = "contract-other-002".into();
    assert_eq!(
        evaluate_case(&case, &mismatched).unwrap_err().reason_code(),
        "observation_case_mismatch"
    );

    let mut unknown = observation_for(&case);
    unknown.sources.push(ObservedSource {
        id: "QW5zd2VyTGVha0Jsb2I".into(),
        kind: SourceKind::Local,
        authorization_scope_id: None,
    });
    assert_eq!(
        evaluate_case(&case, &unknown).unwrap_err().reason_code(),
        "observation_identifier_unsafe"
    );

    let mut duplicate = observation_for(&case);
    duplicate.sources.push(duplicate.sources[0].clone());
    assert_eq!(
        evaluate_case(&case, &duplicate).unwrap_err().reason_code(),
        "observation_source_duplicate"
    );
}

#[test]
fn malicious_raw_path_url_domain_and_encoded_observations_never_reach_a_verdict() {
    let case = manifest_fixture();
    for value in [
        "raw answer exfiltration",
        "/private/notes/secret.md",
        "https://private.invalid/note",
        "private.invalid",
        "cmF3IGFuc3dlciBleGZpbHRyYXRpb24",
        "72617720616e7377657220657866696c74726174696f6e",
        "MFRGGZDFMZTWQ2LKNNWG23TPOI",
        "cmF3LWFuc3dlcl9leGZpbHRyYXRpb24",
    ] {
        let mut observation = observation_for(&case);
        observation.sources[0].id = value.into();
        let error = evaluate_case(&case, &observation).unwrap_err();
        assert_eq!(
            error.reason_code(),
            "observation_identifier_unsafe",
            "{value}"
        );
        assert!(!error.to_string().contains(value));
    }

    let verdict = evaluate_case(&case, &observation_for(&case)).unwrap();
    let serialized = serde_json::to_string(&verdict).unwrap();
    for forbidden in ["raw answer", "/private/", "https://", ".invalid"] {
        assert!(!serialized.contains(forbidden));
    }
}

#[test]
fn explicit_scope_is_verified_for_each_local_source() {
    let mut case = manifest_fixture();
    case.local_authorization.implicit_vault = ImplicitVaultExpectation::Forbidden;
    case.local_authorization.explicit_reference_ids.clear();
    case.local_authorization.explicit_scope_id = Some("scope-synthetic".into());
    case.local_authorization.explicit_scope_source_ids = vec!["local-authority".into()];
    let mut outside = observation_for(&case);
    outside.sources = vec![ObservedSource {
        id: "local-scope-outside".into(),
        kind: SourceKind::Local,
        authorization_scope_id: Some("scope-synthetic".into()),
    }];
    outside.fact_supports.clear();
    outside.citations.clear();

    let error = evaluate_case(&case, &outside).unwrap_err();
    assert_eq!(error.reason_code(), "observation_scope_outside");

    let mut inside = observation_for(&case);
    inside
        .sources
        .retain(|source| source.kind == SourceKind::Local);
    inside.sources[0].authorization_scope_id = Some("scope-synthetic".into());
    let verdict = evaluate_case(&case, &inside).unwrap();
    assert_eq!(verdict.authorization().status(), CheckStatus::Pass);
}

#[test]
fn citation_must_bind_to_the_fact_sources_actually_used_by_the_answer() {
    let mut case = manifest_fixture();
    case.required_facts[0]
        .allowed_sources
        .push("web-authority".into());
    let mut observation = observation_for(&case);
    observation.fact_supports[0].source_ids = vec!["local-authority".into()];
    observation.citations[0].source_id = "web-authority".into();

    let error = evaluate_case(&case, &observation).unwrap_err();

    assert_eq!(error.reason_code(), "observation_citation_support_mismatch");
}

#[test]
fn extra_web_search_is_advisory_only_after_explicit_non_contamination_proof() {
    let mut case = manifest_fixture();
    case.evidence_group = EvidenceGroup::LocalOnly;
    case.required_sources
        .retain(|source| source.kind == SourceKind::Local);
    for fact in &mut case.required_facts {
        fact.allowed_sources
            .retain(|source_id| source_id.starts_with("local-"));
    }
    let mut contaminated = observation_for(&case);
    contaminated.tool_calls = vec!["read_note".into(), "web_search".into()];
    contaminated.web_answer_contamination =
        super::agent_capacity_eval::WebAnswerContamination::Detected;
    let contaminated_verdict = evaluate_case(&case, &contaminated).unwrap();
    assert_eq!(contaminated_verdict.safety().status(), CheckStatus::Fail);
    assert!(!contaminated_verdict.overall_pass());

    let mut clean = contaminated;
    clean.web_answer_contamination =
        super::agent_capacity_eval::WebAnswerContamination::ConfirmedAbsent;
    let clean_verdict = evaluate_case(&case, &clean).unwrap();
    assert_eq!(clean_verdict.route_efficiency().status(), CheckStatus::Fail);
    assert!(clean_verdict.overall_pass());
}

#[test]
fn unauthorized_local_read_is_a_fatal_security_failure() {
    let mut case = manifest_fixture();
    case.evidence_group = EvidenceGroup::NoRetrieval;
    case.local_authorization.explicit_reference_ids.clear();
    case.local_authorization.explicit_scope_id = None;
    case.local_authorization.implicit_vault = ImplicitVaultExpectation::Forbidden;
    case.required_sources.clear();
    case.required_facts.clear();
    let mut observation = observation_for(&case);
    observation.sources = vec![ObservedSource {
        id: "local-scope-outside".into(),
        kind: SourceKind::Local,
        authorization_scope_id: None,
    }];
    observation.tool_calls = vec!["read_note".into()];

    let error = evaluate_case(&case, &observation).unwrap_err();

    assert_eq!(error.reason_code(), "observation_scope_outside");
}

#[test]
fn unnecessary_web_search_is_non_fatal_when_answer_is_not_contaminated() {
    let mut case = manifest_fixture();
    case.evidence_group = EvidenceGroup::LocalOnly;
    case.required_sources
        .retain(|source| source.kind == SourceKind::Local);
    for fact in &mut case.required_facts {
        fact.allowed_sources
            .retain(|source_id| source_id.starts_with("local-"));
    }
    case.tool_policy.allowed.push("web_search".into());
    let mut observation = observation_for(&case);
    observation.tool_calls = vec!["read_note".into(), "web_search".into()];

    let verdict = evaluate_case(&case, &observation).unwrap();

    assert_eq!(verdict.route_efficiency().status(), CheckStatus::Fail);
    assert_eq!(
        verdict.route_efficiency().reason_code().as_str(),
        "unnecessary_web_search"
    );
    assert!(verdict.overall_pass());
}

#[test]
fn required_web_search_policy_fails_route_when_no_web_call_was_observed() {
    let mut case = manifest_fixture();
    case.tool_policy.web_search = super::agent_capacity_eval::WebSearchPolicy::Required;
    let observation = observation_for(&case);

    let verdict = evaluate_case(&case, &observation).unwrap();

    assert_eq!(verdict.route_efficiency().status(), CheckStatus::Fail);
    assert_eq!(
        verdict.route_efficiency().reason_code().as_str(),
        "required_web_search_missing"
    );
    assert!(!verdict.overall_pass());
}

fn provider(base_url: &str, endpoint_family: EndpointFamily) -> ProviderConfig {
    ProviderConfig {
        name: "contract-provider".into(),
        base_url: base_url.into(),
        api_key: None,
        model: "contract-model".into(),
        endpoint_family,
    }
}

fn request(provider: ProviderConfig) -> GatewayRequest {
    GatewayRequest {
        provider,
        messages: vec![LlmMessage {
            role: MessageRole::User,
            content: "protocol probe".into(),
            tool_call_id: None,
            tool_calls: None,
            ..Default::default()
        }],
        tools: vec![LlmToolDef {
            tool_type: "function".into(),
            function: LlmFunctionDef {
                name: "web_search".into(),
                description: "contract search".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {"query": {"type": "string"}},
                    "required": ["query"]
                }),
            },
        }],
        max_tokens: Some(128),
        input_token_budget: None,
        temperature: Some(0.0),
        stream: false,
        thinking: false,
        reasoning: ResolvedReasoningRequest::disabled(),
        continuation: None,
        skip_stub_ids: Vec::new(),
    }
}

#[tokio::test]
async fn openai_compatible_double_exercises_real_gateway_contract() {
    let double = spawn_llm_protocol_double(vec![HttpResponseScript::json(serde_json::json!({
        "choices": [{
            "message": {"content": "contract-ok"},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 2, "completion_tokens": 1, "total_tokens": 3}
    }))])
    .await
    .unwrap();
    let gateway = ModelGateway::new(reqwest::Client::new(), Vec::new());

    let response = gateway
        .send_request(request(provider(
            &double.base_url,
            EndpointFamily::OpenAiCompatibleChatCompletions,
        )))
        .await
        .unwrap();
    let captures = double.finish().await.unwrap();

    assert_eq!(response.content.as_deref(), Some("contract-ok"));
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].path, "/v1/chat/completions");
    assert_eq!(captures[0].body["model"], "contract-model");
    assert_eq!(captures[0].body["tools"][0]["type"], "function");
}

#[tokio::test]
async fn openai_tool_continuation_uses_assistant_call_then_tool_result_shape() {
    let double = spawn_llm_protocol_double(vec![
        HttpResponseScript::json(serde_json::json!({
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [{
                        "id": "call-openai-contract",
                        "type": "function",
                        "function": {"name": "web_search", "arguments": "{\"query\":\"synthetic\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        })),
        HttpResponseScript::json(serde_json::json!({
            "choices": [{"message": {"content": "continued-openai"}, "finish_reason": "stop"}]
        })),
    ])
    .await
    .unwrap();
    let gateway = ModelGateway::new(reqwest::Client::new(), Vec::new());
    let mut continued = request(provider(
        &double.base_url,
        EndpointFamily::OpenAiCompatibleChatCompletions,
    ));
    let first = gateway.send_request(continued.clone()).await.unwrap();
    continued.messages.push(LlmMessage {
        role: MessageRole::Assistant,
        content: String::new().into(),
        tool_call_id: None,
        tool_calls: Some(first.tool_calls),
        reasoning_content: None,
    });
    continued.messages.push(LlmMessage {
        role: MessageRole::Tool,
        content: r#"{"success":true}"#.into(),
        tool_call_id: Some("call-openai-contract".into()),
        tool_calls: None,
        reasoning_content: None,
    });
    let second = gateway.send_request(continued).await.unwrap();
    let captures = double.finish().await.unwrap();

    assert_eq!(second.content.as_deref(), Some("continued-openai"));
    assert_eq!(captures.len(), 2);
    assert_eq!(captures[1].body["messages"][1]["role"], "assistant");
    assert_eq!(
        captures[1].body["messages"][1]["tool_calls"][0]["id"],
        "call-openai-contract"
    );
    assert_eq!(captures[1].body["messages"][2]["role"], "tool");
    assert_eq!(
        captures[1].body["messages"][2]["tool_call_id"],
        "call-openai-contract"
    );
}

#[tokio::test]
async fn anthropic_messages_double_exercises_real_gateway_contract() {
    let double = spawn_llm_protocol_double(vec![HttpResponseScript::json(serde_json::json!({
        "content": [
            {"type": "text", "text": "contract-tool"},
            {
                "type": "tool_use",
                "id": "call-contract",
                "name": "web_search",
                "input": {"query": "synthetic"}
            }
        ],
        "usage": {"input_tokens": 3, "output_tokens": 2},
        "stop_reason": "tool_use"
    }))])
    .await
    .unwrap();
    let gateway = ModelGateway::new(reqwest::Client::new(), Vec::new());

    let response = gateway
        .send_request(request(provider(
            &double.base_url,
            EndpointFamily::AnthropicMessages,
        )))
        .await
        .unwrap();
    let captures = double.finish().await.unwrap();

    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].function.name, "web_search");
    assert_eq!(captures[0].path, "/v1/messages");
    assert_eq!(captures[0].body["tools"][0]["name"], "web_search");
    assert!(captures[0].body.get("messages").is_some());
}

#[tokio::test]
async fn anthropic_tool_continuation_uses_tool_use_and_tool_result_blocks() {
    let double = spawn_llm_protocol_double(vec![
        HttpResponseScript::json(serde_json::json!({
            "content": [{
                "type": "tool_use",
                "id": "call-anthropic-contract",
                "name": "web_search",
                "input": {"query": "synthetic"}
            }],
            "stop_reason": "tool_use"
        })),
        HttpResponseScript::json(serde_json::json!({
            "content": [{"type": "text", "text": "continued-anthropic"}],
            "stop_reason": "end_turn"
        })),
    ])
    .await
    .unwrap();
    let gateway = ModelGateway::new(reqwest::Client::new(), Vec::new());
    let mut continued = request(provider(
        &double.base_url,
        EndpointFamily::AnthropicMessages,
    ));
    let first = gateway.send_request(continued.clone()).await.unwrap();
    continued.messages.push(LlmMessage {
        role: MessageRole::Assistant,
        content: String::new().into(),
        tool_call_id: None,
        tool_calls: Some(first.tool_calls),
        reasoning_content: None,
    });
    continued.messages.push(LlmMessage {
        role: MessageRole::Tool,
        content: r#"{"success":true}"#.into(),
        tool_call_id: Some("call-anthropic-contract".into()),
        tool_calls: None,
        reasoning_content: None,
    });
    let second = gateway.send_request(continued).await.unwrap();
    let captures = double.finish().await.unwrap();

    assert_eq!(second.content.as_deref(), Some("continued-anthropic"));
    assert_eq!(captures[1].body["messages"][1]["role"], "assistant");
    assert_eq!(
        captures[1].body["messages"][1]["content"][0]["type"],
        "tool_use"
    );
    assert_eq!(captures[1].body["messages"][2]["role"], "user");
    assert_eq!(
        captures[1].body["messages"][2]["content"][0]["type"],
        "tool_result"
    );
    assert_eq!(
        captures[1].body["messages"][2]["content"][0]["tool_use_id"],
        "call-anthropic-contract"
    );
}

fn responses_reasoning() -> ResolvedReasoningRequest {
    ResolvedReasoningRequest {
        mode: ReasoningMode::Low,
        adapter: ReasoningAdapter::OpenAiResponses,
        control: ReasoningControl::Effort,
        visibility: ReasoningVisibility::HiddenChannel,
        requested: true,
        isolate_output: true,
    }
}

#[tokio::test]
async fn responses_double_preserves_real_continuation_contract() {
    let double = spawn_llm_protocol_double(vec![
        HttpResponseScript::json(serde_json::json!({
            "id": "response-contract-1",
            "status": "completed",
            "output": [{
                "type": "function_call",
                "call_id": "call-contract-1",
                "name": "web_search",
                "arguments": "{\"query\":\"synthetic\"}"
            }],
            "usage": {"input_tokens": 2, "output_tokens": 1, "total_tokens": 3}
        })),
        HttpResponseScript::json(serde_json::json!({
            "id": "response-contract-2",
            "status": "completed",
            "output": [{
                "type": "message",
                "content": [{"type": "output_text", "text": "continued-ok"}]
            }],
            "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}
        })),
    ])
    .await
    .unwrap();
    let gateway = ModelGateway::new(reqwest::Client::new(), Vec::new());
    let mut first_request = request(provider(
        &double.base_url,
        EndpointFamily::OpenAiCompatibleChatCompletions,
    ));
    first_request.reasoning = responses_reasoning();
    let first = gateway.send_request(first_request.clone()).await.unwrap();
    let continuation = first.continuation.clone().expect("response id retained");

    first_request.messages.push(LlmMessage {
        role: MessageRole::Assistant,
        content: String::new().into(),
        tool_call_id: None,
        tool_calls: Some(vec![ToolCall::new(
            "call-contract-1",
            "web_search",
            r#"{"query":"synthetic"}"#,
        )]),
        reasoning_content: None,
    });
    first_request.messages.push(LlmMessage {
        role: MessageRole::Tool,
        content: r#"{"success":true}"#.into(),
        tool_call_id: Some("call-contract-1".into()),
        tool_calls: None,
        reasoning_content: None,
    });
    first_request.continuation = Some(continuation);

    let second = gateway.send_request(first_request).await.unwrap();
    let captures = double.finish().await.unwrap();

    assert_eq!(second.content.as_deref(), Some("continued-ok"));
    assert_eq!(captures[1].path, "/v1/responses");
    assert_eq!(
        captures[1].body["previous_response_id"],
        "response-contract-1"
    );
    assert_eq!(captures[1].body["input"].as_array().unwrap().len(), 1);
    assert_eq!(captures[1].body["input"][0]["type"], "function_call_output");
}

#[test]
fn mcp_search_only_double_uses_real_mapping_and_normalization_contract() {
    let arguments = super::web_evidence_broker::build_mcp_search_arguments(
        r#"{"tool":"search","queryArg":"query","maxResultsArg":"limit"}"#,
        "synthetic",
        4,
    );
    let result = serde_json::json!({
        "content": [{
            "type": "text",
            "text": "[1] title: Contract A\nurl: https://source.invalid/a\nsnippet: synthetic"
        }],
        "isError": false
    });
    let diagnostic =
        super::web_evidence_broker::diagnose_mcp_search_result("contract-search", &result);

    assert_eq!(arguments["query"], "synthetic");
    assert_eq!(arguments["limit"], 4);
    assert_eq!(diagnostic.parsed_row_count, 1);
    assert_eq!(diagnostic.usable_https_row_count, 1);
    assert!(diagnostic.failure_reason.is_none());
}

#[test]
fn mcp_search_and_fetch_double_keeps_only_https_evidence_usable() {
    let contract = McpCapabilityContract::from_mappings(
        Some(r#"{"tool":"search","queryArg":"query"}"#),
        Some(r#"{"tool":"fetch","urlArg":"url"}"#),
    )
    .unwrap();
    let result = serde_json::json!({
        "content": [{
            "type": "text",
            "text": concat!(
                "[1] title: Secure\nurl: https://source.invalid/secure\nsnippet: secure\n",
                "[2] title: Insecure\nurl: http://source.invalid/insecure\nsnippet: insecure"
            )
        }],
        "isError": false
    });
    let diagnostic =
        super::web_evidence_broker::diagnose_mcp_search_result("contract-search-fetch", &result);

    assert_eq!(
        contract.validation_level(),
        ProtocolValidationLevel::MappingShapeVerified
    );
    assert!(contract.supports(McpOperation::Search));
    assert!(contract.supports(McpOperation::Fetch));
    assert_eq!(diagnostic.parsed_row_count, 2);
    assert_eq!(diagnostic.usable_https_row_count, 1);
    assert_eq!(diagnostic.rejected_non_https_row_count, 1);
}

#[test]
fn mcp_contract_rejects_fetch_only_and_unmapped_operations() {
    let fetch_only =
        McpCapabilityContract::from_mappings(None, Some(r#"{"tool":"fetch","urlArg":"url"}"#))
            .unwrap_err();
    assert_eq!(fetch_only.reason_code(), "mcp_fetch_without_search");

    let search_only =
        McpCapabilityContract::from_mappings(Some(r#"{"tool":"search","queryArg":"query"}"#), None)
            .unwrap();
    let unsupported = search_only
        .require(McpOperation::Fetch)
        .expect_err("search-only contract cannot claim fetch");
    assert_eq!(unsupported.reason_code(), "mcp_operation_unmapped");
}

#[test]
fn mcp_transport_contract_rejects_manual_discovery_and_deserialization() {
    let mapping =
        McpCapabilityContract::from_mappings(Some(r#"{"tool":"search","queryArg":"query"}"#), None)
            .unwrap();
    let manual_discovery = McpStdioDiscovery {
        protocol_version: super::mcp_host_runtime::MCP_PROTOCOL_VERSION.into(),
        server_name: "iris-contract-mcp".into(),
        server_version: None,
        tools: vec![McpToolDefinition {
            name: "search".into(),
            title: None,
            description: None,
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
        }],
        stderr_summary: None,
    };

    let manual_error = McpTransportContract::verify_discovery(&mapping, &manual_discovery)
        .expect_err("a bare discovery response has no transport provenance");
    assert_eq!(
        manual_error.reason_code(),
        "mcp_transport_provenance_required"
    );
    assert!(serde_json::from_str::<McpTransportContract>(
        r#"{"validationLevel":"contract_verified"}"#
    )
    .is_err());
}

#[tokio::test]
async fn real_stdio_mcp_transport_discovers_search_only_and_calls_search() {
    let db = Database::open_in_memory().unwrap();
    install_contract_stdio_provider(&db, "contract-search", "search-only", false);

    let probe = probe_provider_stdio_tools(
        &db,
        "contract-search",
        stdio_options(Duration::from_secs(2)),
    )
    .await;
    let discovery = probe
        .discovery()
        .expect("real stdio discovery must complete");
    assert_eq!(discovery.server_name, "iris-contract-mcp");
    assert_eq!(
        discovery
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        vec!["search"]
    );
    let mapping =
        McpCapabilityContract::from_mappings(Some(r#"{"tool":"search","queryArg":"query"}"#), None)
            .unwrap();
    assert_eq!(
        McpTransportContract::verify_attested_probe(&mapping, probe)
            .unwrap()
            .validation_level(),
        ProtocolValidationLevel::ContractVerified
    );

    let call = call_required_capability(
        &db,
        "web.search",
        serde_json::json!({"query": "synthetic"}),
        stdio_options(Duration::from_secs(2)),
    )
    .await
    .expect("real stdio search call must complete");
    assert_eq!(call.provider_id, "contract-search");
    assert_eq!(call.tool_name, "search");
    let diagnostic =
        super::web_evidence_broker::diagnose_mcp_search_result("contract-search", &call.result);
    assert_eq!(diagnostic.usable_https_row_count, 1);
}

#[tokio::test]
async fn real_stdio_mcp_transport_discovers_and_calls_search_and_fetch() {
    let db = Database::open_in_memory().unwrap();
    install_contract_stdio_provider(&db, "contract-search-fetch", "search-fetch", true);

    let probe = probe_provider_stdio_tools(
        &db,
        "contract-search-fetch",
        stdio_options(Duration::from_secs(2)),
    )
    .await;
    let discovery = probe
        .discovery()
        .expect("real stdio discovery must complete");
    assert_eq!(discovery.tools.len(), 2);
    let mapping = McpCapabilityContract::from_mappings(
        Some(r#"{"tool":"search","queryArg":"query"}"#),
        Some(r#"{"tool":"fetch","urlArg":"url"}"#),
    )
    .unwrap();
    assert_eq!(
        McpTransportContract::verify_attested_probe(&mapping, probe)
            .unwrap()
            .validation_level(),
        ProtocolValidationLevel::ContractVerified
    );

    let fetch = call_required_capability(
        &db,
        "web.fetch",
        serde_json::json!({"url": "https://source.invalid/contract"}),
        stdio_options(Duration::from_secs(2)),
    )
    .await
    .expect("real stdio fetch call must complete");
    assert_eq!(fetch.tool_name, "fetch");
    assert_eq!(fetch.result["content"][0]["text"], "fetch-result");
}

#[tokio::test]
async fn real_stdio_mcp_transport_malformed_and_timeout_remain_safe_failures() {
    let malformed_db = Database::open_in_memory().unwrap();
    install_contract_stdio_provider(&malformed_db, "contract-malformed", "malformed", false);
    let malformed = probe_provider_stdio_tools(
        &malformed_db,
        "contract-malformed",
        stdio_options(Duration::from_secs(1)),
    )
    .await;
    let malformed = McpTransportFailureContract::from_probe(malformed)
        .expect("malformed MCP output must be an attested failure");
    assert_eq!(
        malformed.outcome().reason_code(),
        "mcp_protocol_unavailable"
    );
    assert_eq!(
        malformed.validation_level(),
        ProtocolValidationLevel::ContractVerified
    );

    let timeout_db = Database::open_in_memory().unwrap();
    install_contract_stdio_provider(&timeout_db, "contract-timeout", "timeout", false);
    let timeout = probe_provider_stdio_tools(
        &timeout_db,
        "contract-timeout",
        stdio_options(Duration::from_millis(120)),
    )
    .await;
    let timeout = McpTransportFailureContract::from_probe(timeout)
        .expect("non-responsive MCP output must be an attested failure");
    assert_eq!(timeout.outcome().reason_code(), "mcp_protocol_timeout");
    assert_eq!(
        timeout.validation_level(),
        ProtocolValidationLevel::ContractVerified
    );
}

#[test]
fn mcp_error_output_remains_a_safe_contract_failure() {
    let result = serde_json::json!({
        "content": [{"type": "text", "text": "provider failed synthetic"}],
        "isError": true
    });
    let diagnostic =
        super::web_evidence_broker::diagnose_mcp_search_result("contract-error", &result);

    assert_eq!(
        diagnostic.failure_reason.as_deref(),
        Some("mcp_search_provider_error")
    );
    assert_eq!(diagnostic.usable_https_row_count, 0);
    assert!(!diagnostic.body.contains("api_key"));
}

#[test]
fn mcp_timeout_is_recorded_as_contract_outcome_not_vendor_capability() {
    let outcome = ProtocolContractOutcome::from_mcp_runtime_failure(
        super::mcp_host_runtime::McpRuntimeFailureKind::Timeout,
    );

    assert_eq!(outcome.reason_code(), "mcp_protocol_timeout");
    assert_eq!(
        outcome.validation_level(),
        ProtocolValidationLevel::FailureClassifiedOnly
    );
    assert!(!outcome.live_vendor_tested());
}

#[tokio::test]
async fn malformed_llm_output_returns_safe_gateway_error() {
    let double = spawn_llm_protocol_double(vec![HttpResponseScript::raw(
        200,
        "not-json synthetic body",
    )])
    .await
    .unwrap();
    let gateway = ModelGateway::new(reqwest::Client::new(), Vec::new());

    let error = gateway
        .send_request(request(provider(
            &double.base_url,
            EndpointFamily::OpenAiCompatibleChatCompletions,
        )))
        .await
        .unwrap_err();
    let _ = double.finish().await.unwrap();

    assert_eq!(error.to_string(), "llm_response_invalid_json");
    assert!(!error.to_string().contains("synthetic body"));
}

#[tokio::test]
async fn slow_llm_double_is_classified_as_retryable_timeout() {
    let double = spawn_llm_protocol_double(vec![HttpResponseScript::json(serde_json::json!({
        "choices": [{
            "message": {"content": "too-late"},
            "finish_reason": "stop"
        }]
    }))
    .with_delay(Duration::from_millis(500))])
    .await
    .unwrap();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(200))
        .build()
        .unwrap();
    let gateway = ModelGateway::new(client, Vec::new());

    let error = gateway
        .send_request(request(provider(
            &double.base_url,
            EndpointFamily::OpenAiCompatibleChatCompletions,
        )))
        .await
        .unwrap_err();
    let _ = double.finish().await.unwrap();
    let failure = super::provider_router::classify_provider_failure_from_app_error(&error);

    assert_eq!(failure, ProviderFailure::Timeout);
    assert!(failure.is_retryable());
}

fn candidate(id: &str) -> ProviderCandidate {
    ProviderCandidate {
        provider_id: id.into(),
        model: "contract-model".into(),
        base_url: format!("https://{id}.invalid"),
        endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
        supports_streaming: true,
        supports_tools: true,
        supports_vision: false,
        supports_reasoning: false,
        input_budget_tokens: 4_096,
        output_budget_tokens: 512,
        security_domain: SecurityDomain::External,
        availability: CandidateAvailability::Available,
        health: CandidateHealth::Unknown,
        reasoning: ResolvedReasoningRequest::disabled(),
        thinking: false,
        credential_service: None,
    }
}

#[test]
fn transient_retry_contract_advances_once_without_claiming_vendor_validation() {
    let router = ProviderRouter::new(vec![candidate("primary"), candidate("secondary")]);
    let selected = router.select_candidates(&ProviderRequirements {
        endpoint_family: None,
        streaming: true,
        tools: true,
        vision: false,
        reasoning: false,
        min_input_budget_tokens: 1,
        min_output_budget_tokens: 1,
        security_domain: SecurityDomain::External,
    });

    let next = router
        .next_candidate_after(&selected, 0, ProviderFailure::Timeout)
        .expect("retryable failure advances");

    assert_eq!(next.provider_id, "secondary");
    assert!(router
        .next_candidate_after(&selected, 0, ProviderFailure::Unauthorized)
        .is_none());
}

#[tokio::test]
async fn production_tool_loop_failover_retries_real_streaming_gateway_boundary() {
    let primary = spawn_llm_protocol_double(vec![HttpResponseScript::raw(
        500,
        r#"{"error":{"message":"synthetic transient"}}"#,
    )])
    .await
    .unwrap();
    let secondary = spawn_llm_protocol_double(vec![HttpResponseScript::sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"recovered\"}}]}\n\ndata: [DONE]\n\n",
    )])
    .await
    .unwrap();
    let route = DirectProviderRoute::from_secret_free_route(ResolvedModelPool {
        resolved: retry_candidate("contract-primary", &primary.base_url),
        failover_candidates: vec![retry_candidate("contract-secondary", &secondary.base_url)],
    })
    .unwrap();
    let db = Database::open_in_memory().unwrap();
    let accepted = RunIntake::start(&db, retry_run_request()).unwrap();
    let sink = CapacityNoopSink;
    let provider = FailoverStreamingToolLoopProvider::new(
        route,
        retry_requirements(),
        &db,
        &accepted.session,
        &sink,
    )
    .with_test_streaming_client(reqwest::Client::new());
    let messages = vec![LlmMessage {
        role: MessageRole::User,
        content: "retry the same tool turn".into(),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    }];
    let mut observer = CapacityNoopStreamObserver;

    let response = provider
        .answer_turn(&accepted.run_id, &messages, &[], &mut observer)
        .await
        .expect("retryable first failure must dispatch the selected fallback");
    let primary_calls = primary.finish().await.unwrap();
    let secondary_calls = secondary.finish().await.unwrap();
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .unwrap()
        .unwrap();

    assert_eq!(response.content.as_deref(), Some("recovered"));
    assert_eq!(primary_calls.len(), 1);
    assert_eq!(secondary_calls.len(), 1);
    assert!(replay.events.iter().any(|event| matches!(
        event.payload(),
        RunEventPayload::ProviderSwitched { ref provider_id, .. }
            if provider_id == "contract-secondary"
    )));
}

#[test]
fn protocol_double_debug_output_does_not_expose_captured_bodies() {
    let double = LlmProtocolDouble::redacted_debug_contract();
    let debug = format!("{double:?}");

    assert!(debug.contains("LlmProtocolDouble"));
    assert!(!debug.contains("protocol probe"));
    assert!(!debug.contains("captured"));
}
