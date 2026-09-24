//! Per-model native-search adapters (K12).
//!
//! One model, one file, one registry entry. Shared code here only looks up the
//! adapter and runs HTTPS / credential / construct. It must not grow MiniMax,
//! DeepSeek, or Gemini field names. Add a new model by adding a sibling file
//! and one line in [`PRODUCTION_ADAPTERS`].

mod deepseek_flash;
mod minimax_m3;

use std::time::Duration;

use serde_json::Value;

use crate::ai_runtime::dual_path_search::{
    NativeSearchSupport, RouteAttemptOutcome, RouteFailureClass, SearchActionIdentity,
};
use crate::ai_runtime::native_search_subrequest::{
    construct_native_search_subrequest, extract_reported_completion_tokens,
    extract_reported_prompt_tokens, native_search_support_for, NativeSearchEndpointRef,
    NativeSearchParse, NativeSearchPublicScope, NativeSearchRequestIdentity,
    NativeSearchSubrequest, NativeSearchSubrequestDraft,
};
use crate::credentials::llm_credential_service;

use self::deepseek_flash::DeepSeekFlashNativeSearchAdapter;
use self::minimax_m3::MinimaxM3NativeSearchAdapter;

/// MCP-only web_search stays at 20s. An Available native adapter is a second
/// serial HTTPS call (hang bound 60s), so the outer tool deadline must cover
/// both or MCP success is cancelled with the native call.
const MCP_SEARCH_DEADLINE: Duration = Duration::from_secs(20);
const NATIVE_SEARCH_HTTP_TIMEOUT: Duration = Duration::from_secs(60);
const NATIVE_SEARCH_DEADLINE_BUFFER: Duration = Duration::from_secs(10);

/// One action's immutable lifetime. Retries reuse this control, including the
/// revocation generation and absolute deadline captured by the dispatch entry.
pub(crate) struct SearchExecutionControl<'a> {
    pub(crate) db: &'a crate::storage::db::Database,
    pub(crate) run_id: String,
    pub(crate) deadline: tokio::time::Instant,
    pub(crate) revocation_epoch: u64,
}

impl SearchExecutionControl<'_> {
    pub(crate) fn check(&self) -> bool {
        use crate::ai_runtime::model_gateway;
        if self.run_id.is_empty()
            || tokio::time::Instant::now() >= self.deadline
            || model_gateway::is_abort_requested(&self.run_id)
            || model_gateway::web_revocation_epoch() != self.revocation_epoch
        {
            return false;
        }
        self.db
            .with_read_conn(|conn| {
                use rusqlite::OptionalExtension;
                let capabilities: String = conn.query_row(
                    "SELECT a.allowed_capabilities_json FROM agent_run_authorizations a
                 JOIN agent_runs r ON r.run_id = a.run_id
                 WHERE a.run_id = ?1 AND r.status = 'running'",
                    [&self.run_id],
                    |row| row.get(0),
                )?;
                let capabilities: Vec<String> = serde_json::from_str(&capabilities)?;
                let setting: Option<String> = conn
                    .query_row(
                        "SELECT value FROM settings WHERE key = 'web_search_enabled'",
                        [],
                        |row| row.get(0),
                    )
                    .optional()?;
                let enabled = match setting {
                    None => true,
                    Some(raw) => serde_json::from_str::<Value>(&raw)?.as_bool() == Some(true),
                };
                Ok(enabled
                    && capabilities
                        .iter()
                        .any(|capability| capability == "web.search"))
            })
            .unwrap_or(false)
    }

    pub(crate) async fn cancelled_or_revoked(&self) {
        tokio::select! {
            _ = crate::ai_runtime::model_gateway::wait_for_abort(&self.run_id) => {},
            _ = crate::ai_runtime::model_gateway::wait_for_web_revocation(self.revocation_epoch) => {},
            _ = tokio::time::sleep_until(self.deadline) => {},
        }
    }
}

/// Outer `web_search` deadline. Native Available adds the native HTTP bound.
pub(crate) fn web_search_call_deadline(endpoint: Option<&NativeSearchEndpointRef>) -> Duration {
    if native_search_support_for(endpoint) == NativeSearchSupport::Available {
        MCP_SEARCH_DEADLINE + NATIVE_SEARCH_HTTP_TIMEOUT + NATIVE_SEARCH_DEADLINE_BUFFER
    } else {
        MCP_SEARCH_DEADLINE
    }
}

