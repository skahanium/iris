//! Isolated provider selection and credential hydration boundary.
//!
//! This module intentionally does not consume `llm::config::ResolvedLlmConfig`:
//! the legacy resolver can already contain a hydrated `String` API key. A future
//! integration must supply these secret-free candidates before dispatching them
//! to a provider adapter.

use std::fmt;

use zeroize::Zeroizing;

use crate::ai_types::{EndpointFamily, ProviderConfig, ResolvedReasoningRequest};
use crate::error::AppResult;

/// Security boundary in which a provider candidate may run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SecurityDomain {
    /// The candidate communicates with an external provider.
    External,
}

/// Current availability reported by provider diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateAvailability {
    /// The provider is enabled and available for dispatch.
    Available,
}

/// Recent health state used during candidate selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateHealth {
    /// No sufficiently recent health result is available.
    Unknown,
    /// A recent diagnostic marked the candidate unhealthy.
    Unhealthy,
}

/// Secret-free provider metadata that can safely participate in selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderCandidate {
    /// Stable configured provider identifier.
    pub(crate) provider_id: String,
    /// Configured model identifier.
    pub(crate) model: String,
    /// Adapter endpoint used only after this candidate is dispatched.
    pub(crate) base_url: String,
    /// Adapter protocol family.
    pub(crate) endpoint_family: EndpointFamily,
    /// Whether the candidate supports streaming responses.
    pub(crate) supports_streaming: bool,
    /// Whether the candidate supports tool calls.
    pub(crate) supports_tools: bool,
    /// Whether the candidate supports image input.
    pub(crate) supports_vision: bool,
    /// Whether the candidate supports reasoning controls.
    pub(crate) supports_reasoning: bool,
    /// Maximum input budget offered by the candidate.
    pub(crate) input_budget_tokens: usize,
    /// Maximum output budget offered by the candidate.
    pub(crate) output_budget_tokens: u32,
    /// Security domain in which this candidate may run.
    pub(crate) security_domain: SecurityDomain,
    /// Dispatch availability from diagnostics/configuration.
    pub(crate) availability: CandidateAvailability,
    /// Recent provider health from diagnostics/request outcomes.
    pub(crate) health: CandidateHealth,
    /// Resolved reasoning controls retained through the dispatch boundary.
    pub(crate) reasoning: ResolvedReasoningRequest,
    /// Legacy adapter flag derived from the resolved reasoning request.
    pub(crate) thinking: bool,
    /// Non-secret encrypted-store service identifier, if this adapter needs one.
    pub(crate) credential_service: Option<String>,
}

/// Requirements used to select ordered provider candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProviderRequirements {
    /// Required adapter protocol family.
    pub(crate) endpoint_family: Option<EndpointFamily>,
    /// Whether the request requires streaming.
    pub(crate) streaming: bool,
    /// Whether the request requires tool calls.
    pub(crate) tools: bool,
    /// Whether the request requires image input.
    pub(crate) vision: bool,
    /// Whether the request requires reasoning controls.
    pub(crate) reasoning: bool,
    /// Minimum supported input budget.
    pub(crate) min_input_budget_tokens: usize,
    /// Minimum supported output budget.
    pub(crate) min_output_budget_tokens: u32,
    /// Required execution security domain.
    pub(crate) security_domain: SecurityDomain,
}

/// Failure classification used to decide whether a different candidate may run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderFailure {
    /// The network connection could not be established or was reset.
    Connection,
    /// The request exceeded its timeout.
    Timeout,
    /// An HTTP failure response.
    HttpStatus(u16),
    /// The provider explicitly reported a temporary outage.
    TemporarilyUnavailable,
    /// The provider rejected authentication.
    Unauthorized,
    /// The provider denied access to the resource.
    Forbidden,
    /// The user or runtime cancelled the request.
    Cancelled,
    /// A failure that cannot be safely classified as transient.
    Unknown,
}

impl ProviderFailure {
    /// Return whether this failure permits trying the next selected candidate.
    pub(crate) fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::Connection
                | Self::Timeout
                | Self::HttpStatus(429)
                | Self::HttpStatus(500..=599)
                | Self::TemporarilyUnavailable
        )
    }
}

