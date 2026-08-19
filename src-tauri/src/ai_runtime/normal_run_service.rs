//! Headless-capable orchestration for accepted normal-domain Runs.
//!
//! The desktop command supplies a Tauri event sink and app handle, while
//! in-process callers can use another sink and omit the handle. Policy,
//! context, evidence, routing, dispatch, and terminalization stay on this one
//! production path.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tauri::AppHandle;

use crate::ai_runtime::agent_run_repository::AgentRunRepository;
use crate::ai_runtime::agent_tool_loop::ToolLoopExecutor;
use crate::ai_runtime::fresh_research_plan::build_fresh_research_plan;
use crate::ai_runtime::prompt_contract::PromptContractV3;
use crate::ai_runtime::run_contract::{
    AssistantRunAccepted, CapabilityId, Effort, FreshFactDomain, Freshness, Modality,
    RunBudgetPolicy, SafeRunErrorCode, VerificationRequirement, WebEvidenceFailureReason,
};
use crate::ai_runtime::run_engine::{
    FailoverStreamingProvider, RunEngine, RunEventSink, WebVerificationFailure,
};
use crate::ai_runtime::run_intake::RunIntake;
use crate::ai_runtime::run_tool_loop::NormalRunToolExecutor;
use crate::ai_runtime::tool_executor::ToolRegistry;
use crate::ai_runtime::tool_surface::{
    classify_time_sensitivity, ToolSurfaceInput, ToolSurfacePlan, ToolSurfacePlanner,
};
use crate::ai_runtime::{LlmMessage, MessageRole, ToolCall};
use crate::ai_types::{AgentIntent, SkillActivationPlanSummary};
use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

fn is_current_fact_domain(domain: FreshFactDomain) -> bool {
    matches!(
        domain,
        FreshFactDomain::Weather
            | FreshFactDomain::News
            | FreshFactDomain::Finance
            | FreshFactDomain::Entertainment
            | FreshFactDomain::Sports
    )
}

fn plan_tool_surface(
    context: &crate::ai_runtime::run_context::RunContext,
    authorized_capabilities: &[CapabilityId],
    web_prefetched: bool,
) -> ToolSurfacePlan {
    let plan = ToolSurfacePlanner::plan(ToolSurfaceInput {
        web_enabled: authorized_capabilities
            .iter()
            .any(|capability| capability.as_str() == "web.search"),
        time_sensitive: classify_time_sensitivity(context.envelope.fresh_fact.domain),
        effort: context.envelope.effort,
        web_prefetched,
        authorized_capabilities: authorized_capabilities.to_vec(),
    });
    tracing::debug!(
        effort = ?plan.effort,
        expose_web_search = plan.expose_web_search,
        web_prefetched = plan.web_prefetched,
        web_instruction = ?plan.web_instruction,
        "tool surface planned"
    );
    plan
}

/// Execute one already-accepted normal-domain Run through the production
/// orchestration path without requiring a desktop runtime.
pub(crate) async fn execute_normal_run(
    state: Arc<AppState>,
    accepted: AssistantRunAccepted,
    vault: Option<PathBuf>,
    app_handle: Option<AppHandle>,
    sink: &impl RunEventSink,
) {
    execute_normal_run_internal(state, accepted, vault, app_handle, sink, None).await;
}

/// Evaluation-only headless entry. It shares the complete production
/// orchestration and adds only an in-memory telemetry observer.
#[cfg(test)]
pub(crate) async fn execute_normal_run_with_eval_telemetry(
    state: Arc<AppState>,
    accepted: AssistantRunAccepted,
    vault: Option<PathBuf>,
    sink: &impl RunEventSink,
    telemetry: &crate::ai_runtime::agent_capacity_eval::EvaluationTelemetryTap,
) {
    execute_normal_run_internal(state, accepted, vault, None, sink, Some(telemetry)).await;
}

