//! C20 dual-path web search coordinator (K11 mechanical subset).
//!
//! Native search is supported only for models with a registered per-model
//! adapter. Unadapted endpoints stay `unsupported` and MCP remains one route.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

use crate::ai_runtime::native_search_subrequest::{
    native_search_support_for, native_search_unsupported_reason_for, NativeSearchEndpointRef,
    NativeSearchUnsupportedReason,
};

#[cfg(test)]
use std::sync::atomic::{AtomicU32, Ordering};

/// Identity of one `web_search` action under K11.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchActionIdentity {
    pub run_id: String,
    pub input_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_run_id: Option<String>,
    #[serde(default)]
    pub model_turn: u32,
    #[serde(default)]
    pub tool_surface_version: String,
    pub action_id: String,
    pub attempt: u32,
}

/// Native search capability fact from C10's minimal surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NativeSearchSupport {
    Unsupported,
    Available,
    TemporarilyFailed,
}

/// Which internal search route produced a candidate URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SearchChannel {
    Native,
    Mcp,
}

/// Failure class for one route. Mutually exclusive with `succeeded`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RouteFailureClass {
    ProtocolOrResultInsufficient,
    TemporaryFailure,
    TransportOrProviderFailure,
}

/// K11 per-route status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RouteStatus {
    pub supported: bool,
    pub attempted: bool,
    pub succeeded: bool,
    pub failed: Option<RouteFailureClass>,
}

impl RouteStatus {
    #[cfg(test)]
    fn legal(&self) -> bool {
        if self.succeeded && self.failed.is_some() {
            return false;
        }
        if self.succeeded && !self.attempted {
            return false;
        }
        if self.failed.is_some() && !self.attempted {
            return false;
        }
        if !self.supported && self.attempted {
            return false;
        }
        true
    }
}

/// One merged URL candidate with channel tags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DualPathCandidate {
    pub url: String,
    pub canonical_url: String,
    pub title: String,
    pub snippet: String,
    pub channels: Vec<SearchChannel>,
}

/// Credentialed hits returned by one route executor.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchHit {
    pub url: String,
    pub title: String,
    pub snippet: String,
}

/// Result of executing (or skipping) one search route.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct RouteAttemptOutcome {
    pub candidates: Vec<SearchHit>,
    pub has_retrieval_credentials: bool,
    pub generated_text_only: bool,
    pub failure: Option<RouteFailureClass>,
    /// MCP providers tried inside one route. Never a dual-path count.
    pub internal_provider_attempts: u32,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub dispatched_attempts: u32,
    pub usage_unknown: bool,
}

/// Successful search-request counts reserved for K11 usage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DualPathSearchUsage {
    pub native: u32,
    pub mcp: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_prompt_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_completion_tokens: Option<u32>,
    #[serde(default)]
    pub native_attempts: u32,
    #[serde(default)]
    pub mcp_attempts: u32,
    #[serde(default)]
    pub native_usage_unknown: bool,
}

/// Coordinator input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DualPathSearchRequest {
    pub identity: SearchActionIdentity,
    pub query: String,
    pub allow_second_route: bool,
}

/// K11 mechanical subset produced by C20.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DualPathSearchOutcome {
    pub identity: SearchActionIdentity,
    pub native: RouteStatus,
    pub mcp: RouteStatus,
    pub candidates: Vec<DualPathCandidate>,
    pub usage: DualPathSearchUsage,
    pub shortage: Option<String>,
    pub executed_in_parallel: bool,
    pub mcp_internal_provider_attempts: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_unsupported_reason: Option<NativeSearchUnsupportedReason>,
    #[serde(default)]
    pub native_has_retrieval_credentials: bool,
}

impl DualPathSearchOutcome {
    /// Both routes were supported and actually attempted. Not a success claim.
    pub(crate) fn both_available_routes_attempted(&self) -> bool {
        self.native.supported && self.mcp.supported && self.native.attempted && self.mcp.attempted
    }

    #[cfg(test)]
    fn native_or_mcp_succeeded(&self) -> bool {
        self.native.succeeded || self.mcp.succeeded
    }
}

/// Capability probe for the native search route.
pub(crate) trait NativeSearchSupportProbe {
    fn native_search_support(&self) -> NativeSearchSupport;
    fn native_search_unsupported_reason(&self) -> Option<NativeSearchUnsupportedReason> {
        match self.native_search_support() {
            NativeSearchSupport::Unsupported => Some(NativeSearchUnsupportedReason::AdapterAbsent),
            _ => None,
        }
    }
}

/// One injectable search route (native or MCP).
pub(crate) trait SearchRoute {
    async fn execute(&self, query: &str) -> RouteAttemptOutcome;
    fn owns_execution_deadline(&self) -> bool {
        false
    }
}

/// Production native support: consult C10 per-endpoint probe plus the per-model
/// adapter registry.
#[derive(Debug, Clone, Default)]
pub(crate) struct ProductionNativeSearchSupport {
    pub endpoint: Option<NativeSearchEndpointRef>,
}

impl NativeSearchSupportProbe for ProductionNativeSearchSupport {
    fn native_search_support(&self) -> NativeSearchSupport {
        native_search_support_for(self.endpoint.as_ref())
    }