/// Classify an application error for provider failover routing.
pub(crate) fn classify_provider_failure_from_app_error(
    error: &crate::error::AppError,
) -> ProviderFailure {
    use crate::error::{AppError, ProviderErrorKind};

    match error {
        AppError::Provider { kind, message } => match kind {
            ProviderErrorKind::Connection => ProviderFailure::Connection,
            ProviderErrorKind::Timeout => ProviderFailure::Timeout,
            ProviderErrorKind::RateLimited => ProviderFailure::HttpStatus(429),
            ProviderErrorKind::Unauthorized => ProviderFailure::Unauthorized,
            ProviderErrorKind::Forbidden => ProviderFailure::Forbidden,
            ProviderErrorKind::TemporarilyUnavailable => ProviderFailure::TemporarilyUnavailable,
            ProviderErrorKind::Cancelled => ProviderFailure::Cancelled,
            ProviderErrorKind::HttpStatus(status) => ProviderFailure::HttpStatus(*status),
            ProviderErrorKind::InvalidResponse | ProviderErrorKind::Unknown => {
                // Preserve legacy message-token fallback for gateway edges that
                // still emit Provider{Unknown} with a descriptive message.
                let from_message = classify_provider_failure_from_message(message);
                if from_message == ProviderFailure::Unknown {
                    ProviderFailure::Unknown
                } else {
                    from_message
                }
            }
        },
        AppError::Http(err) => {
            if err.is_timeout() {
                ProviderFailure::Timeout
            } else if err.is_connect() {
                ProviderFailure::Connection
            } else if let Some(status) = err.status() {
                let code = status.as_u16();
                match code {
                    401 => ProviderFailure::Unauthorized,
                    403 => ProviderFailure::Forbidden,
                    429 => ProviderFailure::HttpStatus(429),
                    503 => ProviderFailure::TemporarilyUnavailable,
                    500..=599 => ProviderFailure::HttpStatus(code),
                    _ => ProviderFailure::HttpStatus(code),
                }
            } else {
                ProviderFailure::Unknown
            }
        }
        AppError::Message(message) => classify_provider_failure_from_message(message),
        _ => classify_provider_failure_from_message(&error.to_string()),
    }
}

fn classify_provider_failure_from_message(message: &str) -> ProviderFailure {
    let message = message.to_ascii_lowercase();
    if message.contains("request aborted") || message.contains("partial_visible_stream_error") {
        return ProviderFailure::Cancelled;
    }
    if message.contains("timeout") || message.contains("deadline") {
        return ProviderFailure::Timeout;
    }
    if message.contains("429") || message.contains("too many requests") {
        return ProviderFailure::HttpStatus(429);
    }
    if message.contains("502") {
        return ProviderFailure::HttpStatus(502);
    }
    if message.contains("503") || message.contains("service unavailable") {
        return ProviderFailure::TemporarilyUnavailable;
    }
    if message.contains("connection") || message.contains("sending request") {
        return ProviderFailure::Connection;
    }
    if message.contains("unauthorized") || message.contains("api key") {
        return ProviderFailure::Unauthorized;
    }
    if message.contains("500") {
        return ProviderFailure::HttpStatus(500);
    }
    ProviderFailure::Unknown
}

/// A candidate with its credential hydrated for one immediate dispatch.
pub(crate) struct HydratedProviderCandidate {
    candidate: ProviderCandidate,
    credential: Option<Zeroizing<String>>,
}

impl HydratedProviderCandidate {
    /// Return the selected candidate metadata.
    pub(crate) fn candidate(&self) -> &ProviderCandidate {
        &self.candidate
    }

    /// Consume the short-lived credential and produce one dispatch configuration.
    ///
    /// The credential remains [`Zeroizing`] until the HTTP adapter has consumed
    /// the configuration. It must never be converted into a plain `String`.
    pub(crate) fn into_provider_config(self) -> ProviderConfig {
        ProviderConfig {
            name: self.candidate.provider_id,
            base_url: self.candidate.base_url,
            api_key: self.credential,
            model: self.candidate.model,
            endpoint_family: self.candidate.endpoint_family,
        }
    }
}