async fn execute_normal_run_internal(
    state: Arc<AppState>,
    accepted: AssistantRunAccepted,
    vault: Option<PathBuf>,
    app_handle: Option<AppHandle>,
    sink: &impl RunEventSink,
    telemetry: Option<&crate::ai_runtime::agent_capacity_eval::EvaluationTelemetryTap>,
) {
    let db = Arc::clone(&state.db);
    if RunEngine::mark_preparing_with_sink(&db, &accepted.session, &accepted.run_id, sink).is_err()
    {
        return;
    }
    let policy = match evaluate_normal_run_policy(&db, &accepted) {
        Ok(policy) => policy,
        Err(_) => {
            let _ = RunEngine::fail_before_dispatch_with_sink(
                &db,
                &accepted.session,
                &accepted.run_id,
                SafeRunErrorCode::PersistenceFailed,
                sink,
            );
            return;
        }
    };
    match RunEngine::enforce_policy_before_dispatch_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        &policy,
        sink,
    ) {
        Ok(true) => {}
        Ok(false) | Err(_) => return,
    }
    let budget_policy = match AgentRunRepository::budget_policy_for_session(
        &db,
        &accepted.session.session_key,
        &accepted.run_id,
    ) {
        Ok(Some(policy)) => policy,
        Ok(None) | Err(_) => {
            let _ = RunEngine::fail_before_dispatch_with_sink(
                &db,
                &accepted.session,
                &accepted.run_id,
                SafeRunErrorCode::PersistenceFailed,
                sink,
            );
            return;
        }
    };
    let authorized_capabilities = match crate::ai_runtime::agent_run_repository::AgentRunRepository::persist_authorization_snapshot(
        &db,
        &accepted.session.session_key,
        &accepted.run_id,
        &policy.allowed_capabilities,
    ) {
        Ok(snapshot) => snapshot,
        Err(_) => {
            let _ = RunEngine::fail_before_dispatch_with_sink(
                &db,
                &accepted.session,
                &accepted.run_id,
                SafeRunErrorCode::PermissionDenied,
                sink,
            );
            return;
        }
    };
    let context = match crate::ai_runtime::run_context::RunContextAssembler::assemble(
        &db,
        vault.as_deref(),
        &accepted.session.session_key,
        &accepted.run_id,
    ) {
        Ok(context) => context,
        Err(error) => {
            let _ = RunEngine::fail_before_dispatch_with_sink(
                &db,
                &accepted.session,
                &accepted.run_id,
                crate::ai_runtime::run_context::classify_context_assembly_failure(&error),
                sink,
            );
            return;
        }
    };
    let domain_plan = context.domain_plan();
    // Never route an external-fact Run to a direct model answer when Web is
    // unavailable. This is a safety denial, not a degraded offline answer.
    if context.envelope.verification_requirement == VerificationRequirement::CurrentRunWeb
        && context.envelope.freshness == Freshness::Offline
    {
        let _ = RunEngine::fail_before_dispatch_with_sink(
            &db,
            &accepted.session,
            &accepted.run_id,
            SafeRunErrorCode::WebVerificationRequired,
            sink,
        );
        return;
    }
    let evidence_ids = match crate::ai_runtime::run_context::RunContextAssembler::register_evidence(
        &db,
        &accepted.run_id,
        &context,
    ) {
        Ok(evidence_ids) => evidence_ids,
        Err(_) => {
            let _ = RunEngine::fail_before_dispatch_with_sink(
                &db,
                &accepted.session,
                &accepted.run_id,
                SafeRunErrorCode::PersistenceFailed,
                sink,
            );
            return;
        }
    };
    // The immutable capability snapshot still determines Web authority. Strict
    // factual Runs consume that authority in the deterministic prefetch below;
    // non-strict Runs may expose it to the model tool surface.
    let execution = dispatch_normal_run_after_context(
        &state,
        app_handle,
        &db,
        &accepted,
        &context,
        &domain_plan,
        &evidence_ids,
        &authorized_capabilities,
        &budget_policy,
        vault.as_deref(),
        sink,
        telemetry,
    )
    .await;
    if let Err(error) = execution {
        let safe_code = SafeRunErrorCode::from_app_error(&error);
        tracing::warn!(
            run_id = %accepted.run_id,
            stage = "execution_exit",
            safe_code = safe_code.as_str(),
            "normal Agent Run exited without a successful result"
        );
        let still_active = RunIntake::get(&db, &accepted.session, &accepted.run_id)
            .ok()
            .flatten()
            .is_some_and(|response| !response.run.state.is_terminal());
        let is_explicit_persistence_failure = matches!(
            &error,
            crate::error::AppError::Message(message)
                if message == SafeRunErrorCode::PersistenceFailed.as_str()
        ) || matches!(
            &error,
            crate::error::AppError::Run(SafeRunErrorCode::PersistenceFailed)
        );
        if still_active
            && safe_code == SafeRunErrorCode::PersistenceFailed
            && !is_explicit_persistence_failure
        {
            let _ =
                RunEngine::fail_active_with_sink(&db, &accepted.session, &accepted.run_id, sink);
        }
    }

    if crate::ai_runtime::model_gateway::is_abort_requested(&accepted.run_id) {
        // The gateway normally clears the marker. This defensive cleanup only
        // covers a provider implementation that exited during cancellation.
        crate::ai_runtime::model_gateway::clear_abort(&accepted.run_id);
    }
}

/// Rebuild and evaluate the persisted normal Run policy before Provider routing.
fn evaluate_normal_run_policy(
    db: &Database,
    accepted: &AssistantRunAccepted,
) -> AppResult<crate::ai_runtime::policy_decision_engine::RunPolicyDecision> {
    let request =
        crate::ai_runtime::agent_run_repository::AgentRunRepository::policy_request_for_session(
            db,
            &accepted.session.session_key,
            &accepted.run_id,
        )?
        .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
    let engine = crate::ai_runtime::document_policy_repository::load_policy_decision_engine(db)?;
    Ok(engine.evaluate_run(request))
}

/// The immutable prompt-only Skill selection for a single normal Run.
///
/// The registry is populated when a vault is selected or a user explicitly
/// refreshes Skills. This function deliberately has no filesystem fallback:
/// scanning an untrusted vault while executing a Run would make the run
/// boundary nondeterministic and would bypass the confirmed cache.
pub(crate) struct CachedSkillActivation {
    pub(crate) plan: Option<SkillActivationPlanSummary>,
    pub(crate) prompt_overlay: String,
}