/// HTTP result returned by an injectable native-search transport.
#[derive(Debug, Clone)]
pub(crate) struct NativeSearchHttpResult {
    pub status: u16,
    pub body: Value,
    pub transport_failed: bool,
}

/// Posts one native-search JSON request. Implementations must not log secrets.
pub(crate) trait NativeSearchTransport {
    /// Wait for the adapter's absolute deadline. Recorded transports may drive
    /// this boundary without replacing dispatch, authorization or settlement.
    async fn wait_until_deadline(&self, deadline: tokio::time::Instant) {
        tokio::time::sleep_until(deadline).await;
    }

    async fn post_json(
        &self,
        url: &str,
        body: &Value,
        headers: &[(String, String)],
        before_dispatch: &(dyn Fn() -> bool + Sync),
    ) -> NativeSearchHttpResult;
}

/// Protocol surface for one adapted model. Keep implementations in sibling files.
pub(crate) trait NativeSearchModelAdapter: Send + Sync {
    /// Catalog primary key; must exist in `find_model`.
    fn id(&self) -> &'static str;
    /// Wire model name. Default: `self.id()`.
    fn outbound_model(&self) -> &'static str {
        self.id()
    }
    fn matches(&self, model_id: &str) -> bool;
    fn request_url(&self, api_base: &str) -> Result<String, RouteFailureClass>;
    fn outbound_body(&self, subrequest: &NativeSearchSubrequest) -> Value;
    fn parse_response(&self, body: &Value) -> NativeSearchParse;
    fn constrain_output(&self, body: &mut Value, max_tokens: u32);
    /// HTTP headers including the credential. Must not be logged.
    fn http_headers(&self, token: &str) -> Vec<(String, String)> {
        vec![
            ("Authorization".into(), format!("Bearer {token}")),
            ("Content-Type".into(), "application/json".into()),
            ("Accept".into(), "application/json".into()),
        ]
    }
    /// Retry once on HTTP 200 generated-text-only (model skipped hosted search).
    fn retry_text_only_once(&self) -> bool {
        false
    }
}

/// Production adapters. Order does not imply priority; lookup is exclusive match.
const PRODUCTION_ADAPTERS: &[&dyn NativeSearchModelAdapter] = &[
    &MinimaxM3NativeSearchAdapter,
    &DeepSeekFlashNativeSearchAdapter,
];

/// Production adapter slice. Tests assert catalog ids; lookup stays exclusive match.
#[cfg(test)]
pub(crate) fn production_adapters() -> &'static [&'static dyn NativeSearchModelAdapter] {
    PRODUCTION_ADAPTERS
}

/// Number of production native-search adapters. Not a capability advertisement.
#[cfg(test)]
pub(crate) fn production_adapter_count() -> usize {
    production_adapters().len()
}

/// HTTPS base and credential service for one production native-search call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProductionRouteBinding {
    pub api_base: String,
    pub credential_service: String,
}

/// Bind catalog identity before any billed request. Catalog miss is protocol, not temporary.
pub(crate) fn bind_production_route(
    adapter: &dyn NativeSearchModelAdapter,
    endpoint: &NativeSearchEndpointRef,
) -> Result<ProductionRouteBinding, RouteFailureClass> {
    let Some(entry) = crate::llm::model_catalog::find_model(adapter.id()) else {
        return Err(RouteFailureClass::ProtocolOrResultInsufficient);
    };
    let api_base = endpoint
        .api_base
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| crate::llm::providers::api_base(entry.provider_id, None));
    let credential_service = endpoint
        .credential_service
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| llm_credential_service(entry.provider_id));
    Ok(ProductionRouteBinding {
        api_base,
        credential_service,
    })
}

/// Trim, require `https://`, strip each suffix at most once (longer first).
/// Longer first is required because `/messages` is a suffix of `/v1/messages`;
/// stripping the shorter token first would leave `/v1` and the next join would
/// 404. The reverse-order case is pinned by
/// `https_api_base_strips_known_suffixes_once`.
/// Must not mention vendor JSON field names or hosted-search tool types.
pub(super) fn https_api_base_without_suffixes(
    api_base: &str,
    suffixes: &[&str],
) -> Result<String, RouteFailureClass> {
    let mut base = api_base.trim().trim_end_matches('/').to_string();
    if !base.starts_with("https://") {
        return Err(RouteFailureClass::TransportOrProviderFailure);
    }
    let mut ordered: Vec<&str> = suffixes.to_vec();
    ordered.sort_by_key(|suffix| std::cmp::Reverse(suffix.len()));
    for suffix in ordered {
        if let Some(stripped) = base.strip_suffix(suffix) {
            base = stripped.trim_end_matches('/').to_string();
        }
    }
    Ok(base)
}

