//! Secret-free assembly for normal-domain direct text streaming requests.

use crate::ai_runtime::provider_router::{
    CandidateAvailability, CandidateHealth, ProviderCandidate, ProviderFailure,
    ProviderRequirements, ProviderRouter, SecurityDomain,
};
use crate::ai_types::{ProviderConfig, ResolvedReasoningRequest};
use crate::error::{AppError, AppResult};
use crate::llm::config::{ResolvedLlmConfig, ResolvedModelPool};

/// Normal-domain direct adapter route built solely from a secret-free resolver result.
///
/// This is intentionally an assembly boundary, not a dispatcher. It provides ordered
/// direct text-streaming candidates and permits credential hydration only for the
/// candidate selected for an immediate request.
#[derive(Debug, Clone)]
pub(crate) struct DirectProviderRoute {
    router: ProviderRouter,
    model_override: Option<(String, String)>,
}

/// One immediately dispatchable direct-provider configuration and its output cap.
///
/// This value exists only after the selected candidate's credential has been
/// hydrated. It must not be retained after the request completes.
pub(crate) struct DirectProviderDispatch {
    pub(crate) provider: ProviderConfig,
    pub(crate) max_output_tokens: u32,
    /// The resolved reasoning controls for this candidate, preserved through routing.
    pub(crate) reasoning: ResolvedReasoningRequest,
    /// Legacy adapter flag derived from the same resolved reasoning request.
    pub(crate) thinking: bool,
}

impl DirectProviderRoute {
    /// Assemble primary and same-slot failover candidates from the secret-free resolver.
    pub(crate) fn from_secret_free_route(route: ResolvedModelPool) -> AppResult<Self> {
        let candidates = std::iter::once(route.resolved)
            .chain(route.failover_candidates)
            .map(provider_candidate_from_resolved)
            .collect();

        Ok(Self {
            router: ProviderRouter::new(candidates),
            model_override: None,
        })
    }

    /// Restrict this Run to one explicit configured provider/model. Capability
    /// filtering still happens before the selected candidate is hydrated.
    pub(crate) fn with_model_override(mut self, provider_id: String, model_id: String) -> Self {
        self.model_override = Some((provider_id, model_id));
        self
    }

    /// Select ordered streaming candidates for actual Run requirements.
    ///
    /// Unlike the legacy direct helper this preserves tool, vision, and reasoning
    /// requirements. The gateway supports each candidate's own endpoint family,
    /// so no protocol family is imposed here.
    pub(crate) fn select_streaming_for_requirements(
        &self,
        requirements: ProviderRequirements,
    ) -> Vec<&ProviderCandidate> {
        self.filter_model_override(self.router.select_candidates(&requirements))
    }

    fn filter_model_override<'a>(
        &self,
        candidates: Vec<&'a ProviderCandidate>,
    ) -> Vec<&'a ProviderCandidate> {
        let Some((provider_id, model_id)) = &self.model_override else {
            return candidates;
        };
        candidates
            .into_iter()
            .filter(|candidate| {
                candidate.provider_id == *provider_id && candidate.model == *model_id
            })
            .collect()
    }

    /// Hydrate one selected generic streaming candidate for immediate dispatch.
    ///
    /// Only the selected candidate's credential is read. Its reasoning settings
    /// remain tied to the exact provider/model chosen by the ranking phase.
    pub(crate) fn hydrate_selected_streaming_dispatch_with<F>(
        &self,
        requirements: ProviderRequirements,
        selected_index: usize,
        read_credential: F,
    ) -> AppResult<DirectProviderDispatch>
    where
        F: FnMut(&str) -> AppResult<zeroize::Zeroizing<String>>,
    {
        let candidate = self
            .select_streaming_for_requirements(requirements)
            .get(selected_index)
            .copied()
            .ok_or_else(|| AppError::msg("agent_run_no_capable_model"))?;
        self.router
            .hydrate_candidate_with(candidate, read_credential)
            .map(|candidate| {
                let reasoning = candidate.candidate().reasoning;
                let thinking = candidate.candidate().thinking;
                let max_output_tokens = candidate.candidate().output_budget_tokens;
                DirectProviderDispatch {
                    provider: candidate.into_provider_config(),
                    max_output_tokens,
                    reasoning,
                    thinking,
                }
            })
    }

    /// Hydrate one generic streaming candidate from the encrypted credential store.
    pub(crate) fn hydrate_selected_streaming_dispatch(
        &self,
        requirements: ProviderRequirements,
        selected_index: usize,
    ) -> AppResult<DirectProviderDispatch> {
        self.hydrate_selected_streaming_dispatch_with(
            requirements,
            selected_index,
            crate::credentials::get_runtime_secret,
        )
    }

    /// Return the next ranked generic candidate after a strictly transient failure.
    pub(crate) fn next_selected_index_after_for_requirements(
        &self,
        requirements: ProviderRequirements,
        attempted_index: usize,
        failure: ProviderFailure,
    ) -> Option<usize> {
        let selected = self.select_streaming_for_requirements(requirements);
        self.router
            .next_candidate_after(selected.as_slice(), attempted_index, failure)
            .and_then(|next| {
                selected
                    .iter()
                    .position(|candidate| std::ptr::eq(*candidate, next))
            })
    }

    /// Return the selected generic candidate's safe provider identifier.
    pub(crate) fn selected_provider_id_for_requirements(
        &self,
        requirements: ProviderRequirements,
        selected_index: usize,
    ) -> Option<&str> {
        self.select_streaming_for_requirements(requirements)
            .get(selected_index)
            .map(|candidate| candidate.provider_id.as_str())
    }
}