pub(crate) fn build_cached_skill_activation(
    state: &AppState,
    vault: Option<&std::path::Path>,
    context: &crate::ai_runtime::run_context::RunContext,
    authorized_capabilities: &[crate::ai_runtime::run_contract::CapabilityId],
) -> AppResult<CachedSkillActivation> {
    let Some(vault) = vault else {
        return Ok(CachedSkillActivation {
            plan: None,
            prompt_overlay: String::new(),
        });
    };
    // A cache miss is deliberately safe-empty rather than a filesystem
    // fallback or a Run failure. Vault activation/explicit refresh populates
    // the registry; a transient cache lifecycle gap may only suppress optional
    // prompt-only Skills, never change the Run's tool authority or availability.
    let Some((skills, index)) = state.cached_skill_activation_for_vault(vault)? else {
        return Ok(CachedSkillActivation {
            plan: None,
            prompt_overlay: String::new(),
        });
    };
    let embedding_scheduler = state.embedding_scheduler();
    let source_hints = context
        .materials
        .iter()
        .map(|material| material.source_path.clone())
        .chain(
            context
                .local_retrieval_packets
                .iter()
                .filter_map(|packet| packet.source_path.clone()),
        )
        .collect::<Vec<_>>();
    let intent = skill_intent_for_run(context, authorized_capabilities);
    let query_embedding = crate::ai_runtime::skills::SKILL_VECTOR_RERANK_DEFAULT_ENABLED
        .then(|| embedding_scheduler.cached_skill_activation_query(&context.user_message))
        .flatten();
    let plan = crate::ai_runtime::skills::build_skill_activation_plan_for_task_with_query_embedding(
        &skills,
        intent,
        &context.user_message,
        &source_hints,
        (!index.is_empty()).then_some(&index),
        query_embedding.as_deref(),
    );
    let selected = crate::ai_runtime::skills::activated_skills_from_plan(&plan, &skills);
    if selected.is_empty() {
        return Ok(CachedSkillActivation {
            plan: None,
            prompt_overlay: String::new(),
        });
    }
    Ok(CachedSkillActivation {
        plan: Some(plan),
        prompt_overlay: crate::ai_runtime::skills::inject_selected_skills_into_prompt(
            vault, &selected,
        ),
    })
}