impl fmt::Debug for HydratedProviderCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HydratedProviderCandidate")
            .field("candidate", &self.candidate)
            .field(
                "credential",
                &if self.credential.is_some() {
                    "[REDACTED]"
                } else {
                    "[NONE]"
                },
            )
            .finish()
    }
}

/// Selects secret-free candidates and hydrates only a dispatched candidate.
#[derive(Debug, Clone)]
pub(crate) struct ProviderRouter {
    candidates: Vec<ProviderCandidate>,
}

impl ProviderRouter {
    /// Create a router that preserves the supplied primary-to-failover order.
    pub(crate) fn new(candidates: Vec<ProviderCandidate>) -> Self {
        Self { candidates }
    }

    /// Return matching candidates without reading or carrying credentials.
    pub(crate) fn select_candidates(
        &self,
        requirements: &ProviderRequirements,
    ) -> Vec<&ProviderCandidate> {
        self.candidates
            .iter()
            .filter(|candidate| candidate_satisfies(candidate, requirements))
            .collect()
    }

    /// Hydrate a candidate with an injected credential reader for tests/adapters.
    pub(crate) fn hydrate_candidate_with<F>(
        &self,
        candidate: &ProviderCandidate,
        mut read_credential: F,
    ) -> AppResult<HydratedProviderCandidate>
    where
        F: FnMut(&str) -> AppResult<Zeroizing<String>>,
    {
        let credential = candidate
            .credential_service
            .as_deref()
            .map(&mut read_credential)
            .transpose()?;
        Ok(HydratedProviderCandidate {
            candidate: candidate.clone(),
            credential,
        })
    }

    /// Return the next candidate only after a strictly retryable failure.
    pub(crate) fn next_candidate_after<'a>(
        &self,
        selected: &'a [&'a ProviderCandidate],
        attempted_index: usize,
        failure: ProviderFailure,
    ) -> Option<&'a ProviderCandidate> {
        if !failure.is_retryable() {
            return None;
        }
        selected.get(attempted_index.saturating_add(1)).copied()
    }
}

fn candidate_satisfies(candidate: &ProviderCandidate, requirements: &ProviderRequirements) -> bool {
    requirements
        .endpoint_family
        .is_none_or(|endpoint_family| candidate.endpoint_family == endpoint_family)
        && (!requirements.streaming || candidate.supports_streaming)
        && (!requirements.tools || candidate.supports_tools)
        && (!requirements.vision || candidate.supports_vision)
        && (!requirements.reasoning || candidate.supports_reasoning)
        && candidate.input_budget_tokens >= requirements.min_input_budget_tokens
        && candidate.output_budget_tokens >= requirements.min_output_budget_tokens
        && candidate.security_domain == requirements.security_domain
        && candidate.availability == CandidateAvailability::Available
        && candidate.health != CandidateHealth::Unhealthy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{AppError, ProviderErrorKind};

    #[test]
    fn classifies_cancelled_message_tokens() {
        let err = AppError::msg("request aborted by user");
        assert_eq!(
            classify_provider_failure_from_app_error(&err),
            ProviderFailure::Cancelled
        );
    }

    #[test]
    fn classifies_structured_provider_rate_limit_without_message_scan() {
        let err = AppError::provider(ProviderErrorKind::RateLimited, "请求过于频繁，请稍后再试。");
        assert_eq!(
            classify_provider_failure_from_app_error(&err),
            ProviderFailure::HttpStatus(429)
        );
    }

    #[test]
    fn classifies_structured_provider_timeout() {
        let err = AppError::provider(ProviderErrorKind::Timeout, "deadline exceeded");
        assert_eq!(
            classify_provider_failure_from_app_error(&err),
            ProviderFailure::Timeout
        );
    }

    #[test]
    fn classifies_rate_limited_message_fallback() {
        let err = AppError::msg("provider returned HTTP 429 too many requests");
        assert_eq!(
            classify_provider_failure_from_app_error(&err),
            ProviderFailure::HttpStatus(429)
        );
    }
}