    fn native_search_unsupported_reason(&self) -> Option<NativeSearchUnsupportedReason> {
        native_search_unsupported_reason_for(self.endpoint.as_ref())
    }
}

impl NativeSearchSupportProbe for NativeSearchSupport {
    fn native_search_support(&self) -> NativeSearchSupport {
        *self
    }
}

/// Native executor used in production. Must not be called while the probe is unsupported.
#[derive(Clone, Default)]
pub(crate) struct ProductionNativeSearchRoute<'a> {
    pub endpoint: Option<NativeSearchEndpointRef>,
    pub identity: SearchActionIdentity,
    pub control: Option<&'a crate::ai_runtime::native_search_adapter::SearchExecutionControl<'a>>,
}

impl SearchRoute for ProductionNativeSearchRoute<'_> {
    fn owns_execution_deadline(&self) -> bool {
        true
    }
    async fn execute(&self, query: &str) -> RouteAttemptOutcome {
        let Some(control) = self.control else {
            return RouteAttemptOutcome {
                failure: Some(RouteFailureClass::ProtocolOrResultInsufficient),
                ..Default::default()
            };
        };
        crate::ai_runtime::native_search_adapter::execute_production_route(
            control,
            self.endpoint.as_ref(),
            &self.identity,
            query,
        )
        .await
    }
}

/// Counted test double for one route.
#[cfg(test)]
#[derive(Debug)]
struct ScriptedSearchRoute {
    calls: AtomicU32,
    outcome: RouteAttemptOutcome,
}

#[cfg(test)]
impl ScriptedSearchRoute {
    fn new(outcome: RouteAttemptOutcome) -> Self {
        Self {
            calls: AtomicU32::new(0),
            outcome,
        }
    }