fn skill_intent_for_run(
    context: &crate::ai_runtime::run_context::RunContext,
    authorized_capabilities: &[crate::ai_runtime::run_contract::CapabilityId],
) -> AgentIntent {
    use crate::ai_runtime::run_contract::Effect;

    match context.envelope.effect {
        Effect::Draft | Effect::Apply => AgentIntent::Write,
        // The policy snapshot is the only source from which Skills can infer
        // that Web research is available. Freshness is a completion
        // requirement, never an implicit capability grant.
        Effect::Answer
            if authorized_capabilities
                .iter()
                .any(|capability| capability.as_str() == "web.search") =>
        {
            AgentIntent::Research
        }
        Effect::Answer
            if !context.materials.is_empty() || !context.retrieval_scope.is_unrestricted() =>
        {
            AgentIntent::AskNotes
        }
        Effect::Answer => AgentIntent::Chat,
    }
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_normal_run_after_context(
    state: &Arc<AppState>,
    app_handle: Option<AppHandle>,
    db: &Database,
    accepted: &AssistantRunAccepted,
    context: &crate::ai_runtime::run_context::RunContext,
    domain_plan: &crate::ai_runtime::domain_executor::DomainExecutionPlan,
    registered_evidence_ids: &[i64],
    authorized_capabilities: &[crate::ai_runtime::run_contract::CapabilityId],
    budget_policy: &RunBudgetPolicy,
    vault: Option<&std::path::Path>,
    sink: &impl RunEventSink,
    telemetry: Option<&crate::ai_runtime::agent_capacity_eval::EvaluationTelemetryTap>,
) -> AppResult<()> {
    let active_skills =
        build_cached_skill_activation(state, vault, context, authorized_capabilities)?;
    let mut messages =
        context.messages_with_domain_plan_and_skills(domain_plan, &active_skills.prompt_overlay);
    let routing_prompt = context.prompt_with_domain_plan(domain_plan);
    let mut evidence_ids = registered_evidence_ids.to_vec();
    evidence_ids.sort_unstable();
    evidence_ids.dedup();
    tracing::info!(
        run_id = %accepted.run_id,
        web_mode = ?context.envelope.freshness,
        web_reason = ?context.envelope.web_reason,
        web_execution = match context.envelope.freshness {
            Freshness::Offline => "skipped",
            Freshness::WebPreferred => "model_decides",
            Freshness::WebRequired => "evidence_required",
        },
        "Run Web decision"
    );

    if context.envelope.verification_requirement == VerificationRequirement::CurrentRunWeb {
        return dispatch_required_web_verified_run(
            state,
            app_handle,
            db,
            accepted,
            context,
            domain_plan,
            &mut messages,
            &evidence_ids,
            authorized_capabilities,
            budget_policy,
            active_skills.plan,
            sink,
            telemetry,
        )
        .await;
    }

    // ToolLoop/Durable Runs receive only the snapshot-authorized surface. The model may call
    // web_search when authorized; search failure emits CapabilityDegraded.
    let needs_follow_up_tools =
        matches!(context.envelope.effort, Effort::ToolLoop | Effort::Durable);
    if needs_follow_up_tools {
        let required_web_provider_snapshots = authorized_capabilities
            .iter()
            .any(|capability| capability.as_str() == "web.search")
            .then(|| crate::ai_runtime::mcp_runtime_registry::resolve_web_search_provider_route(db))
            .transpose()
            .ok()
            .flatten()
            .unwrap_or_default();
        let registry = ToolRegistry::for_run(db, &accepted.run_id)?;
        let mut tool_surface_plan = plan_tool_surface(context, authorized_capabilities, false);
        let mut tools = ToolRegistry::constrain_for_run_context(
            registry.tools_for_authorized_capabilities(
                authorized_capabilities,
                context.envelope.effort != Effort::Durable,
            ),
            context.envelope.context,
            &context.retrieval_scope,
        );
        if !tool_surface_plan.expose_web_search {
            tools.retain(|tool| tool.name != "web_search");
        }
        tool_surface_plan.tool_names = tools.iter().map(|tool| tool.name.clone()).collect();
        let requirements = crate::ai_runtime::provider_router::ProviderRequirements {
            endpoint_family: None,
            streaming: true,
            tools: true,
            vision: context.envelope.modalities.contains(&Modality::Image),
            reasoning: false,
            min_input_budget_tokens: crate::ai_runtime::text_support::estimate_tokens(
                &routing_prompt,
            ),
            min_output_budget_tokens: 1,
            security_domain: crate::ai_runtime::provider_router::SecurityDomain::External,
        };
        let route = resolve_normal_route(
            db,
            accepted,
            context,
            requirements.min_input_budget_tokens,
            requirements.vision,
            true,
            sink,
        )?;
        let provider =
            FailoverStreamingProvider::new(route, requirements, db, &accepted.session, sink);
        #[cfg(test)]
        let provider = if let Some(client) = state.test_streaming_client() {
            provider.with_test_streaming_client(client)
        } else {
            provider
        };
        let mut executor = NormalRunToolExecutor::new(
            state,
            app_handle,
            accepted,
            context,
            authorized_capabilities.to_vec(),
            budget_policy.clone(),
            sink,
            required_web_provider_snapshots,
        )
        .with_allowed_tool_names(&tool_surface_plan.tool_names)
        .with_skill_activation_plan(active_skills.plan.clone())
        .with_child_run_provider(&provider);
        if is_current_fact_domain(context.envelope.fresh_fact.domain) {
            let plan = build_fresh_research_plan(
                &context.user_message,
                &context.envelope.fresh_fact,
                "zh-CN",
                None,
            )?;
            executor = executor.with_fresh_research_budget(plan.budget);
        }
        return if let Some(telemetry) = telemetry {
            RunEngine::execute_tool_loop_with_eval_telemetry(
                db,
                &accepted.session,
                &accepted.run_id,
                messages,
                tools,
                &evidence_ids,
                Some(domain_plan),
                &provider,
                &executor,
                sink,
                telemetry,
            )
            .await
        } else {
            RunEngine::execute_tool_loop_with_sink(
                db,
                &accepted.session,
                &accepted.run_id,
                messages,
                tools,
                &evidence_ids,
                Some(domain_plan),
                &provider,
                &executor,
                sink,
            )
            .await
        };
    }

    let direct_requirements = crate::ai_runtime::provider_router::ProviderRequirements {
        endpoint_family: None,
        streaming: true,
        tools: false,
        vision: context.envelope.modalities.contains(&Modality::Image),
        reasoning: false,
        min_input_budget_tokens: crate::ai_runtime::text_support::estimate_tokens(&routing_prompt),
        min_output_budget_tokens: 1,
        security_domain: crate::ai_runtime::provider_router::SecurityDomain::External,
    };
    let route = resolve_normal_route(
        db,
        accepted,
        context,
        direct_requirements.min_input_budget_tokens,
        direct_requirements.vision,
        false,
        sink,
    )?;
    let provider =
        FailoverStreamingProvider::new(route, direct_requirements, db, &accepted.session, sink);
    #[cfg(test)]
    let provider = if let Some(client) = state.test_streaming_client() {
        provider.with_test_streaming_client(client)
    } else {
        provider
    };
    if let Some(telemetry) = telemetry {
        RunEngine::execute_direct_streaming_with_messages_evidence_and_domain_plan_with_eval_telemetry(
            db,
            &accepted.session,
            &accepted.run_id,
            &messages,
            &evidence_ids,
            domain_plan,
            &provider,
            sink,
            telemetry,
        )
        .await
    } else {
        RunEngine::execute_direct_streaming_with_messages_evidence_and_domain_plan_with_sink(
            db,
            &accepted.session,
            &accepted.run_id,
            &messages,
            &evidence_ids,
            domain_plan,
            &provider,
            sink,
        )
        .await
    }
}

/// Execute a strict external-fact Run as one deterministic evidence fetch
/// followed by one tool-free model turn. The model never decides whether the
/// first required Web search happens.
#[allow(clippy::too_many_arguments)]
async fn dispatch_required_web_verified_run(
    state: &Arc<AppState>,
    app_handle: Option<AppHandle>,
    db: &Database,
    accepted: &AssistantRunAccepted,
    context: &crate::ai_runtime::run_context::RunContext,
    domain_plan: &crate::ai_runtime::domain_executor::DomainExecutionPlan,
    messages: &mut Vec<LlmMessage>,
    registered_evidence_ids: &[i64],
    authorized_capabilities: &[crate::ai_runtime::run_contract::CapabilityId],
    budget_policy: &RunBudgetPolicy,
    skill_plan: Option<SkillActivationPlanSummary>,
    sink: &impl RunEventSink,
    telemetry: Option<&crate::ai_runtime::agent_capacity_eval::EvaluationTelemetryTap>,
) -> AppResult<()> {
    // A required Web Run must bind one concrete provider before any work starts.
    // Do not turn a resolver failure into `None`: a frozen empty selection is
    // indistinguishable from a later provider outage and previously produced a
    // misleading generic result after the tool stage had already begun.
    let provider_snapshots =
        match crate::ai_runtime::mcp_runtime_registry::resolve_web_search_provider_route(db) {
            Ok(snapshots) => snapshots,
            Err(_) => {
                let _ = RunEngine::fail_web_verification_with_sink(
                    db,
                    &accepted.session,
                    &accepted.run_id,
                    WebVerificationFailure {
                        code: SafeRunErrorCode::WebProviderUnavailable,
                        reason: WebEvidenceFailureReason::ProviderUnavailable,
                        retryable: false,
                        attempt_count: 0,
                        duration_bucket: "not_started",
                    },
                    sink,
                );
                return Err(AppError::msg(
                    SafeRunErrorCode::WebProviderUnavailable.as_str(),
                ));
            }
        };
    let executor = NormalRunToolExecutor::new(
        state,
        app_handle,
        accepted,
        context,
        authorized_capabilities.to_vec(),
        budget_policy.clone(),
        sink,
        provider_snapshots,
    )
    .with_allowed_tool_names(&["web_search".to_string()])
    .with_skill_activation_plan(skill_plan);
    let query = required_web_query(context)?;
    let first_prefetch = crate::ai_runtime::agent_tool_loop::ToolLoopExecutor::execute(
        &executor,
        &accepted.run_id,
        &ToolCall::new(
            "required-web-evidence",
            "web_search",
            serde_json::json!({ "query": query }).to_string(),
        ),
        1,
    )
    .await;
    let first_prefetch = match first_prefetch {
        Ok(result) => result,
        Err(_) => {
            let _ = RunEngine::fail_web_verification_with_sink(
                db,
                &accepted.session,
                &accepted.run_id,
                WebVerificationFailure {
                    code: SafeRunErrorCode::WebEvidenceInvalid,
                    reason: WebEvidenceFailureReason::Unknown,
                    retryable: false,
                    attempt_count: 1,
                    duration_bucket: "not_started",
                },
                sink,
            );
            return Err(AppError::msg(SafeRunErrorCode::WebEvidenceInvalid.as_str()));
        }
    };
    let evidence_results = vec![first_prefetch
        .output
        .get("results")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]))];
    let prefetch_succeeded = first_prefetch.success;
    if !prefetch_succeeded || !executor.has_web_evidence() {
        let (code, failure_reason, retryable, attempt_count) =
            executor.web_verification_failure_details();
        let _ = RunEngine::fail_web_verification_with_sink(
            db,
            &accepted.session,
            &accepted.run_id,
            WebVerificationFailure {
                code,
                reason: failure_reason,
                retryable,
                attempt_count,
                duration_bucket: strict_web_duration_bucket(Duration::from_millis(
                    first_prefetch.duration_ms,
                )),
            },
            sink,
        );
        return Err(AppError::msg(code.as_str()));
    }

    let evidence_json = serde_json::to_string(&evidence_results).map_err(AppError::from)?;
    messages.insert(
        1,
        LlmMessage {
            role: MessageRole::System,
            content: PromptContractV3::web_evidence_data_prompt(&evidence_json, false).into(),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        },
    );
    let mut verified_evidence_ids = registered_evidence_ids.to_vec();
    verified_evidence_ids.extend(executor.evidence_ids());
    verified_evidence_ids.sort_unstable();
    verified_evidence_ids.dedup();

    // A simple external fact is already fully planned: obtain one required
    // evidence packet, then let the existing Direct Run path produce one
    // tool-free answer. Do not promote it into the general Agent loop merely
    // because Web verification was required.
    if context.envelope.effort == Effort::Direct {
        let requirements = crate::ai_runtime::provider_router::ProviderRequirements {
            endpoint_family: None,
            streaming: true,
            tools: false,
            vision: context.envelope.modalities.contains(&Modality::Image),
            reasoning: false,
            min_input_budget_tokens: crate::ai_runtime::text_support::estimate_tokens(
                &serde_json::to_string(messages).map_err(AppError::from)?,
            ),
            min_output_budget_tokens: 512,
            security_domain: crate::ai_runtime::provider_router::SecurityDomain::External,
        };
        let route = resolve_normal_route(
            db,
            accepted,
            context,
            requirements.min_input_budget_tokens,
            requirements.vision,
            false,
            sink,
        )?;
        let provider =
            FailoverStreamingProvider::new(route, requirements, db, &accepted.session, sink);
        #[cfg(test)]
        let provider = if let Some(client) = state.test_streaming_client() {
            provider.with_test_streaming_client(client)
        } else {
            provider
        };
        return if let Some(telemetry) = telemetry {
            RunEngine::execute_direct_streaming_with_messages_evidence_and_domain_plan_with_eval_telemetry(
                db,
                &accepted.session,
                &accepted.run_id,
                messages,
                &verified_evidence_ids,
                domain_plan,
                &provider,
                sink,
                telemetry,
            )
            .await
        } else {
            RunEngine::execute_direct_streaming_with_messages_evidence_and_domain_plan_with_sink(
                db,
                &accepted.session,
                &accepted.run_id,
                messages,
                &verified_evidence_ids,
                domain_plan,
                &provider,
                sink,
            )
            .await
        };
    }
    // Required Web evidence is prefetched deterministically. For ordinary
    // strict facts the final model only receives local follow-up tools and
    // `web_search` stays hidden. Current-fact research keeps `web_search`
    // visible so the model can issue bounded follow-up searches for unresolved
    // evidence gaps.
    let research_mode = is_current_fact_domain(context.envelope.fresh_fact.domain);
    let mut tool_surface_plan = plan_tool_surface(context, authorized_capabilities, !research_mode);
    let registry = ToolRegistry::for_run(db, &accepted.run_id)?;
    let mut tools = if research_mode {
        ToolRegistry::constrain_for_run_context(
            registry.tools_for_authorized_capabilities(authorized_capabilities, true),
            context.envelope.context,
            &context.retrieval_scope,
        )
    } else {
        let local_follow_up_capabilities = strict_follow_up_capabilities(authorized_capabilities);
        ToolRegistry::constrain_for_run_context(
            registry.tools_for_authorized_capabilities(&local_follow_up_capabilities, true),
            context.envelope.context,
            &context.retrieval_scope,
        )
    };
    if !tool_surface_plan.expose_web_search {
        tools.retain(|tool| tool.name != "web_search");
    }
    let has_local_follow_up_tools = !tools.is_empty();
    let serialized_messages = serde_json::to_string(messages).map_err(AppError::from)?;
    let mut requirements = crate::ai_runtime::provider_router::ProviderRequirements {
        endpoint_family: None,
        streaming: true,
        tools: has_local_follow_up_tools,
        vision: context.envelope.modalities.contains(&Modality::Image),
        reasoning: false,
        min_input_budget_tokens: crate::ai_runtime::text_support::estimate_tokens(
            &serialized_messages,
        ),
        min_output_budget_tokens: 512,
        security_domain: crate::ai_runtime::provider_router::SecurityDomain::External,
    };
    let mut route = resolve_normal_route(
        db,
        accepted,
        context,
        requirements.min_input_budget_tokens,
        requirements.vision,
        has_local_follow_up_tools,
        sink,
    )?;
    let mut structured_finalization = false;
    let tool_capable_requirements = crate::ai_runtime::provider_router::ProviderRequirements {
        tools: true,
        ..requirements
    };
    if calibrated_structured_finalization_route(db, context, tool_capable_requirements)?.is_some() {
        messages[1].content =
            PromptContractV3::web_evidence_data_prompt(&evidence_json, true).into();
        let strict_requirements = crate::ai_runtime::provider_router::ProviderRequirements {
            min_input_budget_tokens: crate::ai_runtime::text_support::estimate_tokens(
                &serde_json::to_string(messages).map_err(AppError::from)?,
            ),
            tools: true,
            ..tool_capable_requirements
        };
        if let Some(strict_route) =
            calibrated_structured_finalization_route(db, context, strict_requirements)?
        {
            route = strict_route;
            requirements = strict_requirements;
            structured_finalization = true;
        } else {
            messages[1].content =
                PromptContractV3::web_evidence_data_prompt(&evidence_json, false).into();
        }
    }
    // Current-fact research no longer depends on the empty calibrated
    // whitelist: the Run must expose `submit_final_answer` and fail closed if
    // the model route cannot support tool continuation.
    if research_mode && !structured_finalization {
        if !has_local_follow_up_tools {
            let _ = RunEngine::fail_before_dispatch_with_sink(
                db,
                &accepted.session,
                &accepted.run_id,
                SafeRunErrorCode::GroundedFinalizationUnavailable,
                sink,
            );
            return Err(AppError::run(
                SafeRunErrorCode::GroundedFinalizationUnavailable,
            ));
        }
        structured_finalization = true;
        messages[1].content =
            PromptContractV3::web_evidence_data_prompt(&evidence_json, true).into();
    }
    if structured_finalization {
        tools.push(crate::ai_runtime::final_answer_submission::tool_spec());
    }
    tool_surface_plan.tool_names = tools.iter().map(|tool| tool.name.clone()).collect();
    let mut executor = executor.with_allowed_tool_names(&tool_surface_plan.tool_names);
    if research_mode {
        let plan = build_fresh_research_plan(
            &context.user_message,
            &context.envelope.fresh_fact,
            "zh-CN",
            None,
        )?;
        executor = executor.with_fresh_research_budget(plan.budget);
    }
    let provider = FailoverStreamingProvider::new(route, requirements, db, &accepted.session, sink);
    #[cfg(test)]
    let provider = if let Some(client) = state.test_streaming_client() {
        provider.with_test_streaming_client(client)
    } else {
        provider
    };
    if let Some(telemetry) = telemetry {
        RunEngine::execute_tool_loop_with_eval_telemetry(
            db,
            &accepted.session,
            &accepted.run_id,
            messages.clone(),
            tools.clone(),
            registered_evidence_ids,
            Some(domain_plan),
            &provider,
            &executor,
            sink,
            telemetry,
        )
        .await
    } else {
        RunEngine::execute_tool_loop_with_sink(
            db,
            &accepted.session,
            &accepted.run_id,
            messages.clone(),
            tools,
            registered_evidence_ids,
            Some(domain_plan),
            &provider,
            &executor,
            sink,
        )
        .await
    }
}

