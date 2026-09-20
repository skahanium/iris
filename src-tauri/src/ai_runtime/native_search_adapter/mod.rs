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
    collect_search_event_kinds, construct_native_search_subrequest,
    extract_reported_completion_tokens, extract_reported_prompt_tokens, native_search_support_for,
    NativeSearchEndpointRef, NativeSearchParse, NativeSearchPublicScope,
    NativeSearchRequestIdentity, NativeSearchSubrequest, NativeSearchSubrequestDraft,
    RetrievalObservation,
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
    async fn post_json(
        &self,
        url: &str,
        body: &Value,
        headers: &[(String, String)],
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

/// Execute one constructed native-search subrequest through `transport`.
pub(crate) async fn execute_native_search<T: NativeSearchTransport>(
    draft: NativeSearchSubrequestDraft,
    transport: &T,
    bearer: Option<&str>,
    api_base: &str,
) -> RouteAttemptOutcome {
    let Some(adapter) = lookup_production_adapter(&draft.endpoint.model_id) else {
        return protocol_insufficient();
    };
    let constructed = match construct_native_search_subrequest(draft) {
        Ok(subrequest) => subrequest,
        Err(_) => return protocol_insufficient(),
    };
    let url = match adapter.request_url(api_base) {
        Ok(url) if url.starts_with("https://") => url,
        _ => return transport_failure(),
    };
    let Some(token) = bearer.filter(|value| !value.is_empty()) else {
        return temporary_failure();
    };
    let body = adapter.outbound_body(&constructed);
    let headers = adapter.http_headers(token);
    let mut result = transport.post_json(&url, &body, &headers).await;
    if result.transport_failed {
        return transport_failure();
    }
    let mut outcome = match result.status {
        200 => outcome_from_isolated_parse(
            adapter.parse_response(&result.body),
            &constructed,
            &result.body,
        ),
        401 | 403 | 429 => return temporary_failure(),
        _ => return transport_failure(),
    };
    if adapter.retry_text_only_once()
        && result.status == 200
        && outcome.generated_text_only
        && !outcome.has_retrieval_credentials
    {
        result = transport.post_json(&url, &body, &headers).await;
        if result.transport_failed {
            return transport_failure();
        }
        outcome = match result.status {
            200 => outcome_from_isolated_parse(
                adapter.parse_response(&result.body),
                &constructed,
                &result.body,
            ),
            401 | 403 | 429 => temporary_failure(),
            _ => transport_failure(),
        };
    }
    outcome
}

/// Production native route: hydrate credentials and POST through the live client.
pub(crate) async fn execute_production_route(
    endpoint: Option<&NativeSearchEndpointRef>,
    identity: &SearchActionIdentity,
    query: &str,
) -> RouteAttemptOutcome {
    let Some(endpoint) = endpoint.cloned() else {
        return protocol_insufficient();
    };
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
            recorded_bearer_for_run(&identity.run_id).or_else(|| {
                crate::credentials::get_runtime_secret(&binding.credential_service)
                    .ok()
                    .map(|value| value.to_string())
            })
        }
        #[cfg(not(test))]
        {
            crate::credentials::get_runtime_secret(&binding.credential_service)
                .ok()
                .map(|value| value.to_string())
        }
    };
    let draft = NativeSearchSubrequestDraft {
        query: query.to_string(),
        public_scope: NativeSearchPublicScope::default(),
        identity: NativeSearchRequestIdentity {
            run_id: identity.run_id.clone(),
            input_revision: identity.input_revision,
            parent_call_id: identity.action_id.clone(),
            attempt: identity.attempt,
        },
        endpoint,
        private_material: None,
    };
    #[cfg(test)]
    if let Some(transport) = recorded_transport_for_run(&identity.run_id) {
        return execute_native_search(draft, &transport, secret.as_deref(), &binding.api_base)
            .await;
    }
    execute_native_search(
        draft,
        &LiveNativeSearchTransport,
        secret.as_deref(),
        &binding.api_base,
    )
    .await
}

struct LiveNativeSearchTransport;

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
#[derive(Clone)]
pub(crate) struct RecordedNativeSearchTransport {
    results: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<NativeSearchHttpResult>>>,
    call_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    captured_urls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    captured_headers: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
}

#[cfg(test)]
impl RecordedNativeSearchTransport {
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
    async fn post_json(
        &self,
        url: &str,
        _body: &Value,
        headers: &[(String, String)],
    ) -> NativeSearchHttpResult {
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
        let request = request.json(body).send().await;
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

fn outcome_from_isolated_parse(
    parse: NativeSearchParse,
    constructed: &NativeSearchSubrequest,
    body: &Value,
) -> RouteAttemptOutcome {
    let observation = RetrievalObservation::from_isolated_parse(
        SearchActionIdentity {
            run_id: constructed.identity.run_id.clone(),
            input_revision: constructed.identity.input_revision,
            action_id: constructed.identity.parent_call_id.clone(),
            attempt: constructed.identity.attempt,
        },
        &parse,
        collect_search_event_kinds(body),
        extract_reported_prompt_tokens(body),
        extract_reported_completion_tokens(body),
    );
    let mut outcome = parse.into_route_outcome();
    outcome.prompt_tokens = observation.prompt_tokens;
    outcome.completion_tokens = observation.completion_tokens;
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