    fn call_count(&self) -> u32 {
        self.calls.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
impl SearchRoute for ScriptedSearchRoute {
    async fn execute(&self, _query: &str) -> RouteAttemptOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.outcome.clone()
    }
}

/// Coordinate native and MCP search as two K11 routes.
///
/// MCP failover is one route. Native is the second route when supported.
/// First slice runs attempted routes serially and records `executed_in_parallel=false`.
pub(crate) async fn coordinate_dual_path_search<P, N, M>(
    request: DualPathSearchRequest,
    probe: &P,
    native: &N,
    mcp: &M,
) -> DualPathSearchOutcome
where
    P: NativeSearchSupportProbe,
    N: SearchRoute,
    M: SearchRoute,
{
    let native_support = probe.native_search_support();
    let attempt_native =
        matches!(native_support, NativeSearchSupport::Available) && request.allow_second_route;

    // MCP is the existing production route and is attempted first. Native, when
    // allowed, is the second route and runs after MCP in this serial slice.
    let mcp_outcome = mcp.execute(&request.query).await;
    let native_outcome = if attempt_native
        && !crate::ai_runtime::model_gateway::is_abort_requested(&request.identity.run_id)
    {
        Some(execute_native_with_timeout(native, &request.query).await)
    } else {
        None
    };

    let native_status = native_route_status(native_support, native_outcome.as_ref());
    let mcp_status = route_status_from_outcome(true, &mcp_outcome);
    let shortage = insufficiency_note(
        native_support,
        request.allow_second_route,
        &mcp_outcome,
        native_outcome.as_ref(),
    );
    let usage = DualPathSearchUsage {
        native: u32::from(native_status.succeeded),
        mcp: u32::from(mcp_status.succeeded),
        native_prompt_tokens: native_outcome
            .as_ref()
            .and_then(|outcome| outcome.prompt_tokens),
        native_completion_tokens: native_outcome
            .as_ref()
            .and_then(|outcome| outcome.completion_tokens),
        native_attempts: native_outcome
            .as_ref()
            .map_or(0, |outcome| outcome.dispatched_attempts),
        mcp_attempts: mcp_outcome.dispatched_attempts,
        native_usage_unknown: native_outcome
            .as_ref()
            .is_some_and(|outcome| outcome.usage_unknown),
    };
    let candidates = merge_route_candidates(
        native_status
            .succeeded
            .then_some(native_outcome.as_ref())
            .flatten(),
        mcp_status.succeeded.then_some(&mcp_outcome),
    );

    DualPathSearchOutcome {
        identity: request.identity,
        native: native_status,
        mcp: mcp_status,
        candidates,
        usage,
        shortage,
        executed_in_parallel: false,
        mcp_internal_provider_attempts: mcp_outcome.internal_provider_attempts,
        native_unsupported_reason: match native_support {
            NativeSearchSupport::Unsupported => probe.native_search_unsupported_reason(),
            _ => None,
        },
        native_has_retrieval_credentials: native_outcome
            .as_ref()
            .is_some_and(|outcome| outcome.has_retrieval_credentials),
    }
}

fn native_route_timeout() -> Duration {
    #[cfg(test)]
    {
        Duration::from_millis(80)
    }
    #[cfg(not(test))]
    {
        Duration::from_secs(60)
    }
}

async fn execute_native_with_timeout<N: SearchRoute>(
    native: &N,
    query: &str,
) -> RouteAttemptOutcome {
    if native.owns_execution_deadline() {
        return native.execute(query).await;
    }
    match tokio::time::timeout(native_route_timeout(), native.execute(query)).await {
        Ok(outcome) => outcome,
        Err(_) => RouteAttemptOutcome {
            failure: Some(RouteFailureClass::TemporaryFailure),
            internal_provider_attempts: 1,
            dispatched_attempts: 1,
            usage_unknown: true,
            ..RouteAttemptOutcome::default()
        },
    }
}

fn native_route_status(
    support: NativeSearchSupport,
    outcome: Option<&RouteAttemptOutcome>,
) -> RouteStatus {
    match support {
        NativeSearchSupport::Unsupported => RouteStatus {
            supported: false,
            attempted: false,
            succeeded: false,
            failed: None,
        },
        NativeSearchSupport::TemporarilyFailed => RouteStatus {
            supported: true,
            attempted: false,
            succeeded: false,
            failed: None,
        },
        NativeSearchSupport::Available => match outcome {
            Some(outcome) => route_status_from_outcome(true, outcome),
            None => RouteStatus {
                supported: true,
                attempted: false,
                succeeded: false,
                failed: None,
            },
        },
    }
}

fn route_status_from_outcome(supported: bool, outcome: &RouteAttemptOutcome) -> RouteStatus {
    if outcome.dispatched_attempts == 0 {
        // K11's state machine keeps `failed ⊆ attempted`, so a pre-dispatch
        // refusal cannot claim `failed` here. Its failure class must instead be
        // named in the insufficiency note (`insufficiency_note`); silently
        // folding it away would read as "never tried, nothing wrong".
        return RouteStatus {
            supported,
            ..Default::default()
        };
    }
    let protocol_insufficient = outcome.generated_text_only || !outcome.has_retrieval_credentials;
    if protocol_insufficient {
        return RouteStatus {
            supported,
            attempted: true,
            succeeded: false,
            failed: Some(
                outcome
                    .failure
                    .unwrap_or(RouteFailureClass::ProtocolOrResultInsufficient),
            ),
        };
    }
    if let Some(failure) = outcome.failure {
        return RouteStatus {
            supported,
            attempted: true,
            succeeded: false,
            failed: Some(failure),
        };
    }
    RouteStatus {
        supported,
        attempted: true,
        succeeded: true,
        failed: None,
    }
}

fn merge_route_candidates(
    native: Option<&RouteAttemptOutcome>,
    mcp: Option<&RouteAttemptOutcome>,
) -> Vec<DualPathCandidate> {
    let mut merged = Vec::new();
    if let Some(outcome) = native {
        for hit in &outcome.candidates {
            push_or_merge_candidate(&mut merged, hit, SearchChannel::Native);
        }
    }
    if let Some(outcome) = mcp {
        for hit in &outcome.candidates {
            push_or_merge_candidate(&mut merged, hit, SearchChannel::Mcp);
        }
    }
    merged
}

fn push_or_merge_candidate(
    merged: &mut Vec<DualPathCandidate>,
    hit: &SearchHit,
    channel: SearchChannel,
) {
    let canonical_url = canonicalize_search_url(&hit.url);
    if let Some(existing) = merged
        .iter_mut()
        .find(|candidate| candidate.canonical_url == canonical_url)
    {
        if !existing.channels.contains(&channel) {
            existing.channels.push(channel);
        }
        return;
    }
    merged.push(DualPathCandidate {
        url: hit.url.clone(),
        canonical_url,
        title: hit.title.clone(),
        snippet: hit.snippet.clone(),
        channels: vec![channel],
    });
}

/// Structured dual-path status for tool / diagnostic payloads. Must not say 能力降级.
pub(crate) fn dual_path_status_json(outcome: &DualPathSearchOutcome) -> Value {
    json!({
        "requestIdentity": {
            "runId": outcome.identity.run_id,
            "inputRevision": outcome.identity.input_revision,
            "childRunId": outcome.identity.child_run_id,
            "modelTurn": outcome.identity.model_turn,
            "toolSurfaceVersion": outcome.identity.tool_surface_version,
            "actionId": outcome.identity.action_id,
            "attempt": outcome.identity.attempt,
        },
        "native": native_route_status_json(outcome),
        "mcp": route_status_json(&outcome.mcp),
        "candidates": outcome.candidates.iter().map(|candidate| {
            json!({
                "url": candidate.url,
                "canonicalUrl": candidate.canonical_url,
                "title": candidate.title,
                "snippet": candidate.snippet,
                "channels": candidate.channels.iter().map(channel_name).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
        "usage": {
            "native": outcome.usage.native,
            "mcp": outcome.usage.mcp,
            "nativePromptTokens": outcome.usage.native_prompt_tokens,
            "nativeCompletionTokens": outcome.usage.native_completion_tokens,
            "nativeAttempts": outcome.usage.native_attempts,
            "mcpAttempts": outcome.usage.mcp_attempts,
            "nativeUsageUnknown": outcome.usage.native_usage_unknown,
        },
        "shortage": outcome.shortage,
        "executedInParallel": outcome.executed_in_parallel,
        "mcpInternalProviderAttempts": outcome.mcp_internal_provider_attempts,
        "bothAvailableRoutesAttempted": outcome.both_available_routes_attempted(),
    })
}

fn route_status_json(status: &RouteStatus) -> Value {
    json!({
        "supported": status.supported,
        "attempted": status.attempted,
        "succeeded": status.succeeded,
        "failed": status.failed.map(failure_class_name),
    })
}

fn native_route_status_json(outcome: &DualPathSearchOutcome) -> Value {
    let mut native = route_status_json(&outcome.native);
    native["unsupportedReason"] = match outcome.native_unsupported_reason {
        Some(reason) => json!(reason),
        None => Value::Null,
    };
    native["hasRetrievalCredentials"] = json!(outcome.native_has_retrieval_credentials);
    native["citationCount"] = json!(outcome
        .candidates
        .iter()
        .filter(|candidate| candidate.channels.contains(&SearchChannel::Native))
        .count());
    native
}

fn channel_name(channel: &SearchChannel) -> &'static str {
    match channel {
        SearchChannel::Native => "native",
        SearchChannel::Mcp => "mcp",
    }
}

fn failure_class_name(class: RouteFailureClass) -> &'static str {
    match class {
        RouteFailureClass::ProtocolOrResultInsufficient => "protocol_or_result_insufficient",
        RouteFailureClass::TemporaryFailure => "temporary_failure",
        RouteFailureClass::TransportOrProviderFailure => "transport_or_provider_failure",
    }
}

/// K11: `supported && !attempted` is legal but never a compliant dual-route
/// execution, so it must carry an insufficiency note. Zero-dispatch failures
/// name their stable failure class here instead of being silently folded away.
fn insufficiency_note(
    support: NativeSearchSupport,
    allow_second_route: bool,
    mcp_outcome: &RouteAttemptOutcome,
    native_outcome: Option<&RouteAttemptOutcome>,
) -> Option<String> {
    match support {
        NativeSearchSupport::Available if !allow_second_route => {
            return Some(
                "本次额度不足以启动第二条搜索路线；不以单路结果冒充双路完成。".to_string(),
            );
        }
        NativeSearchSupport::TemporarilyFailed => {
            return Some(
                "原生搜索能力探测临时不可用，本次未尝试第二条搜索路线；不以单路结果冒充双路完成。"
                    .to_string(),
            );
        }
        _ => {}
    }
    if let Some(failure) = native_outcome
        .filter(|outcome| outcome.dispatched_attempts == 0)
        .and_then(|outcome| outcome.failure)
    {
        return Some(format!(
            "原生搜索路线未派发：{}；不以单路结果冒充双路完成。",
            failure_class_name(failure)
        ));
    }
    if let Some(failure) = mcp_outcome
        .failure
        .filter(|_| mcp_outcome.dispatched_attempts == 0)
    {
        return Some(format!(
            "MCP 搜索路线未派发：{}。",
            failure_class_name(failure)
        ));
    }
    None
}

fn canonicalize_search_url(url: &str) -> String {
    let trimmed = url.trim();
    let Ok(mut normalized) = reqwest::Url::parse(trimmed) else {
        return trimmed
            .split_once('#')
            .map_or_else(|| trimmed.to_string(), |(before, _)| before.to_string());
    };
    normalized.set_fragment(None);
    if matches!(
        (normalized.scheme(), normalized.port()),
        ("https", Some(443)) | ("http", Some(80))
    ) {
        let _ = normalized.set_port(None);
    }
    normalized.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn review_regression_d_rejected_route_is_not_an_attempt() {
        let native = ScriptedSearchRoute::new(RouteAttemptOutcome {
            failure: Some(RouteFailureClass::TemporaryFailure),
            ..Default::default()
        });
        let mcp = ScriptedSearchRoute::new(credentialed(vec![hit("https://mcp.example/kept")], 1));
        let outcome = coordinate_dual_path_search(
            request(true),
            &NativeSearchSupport::Available,
            &native,
            &mcp,
        )
        .await;
        assert!(!outcome.native.attempted);
        assert!(outcome.native.failed.is_none());
        assert!(outcome.mcp.succeeded);
    }

    #[tokio::test]
    async fn review_regression_d_failed_attempts_keep_usage_unknown_and_known_tokens() {
        let native = ScriptedSearchRoute::new(RouteAttemptOutcome {
            failure: Some(RouteFailureClass::TemporaryFailure),
            dispatched_attempts: 2,
            internal_provider_attempts: 2,
            prompt_tokens: Some(11),
            completion_tokens: Some(7),
            usage_unknown: true,
            ..Default::default()
        });
        let mcp = ScriptedSearchRoute::new(credentialed(vec![hit("https://mcp.example/kept")], 1));
        let outcome = coordinate_dual_path_search(
            request(true),
            &NativeSearchSupport::Available,
            &native,
            &mcp,
        )
        .await;
        let status = dual_path_status_json(&outcome);
        assert_eq!(status["usage"]["nativeAttempts"], 2);
        assert_eq!(status["usage"]["nativeUsageUnknown"], true);
        assert_eq!(status["usage"]["nativePromptTokens"], 11);
        assert_eq!(status["usage"]["native"], 0);
        assert!(outcome.mcp.succeeded);
    }

    fn identity() -> SearchActionIdentity {
        SearchActionIdentity {
            run_id: "run-1".into(),
            input_revision: "1".into(),
            child_run_id: None,
            model_turn: 1,
            tool_surface_version: "fixture-tools-v1".into(),
            action_id: "search-1".into(),
            attempt: 1,
        }
    }

    fn request(allow_second_route: bool) -> DualPathSearchRequest {
        DualPathSearchRequest {
            identity: identity(),
            query: "example query".into(),
            allow_second_route,
        }
    }

    fn hit(url: &str) -> SearchHit {
        SearchHit {
            url: url.into(),
            title: "title".into(),
            snippet: "snippet".into(),
        }
    }

    fn credentialed(hits: Vec<SearchHit>, internal_provider_attempts: u32) -> RouteAttemptOutcome {
        RouteAttemptOutcome {
            candidates: hits,
            has_retrieval_credentials: true,
            generated_text_only: false,
            failure: None,
            internal_provider_attempts,
            dispatched_attempts: internal_provider_attempts,
            ..Default::default()
        }
    }

    fn transport_failure(internal_provider_attempts: u32) -> RouteAttemptOutcome {
        RouteAttemptOutcome {
            candidates: Vec::new(),
            has_retrieval_credentials: false,
            generated_text_only: false,
            failure: Some(RouteFailureClass::TransportOrProviderFailure),
            internal_provider_attempts,
            dispatched_attempts: internal_provider_attempts,
            ..Default::default()
        }
    }

    fn assert_legal(outcome: &DualPathSearchOutcome) {
        assert!(outcome.native.legal(), "native status illegal: {outcome:?}");
        assert!(outcome.mcp.legal(), "mcp status illegal: {outcome:?}");
        assert!(!outcome.executed_in_parallel);
    }

    fn assert_no_degradation_copy(outcome: &DualPathSearchOutcome) {
        let blob = format!("{outcome:?}{}", dual_path_status_json(outcome));
        for needle in ["能力降级", "模型能力降级", "模型出错"] {
            assert!(
                !blob.contains(needle),
                "user-visible dual-path copy must not say {needle}: {blob}"
            );
        }
    }

    fn channels_of(outcome: &DualPathSearchOutcome, canonical_url: &str) -> Vec<SearchChannel> {
        outcome
            .candidates
            .iter()
            .find(|candidate| candidate.canonical_url == canonical_url)
            .map(|candidate| candidate.channels.clone())
            .unwrap_or_default()
    }

    #[tokio::test]
    async fn both_supported_routes_are_attempted_with_credentialed_success() {
        let native =
            ScriptedSearchRoute::new(credentialed(vec![hit("https://native.example/a")], 1));
        let mcp = ScriptedSearchRoute::new(credentialed(vec![hit("https://mcp.example/b")], 1));

        let outcome = coordinate_dual_path_search(
            request(true),
            &NativeSearchSupport::Available,
            &native,
            &mcp,
        )
        .await;

        assert_legal(&outcome);
        assert_eq!(native.call_count(), 1);
        assert_eq!(mcp.call_count(), 1);
        assert!(outcome.native.supported && outcome.native.attempted && outcome.native.succeeded);
        assert!(outcome.mcp.supported && outcome.mcp.attempted && outcome.mcp.succeeded);
        assert!(outcome.both_available_routes_attempted());
        assert_eq!(outcome.usage.native, 1);
        assert_eq!(outcome.usage.mcp, 1);
        assert_eq!(outcome.candidates.len(), 2);
        assert!(channels_of(&outcome, "https://native.example/a").contains(&SearchChannel::Native));
        assert!(channels_of(&outcome, "https://mcp.example/b").contains(&SearchChannel::Mcp));
    }

    #[tokio::test]
    async fn same_canonical_url_merges_into_one_candidate_with_two_channels() {
        let native = ScriptedSearchRoute::new(credentialed(
            vec![hit("https://example.com/story#from-native")],
            1,
        ));
        let mcp = ScriptedSearchRoute::new(credentialed(vec![hit("https://example.com/story")], 1));

        let outcome = coordinate_dual_path_search(
            request(true),
            &NativeSearchSupport::Available,
            &native,
            &mcp,
        )
        .await;

        assert_legal(&outcome);
        assert_eq!(outcome.candidates.len(), 1);
        assert_eq!(
            outcome.candidates[0].canonical_url,
            canonicalize_search_url("https://example.com/story")
        );
        assert_eq!(
            outcome.candidates[0].channels,
            vec![SearchChannel::Native, SearchChannel::Mcp]
        );
    }

    #[tokio::test]
    async fn native_unsupported_is_normal_single_mcp_without_degradation_copy() {
        let native = ScriptedSearchRoute::new(credentialed(
            vec![hit("https://native.example/should-not-run")],
            1,
        ));
        let mcp = ScriptedSearchRoute::new(credentialed(vec![hit("https://mcp.example/b")], 1));

        let outcome = coordinate_dual_path_search(
            request(true),
            &NativeSearchSupport::Unsupported,
            &native,
            &mcp,
        )
        .await;

        assert_legal(&outcome);
        assert_eq!(native.call_count(), 0);
        assert_eq!(mcp.call_count(), 1);
        assert!(!outcome.native.supported);
        assert!(!outcome.native.attempted);
        assert!(!outcome.native.succeeded);
        assert!(outcome.native.failed.is_none());
        assert!(outcome.mcp.supported && outcome.mcp.attempted && outcome.mcp.succeeded);
        assert!(!outcome.both_available_routes_attempted());
        assert_eq!(outcome.usage.native, 0);
        assert_eq!(outcome.usage.mcp, 1);
        assert_no_degradation_copy(&outcome);
        assert!(outcome.shortage.is_none());
    }

    #[tokio::test]
    async fn native_failure_keeps_mcp_success() {
        let native = ScriptedSearchRoute::new(transport_failure(1));
        let mcp = ScriptedSearchRoute::new(credentialed(vec![hit("https://mcp.example/b")], 1));

        let outcome = coordinate_dual_path_search(
            request(true),
            &NativeSearchSupport::Available,
            &native,
            &mcp,
        )
        .await;

        assert_legal(&outcome);
        assert_eq!(native.call_count(), 1);
        assert_eq!(mcp.call_count(), 1);
        assert!(outcome.native.attempted && !outcome.native.succeeded);
        assert_eq!(
            outcome.native.failed,
            Some(RouteFailureClass::TransportOrProviderFailure)
        );
        assert!(outcome.mcp.succeeded);
        assert_eq!(outcome.candidates.len(), 1);
        assert!(channels_of(&outcome, "https://mcp.example/b").contains(&SearchChannel::Mcp));
        assert_eq!(outcome.usage.native, 0);
        assert_eq!(outcome.usage.mcp, 1);
    }

    #[tokio::test]
    async fn mcp_failure_keeps_credentialed_native_success() {
        let native =
            ScriptedSearchRoute::new(credentialed(vec![hit("https://native.example/a")], 1));
        let mcp = ScriptedSearchRoute::new(transport_failure(1));

        let outcome = coordinate_dual_path_search(
            request(true),
            &NativeSearchSupport::Available,
            &native,
            &mcp,
        )
        .await;

        assert_legal(&outcome);
        assert!(outcome.native.succeeded);
        assert!(outcome.mcp.attempted && !outcome.mcp.succeeded);
        assert_eq!(
            outcome.mcp.failed,
            Some(RouteFailureClass::TransportOrProviderFailure)
        );
        assert_eq!(outcome.candidates.len(), 1);
        assert!(channels_of(&outcome, "https://native.example/a").contains(&SearchChannel::Native));
        assert_eq!(outcome.usage.native, 1);
        assert_eq!(outcome.usage.mcp, 0);
    }

    #[tokio::test]
    async fn both_routes_failed_is_not_a_fake_success() {
        let native = ScriptedSearchRoute::new(transport_failure(1));
        let mcp = ScriptedSearchRoute::new(transport_failure(1));

        let outcome = coordinate_dual_path_search(
            request(true),
            &NativeSearchSupport::Available,
            &native,
            &mcp,
        )
        .await;

        assert_legal(&outcome);
        assert!(!outcome.native.succeeded && outcome.native.failed.is_some());
        assert!(!outcome.mcp.succeeded && outcome.mcp.failed.is_some());
        assert!(outcome.candidates.is_empty());
        assert_eq!(outcome.usage.native, 0);
        assert_eq!(outcome.usage.mcp, 0);
        assert!(!outcome.native_or_mcp_succeeded());
        assert!(outcome.both_available_routes_attempted());
    }

    #[tokio::test]
    async fn native_generated_text_without_credentials_is_protocol_insufficient_not_unsupported_or_succeeded(
    ) {
        let native = ScriptedSearchRoute::new(RouteAttemptOutcome {
            candidates: vec![hit("https://generated.example/made-up")],
            has_retrieval_credentials: false,
            generated_text_only: true,
            failure: None,
            internal_provider_attempts: 1,
            dispatched_attempts: 1,
            ..Default::default()
        });
        let mcp = ScriptedSearchRoute::new(credentialed(vec![hit("https://mcp.example/b")], 1));

        let outcome = coordinate_dual_path_search(
            request(true),
            &NativeSearchSupport::Available,
            &native,
            &mcp,
        )
        .await;

        assert_legal(&outcome);
        assert!(outcome.native.supported);
        assert!(outcome.native.attempted);
        assert!(!outcome.native.succeeded);
        assert_eq!(
            outcome.native.failed,
            Some(RouteFailureClass::ProtocolOrResultInsufficient)
        );
        assert!(outcome.mcp.succeeded);
        assert!(!channels_of(&outcome, "https://generated.example/made-up")
            .contains(&SearchChannel::Native));
        assert_eq!(outcome.usage.native, 0);
    }

    #[tokio::test]
    async fn dual_mcp_failover_is_one_route_not_dual_path() {
        let native = ScriptedSearchRoute::new(credentialed(
            vec![hit("https://native.example/should-not-run")],
            1,
        ));
        let mcp = ScriptedSearchRoute::new(credentialed(vec![hit("https://mcp.example/b")], 2));

        let outcome = coordinate_dual_path_search(
            request(true),
            &NativeSearchSupport::Unsupported,
            &native,
            &mcp,
        )
        .await;

        assert_legal(&outcome);
        assert_eq!(native.call_count(), 0);
        assert_eq!(mcp.call_count(), 1);
        assert!(outcome.mcp.attempted && outcome.mcp.succeeded);
        assert!(!outcome.native.attempted);
        assert_eq!(outcome.mcp_internal_provider_attempts, 2);
        assert!(!outcome.both_available_routes_attempted());
        assert_eq!(
            outcome
                .candidates
                .iter()
                .filter(|candidate| candidate.channels.contains(&SearchChannel::Mcp))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn forbidding_second_route_does_not_claim_dual_path_complete() {
        let native =
            ScriptedSearchRoute::new(credentialed(vec![hit("https://native.example/a")], 1));
        let mcp = ScriptedSearchRoute::new(credentialed(vec![hit("https://mcp.example/b")], 1));

        let outcome = coordinate_dual_path_search(
            request(false),
            &NativeSearchSupport::Available,
            &native,
            &mcp,
        )
        .await;

        assert_legal(&outcome);
        assert_eq!(native.call_count(), 0);
        assert_eq!(mcp.call_count(), 1);
        assert!(outcome.native.supported);
        assert!(!outcome.native.attempted);
        assert!(outcome.mcp.attempted && outcome.mcp.succeeded);
        assert!(!outcome.both_available_routes_attempted());
        assert!(outcome
            .shortage
            .as_deref()
            .is_some_and(|text| text.contains("不以单路结果冒充双路完成")));
        assert_no_degradation_copy(&outcome);
    }

    #[tokio::test]
    async fn production_native_probe_is_unsupported_and_still_runs_mcp() {
        let native = ScriptedSearchRoute::new(credentialed(
            vec![hit("https://native.example/should-not-run")],
            1,
        ));
        let mcp = ScriptedSearchRoute::new(credentialed(vec![hit("https://mcp.example/b")], 1));

        assert_eq!(
            ProductionNativeSearchSupport::default().native_search_support(),
            NativeSearchSupport::Unsupported
        );

        let outcome = coordinate_dual_path_search(
            request(true),
            &ProductionNativeSearchSupport::default(),
            &native,
            &mcp,
        )
        .await;

        assert_legal(&outcome);
        assert_eq!(native.call_count(), 0);
        assert_eq!(mcp.call_count(), 1);
        assert!(!outcome.native.supported);
        assert!(!outcome.native.attempted);
        assert!(outcome.mcp.succeeded);
        assert_no_degradation_copy(&outcome);
    }

    #[tokio::test]
    async fn temporarily_failed_native_probe_is_not_rewritten_to_unsupported() {
        let native = ScriptedSearchRoute::new(credentialed(
            vec![hit("https://native.example/should-not-run")],
            1,
        ));
        let mcp = ScriptedSearchRoute::new(credentialed(vec![hit("https://mcp.example/b")], 1));

        let outcome = coordinate_dual_path_search(
            request(true),
            &NativeSearchSupport::TemporarilyFailed,
            &native,
            &mcp,
        )
        .await;

        assert_legal(&outcome);
        assert_eq!(native.call_count(), 0);
        assert!(outcome.native.supported);
        assert!(!outcome.native.attempted);
        assert!(!outcome.native.succeeded);
        assert_eq!(outcome.native.failed, None);
        assert!(outcome.mcp.succeeded);
        assert!(
            outcome
                .shortage
                .as_deref()
                .is_some_and(|text| text.contains("临时不可用")),
            "supported&&!attempted must carry an insufficiency note: {:?}",
            outcome.shortage
        );
    }

    #[tokio::test]
    async fn dual_path_json_exposes_native_credentials_count_and_parent_identity() {
        let native =
            ScriptedSearchRoute::new(credentialed(vec![hit("https://native.example/a")], 1));
        let mcp = ScriptedSearchRoute::new(credentialed(vec![hit("https://mcp.example/b")], 1));
        let outcome = coordinate_dual_path_search(
            request(true),
            &NativeSearchSupport::Available,
            &native,
            &mcp,
        )
        .await;
        let json = dual_path_status_json(&outcome);
        assert_eq!(json["requestIdentity"]["runId"], "run-1");
        assert_eq!(json["requestIdentity"]["actionId"], "search-1");
        assert_eq!(json["native"]["hasRetrievalCredentials"], true);
        assert_eq!(json["native"]["citationCount"], 1);
        assert_eq!(json["usage"]["native"], 1);
        assert!(json["usage"].get("nativePromptTokens").is_some());
        assert!(json["usage"].get("nativeCompletionTokens").is_some());
    }

    struct AbortAfterMcpRoute {
        outcome: RouteAttemptOutcome,
        run_id: String,
    }

    impl SearchRoute for AbortAfterMcpRoute {
        async fn execute(&self, _query: &str) -> RouteAttemptOutcome {
            crate::ai_runtime::model_gateway::request_abort(&self.run_id);
            self.outcome.clone()
        }
    }

    struct MustNotRunRoute {
        calls: AtomicU32,
    }

    impl SearchRoute for MustNotRunRoute {
        async fn execute(&self, _query: &str) -> RouteAttemptOutcome {
            self.calls.fetch_add(1, Ordering::SeqCst);
            panic!("native search must not start after the run was cancelled");
        }
    }

    #[tokio::test]
    async fn mcp_success_then_cancel_must_not_start_native() {
        let run_id = "run-cancel-after-mcp";
        crate::ai_runtime::model_gateway::clear_abort(run_id);
        let mcp = AbortAfterMcpRoute {
            outcome: credentialed(vec![hit("https://mcp.example/keep")], 1),
            run_id: run_id.into(),
        };
        let native = MustNotRunRoute {
            calls: AtomicU32::new(0),
        };
        let mut request = request(true);
        request.identity.run_id = run_id.into();

        let outcome =
            coordinate_dual_path_search(request, &NativeSearchSupport::Available, &native, &mcp)
                .await;

        crate::ai_runtime::model_gateway::clear_abort(run_id);
        assert_eq!(native.calls.load(Ordering::SeqCst), 0);
        assert!(outcome.mcp.succeeded);
        assert!(!outcome.native.attempted);
        assert_eq!(outcome.candidates.len(), 1);
        assert!(channels_of(&outcome, "https://mcp.example/keep").contains(&SearchChannel::Mcp));
    }

    struct HangNativeRoute;

    impl SearchRoute for HangNativeRoute {
        async fn execute(&self, _query: &str) -> RouteAttemptOutcome {
            std::future::pending().await
        }
    }

    #[tokio::test]
    async fn mcp_success_is_kept_when_native_times_out() {
        let mcp = ScriptedSearchRoute::new(credentialed(vec![hit("https://mcp.example/keep")], 1));
        let outcome = coordinate_dual_path_search(
            request(true),
            &NativeSearchSupport::Available,
            &HangNativeRoute,
            &mcp,
        )
        .await;

        assert!(outcome.mcp.succeeded);
        assert!(outcome.native.attempted);
        assert!(!outcome.native.succeeded);
        assert_eq!(
            outcome.native.failed,
            Some(RouteFailureClass::TemporaryFailure)
        );
        assert_eq!(outcome.candidates.len(), 1);
        assert!(channels_of(&outcome, "https://mcp.example/keep").contains(&SearchChannel::Mcp));
    }

    // K11: `supported && !attempted` must carry an insufficiency note. A
    // pre-dispatch refusal (zero dispatched attempts + failure class) must not
    // fold into a silent "never tried, nothing wrong" status.
    #[tokio::test]
    async fn zero_dispatch_native_failure_is_named_in_the_insufficiency_note() {
        let native = ScriptedSearchRoute::new(RouteAttemptOutcome {
            failure: Some(RouteFailureClass::ProtocolOrResultInsufficient),
            ..RouteAttemptOutcome::default()
        });
        let mcp = ScriptedSearchRoute::new(credentialed(vec![hit("https://mcp.example/b")], 1));

        let outcome = coordinate_dual_path_search(
            request(true),
            &NativeSearchSupport::Available,
            &native,
            &mcp,
        )
        .await;

        assert_legal(&outcome);
        assert!(outcome.native.supported);
        assert!(!outcome.native.attempted);
        assert_eq!(outcome.native.failed, None);
        assert!(outcome.mcp.succeeded);
        assert!(
            outcome
                .shortage
                .as_deref()
                .is_some_and(|text| text.contains("protocol_or_result_insufficient")),
            "a pre-dispatch failure class must be named, not silently dropped: {:?}",
            outcome.shortage
        );
        assert_no_degradation_copy(&outcome);
    }

    #[tokio::test]
    async fn zero_dispatch_mcp_failure_is_named_in_the_insufficiency_note() {
        let native =
            ScriptedSearchRoute::new(credentialed(vec![hit("https://native.example/a")], 1));
        let mcp = ScriptedSearchRoute::new(RouteAttemptOutcome {
            failure: Some(RouteFailureClass::TransportOrProviderFailure),
            ..RouteAttemptOutcome::default()
        });

        let outcome = coordinate_dual_path_search(
            request(true),
            &NativeSearchSupport::Available,
            &native,
            &mcp,
        )
        .await;

        assert_legal(&outcome);
        assert!(outcome.mcp.supported);
        assert!(!outcome.mcp.attempted);
        assert!(!outcome.mcp.succeeded);
        assert!(
            outcome
                .shortage
                .as_deref()
                .is_some_and(|text| text.contains("transport_or_provider_failure")),
            "a zero-dispatch MCP failure class must be named: {:?}",
            outcome.shortage
        );
    }

    #[test]
    fn search_identity_preserves_opaque_turn_and_action_ids() {
        let identity = SearchActionIdentity {
            input_revision: "turn-opaque-revision".into(),
            action_id: "provider-call-opaque".into(),
            ..identity()
        };
        let encoded = serde_json::to_value(&identity).unwrap();
        assert_eq!(encoded["inputRevision"], "turn-opaque-revision");
        assert_eq!(encoded["actionId"], "provider-call-opaque");
    }
}