/// Strict structured finalization is a route capability, not a prompt-only
/// preference. Entries are added only after the user-approved live pilot has
/// passed for that exact provider/model pair.
fn calibrated_structured_finalization_enabled(
    route: &crate::ai_runtime::direct_provider_route::DirectProviderRoute,
    requirements: &crate::ai_runtime::provider_router::ProviderRequirements,
) -> bool {
    const VERIFIED: &[(&str, &str)] = &[];
    route
        .selected_provider_model_for_requirements(*requirements, 0)
        .is_some_and(|(provider, model)| {
            VERIFIED.iter().any(|(verified_provider, verified_model)| {
                provider == *verified_provider && model == *verified_model
            })
        })
}

/// Select an exact calibrated route only when the same provider/model can
/// satisfy a tool-capable finalization request. A verified pair can never gain
/// the internal final-answer tool through a tool-free route.
fn calibrated_structured_finalization_route(
    db: &Database,
    context: &crate::ai_runtime::run_context::RunContext,
    requirements: crate::ai_runtime::provider_router::ProviderRequirements,
) -> AppResult<Option<crate::ai_runtime::direct_provider_route::DirectProviderRoute>> {
    if !requirements.tools {
        return Ok(None);
    }
    let route = match build_normal_route(
        db,
        context,
        requirements.min_input_budget_tokens,
        requirements.vision,
        true,
    ) {
        Ok(route) => route,
        Err(_) => return Ok(None),
    };
    if calibrated_structured_finalization_enabled(&route, &requirements) {
        Ok(Some(route))
    } else {
        Ok(None)
    }
}