/// Resolve the adapter for one model id. Never matches on provider brand alone.
pub(crate) fn lookup_production_adapter(
    model_id: &str,
) -> Option<&'static dyn NativeSearchModelAdapter> {
    PRODUCTION_ADAPTERS
        .iter()
        .copied()
        .find(|adapter| adapter.matches(model_id))
}

/// Test seam: fixtures explicitly bind the same ledger used by production.
#[cfg(test)]
pub(crate) async fn execute_native_search<T: NativeSearchTransport>(
    draft: NativeSearchSubrequestDraft,
    transport: &T,
    bearer: Option<&str>,
    api_base: &str,
) -> RouteAttemptOutcome {
    execute_controlled_native_search(None, draft, transport, bearer, api_base).await
}

fn native_search_turn_budget(
    control: Option<&SearchExecutionControl<'_>>,
    is_child: bool,
) -> Option<crate::ai_runtime::agent_tool_loop::AgentModelTurnBudget> {
    let policy = match control {
        Some(control) => {
            use crate::ai_runtime::agent_run_repository::AgentRunRepository;
            let run = AgentRunRepository::get(control.db, &control.run_id).ok()??;
            AgentRunRepository::budget_policy_for_session(
                control.db,
                &run.run.session.session_key,
                &control.run_id,
            )
            .ok()??
        }
        None => {
            #[cfg(test)]
            {
                crate::ai_runtime::run_contract::RunBudgetPolicy::standard()
            }
            #[cfg(not(test))]
            {
                return None;
            }
        }
    };
    let prompt = if is_child {
        policy
            .child_input_tokens_per_turn
            .min(policy.max_prompt_tokens)
    } else {
        policy.max_prompt_tokens
    };
    let output = if is_child {
        policy
            .child_output_tokens_per_turn
            .min(policy.max_turn_output_tokens)
    } else {
        policy.max_turn_output_tokens
    }
    .min(2048);
    if prompt == 0 || output == 0 {
        return None;
    }
    Some(crate::ai_runtime::agent_tool_loop::AgentModelTurnBudget {
        max_prompt_tokens: Some(prompt),
        max_completion_tokens: Some(policy.max_completion_tokens),
        max_turn_output_tokens: Some(output),
    })
}

