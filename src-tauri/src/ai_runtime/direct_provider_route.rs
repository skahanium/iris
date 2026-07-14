//! Secret-free assembly for normal-domain direct text streaming requests.

use crate::ai_runtime::provider_router::{
    CandidateAvailability, CandidateHealth, ProviderCandidate, ProviderFailure,
    ProviderRequirements, ProviderRouter, SecurityDomain,
};
use crate::ai_types::{EndpointFamily, ProviderConfig, ResolvedReasoningRequest, RoutingPolicy};
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
    routing_policy: RoutingPolicy,
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
    pub(crate) fn from_secret_free_route(route: ResolvedCapabilityRoute) -> AppResult<Self> {
        let routing_policy = route.routing_policy;

        let candidates = std::iter::once(route.resolved)
            .chain(route.failover_candidates)
            .map(provider_candidate_from_resolved)
            .collect();

        Ok(Self {
            router: ProviderRouter::new(candidates),
            routing_policy,
            model_override: None,
        })
    }

    /// Return the persisted ranking policy carried by the resolved route.
    pub(crate) fn routing_policy(&self) -> RoutingPolicy {
        self.routing_policy
    }

    /// Apply the user-selected per-Run ranking preference after all hard
    /// capability filtering has already been fixed by the resolver.
    pub(crate) fn with_routing_policy(mut self, policy: RoutingPolicy) -> Self {
        self.routing_policy = policy;
        self
    }

    /// Restrict this Run to one explicit configured provider/model. Capability
    /// filtering still happens before the selected candidate is hydrated.
    pub(crate) fn with_model_override(mut self, provider_id: String, model_id: String) -> Self {
        self.model_override = Some((provider_id, model_id));
        self
    }

    /// Select external direct text-streaming candidates that intentionally do not expose tools.
    pub(crate) fn select_text_streaming_no_tools(
        &self,
        endpoint_family: EndpointFamily,
    ) -> Vec<&ProviderCandidate> {
        self.filter_model_override(self.router.select_candidates(&ProviderRequirements {
            endpoint_family: Some(endpoint_family),
            streaming: true,
            tools: false,
            vision: false,
            reasoning: false,
            min_input_budget_tokens: 0,
            min_output_budget_tokens: 0,
            security_domain: SecurityDomain::External,
        }))
    }

    /// Select ordered streaming candidates for actual Run requirements.
    ///
    /// Unlike the legacy direct helper this preserves tool, vision, and reasoning
    /// requirements. The gateway supports each candidate's own endpoint family,
    /// so no protocol family is imposed here.
    pub(crate) fn select_streaming_for_requirements(
        &self,
        requirements: ProviderRequirements,
        policy: RoutingPolicy,
    ) -> Vec<&ProviderCandidate> {
        self.filter_model_override(self.router.select_ranked_candidates(&requirements, policy))
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
        policy: RoutingPolicy,
        selected_index: usize,
        read_credential: F,
    ) -> AppResult<DirectProviderDispatch>
    where
        F: FnMut(&str) -> AppResult<zeroize::Zeroizing<String>>,
    {
        let candidate = self
            .select_streaming_for_requirements(requirements, policy)
            .get(selected_index)
            .copied()
            .ok_or_else(|| AppError::msg("agent_run_no_capable_model"))?;
        self.router
            .hydrate_candidate_with(candidate, read_credential)
            .map(|candidate| {
                let reasoning = candidate.candidate().reasoning;
                let thinking = candidate.candidate().thinking;
                let max_output_tokens = candidate.candidate().output_budget_tokens;
                let slot = candidate.candidate().slot;
                DirectProviderDispatch {
                    provider: candidate.into_provider_config(slot),
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
        policy: RoutingPolicy,
        selected_index: usize,
    ) -> AppResult<DirectProviderDispatch> {
        self.hydrate_selected_streaming_dispatch_with(
            requirements,
            policy,
            selected_index,
            crate::credentials::get_runtime_secret,
        )
    }

    /// Hydrate a generic candidate using the route's persisted ranking policy.
    pub(crate) fn hydrate_selected_streaming_dispatch_for_configured_policy(
        &self,
        requirements: ProviderRequirements,
        selected_index: usize,
    ) -> AppResult<DirectProviderDispatch> {
        self.hydrate_selected_streaming_dispatch(requirements, self.routing_policy, selected_index)
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
        self.hydrate_selected_streaming_dispatch_with(
            ProviderRequirements {
                endpoint_family: Some(endpoint_family),
                streaming: true,
                tools: false,
                vision: false,
                reasoning: false,
                min_input_budget_tokens: 0,
                min_output_budget_tokens: 0,
                security_domain: SecurityDomain::External,
            },
            RoutingPolicy::Balanced,
            selected_index,
            read_credential,
        )
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

    /// Return the safe configured provider identifier for one selected candidate.
    pub(crate) fn selected_provider_id(
        &self,
        endpoint_family: EndpointFamily,
        selected_index: usize,
    ) -> Option<&str> {
        self.select_text_streaming_no_tools(endpoint_family)
            .get(selected_index)
            .map(|candidate| candidate.provider_id.as_str())
    }

    /// Return the next ranked generic candidate after a strictly transient failure.
    pub(crate) fn next_selected_index_after_for_requirements(
        &self,
        requirements: ProviderRequirements,
        policy: RoutingPolicy,
        attempted_index: usize,
        failure: ProviderFailure,
    ) -> Option<usize> {
        let selected = self.select_streaming_for_requirements(requirements, policy);
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
        policy: RoutingPolicy,
        selected_index: usize,
    ) -> Option<&str> {
        self.select_streaming_for_requirements(requirements, policy)
            .get(selected_index)
            .map(|candidate| candidate.provider_id.as_str())
    }
}

fn provider_candidate_from_resolved(resolved: ResolvedLlmConfig) -> ProviderCandidate {
    let credential_service = crate::llm::providers::requires_api_key(&resolved.provider_id)
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
        quality_score_millis: 500,
        latency_score_millis: 500,
        cost_score_millis: 500,
        credential_service,
        slot: resolved.capability_slot,
        reasoning: resolved.reasoning,
        thinking: resolved.thinking || resolved.reasoning.requested,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::provider_router::{ProviderRequirements, SecurityDomain};
    use crate::ai_types::{
        CapabilitySlot, EndpointFamily, ReasoningAdapter, ReasoningControl, ReasoningMode,
        ResolvedReasoningRequest, RoutingPolicy,
    };
    use crate::llm::config::{ResolvedCapabilityRoute, ResolvedLlmConfig};

    fn resolved(
        provider_id: &str,
        model: &str,
        endpoint_family: EndpointFamily,
    ) -> ResolvedLlmConfig {
        ResolvedLlmConfig {
            capability_slot: CapabilitySlot::Fast,
            provider_id: provider_id.into(),
            model: model.into(),
            base_url: format!("https://{provider_id}.example/v1"),
            thinking: false,
            reasoning: ResolvedReasoningRequest::default(),
            input_budget: 16_000,
            output_budget: 2_000,
            context_strategy: crate::ai_types::ContextStrategy::Hybrid,
            endpoint_family,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: true,
            supports_reasoning: true,
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
            routing_policy: RoutingPolicy::Balanced,
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
        assert_eq!(
            provider.provider.api_key.as_deref().map(String::as_str),
            Some("primary-secret")
        );
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
    fn exposes_a_production_hydrator_for_an_indexed_fast_dispatch() {
        let _hydrate: fn(
            &DirectProviderRoute,
            EndpointFamily,
            usize,
        ) -> crate::error::AppResult<DirectProviderDispatch> =
            DirectProviderRoute::hydrate_selected_text_streaming_no_tools_as_fast_dispatch;
    }

    #[test]
    fn generic_dispatch_preserves_real_capabilities_reasoning_and_zeroizing_credential() {
        let mut resolved_route = route(
            EndpointFamily::OpenAiCompatibleChatCompletions,
            [EndpointFamily::OpenAiCompatibleChatCompletions],
        );
        resolved_route.resolved.capability_slot = CapabilitySlot::AgentTools;
        resolved_route.resolved.reasoning = ResolvedReasoningRequest {
            mode: ReasoningMode::Medium,
            adapter: ReasoningAdapter::OpenAiResponses,
            control: ReasoningControl::Effort,
            visibility: crate::ai_types::ReasoningVisibility::HiddenChannel,
            requested: true,
            isolate_output: true,
        };
        let route = DirectProviderRoute::from_secret_free_route(resolved_route)
            .expect("assemble generic route");
        let requirements = ProviderRequirements {
            endpoint_family: None,
            streaming: true,
            tools: true,
            vision: false,
            reasoning: true,
            min_input_budget_tokens: 1,
            min_output_budget_tokens: 1,
            security_domain: SecurityDomain::External,
        };

        let dispatch = route
            .hydrate_selected_streaming_dispatch_with(
                requirements,
                RoutingPolicy::Balanced,
                0,
                |_| Ok(zeroize::Zeroizing::new("secret".to_string())),
            )
            .expect("hydrate generic candidate");

        assert_eq!(dispatch.provider.slot, CapabilitySlot::AgentTools);
        assert!(dispatch.thinking);
        assert_eq!(dispatch.reasoning.mode, ReasoningMode::Medium);
        assert_eq!(
            dispatch.provider.api_key.as_deref().map(String::as_str),
            Some("secret")
        );
    }

    #[test]
    fn model_override_restricts_selection_before_any_credential_is_read() {
        let route = DirectProviderRoute::from_secret_free_route(route(
            EndpointFamily::OpenAiCompatibleChatCompletions,
            [EndpointFamily::OpenAiCompatibleChatCompletions],
        ))
        .expect("assemble route")
        .with_model_override("deepseek".into(), "backup-0-model".into());
        let requirements = ProviderRequirements {
            endpoint_family: None,
            streaming: true,
            tools: true,
            vision: false,
            reasoning: false,
            min_input_budget_tokens: 1,
            min_output_budget_tokens: 1,
            security_domain: SecurityDomain::External,
        };
        let mut reads = Vec::new();
        let dispatch = route
            .hydrate_selected_streaming_dispatch_with(
                requirements,
                RoutingPolicy::Balanced,
                0,
                |service| {
                    reads.push(service.to_string());
                    Ok(zeroize::Zeroizing::new("override-secret".to_string()))
                },
            )
            .expect("hydrate override only");

        assert_eq!(dispatch.provider.name, "deepseek");
        assert_eq!(dispatch.provider.model, "backup-0-model");
        assert_eq!(reads, vec!["iris.llm.deepseek"]);
    }
}