pub(crate) fn strict_follow_up_capabilities(
    authorized_capabilities: &[crate::ai_runtime::run_contract::CapabilityId],
) -> Vec<crate::ai_runtime::run_contract::CapabilityId> {
    const ALLOWED: &[&str] = &["vault.read", "external.read"];
    authorized_capabilities
        .iter()
        .filter(|capability| ALLOWED.contains(&capability.as_str()))
        .cloned()
        .collect()
}

fn required_web_query(context: &crate::ai_runtime::run_context::RunContext) -> AppResult<String> {
    if matches!(
        context.envelope.fresh_fact.domain,
        FreshFactDomain::Weather
            | FreshFactDomain::News
            | FreshFactDomain::Finance
            | FreshFactDomain::Entertainment
            | FreshFactDomain::Sports
            | FreshFactDomain::GenericWeb
    ) {
        let plan = build_fresh_research_plan(
            &context.user_message,
            &context.envelope.fresh_fact,
            "zh-CN",
            None,
        )?;
        return Ok(plan.initial_query);
    }
    let prior_users_newest_first = context
        .recent_messages
        .iter()
        .rev()
        .filter(|message| message.role == "user")
        .map(|message| message.content.clone())
        .collect::<Vec<_>>();
    let authorized_materials = context
        .materials
        .iter()
        .filter(|material| {
            matches!(
                material.origin,
                crate::ai_runtime::domain_executor::DomainMaterialOrigin::UserAuthorizedMaterial
            )
        })
        .map(|material| material.content.clone())
        .collect::<Vec<_>>();
    let has_automatic_local_material = context.materials.iter().any(|material| {
        matches!(
            material.origin,
            crate::ai_runtime::domain_executor::DomainMaterialOrigin::LocalRetrieval { .. }
        )
    });
    Ok(required_web_query_from_context_sources(
        &context.user_message,
        &prior_users_newest_first,
        authorized_materials,
        has_automatic_local_material,
    ))
}

