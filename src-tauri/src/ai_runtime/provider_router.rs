//! Isolated provider selection and credential hydration boundary.
//!
//! This module intentionally does not consume `llm::config::ResolvedLlmConfig`:
//! the legacy resolver can already contain a hydrated `String` API key. A future
//! integration must supply these secret-free candidates before dispatching them
//! to a provider adapter.

use std::fmt;

use zeroize::Zeroizing;

use crate::ai_types::{CapabilitySlot, EndpointFamily, ProviderConfig};
use crate::error::AppResult;

/// Security boundary in which a provider candidate may run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SecurityDomain {
    /// The candidate communicates with an external provider.
    External,
    /// The candidate stays in the local security domain.
    Local,
}

/// Current availability reported by provider diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateAvailability {
    /// The provider is enabled and available for dispatch.
    Available,
    /// The provider must not be selected.
    Unavailable,
}

/// Recent health state used during candidate selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateHealth {
    /// A recent diagnostic or successful request marked the candidate healthy.
    Healthy,
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
    /// Non-secret encrypted-store service identifier, if this adapter needs one.
    pub(crate) credential_service: Option<String>,
}

/// Requirements used to select ordered provider candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProviderRequirements {
    /// Required adapter protocol family.
    pub(crate) endpoint_family: EndpointFamily,
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
    /// The submitted request did not match the provider schema.
    Schema,
    /// The request exceeded the model context window.
    ContextLimit,
    /// The user or runtime cancelled the request.
    Cancelled,
    /// Policy denied the dispatch or its required capability.
    PolicyDenied,
    /// The selected provider is not allowed in the requested security domain.
    SecurityDomainMismatch,
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

    /// Return the credential for immediate adapter request construction.
    pub(crate) fn credential(&self) -> Option<&Zeroizing<String>> {
        self.credential.as_ref()
    }

    /// Report credential presence without exposing its value.
    pub(crate) fn has_credential(&self) -> bool {
        self.credential.is_some()
    }

    /// Consume the short-lived credential and produce one dispatch configuration.
    ///
    /// Callers must dispatch the returned configuration immediately and must not
    /// retain it alongside unselected candidates.
    pub(crate) fn into_provider_config(self, slot: CapabilitySlot) -> ProviderConfig {
        ProviderConfig {
            name: self.candidate.provider_id,
            base_url: self.candidate.base_url,
            api_key: self.credential.map(|credential| credential.to_string()),
            model: self.candidate.model,
            slot,
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

    /// Hydrate this candidate's credential only for its immediate dispatch.
    pub(crate) fn hydrate_candidate(
        &self,
        candidate: &ProviderCandidate,
    ) -> AppResult<HydratedProviderCandidate> {
        self.hydrate_candidate_with(candidate, crate::credentials::get_runtime_secret)
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
    candidate.endpoint_family == requirements.endpoint_family
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

    fn candidate(provider_id: &str, model: &str) -> ProviderCandidate {
        ProviderCandidate {
            provider_id: provider_id.into(),
            model: model.into(),
            base_url: format!("https://{provider_id}.example/v1"),
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: false,
            supports_reasoning: true,
            input_budget_tokens: 32_000,
            output_budget_tokens: 4_000,
            security_domain: SecurityDomain::External,
            availability: CandidateAvailability::Available,
            health: CandidateHealth::Healthy,
            credential_service: Some(format!("iris.llm.{provider_id}")),
        }
    }

    fn requirements() -> ProviderRequirements {
        ProviderRequirements {
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
            streaming: true,
            tools: true,
            vision: false,
            reasoning: true,
            min_input_budget_tokens: 16_000,
            min_output_budget_tokens: 2_000,
            security_domain: SecurityDomain::External,
        }
    }

    #[test]
    fn select_candidates_filters_by_capability_domain_availability_and_health_without_hydration() {
        let router = ProviderRouter::new(vec![
            ProviderCandidate {
                supports_tools: false,
                ..candidate("no-tools", "no-tools-model")
            },
            ProviderCandidate {
                security_domain: SecurityDomain::Local,
                ..candidate("local", "local-model")
            },
            ProviderCandidate {
                availability: CandidateAvailability::Unavailable,
                ..candidate("offline", "offline-model")
            },
            ProviderCandidate {
                health: CandidateHealth::Unhealthy,
                ..candidate("unhealthy", "unhealthy-model")
            },
            ProviderCandidate {
                health: CandidateHealth::Unknown,
                ..candidate("unknown", "unknown-model")
            },
            candidate("primary", "primary-model"),
            candidate("backup", "backup-model"),
        ]);

        let selected = router.select_candidates(&requirements());

        assert_eq!(
            selected
                .iter()
                .map(|candidate| candidate.provider_id.as_str())
                .collect::<Vec<_>>(),
            vec!["unknown", "primary", "backup"]
        );
        assert!(selected
            .iter()
            .all(|candidate| candidate.credential_service.is_some()));
    }

    #[test]
    fn hydrate_candidate_reads_only_the_dispatched_candidate_credential() {
        let router = ProviderRouter::new(vec![candidate("primary", "primary-model")]);
        let selected = router.select_candidates(&requirements());
        let mut read_services = Vec::new();

        let hydrated = router
            .hydrate_candidate_with(selected[0], |service| {
                read_services.push(service.to_string());
                Ok(zeroize::Zeroizing::new("secret".to_string()))
            })
            .expect("hydrate selected candidate");

        assert_eq!(read_services, vec!["iris.llm.primary"]);
        assert_eq!(hydrated.candidate().provider_id, "primary");
        assert!(hydrated.has_credential());
        assert!(!format!("{hydrated:?}").contains("secret"));
    }

    #[test]
    fn retryable_failures_are_limited_to_transient_transport_and_server_conditions() {
        for failure in [
            ProviderFailure::Connection,
            ProviderFailure::Timeout,
            ProviderFailure::HttpStatus(429),
            ProviderFailure::HttpStatus(500),
            ProviderFailure::HttpStatus(599),
            ProviderFailure::TemporarilyUnavailable,
        ] {
            assert!(failure.is_retryable(), "{failure:?} should be retryable");
        }

        for failure in [
            ProviderFailure::HttpStatus(400),
            ProviderFailure::HttpStatus(401),
            ProviderFailure::HttpStatus(403),
            ProviderFailure::HttpStatus(408),
            ProviderFailure::Unauthorized,
            ProviderFailure::Forbidden,
            ProviderFailure::Schema,
            ProviderFailure::ContextLimit,
            ProviderFailure::Cancelled,
            ProviderFailure::PolicyDenied,
            ProviderFailure::SecurityDomainMismatch,
            ProviderFailure::Unknown,
        ] {
            assert!(!failure.is_retryable(), "{failure:?} must not retry");
        }
    }

    #[test]
    fn next_candidate_requires_a_retryable_failure_and_preserves_order() {
        let router = ProviderRouter::new(vec![
            candidate("primary", "primary-model"),
            candidate("backup", "backup-model"),
        ]);
        let selected = router.select_candidates(&requirements());

        assert_eq!(
            router
                .next_candidate_after(&selected, 0, ProviderFailure::Timeout)
                .map(|candidate| candidate.provider_id.as_str()),
            Some("backup")
        );
        assert!(router
            .next_candidate_after(&selected, 0, ProviderFailure::HttpStatus(401))
            .is_none());
    }
}