/// Execute with one action control across every transport attempt. Production
/// callers must provide the Run-bound control; only offline fixtures omit it.
pub(crate) async fn execute_controlled_native_search<T: NativeSearchTransport>(
    control: Option<&SearchExecutionControl<'_>>,
    draft: NativeSearchSubrequestDraft,
    transport: &T,
    bearer: Option<&str>,
    api_base: &str,
) -> RouteAttemptOutcome {
    use crate::ai_runtime::model_turn_ledger::{self, AttemptPurpose};
    #[cfg(not(test))]
    if control.is_none() {
        return protocol_insufficient();
    }
    let Some(adapter) = lookup_production_adapter(&draft.endpoint.model_id) else {
        return protocol_insufficient();
    };
    let constructed = match construct_native_search_subrequest(draft) {
        Ok(subrequest) => subrequest,
        Err(_) => return protocol_insufficient(),
    };
    if control.is_some_and(|c| c.run_id != constructed.identity.run_id || !c.check()) {
        return protocol_insufficient();
    }
    let url = match adapter.request_url(api_base) {
        Ok(url) if url.starts_with("https://") => url,
        _ => return transport_failure(),
    };
    let Some(token) = bearer.filter(|value| !value.is_empty()) else {
        return temporary_failure();
    };
    let mut body = adapter.outbound_body(&constructed);
    let headers = adapter.http_headers(token);
    // Known construction failures are rejected before acquiring any reservation.
    if reqwest::Url::parse(&url).is_err()
        || headers.iter().any(|(name, value)| {
            reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_err()
                || reqwest::header::HeaderValue::from_str(value).is_err()
        })
    {
        return transport_failure();
    }
    let mut accumulated = RouteAttemptOutcome::default();
    let deadline = control.map_or_else(
        || tokio::time::Instant::now() + NATIVE_SEARCH_HTTP_TIMEOUT,
        |c| {
            c.deadline
                .min(tokio::time::Instant::now() + NATIVE_SEARCH_HTTP_TIMEOUT)
        },
    );
    let scope = model_turn_ledger::current_scope(&constructed.identity.run_id);
    let db = control.map(|c| c.db);
    let Some(budget) = native_search_turn_budget(
        control,
        scope != constructed.identity.run_id || constructed.identity.child_run_id.is_some(),
    ) else {
        return protocol_insufficient();
    };
    adapter.constrain_output(&mut body, budget.max_turn_output_tokens.unwrap_or(0));
    let mut attempt_ids = Vec::new();
    for attempt in 0..=u32::from(adapter.retry_text_only_once()) {
        if control.is_some_and(|c| !c.check())
            || crate::ai_runtime::model_gateway::is_abort_requested(&scope)
            || tokio::time::Instant::now() >= deadline
        {
            accumulated.failure = Some(RouteFailureClass::TemporaryFailure);
            accumulated.has_retrieval_credentials = false;
            return accumulated;
        }
        let prompt = u32::try_from(crate::ai_runtime::text_support::estimate_tokens(
            &body.to_string(),
        ))
        .unwrap_or(u32::MAX);
        if budget.max_prompt_tokens.is_none_or(|limit| prompt > limit) {
            accumulated.failure = Some(RouteFailureClass::ProtocolOrResultInsufficient);
            return accumulated;
        }
        let lease = match model_turn_ledger::claim_attempt(
            db,
            &scope,
            AttemptPurpose::NativeSearch,
            budget,
        ) {
            Ok(lease) => lease,
            Err(_) => {
                accumulated.failure = Some(RouteFailureClass::ProtocolOrResultInsufficient);
                return accumulated;
            }
        };
        adapter.constrain_output(&mut body, lease.budget.max_turn_output_tokens.unwrap_or(0));
        attempt_ids.push(lease.id);
        let dispatched = std::sync::atomic::AtomicBool::new(false);
        let before_dispatch = || {
            if control.is_some_and(|c| !c.check())
                || tokio::time::Instant::now() >= deadline
                || model_turn_ledger::mark_dispatched(db, &lease).is_err()
            {
                return false;
            }
            dispatched.store(true, std::sync::atomic::Ordering::Release);
            true
        };
        let post = transport.post_json(&url, &body, &headers, &before_dispatch);
        let stopped = async {
            if let Some(control) = control {
                control.cancelled_or_revoked().await;
            } else {
                crate::ai_runtime::model_gateway::wait_for_abort(&scope).await;
            }
        };
        let result = tokio::select! {
            biased;
            _ = stopped => None,
            _ = transport.wait_until_deadline(deadline) => None,
            response = post => Some(response),
        };
        let prompt = result
            .as_ref()
            .and_then(|r| extract_reported_prompt_tokens(&r.body));
        let completion = result
            .as_ref()
            .and_then(|r| extract_reported_completion_tokens(&r.body));
        let usage =
            (prompt.is_some() || completion.is_some()).then(|| crate::ai_types::TokenUsage {
                prompt_tokens: prompt.unwrap_or(0),
                completion_tokens: completion.unwrap_or(0),
                total_tokens: prompt.unwrap_or(0).saturating_add(completion.unwrap_or(0)),
                ..Default::default()
            });
        if dispatched.load(std::sync::atomic::Ordering::Acquire) {
            accumulated.dispatched_attempts += 1;
            accumulated.internal_provider_attempts += 1;
        }
        if model_turn_ledger::settle_attempt(db, &lease, usage.as_ref()).is_err() {
            accumulated.failure = Some(RouteFailureClass::TransportOrProviderFailure);
            return accumulated;
        }
        (
            accumulated.prompt_tokens,
            accumulated.completion_tokens,
            accumulated.usage_unknown,
        ) = model_turn_ledger::usage_for_attempts(&lease.root_run_id, &attempt_ids);
        let status = result.as_ref().map(|result| result.status);
        let exceeds_budget = prompt.is_some_and(|tokens| {
            lease
                .budget
                .max_prompt_tokens
                .is_some_and(|limit| tokens > limit)
        }) || completion.is_some_and(|tokens| {
            lease
                .budget
                .max_turn_output_tokens
                .is_some_and(|limit| tokens > limit)
        });
        let mut outcome = if exceeds_budget {
            RouteAttemptOutcome {
                failure: Some(RouteFailureClass::ProtocolOrResultInsufficient),
                ..Default::default()
            }
        } else {
            match result {
                Some(result) if !result.transport_failed => match result.status {
                    200 => outcome_from_isolated_parse(
                        adapter.parse_response(&result.body),
                        &result.body,
                    ),
                    401 | 403 | 429 => temporary_failure(),
                    _ => transport_failure(),
                },
                Some(_) => transport_failure(),
                None => temporary_failure(),
            }
        };
        if control.is_some_and(|c| !c.check())
            || crate::ai_runtime::model_gateway::is_abort_requested(&scope)
        {
            outcome = temporary_failure();
        }
        if let Some(db) = db {
            record_native_attempt(
                db,
                &constructed.identity,
                lease.id,
                status,
                dispatched.load(std::sync::atomic::Ordering::Acquire),
                &outcome,
                (prompt, completion),
            );
        }
        accumulated.candidates = outcome.candidates;
        accumulated.has_retrieval_credentials = outcome.has_retrieval_credentials;
        accumulated.generated_text_only = outcome.generated_text_only;
        accumulated.failure = outcome.failure;
        if !outcome.generated_text_only || outcome.has_retrieval_credentials || attempt > 0 {
            break;
        }
    }
    accumulated
}