/// Use only material the user explicitly authorized for this Run to make a
/// short deictic question searchable. Automatic local retrieval is deliberately
/// absent from this API, so it cannot be disclosed by query construction.
#[cfg(test)]
pub(crate) fn required_web_query_from_authorized_material(
    current: &str,
    prior_users_newest_first: &[String],
    authorized_materials: impl IntoIterator<Item = String>,
) -> String {
    required_web_query_from_context_sources(
        current,
        prior_users_newest_first,
        authorized_materials,
        false,
    )
}

fn required_web_query_from_context_sources(
    current: &str,
    prior_users_newest_first: &[String],
    authorized_materials: impl IntoIterator<Item = String>,
    has_automatic_local_material: bool,
) -> String {
    let query = public_web_query_for_mixed_local_context(
        &required_web_query_from_user_history(current, prior_users_newest_first),
        has_automatic_local_material,
    );
    if !is_context_dependent_web_follow_up(&query) {
        return query;
    }
    let Some(subject) = authorized_materials
        .into_iter()
        .find_map(|material| compact_authorized_query_subject(&material))
    else {
        return query;
    };
    format!("{subject} {query}").chars().take(360).collect()
}

fn compact_authorized_query_subject(material: &str) -> Option<String> {
    let mut subject = String::new();
    let mut previous_was_space = true;
    let mut written = 0_usize;
    for character in material.chars() {
        if character.is_control() || character.is_whitespace() {
            if !previous_was_space && written < 96 {
                subject.push(' ');
                written += 1;
            }
            previous_was_space = true;
            continue;
        }
        if written == 96 {
            break;
        }
        subject.push(character);
        written += 1;
        previous_was_space = false;
    }
    let subject = subject.trim().to_string();
    (!subject.is_empty()).then_some(subject)
}

/// Keep automatic local-retrieval wording out of a strict Web query while
/// preserving fail-closed behavior when no public clause can be identified.
fn public_web_query_for_mixed_local_context(query: &str, has_local_material: bool) -> String {
    if !has_local_material {
        return query.to_string();
    }
    const WEB_CUES: [&str; 10] = [
        "联网", "最新", "公开", "核实", "检索", "web", "current", "public", "browse", "search",
    ];
    let lowercase = query.to_ascii_lowercase();
    let Some(start) = WEB_CUES.iter().filter_map(|cue| lowercase.find(cue)).min() else {
        return query.to_string();
    };
    let public_clause = query[start..]
        .split_inclusive(['。', '！', '？', '\n'])
        .next()
        .unwrap_or_default()
        .trim();
    if public_clause.chars().count() < 4 {
        query.to_string()
    } else {
        public_clause.to_string()
    }
}

