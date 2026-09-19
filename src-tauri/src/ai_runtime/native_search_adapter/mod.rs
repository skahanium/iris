//! Per-model native-search adapters (K12).
//!
//! One model, one file, one registry entry. Shared code here only looks up the
//! adapter and runs HTTPS / credential / construct. It must not grow MiniMax,
//! DeepSeek, or Gemini field names. Add a new model by adding a sibling file
//! and one line in [`PRODUCTION_ADAPTERS`].

mod minimax_m3;

use std::time::Duration;

use serde_json::Value;

use crate::ai_runtime::dual_path_search::{
    NativeSearchSupport, RouteAttemptOutcome, RouteFailureClass, SearchActionIdentity,
};
use crate::ai_runtime::native_search_subrequest::{
    construct_native_search_subrequest, native_search_support_for, NativeSearchEndpointRef,
    NativeSearchParse, NativeSearchPublicScope, NativeSearchRequestIdentity,
    NativeSearchSubrequest, NativeSearchSubrequestDraft,
};
use crate::credentials::llm_credential_service;

use self::minimax_m3::MinimaxM3NativeSearchAdapter;

/// MCP-only web_search stays at 20s. MiniMax native search is a second serial
/// HTTPS Responses call (live probe ~4s, hang bound 60s), so the outer tool
/// deadline must cover both or MCP success is cancelled with the native call.
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
        bearer: Option<&str>,
    ) -> NativeSearchHttpResult;
}

/// Protocol surface for one adapted model. Keep implementations in sibling files.
pub(crate) trait NativeSearchModelAdapter: Send + Sync {
    /// Stable adapter identity. Used by registry tests and future diagnostics.
    #[allow(dead_code)]
    fn id(&self) -> &'static str;
    fn matches(&self, model_id: &str) -> bool;
    fn request_url(&self, api_base: &str) -> Result<String, RouteFailureClass>;
    fn outbound_body(&self, subrequest: &NativeSearchSubrequest) -> Value;
    fn parse_response(&self, body: &Value) -> NativeSearchParse;
    /// MiniMax Responses sometimes returns HTTP 200 without `web_search_call`.
    fn retry_text_only_once(&self) -> bool {
        false
    }
}

/// Production adapters. Order does not imply priority; lookup is exclusive match.
const PRODUCTION_ADAPTERS: &[&dyn NativeSearchModelAdapter] = &[&MinimaxM3NativeSearchAdapter];

/// Number of production native-search adapters. Not a capability advertisement.
pub(crate) fn production_adapter_count() -> usize {
    PRODUCTION_ADAPTERS.len()
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
    let mut result = transport.post_json(&url, &body, Some(token)).await;
    if result.transport_failed {
        return transport_failure();
    }
    let mut outcome = match result.status {
        200 => adapter.parse_response(&result.body).into_route_outcome(),
        401 | 403 | 429 => return temporary_failure(),
        _ => return transport_failure(),
    };
    if adapter.retry_text_only_once()
        && result.status == 200
        && outcome.generated_text_only
        && !outcome.has_retrieval_credentials
    {
        result = transport.post_json(&url, &body, Some(token)).await;
        if result.transport_failed {
            return transport_failure();
        }
        outcome = match result.status {
            200 => adapter.parse_response(&result.body).into_route_outcome(),
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
    let provider_id = crate::llm::model_catalog::find_model(adapter.id())
        .map(|entry| entry.provider_id.to_string())
        .unwrap_or_default();
    let api_base = endpoint
        .api_base
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| crate::llm::providers::api_base(&provider_id, None));
    let service = endpoint
        .credential_service
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| llm_credential_service(&provider_id));
    let secret = crate::credentials::get_runtime_secret(&service).ok();
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
    execute_native_search(
        draft,
        &LiveNativeSearchTransport,
        secret.as_ref().map(|value| value.as_str()),
        &api_base,
    )
    .await
}

struct LiveNativeSearchTransport;

#[cfg(test)]
thread_local! {
    static LAST_LIVE_TRACE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
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
        "url_ok={} status={status} content_type={} raw_len={raw_len} json_ok={json_ok} keys={} output_status={} error={} types={}",
        url.starts_with("https://") && url.contains("/responses"),
        content_type.unwrap_or("-"),
        keys.unwrap_or_else(|| "-".into()),
        body.get("status").and_then(Value::as_str).unwrap_or("-"),
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
        bearer: Option<&str>,
    ) -> NativeSearchHttpResult {
        let failed = NativeSearchHttpResult {
            status: 0,
            body: Value::Null,
            transport_failed: true,
        };
        let Some(token) = bearer.filter(|value| !value.is_empty()) else {
            return failed;
        };
        if !url.starts_with("https://") {
            return failed;
        }
        let Ok(client) = native_search_https_client() else {
            return failed;
        };
        let request = client
            .post(url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(body)
            .send()
            .await;
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