/// Production native route: hydrate credentials and POST through the live client.
pub(crate) async fn execute_production_route(
    control: &SearchExecutionControl<'_>,
    endpoint: Option<&NativeSearchEndpointRef>,
    identity: &SearchActionIdentity,
    query: &str,
) -> RouteAttemptOutcome {
    if !control.check() {
        return protocol_insufficient();
    }
    let Some(endpoint) = endpoint.cloned() else {
        return protocol_insufficient();
    };
    if endpoint.api_base.as_deref().is_none_or(str::is_empty)
        || endpoint
            .credential_service
            .as_deref()
            .is_none_or(str::is_empty)
    {
        return protocol_insufficient();
    }
    let Some(adapter) = lookup_production_adapter(&endpoint.model_id) else {
        return protocol_insufficient();
    };
    let binding = match bind_production_route(adapter, &endpoint) {
        Ok(binding) => binding,
        Err(RouteFailureClass::ProtocolOrResultInsufficient) => {
            return protocol_insufficient();
        }
        Err(RouteFailureClass::TransportOrProviderFailure) => {
            return transport_failure();
        }
        Err(RouteFailureClass::TemporaryFailure) => return temporary_failure(),
    };
    let secret = {
        #[cfg(test)]
        {
            recorded_bearer_for_run(&identity.run_id)
                .map(zeroize::Zeroizing::new)
                .or_else(|| {
                    crate::credentials::get_runtime_secret(&binding.credential_service).ok()
                })
        }
        #[cfg(not(test))]
        {
            crate::credentials::get_runtime_secret(&binding.credential_service).ok()
        }
    };
    let draft = NativeSearchSubrequestDraft {
        query: query.to_string(),
        public_scope: NativeSearchPublicScope::default(),
        identity: NativeSearchRequestIdentity {
            run_id: identity.run_id.clone(),
            input_revision: identity.input_revision.clone(),
            child_run_id: identity.child_run_id.clone(),
            model_turn: identity.model_turn,
            tool_surface_version: identity.tool_surface_version.clone(),
            parent_call_id: identity.action_id.clone(),
            attempt: identity.attempt,
        },
        endpoint,
        private_material: None,
    };
    #[cfg(test)]
    if let Some(transport) = recorded_transport_for_run(&identity.run_id) {
        return execute_controlled_native_search(
            Some(control),
            draft,
            &transport,
            secret.as_ref().map(|value| value.as_str()),
            &binding.api_base,
        )
        .await;
    }
    execute_controlled_native_search(
        Some(control),
        draft,
        &LiveNativeSearchTransport,
        secret.as_ref().map(|value| value.as_str()),
        &binding.api_base,
    )
    .await
}

