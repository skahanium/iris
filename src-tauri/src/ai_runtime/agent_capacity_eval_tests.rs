use std::time::Duration;

use super::agent_capacity_eval::{
    evaluate_case, spawn_llm_protocol_double, AnswerObservation, CaseManifest, CheckStatus,
    CitationObservation, EvidenceGroup, HttpResponseScript, ImplicitVaultExpectation,
    LlmProtocolDouble, McpCapabilityContract, McpOperation, ObservedSource,
    ProtocolContractOutcome, ProtocolValidationLevel, SourceKind, WebState,
};
use super::model_gateway::{GatewayRequest, LlmFunctionDef, LlmToolDef, ModelGateway};
use super::provider_router::{
    CandidateAvailability, CandidateHealth, ProviderCandidate, ProviderFailure,
    ProviderRequirements, ProviderRouter, SecurityDomain,
};
use super::{
    EndpointFamily, LlmMessage, MessageRole, ProviderConfig, ReasoningAdapter, ReasoningControl,
    ReasoningMode, ReasoningVisibility, ResolvedReasoningRequest, ToolCall,
};

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
            })
            .collect(),
        supported_fact_ids: case
            .required_facts
            .iter()
            .map(|fact| fact.id.clone())
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
        safety_violation_codes: Vec::new(),
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
fn parses_versioned_manifest_without_raw_answer_or_endpoint_fields() {
    let case = manifest_fixture();

    assert_eq!(case.schema_version, "agent-answer-v1");
    assert_eq!(case.id, "contract-hybrid-001");
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

    let verdict = evaluate_case(&case, &observation);

    assert_eq!(verdict.required_evidence.status, CheckStatus::Fail);
    assert_eq!(
        verdict.required_evidence.reason_code,
        "required_source_missing"
    );
    assert!(!verdict.overall_pass);
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

    let verdict = evaluate_case(&case, &observation);

    assert_eq!(verdict.required_evidence.status, CheckStatus::Fail);
    assert_eq!(
        verdict.required_evidence.reason_code,
        "required_source_missing"
    );
    assert!(!verdict.overall_pass);
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
        supported_fact_ids: Vec::new(),
        contradicted_fact_ids: Vec::new(),
        citations: Vec::new(),
        tool_calls: Vec::new(),
        disclosures: vec!["web_unavailable".into()],
        degraded: true,
        clarification_requested: false,
        safety_violation_codes: Vec::new(),
    };

    let verdict = evaluate_case(&case, &observation);

    assert_eq!(verdict.required_evidence.status, CheckStatus::Pass);
    assert_eq!(
        verdict.degradation_or_clarification.status,
        CheckStatus::Pass
    );
    assert!(verdict.overall_pass);
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
        id: "local-ungranted".into(),
        kind: SourceKind::Local,
    }];
    observation.tool_calls = vec!["read_note".into()];

    let verdict = evaluate_case(&case, &observation);

    assert_eq!(verdict.authorization.status, CheckStatus::Fail);
    assert_eq!(
        verdict.authorization.reason_code,
        "unauthorized_local_access"
    );
    assert_eq!(verdict.safety.status, CheckStatus::Fail);
    assert!(!verdict.overall_pass);
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

    let verdict = evaluate_case(&case, &observation);

    assert_eq!(verdict.route_efficiency.status, CheckStatus::Fail);
    assert_eq!(
        verdict.route_efficiency.reason_code,
        "unnecessary_web_search"
    );
    assert!(verdict.overall_pass);
}

#[test]
fn required_web_search_policy_fails_route_when_no_web_call_was_observed() {
    let mut case = manifest_fixture();
    case.tool_policy.web_search = super::agent_capacity_eval::WebSearchPolicy::Required;
    let observation = observation_for(&case);

    let verdict = evaluate_case(&case, &observation);

    assert_eq!(verdict.route_efficiency.status, CheckStatus::Fail);
    assert_eq!(
        verdict.route_efficiency.reason_code,
        "required_web_search_missing"
    );
    assert!(!verdict.overall_pass);
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
        ProtocolValidationLevel::ContractVerified
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
        ProtocolValidationLevel::ContractVerified
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

#[test]
fn protocol_double_debug_output_does_not_expose_captured_bodies() {
    let double = LlmProtocolDouble::redacted_debug_contract();
    let debug = format!("{double:?}");

    assert!(debug.contains("LlmProtocolDouble"));
    assert!(!debug.contains("protocol probe"));
    assert!(!debug.contains("captured"));
}
