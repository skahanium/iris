//! Secret-free assembly for normal-domain direct text streaming requests.

use crate::ai_runtime::provider_router::{
    CandidateAvailability, CandidateHealth, ProviderCandidate, ProviderFailure,
    ProviderRequirements, ProviderRouter, SecurityDomain,
};
use crate::ai_types::{CapabilitySlot, EndpointFamily, ProviderConfig};
use crate::error::{AppError, AppResult};
use crate::llm::config::{ResolvedCapabilityRoute, ResolvedLlmConfig};

/// Normal-domain direct adapter route built solely from a secret-free resolver result.
///
/// This is intentionally an assembly boundary, not a dispatcher. It provides ordered
/// direct text-streaming candidates and permits credential hydration only for the
/// candidate selected for an immediate request.
#[derive(Debug, Clone)]
pub(crate) struct DirectProviderRoute {
    router: ProviderRouter,
}

/// One immediately dispatchable direct-provider configuration and its output cap.
///
/// This value exists only after the selected candidate's credential has been
/// hydrated. It must not be retained after the request completes.
pub(crate) struct DirectProviderDispatch {
    pub(crate) provider: ProviderConfig,
    pub(crate) max_output_tokens: u32,
}

impl DirectProviderRoute {
    /// Assemble primary and same-slot failover candidates from the secret-free resolver.
    pub(crate) fn from_secret_free_route(route: ResolvedCapabilityRoute) -> AppResult<Self> {
        ensure_route_is_secret_free(&route)?;

        let candidates = std::iter::once(route.resolved)
            .chain(route.failover_candidates)
            .map(provider_candidate_from_resolved)
            .collect();

        Ok(Self {
            router: ProviderRouter::new(candidates),
        })
    }

    /// Select external direct text-streaming candidates that intentionally do not expose tools.
    pub(crate) fn select_text_streaming_no_tools(
        &self,
        endpoint_family: EndpointFamily,
    ) -> Vec<&ProviderCandidate> {
        self.router.select_candidates(&ProviderRequirements {
            endpoint_family,
            streaming: true,
            tools: false,
            vision: false,
            reasoning: false,
            min_input_budget_tokens: 0,
            min_output_budget_tokens: 0,
            security_domain: SecurityDomain::External,
        })
    }

    /// Hydrate one indexed direct selection and immediately convert it for Fast dispatch.
    ///
    /// The index is resolved inside this method against the direct text-streaming
    /// selection. Therefore an unselected candidate cannot cause a credential read.
    pub(crate) fn hydrate_selected_text_streaming_no_tools_as_fast_provider_config_with<F>(
        &self,
        endpoint_family: EndpointFamily,
        selected_index: usize,
        read_credential: F,
    ) -> AppResult<DirectProviderDispatch>
    where
        F: FnMut(&str) -> AppResult<zeroize::Zeroizing<String>>,
    {
        let candidate = self
            .select_text_streaming_no_tools(endpoint_family)
            .get(selected_index)
            .copied()
            .ok_or_else(|| AppError::msg("no matching direct text streaming provider candidate"))?;
        self.router
            .hydrate_candidate_with(candidate, read_credential)
            .map(|candidate| DirectProviderDispatch {
                max_output_tokens: candidate.candidate().output_budget_tokens,
                provider: candidate.into_provider_config(CapabilitySlot::Fast),
            })
    }

    /// Hydrate the indexed direct selection with the encrypted credential store.
    ///
    /// The only production credential read is delegated to the selected
    /// candidate inside [`Self::hydrate_selected_text_streaming_no_tools_as_fast_provider_config_with`].
    pub(crate) fn hydrate_selected_text_streaming_no_tools_as_fast_dispatch(
        &self,
        endpoint_family: EndpointFamily,
        selected_index: usize,
    ) -> AppResult<DirectProviderDispatch> {
        self.hydrate_selected_text_streaming_no_tools_as_fast_provider_config_with(
            endpoint_family,
            selected_index,
            crate::credentials::get_runtime_secret,
        )
    }