/// Build a compact search query without blindly concatenating every adjacent
/// user turn. Retries need the prior substantive subject; independent new
/// questions must not inherit noise such as "你再试试".
pub(crate) fn required_web_query_from_user_history(
    current: &str,
    prior_users_newest_first: &[String],
) -> String {
    const MAX_CURRENT_CHARS: usize = 240;
    const MAX_QUERY_CHARS: usize = 360;
    let current = current
        .trim()
        .chars()
        .take(MAX_CURRENT_CHARS)
        .collect::<String>();
    if current.is_empty() {
        return current;
    }
    let prior = prior_users_newest_first
        .iter()
        .map(|message| message.trim())
        .find(|message| !message.is_empty() && !is_web_retry_instruction(message))
        .map(|message| message.chars().take(MAX_CURRENT_CHARS).collect::<String>());
    let query = match (
        is_web_retry_instruction(&current) || is_context_dependent_web_follow_up(&current),
        prior,
    ) {
        (true, Some(prior)) => format!("{prior}\n{current}"),
        _ => current,
    };
    query.chars().take(MAX_QUERY_CHARS).collect()
}

fn is_web_retry_instruction(message: &str) -> bool {
    let normalized = message
        .chars()
        .filter(|character| {
            !character.is_whitespace() && !matches!(character, '?' | '？' | '。' | '！' | '!')
        })
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "你再试试" | "再试试" | "重试" | "继续" | "继续回答" | "retry" | "tryagain" | "continue"
    )
}

fn is_context_dependent_web_follow_up(message: &str) -> bool {
    let compact = message.trim();
    compact.chars().count() <= 48
        && (compact.contains(['这', '那', '它', '此', '该'])
            || compact.to_ascii_lowercase().contains("this")
            || compact.to_ascii_lowercase().contains("that")
            || compact.to_ascii_lowercase().contains(" it "))
}

fn strict_web_duration_bucket(duration: Duration) -> &'static str {
    if duration.is_zero() {
        "not_started"
    } else if duration < Duration::from_secs(1) {
        "under_1s"
    } else if duration < Duration::from_secs(3) {
        "1s_to_3s"
    } else if duration < Duration::from_secs(20) {
        "3s_to_20s"
    } else {
        "budget_exhausted"
    }
}

fn resolve_normal_route(
    db: &Database,
    accepted: &AssistantRunAccepted,
    context: &crate::ai_runtime::run_context::RunContext,
    context_tokens: usize,
    has_images: bool,
    needs_tools: bool,
    sink: &impl RunEventSink,
) -> AppResult<crate::ai_runtime::direct_provider_route::DirectProviderRoute> {
    let route = build_normal_route(db, context, context_tokens, has_images, needs_tools);
    match route {
        Ok(route) => Ok(route),
        Err(error) => {
            let code = dispatch_failure_code(&error);
            RunEngine::fail_before_dispatch_with_sink(
                db,
                &accepted.session,
                &accepted.run_id,
                code,
                sink,
            )?;
            Err(AppError::msg(code.as_str()))
        }
    }
}

fn build_normal_route(
    db: &Database,
    context: &crate::ai_runtime::run_context::RunContext,
    context_tokens: usize,
    has_images: bool,
    needs_tools: bool,
) -> AppResult<crate::ai_runtime::direct_provider_route::DirectProviderRoute> {
    let requirements = crate::llm::config::ModelPoolRequirements {
        context_tokens,
        has_images,
        needs_tools,
        needs_reasoning: false,
    };
    match context.model_override() {
        Some(model) => crate::llm::config::resolve_model_override_for_requirements_without_secret(
            db,
            &crate::llm::config::ModelReference {
                provider_id: model.provider_id,
                model_id: model.model_id,
            },
            requirements,
        )
        .map(|resolved| crate::llm::config::ResolvedModelPool {
            resolved,
            failover_candidates: Vec::new(),
        }),
        None => {
            crate::llm::config::resolve_model_pool_for_requirements_without_secret(db, requirements)
        }
    }
    .and_then(crate::ai_runtime::direct_provider_route::DirectProviderRoute::from_secret_free_route)
}

fn dispatch_failure_code(error: &AppError) -> SafeRunErrorCode {
    if error.to_string() == "agent_run_no_capable_model" {
        SafeRunErrorCode::NoCapableModel
    } else {
        SafeRunErrorCode::ProviderUnavailable
    }
}

#[cfg(test)]
mod tests {
    use super::calibrated_structured_finalization_enabled;
    use crate::ai_runtime::direct_provider_route::DirectProviderRoute;
    use crate::ai_runtime::provider_router::{ProviderRequirements, SecurityDomain};
    use crate::ai_types::{EndpointFamily, ResolvedReasoningRequest};
    use crate::llm::config::{ResolvedLlmConfig, ResolvedModelPool};

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
            tools: true,
            vision: false,
            reasoning: false,
            min_input_budget_tokens: 1,
            min_output_budget_tokens: 1,
            security_domain: SecurityDomain::External,
        }
    }

    #[test]
    fn structured_verifier_requires_registered_rule() {
        let route = DirectProviderRoute::from_secret_free_route(ResolvedModelPool {
            resolved: resolved("any-provider", "any-model"),
            failover_candidates: Vec::new(),
        })
        .expect("pool route");

        assert!(
            !calibrated_structured_finalization_enabled(&route, &requirements()),
            "no provider/model pair may be promoted to structured VERIFIED before a registered rule exists"
        );
    }
}