fn record_native_attempt(
    db: &crate::storage::db::Database,
    identity: &NativeSearchRequestIdentity,
    attempt_id: u32,
    status: Option<u16>,
    dispatched: bool,
    outcome: &RouteAttemptOutcome,
    usage: (Option<u32>, Option<u32>),
) {
    let correlation = crate::ai_runtime::boundary_events::BoundaryCorrelation {
        run_id: identity.run_id.clone(),
        input_revision: identity.input_revision.clone(),
        parent_run_id: identity
            .child_run_id
            .as_ref()
            .map(|_| identity.run_id.clone()),
        child_run_id: identity.child_run_id.clone(),
        model_turn: identity.model_turn,
        call_id: identity.parent_call_id.clone(),
        attempt_id: format!("native-{attempt_id}"),
        tool_surface_version: identity.tool_surface_version.clone(),
        protocol_adapter: "native_search_subrequest".into(),
    };
    let status_class = match status {
        Some(200..=299) => "2xx",
        Some(400..=499) => "4xx",
        Some(500..=599) => "5xx",
        _ => "unknown",
    };
    let _ = crate::ai_runtime::boundary_events::record_native_search_observation(
        db,
        &correlation,
        &native_attempt_witness_payload(status_class, outcome, usage, dispatched),
    );
}

/// Content-free C26 witness payload for one isolated native-search attempt.
pub(crate) fn native_attempt_witness_payload(
    status_class: &str,
    outcome: &RouteAttemptOutcome,
    usage: (Option<u32>, Option<u32>),
    dispatched: bool,
) -> serde_json::Value {
    let (prompt, completion) = usage;
    serde_json::json!({
        "kind":"native_search_subrequest", "origin":"isolated_subrequest", "https":true,
        "statusClass":status_class, "hasRetrievalCredentials":outcome.has_retrieval_credentials,
        "citationCount":outcome.candidates.len(), "promptTokens":prompt, "completionTokens":completion,
        "tokenUsageReported":prompt.is_some() && completion.is_some(), "dispatched":dispatched,
        "budgetKind":"model_auxiliary_request", "isNetworkToolDispatch":false,
        "eventKinds":outcome.event_kinds
    })
}

pub(super) struct LiveNativeSearchTransport;