    /// Return the next direct selection index only when the preceding failure is retryable.
    pub(crate) fn next_selected_index_after(
        &self,
        endpoint_family: EndpointFamily,
        attempted_index: usize,
        failure: ProviderFailure,
    ) -> Option<usize> {
        let selected = self.select_text_streaming_no_tools(endpoint_family);
        self.router
            .next_candidate_after(selected.as_slice(), attempted_index, failure)
            .and_then(|next| {
                selected
                    .iter()
                    .position(|candidate| std::ptr::eq(*candidate, next))
            })
    }
}

fn ensure_route_is_secret_free(route: &ResolvedCapabilityRoute) -> AppResult<()> {
    if route.resolved.api_key.is_some()
        || route
            .failover_candidates
            .iter()
            .any(|candidate| candidate.api_key.is_some())
    {
        return Err(AppError::msg(
            "direct provider route requires a secret-free capability resolution",
        ));
    }
    Ok(())
}

fn provider_candidate_from_resolved(resolved: ResolvedLlmConfig) -> ProviderCandidate {
    let credential_service = crate::llm::providers::requires_api_key(&resolved.provider_id)
        .then(|| crate::llm::providers::credential_service(&resolved.provider_id));

    ProviderCandidate {
        provider_id: resolved.provider_id,
        model: resolved.model,
        base_url: resolved.base_url,
        endpoint_family: resolved.endpoint_family,
        // The direct adapter has a text streaming-only contract. It never exposes tools,
        // vision, or reasoning controls through this route.
        supports_streaming: true,
        supports_tools: false,
        supports_vision: false,
        supports_reasoning: false,
        input_budget_tokens: resolved.input_budget,
        output_budget_tokens: resolved.output_budget,
        security_domain: SecurityDomain::External,
        availability: CandidateAvailability::Available,
        health: CandidateHealth::Unknown,
        credential_service,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_types::{CapabilitySlot, EndpointFamily, ResolvedReasoningRequest};
    use crate::llm::config::{ResolvedCapabilityRoute, ResolvedLlmConfig};

    fn resolved(
        provider_id: &str,
        model: &str,
        endpoint_family: EndpointFamily,
    ) -> ResolvedLlmConfig {
        ResolvedLlmConfig {
            provider_id: provider_id.into(),
            model: model.into(),
            base_url: format!("https://{provider_id}.example/v1"),
            api_key: None,
            thinking: false,
            reasoning: ResolvedReasoningRequest::default(),
            input_budget: 16_000,
            output_budget: 2_000,
            context_strategy: crate::ai_types::ContextStrategy::Hybrid,
            endpoint_family,
        }
    }

    fn route(
        primary_endpoint: EndpointFamily,
        failover_endpoints: impl IntoIterator<Item = EndpointFamily>,
    ) -> ResolvedCapabilityRoute {
        ResolvedCapabilityRoute {
            resolved: resolved("openai", "primary-model", primary_endpoint),
            failover_candidates: failover_endpoints
                .into_iter()
                .enumerate()
                .map(|(index, endpoint)| {
                    resolved(
                        if index == 0 { "deepseek" } else { "qwen" },
                        &format!("backup-{index}-model"),
                        endpoint,
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn selects_primary_direct_text_streaming_no_tools_candidate_and_hydrates_it_as_fast() {
        let route = DirectProviderRoute::from_secret_free_route(route(
            EndpointFamily::OpenAiCompatibleChatCompletions,
            [EndpointFamily::OpenAiCompatibleChatCompletions],
        ))
        .expect("assemble secret-free direct route");
        let selected =
            route.select_text_streaming_no_tools(EndpointFamily::OpenAiCompatibleChatCompletions);
        let mut reads = Vec::new();

        let provider = route
            .hydrate_selected_text_streaming_no_tools_as_fast_provider_config_with(
                EndpointFamily::OpenAiCompatibleChatCompletions,
                0,
                |service| {
                    reads.push(service.to_owned());
                    Ok(zeroize::Zeroizing::new("primary-secret".to_owned()))
                },
            )
            .expect("hydrate primary candidate");

        assert_eq!(
            selected
                .iter()
                .map(|candidate| candidate.provider_id.as_str())
                .collect::<Vec<_>>(),
            vec!["openai", "deepseek"]
        );
        assert_eq!(provider.provider.name, "openai");
        assert_eq!(provider.provider.model, "primary-model");
        assert_eq!(provider.provider.slot, CapabilitySlot::Fast);
        assert_eq!(provider.provider.api_key.as_deref(), Some("primary-secret"));
        assert_eq!(provider.max_output_tokens, 2_000);
        assert_eq!(reads, vec!["iris.llm.openai"]);
    }

    #[test]
    fn hydrates_only_selected_failover_after_a_retryable_failure() {
        let route = DirectProviderRoute::from_secret_free_route(route(
            EndpointFamily::OpenAiCompatibleChatCompletions,
            [EndpointFamily::OpenAiCompatibleChatCompletions],
        ))
        .expect("assemble secret-free direct route");
        let selected =
            route.select_text_streaming_no_tools(EndpointFamily::OpenAiCompatibleChatCompletions);
        let failover_index = route
            .next_selected_index_after(
                EndpointFamily::OpenAiCompatibleChatCompletions,
                0,
                ProviderFailure::Timeout,
            )
            .expect("retryable failure selects failover");
        let mut reads = Vec::new();

        let provider = route
            .hydrate_selected_text_streaming_no_tools_as_fast_provider_config_with(
                EndpointFamily::OpenAiCompatibleChatCompletions,
                failover_index,
                |service| {
                    reads.push(service.to_owned());
                    Ok(zeroize::Zeroizing::new("backup-secret".to_owned()))
                },
            )
            .expect("hydrate selected failover");

        assert_eq!(selected[failover_index].provider_id, "deepseek");
        assert_eq!(provider.provider.name, "deepseek");
        assert_eq!(provider.max_output_tokens, 2_000);
        assert_eq!(reads, vec!["iris.llm.deepseek"]);
    }

    #[test]
    fn returns_no_candidates_when_the_direct_adapter_cannot_use_the_route_protocol() {
        let route = DirectProviderRoute::from_secret_free_route(route(
            EndpointFamily::AnthropicMessages,
            [],
        ))
        .expect("assemble secret-free direct route");

        assert!(route
            .select_text_streaming_no_tools(EndpointFamily::OpenAiCompatibleChatCompletions)
            .is_empty());
        let mut reads = 0;

        assert!(route
            .hydrate_selected_text_streaming_no_tools_as_fast_provider_config_with(
                EndpointFamily::OpenAiCompatibleChatCompletions,
                0,
                |_| {
                    reads += 1;
                    Ok(zeroize::Zeroizing::new("must-not-read".to_owned()))
                },
            )
            .is_err());
        assert_eq!(reads, 0);
    }

    #[test]
    fn rejects_a_route_that_was_not_resolved_without_credentials() {
        let mut resolved_route = route(EndpointFamily::OpenAiCompatibleChatCompletions, []);
        resolved_route.resolved.api_key = Some("must-not-cross-boundary".into());

        assert!(DirectProviderRoute::from_secret_free_route(resolved_route).is_err());
    }

    #[test]
    fn exposes_a_production_hydrator_for_an_indexed_fast_dispatch() {
        let _hydrate: fn(
            &DirectProviderRoute,
            EndpointFamily,
            usize,
        ) -> crate::error::AppResult<DirectProviderDispatch> =
            DirectProviderRoute::hydrate_selected_text_streaming_no_tools_as_fast_dispatch;
    }
}