fn provider_candidate_from_resolved(resolved: ResolvedLlmConfig) -> ProviderCandidate {
    let requires_credential = crate::llm::providers::requires_api_key(&resolved.provider_id);
    #[cfg(test)]
    let requires_credential = requires_credential
        && !(crate::llm::providers::is_custom_provider(&resolved.provider_id)
            && resolved.base_url.starts_with("http://127.0.0.1:"));
    let credential_service = requires_credential
        .then(|| crate::llm::providers::credential_service(&resolved.provider_id));

    ProviderCandidate {
        provider_id: resolved.provider_id,
        model: resolved.model,
        base_url: resolved.base_url,
        endpoint_family: resolved.endpoint_family,
        supports_streaming: resolved.supports_streaming,
        supports_tools: resolved.supports_tools,
        supports_vision: resolved.supports_vision,
        supports_reasoning: resolved.supports_reasoning,
        input_budget_tokens: resolved.input_budget,
        output_budget_tokens: resolved.output_budget,
        security_domain: SecurityDomain::External,
        availability: CandidateAvailability::Available,
        health: CandidateHealth::Unknown,
        credential_service,
        reasoning: resolved.reasoning,
        thinking: resolved.thinking || resolved.reasoning.requested,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::provider_router::ProviderRequirements;
    use crate::ai_types::EndpointFamily;

    fn resolved(provider_id: &str, model: &str) -> ResolvedLlmConfig {
        ResolvedLlmConfig {
            provider_id: provider_id.into(),
            model: model.into(),
            base_url: "https://example.invalid/v1".into(),
            thinking: false,
            reasoning: ResolvedReasoningRequest::disabled(),
            input_budget: 128_000,
            output_budget: 8_192,
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: true,
            supports_reasoning: false,
        }
    }

    fn requirements() -> ProviderRequirements {
        ProviderRequirements {
            endpoint_family: None,
            streaming: true,
            tools: false,
            vision: false,
            reasoning: false,
            min_input_budget_tokens: 1,
            min_output_budget_tokens: 1,
            security_domain: SecurityDomain::External,
        }
    }

    fn route() -> DirectProviderRoute {
        DirectProviderRoute::from_secret_free_route(ResolvedModelPool {
            resolved: resolved("provider-a", "model-a"),
            failover_candidates: vec![resolved("provider-b", "model-b")],
        })
        .expect("pool route")
    }

    #[test]
    fn model_override_can_only_select_an_enabled_pool_candidate() {
        let selected_route = route().with_model_override("provider-b".into(), "model-b".into());
        let selected = selected_route.select_streaming_for_requirements(requirements());
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].provider_id, "provider-b");

        let rejected_route = route().with_model_override("provider-c".into(), "model-c".into());
        let rejected = rejected_route.select_streaming_for_requirements(requirements());
        assert!(rejected.is_empty());
    }

    #[test]
    fn failover_advances_only_after_retryable_failure() {
        let route = route();
        assert_eq!(
            route.next_selected_index_after_for_requirements(
                requirements(),
                0,
                ProviderFailure::Timeout,
            ),
            Some(1)
        );
        assert_eq!(
            route.next_selected_index_after_for_requirements(
                requirements(),
                0,
                ProviderFailure::Unauthorized,
            ),
            None
        );
    }
}