#[cfg(test)]
thread_local! {
    static LAST_LIVE_TRACE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

#[cfg(test)]
static RECORDED_NATIVE_SEARCH_BY_RUN: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, RecordedNativeSearchTransport>>,
> = std::sync::OnceLock::new();

#[cfg(test)]
static RECORDED_NATIVE_BEARER_BY_RUN: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, String>>,
> = std::sync::OnceLock::new();

#[cfg(test)]
fn recorded_native_search_by_run(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, RecordedNativeSearchTransport>> {
    RECORDED_NATIVE_SEARCH_BY_RUN
        .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

#[cfg(test)]
fn recorded_native_bearer_by_run(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, String>> {
    RECORDED_NATIVE_BEARER_BY_RUN
        .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

#[cfg(test)]
fn recorded_transport_for_run(run_id: &str) -> Option<RecordedNativeSearchTransport> {
    recorded_native_search_by_run()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(run_id)
        .cloned()
}

#[cfg(test)]
fn recorded_bearer_for_run(run_id: &str) -> Option<String> {
    recorded_native_bearer_by_run()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(run_id)
        .cloned()
}

#[cfg(test)]
type RecordedResponseHook = std::sync::Arc<std::sync::Mutex<Option<Box<dyn Fn() + Send + Sync>>>>;

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct RecordedNativeSearchTransport {
    results: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<NativeSearchHttpResult>>>,
    call_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    captured_urls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    captured_headers: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
    blocked: std::sync::Arc<std::sync::atomic::AtomicBool>,
    entered: std::sync::Arc<tokio::sync::Notify>,
    deadline_elapsed: std::sync::Arc<tokio::sync::Notify>,
    captured_deadlines: std::sync::Arc<std::sync::Mutex<Vec<tokio::time::Instant>>>,
    before_response: RecordedResponseHook,
}

#[cfg(test)]
impl RecordedNativeSearchTransport {
    pub(crate) fn block_response(&self) {
        self.blocked
            .store(true, std::sync::atomic::Ordering::Release);
    }
    pub(crate) async fn wait_until_dispatched(&self) {
        self.entered.notified().await;
    }
    pub(crate) fn elapse_deadline(&self) {
        self.deadline_elapsed.notify_one();
    }
    pub(crate) fn captured_deadlines(&self) -> Vec<tokio::time::Instant> {
        self.captured_deadlines.lock().unwrap().clone()
    }
    pub(crate) fn before_response(&self, hook: impl Fn() + Send + Sync + 'static) {
        *self.before_response.lock().unwrap() = Some(Box::new(hook));
    }
    pub(crate) fn call_count(&self) -> u32 {
        self.call_count.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub(crate) fn captured_urls(&self) -> Vec<String> {
        self.captured_urls
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub(crate) fn captured_headers(&self) -> Vec<(String, String)> {
        self.captured_headers
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub(crate) fn authorization_present(&self) -> bool {
        self.captured_headers().iter().any(|(name, value)| {
            !value.is_empty()
                && (name.eq_ignore_ascii_case("authorization")
                    || name.eq_ignore_ascii_case("x-api-key"))
        })
    }
}

#[cfg(test)]
impl NativeSearchTransport for RecordedNativeSearchTransport {
    async fn wait_until_deadline(&self, deadline: tokio::time::Instant) {
        self.captured_deadlines.lock().unwrap().push(deadline);
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => {},
            _ = self.deadline_elapsed.notified() => {},
        }
    }

    async fn post_json(
        &self,
        url: &str,
        _body: &Value,
        headers: &[(String, String)],
        before_dispatch: &(dyn Fn() -> bool + Sync),
    ) -> NativeSearchHttpResult {
        if !before_dispatch() {
            return NativeSearchHttpResult {
                status: 0,
                body: Value::Null,
                transport_failed: true,
            };
        }
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.captured_urls
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(url.to_string());
        self.captured_headers
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .extend(headers.iter().cloned());
        if let Some(hook) = self.before_response.lock().unwrap().as_ref() {
            hook();
        }
        self.entered.notify_one();
        if self.blocked.load(std::sync::atomic::Ordering::Acquire) {
            std::future::pending::<()>().await;
        }
        self.results
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pop_front()
            .unwrap_or(NativeSearchHttpResult {
                status: 0,
                body: Value::Null,
                transport_failed: true,
            })
    }
}

#[cfg(test)]
pub(crate) struct RecordedNativeSearchTransportGuard {
    run_id: String,
    transport: RecordedNativeSearchTransport,
}

#[cfg(test)]
impl std::ops::Deref for RecordedNativeSearchTransportGuard {
    type Target = RecordedNativeSearchTransport;

    fn deref(&self) -> &Self::Target {
        &self.transport
    }
}

#[cfg(test)]
impl Drop for RecordedNativeSearchTransportGuard {
    fn drop(&mut self) {
        recorded_native_search_by_run()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.run_id);
        recorded_native_bearer_by_run()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.run_id);
    }
}

#[cfg(test)]
pub(crate) fn install_recorded_native_search_transport(
    run_id: &str,
    results: Vec<NativeSearchHttpResult>,
    bearer: &str,
) -> RecordedNativeSearchTransportGuard {
    let transport = RecordedNativeSearchTransport {
        results: std::sync::Arc::new(std::sync::Mutex::new(results.into())),
        call_count: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
        captured_urls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        captured_headers: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        blocked: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        entered: std::sync::Arc::new(tokio::sync::Notify::new()),
        deadline_elapsed: std::sync::Arc::new(tokio::sync::Notify::new()),
        captured_deadlines: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        before_response: std::sync::Arc::new(std::sync::Mutex::new(None)),
    };
    recorded_native_search_by_run()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(run_id.to_string(), transport.clone());
    recorded_native_bearer_by_run()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(run_id.to_string(), bearer.to_string());
    RecordedNativeSearchTransportGuard {
        run_id: run_id.to_string(),
        transport,
    }
}

#[cfg(test)]
pub(crate) fn last_live_native_search_trace() -> String {
    LAST_LIVE_TRACE.with(|slot| slot.borrow().clone())
}

#[cfg(test)]
fn summarize_native_search_http(
    url: &str,
    status: u16,
    content_type: Option<&str>,
    raw_len: usize,
    json_ok: bool,
    body: &Value,
) -> String {
    let keys = body
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>().join(","));
    let output_types = collect_json_type_names(body);
    format!(
        "https={} status={status} content_type={} raw_len={raw_len} json_ok={json_ok} keys={} output_status={} error={} types={}",
        url.starts_with("https://"),
        content_type.unwrap_or("-"),
        keys.unwrap_or_else(|| "-".into()),
        body.get("status")
            .or_else(|| body.get("stop_reason"))
            .and_then(Value::as_str)
            .unwrap_or("-"),
        match body.get("error") {
            None => "absent",
            Some(Value::Null) => "null",
            Some(_) => "present",
        },
        output_types.join(",")
    )
}

#[cfg(test)]
fn collect_json_type_names(value: &Value) -> Vec<String> {
    let mut names = Vec::new();
    fn walk(value: &Value, names: &mut Vec<String>) {
        if let Some(kind) = value.get("type").and_then(Value::as_str) {
            if !names.iter().any(|existing| existing == kind) {
                names.push(kind.to_string());
            }
        }
        match value {
            Value::Array(items) => {
                for item in items {
                    walk(item, names);
                }
            }
            Value::Object(object) => {
                for child in object.values() {
                    walk(child, names);
                }
            }
            _ => {}
        }
    }
    walk(value, &mut names);
    names
}

impl NativeSearchTransport for LiveNativeSearchTransport {
    async fn post_json(
        &self,
        url: &str,
        body: &Value,
        headers: &[(String, String)],
        before_dispatch: &(dyn Fn() -> bool + Sync),
    ) -> NativeSearchHttpResult {
        let failed = NativeSearchHttpResult {
            status: 0,
            body: Value::Null,
            transport_failed: true,
        };
        if !headers.iter().any(|(name, value)| {
            !value.is_empty()
                && (name.eq_ignore_ascii_case("authorization")
                    || name.eq_ignore_ascii_case("x-api-key"))
        }) {
            return failed;
        }
        if !url.starts_with("https://") {
            return failed;
        }
        let Ok(client) = native_search_https_client() else {
            return failed;
        };
        let mut request = client.post(url);
        for (name, value) in headers {
            request = request.header(name.as_str(), value.as_str());
        }
        let Ok(request) = request.json(body).build() else {
            return failed;
        };
        if !before_dispatch() {
            return failed;
        }
        let request = client.execute(request).await;
        let Ok(response) = request else {
            return failed;
        };
        let status = response.status().as_u16();
        #[cfg(test)]
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let raw = response.text().await.unwrap_or_default();
        let parsed = serde_json::from_str::<Value>(&raw);
        #[cfg(test)]
        let json_ok = parsed.is_ok();
        let body = parsed.unwrap_or(Value::Null);
        #[cfg(test)]
        {
            LAST_LIVE_TRACE.with(|slot| {
                *slot.borrow_mut() = summarize_native_search_http(
                    url,
                    status,
                    content_type.as_deref(),
                    raw.len(),
                    json_ok,
                    &body,
                );
            });
        }
        NativeSearchHttpResult {
            status,
            body,
            transport_failed: false,
        }
    }
}

fn native_search_https_client() -> crate::error::AppResult<reqwest::Client> {
    crate::network::cert_pinning::https_client_builder()
        .timeout(NATIVE_SEARCH_HTTP_TIMEOUT)
        .read_timeout(NATIVE_SEARCH_HTTP_TIMEOUT)
        .build()
        .map_err(|error| crate::error::AppError::msg(error.to_string()))
}

fn attach_reported_usage(outcome: &mut RouteAttemptOutcome, body: &Value) {
    outcome.prompt_tokens = extract_reported_prompt_tokens(body);
    outcome.completion_tokens = extract_reported_completion_tokens(body);
}

fn outcome_from_isolated_parse(parse: NativeSearchParse, body: &Value) -> RouteAttemptOutcome {
    let mut outcome = parse.into_route_outcome();
    attach_reported_usage(&mut outcome, body);
    outcome
}

fn protocol_insufficient() -> RouteAttemptOutcome {
    RouteAttemptOutcome {
        failure: Some(RouteFailureClass::ProtocolOrResultInsufficient),
        generated_text_only: true,
        ..RouteAttemptOutcome::default()
    }
}

fn transport_failure() -> RouteAttemptOutcome {
    RouteAttemptOutcome {
        failure: Some(RouteFailureClass::TransportOrProviderFailure),
        ..RouteAttemptOutcome::default()
    }
}

fn temporary_failure() -> RouteAttemptOutcome {
    RouteAttemptOutcome {
        failure: Some(RouteFailureClass::TemporaryFailure),
        ..RouteAttemptOutcome::default()
    }
}
