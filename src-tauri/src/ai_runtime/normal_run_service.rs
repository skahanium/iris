//! Headless-capable orchestration for accepted normal-domain Runs.
//!
//! The desktop command supplies a Tauri event sink and app handle, while
//! in-process callers can use another sink and omit the handle. Policy,
//! context, evidence, routing, dispatch, and terminalization stay on this one
//! production path.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::AppHandle;

use crate::ai_runtime::run_contract::{
    AssistantRunAccepted, Effort, Freshness, Modality, SafeRunErrorCode,
};
use crate::ai_runtime::run_engine::{
    FailoverStreamingDirectAnswerProvider, FailoverStreamingToolLoopProvider, RunEngine,
    RunEventSink,
};
use crate::ai_runtime::run_intake::RunIntake;
use crate::ai_runtime::run_tool_loop::NormalRunToolExecutor;
use crate::ai_runtime::tool_executor::ToolRegistry;
use crate::ai_types::{AgentIntent, MessageContent, SkillActivationPlanSummary};
use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

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
    // The immutable capability snapshot determines whether web_search enters the model surface;
    // no deterministic prefetch broadens it.
    let execution = dispatch_normal_run_after_context(
        &state,
        app_handle,
        &db,
        &accepted,
        &context,
        &domain_plan,
        &evidence_ids,
        &authorized_capabilities,
        vault.as_deref(),
        sink,
        telemetry,
    )
    .await;
    if let Err(error) = execution {
        let safe_code = serde_json::from_value::<SafeRunErrorCode>(serde_json::Value::String(
            error.to_string(),
        ))
        .unwrap_or(SafeRunErrorCode::PersistenceFailed);
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
        if still_active
            && safe_code == SafeRunErrorCode::PersistenceFailed
            && error.to_string() != SafeRunErrorCode::PersistenceFailed.as_str()
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
        .ok_or_else(|| AppError::msg("agent_run_not_found"))?;
    let engine = crate::ai_runtime::document_policy_repository::load_policy_decision_engine(db)?;
    Ok(engine.evaluate_run(request))
}

/// The immutable prompt-only Skill selection for a single normal Run.
///
/// The registry is populated when a vault is selected or a user explicitly
/// refreshes Skills. This function deliberately has no filesystem fallback:
/// scanning an untrusted vault while executing a Run would make the run
/// boundary nondeterministic and would bypass the confirmed cache.
struct CachedSkillActivation {
    plan: Option<SkillActivationPlanSummary>,
    prompt_overlay: String,
}

fn build_cached_skill_activation(
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
    let skills = state.cached_skills_for_vault(vault)?.unwrap_or_default();
    let index = crate::ai_runtime::skills::load_activation_index(&state.db)?;
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
    let plan = crate::ai_runtime::skills::build_skill_activation_plan_for_task(
        &skills,
        intent,
        &context.user_message,
        &source_hints,
        (!index.is_empty()).then_some(&index),
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

fn append_skill_overlay_to_system_message(
    messages: &mut [crate::ai_runtime::LlmMessage],
    overlay: &str,
) {
    if overlay.is_empty() {
        return;
    }
    let Some(system) = messages.first_mut() else {
        return;
    };
    if let MessageContent::Text(content) = &mut system.content {
        content.push_str("\n\n");
        content.push_str(overlay);
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
    vault: Option<&std::path::Path>,
    sink: &impl RunEventSink,
    telemetry: Option<&crate::ai_runtime::agent_capacity_eval::EvaluationTelemetryTap>,
) -> AppResult<()> {
    let active_skills =
        build_cached_skill_activation(state, vault, context, authorized_capabilities)?;
    let mut messages = context.messages_with_domain_plan(domain_plan);
    append_skill_overlay_to_system_message(&mut messages, &active_skills.prompt_overlay);
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

    // ToolLoop/Durable Runs receive only the snapshot-authorized surface. The model may call
    // web_search when authorized; search failure emits CapabilityDegraded.
    let needs_follow_up_tools =
        matches!(context.envelope.effort, Effort::ToolLoop | Effort::Durable);
    if needs_follow_up_tools {
        let required_web_provider_snapshot = authorized_capabilities
            .iter()
            .any(|capability| capability.as_str() == "web.search")
            .then(|| {
                crate::ai_runtime::mcp_runtime_registry::resolve_selected_web_search_provider(db)
            })
            .transpose()
            .ok()
            .flatten();
        let registry = ToolRegistry::new();
        let tools = ToolRegistry::constrain_for_explicit_references(
            registry.tools_for_authorized_capabilities(
                authorized_capabilities,
                context.envelope.effort != Effort::Durable,
            ),
            context.envelope.context,
            &context.retrieval_scope,
        );
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
        let provider = FailoverStreamingToolLoopProvider::new(
            route,
            requirements,
            db,
            &accepted.session,
            sink,
        );
        #[cfg(test)]
        let provider = if let Some(client) = state.test_streaming_client() {
            provider.with_test_streaming_client(client)
        } else {
            provider
        };
        let executor = NormalRunToolExecutor::new(
            state,
            app_handle,
            accepted,
            context,
            authorized_capabilities.to_vec(),
            sink,
            required_web_provider_snapshot,
        )
        .with_skill_activation_plan(active_skills.plan.clone())
        .with_child_run_provider(&provider);
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
    let provider = FailoverStreamingDirectAnswerProvider::new(
        route,
        direct_requirements,
        db,
        &accepted.session,
        sink,
    );
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

fn resolve_normal_route(
    db: &Database,
    accepted: &AssistantRunAccepted,
    context: &crate::ai_runtime::run_context::RunContext,
    context_tokens: usize,
    has_images: bool,
    needs_tools: bool,
    sink: &impl RunEventSink,
) -> AppResult<crate::ai_runtime::direct_provider_route::DirectProviderRoute> {
    let route = crate::llm::config::resolve_model_pool_for_requirements_without_secret(
        db,
        crate::llm::config::ModelPoolRequirements {
            context_tokens,
            has_images,
            needs_tools,
            needs_reasoning: false,
        },
    )
    .and_then(crate::ai_runtime::direct_provider_route::DirectProviderRoute::from_secret_free_route)
    .map(|route| {
        context.model_override().map_or(route.clone(), |override_| {
            route.with_model_override(override_.provider_id, override_.model_id)
        })
    });
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

fn dispatch_failure_code(error: &AppError) -> SafeRunErrorCode {
    if error.to_string() == "agent_run_no_capable_model" {
        SafeRunErrorCode::NoCapableModel
    } else {
        SafeRunErrorCode::ProviderUnavailable
    }
}
