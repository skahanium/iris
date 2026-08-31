//! Headless-capable orchestration for accepted normal-domain Runs.
//!
//! The desktop command supplies a Tauri event sink and app handle, while
//! in-process callers can use another sink and omit the handle. Policy,
//! context, evidence, routing, dispatch, and terminalization stay on this one
//! production path.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::AppHandle;

use crate::ai_runtime::agent_run_repository::AgentRunRepository;
use crate::ai_runtime::run_contract::{
    AssistantRunAccepted, CapabilityId, Effort, Freshness, Modality, RunBudgetPolicy, RunState,
    SafeRunErrorCode, VerificationRequirement, WebDecisionReason,
};
use crate::ai_runtime::run_engine::{FailoverStreamingProvider, RunEngine, RunEventSink};
use crate::ai_runtime::run_intake::RunIntake;
use crate::ai_runtime::run_tool_loop::NormalRunToolExecutor;
use crate::ai_runtime::tool_executor::ToolRegistry;
use crate::ai_runtime::tool_surface::{ToolSurfaceInput, ToolSurfacePlan, ToolSurfacePlanner};
use crate::ai_runtime::{LlmMessage, MessageContent, MessageRole};
use crate::ai_types::{AgentIntent, SkillActivationPlanSummary};
use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

fn plan_tool_surface(
    context: &crate::ai_runtime::run_context::RunContext,
    authorized_capabilities: &[CapabilityId],
) -> ToolSurfacePlan {
    let plan = ToolSurfacePlanner::plan(ToolSurfaceInput {
        web_enabled: authorized_capabilities
            .iter()
            .any(|capability| capability.as_str() == "web.search"),
        requires_current_web_evidence: matches!(
            context.envelope.verification_requirement,
            VerificationRequirement::CurrentRunWeb
        ),
        effort: context.envelope.effort,
        authorized_capabilities: authorized_capabilities.to_vec(),
    });
    tracing::debug!(
        effort = ?plan.effort,
        expose_web_search = plan.expose_web_search,
        web_instruction = ?plan.web_instruction,
        "tool surface planned"
    );
    plan
}

/// Decide whether this frozen Run needs the existing structured terminal tool.
///
/// Current-Run evidence and structured terminalization are deliberately
/// orthogonal: ordinary WebRequired work still requires evidence, but it may
/// complete with natural prose plus the controlled source group.  The only
/// new-envelope strict case currently represented by the contract is an
/// elevated-stakes current-fact request.
fn requires_structured_finalization(context: &crate::ai_runtime::run_context::RunContext) -> bool {
    matches!(
        context.envelope.web_reason,
        WebDecisionReason::HighStakesCurrentFact
    )
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

/// Verify one completed confirmed change set through the normal Provider route,
/// but expose only `read_note` for the exact frozen targets. Route unavailability
/// is returned to the caller so it can preserve the factual Host report.
pub(crate) async fn execute_post_confirmation_verification(
    state: Arc<AppState>,
    accepted: AssistantRunAccepted,
    vault: Option<PathBuf>,
    targets: &[String],
    execution_report: &str,
    sink: &impl RunEventSink,
) -> AppResult<()> {
    let db = Arc::clone(&state.db);
    let context = crate::ai_runtime::run_context::RunContextAssembler::assemble(
        &db,
        vault.as_deref(),
        &accepted.session.session_key,
        &accepted.run_id,
    )?;
    let decision = evaluate_normal_run_policy(&db, &accepted)?;
    if decision.denial_code.is_some() {
        return Err(AppError::msg("post_confirmation_verification_unavailable"));
    }
    let budget = AgentRunRepository::budget_policy_for_session(
        &db,
        &accepted.session.session_key,
        &accepted.run_id,
    )?
    .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
    if budget.post_confirmation_max_model_turns == 0 {
        return Err(AppError::msg("post_confirmation_verification_unavailable"));
    }
    let registry = ToolRegistry::for_run(&db, &accepted.run_id)?;
    let tools = registry
        .tools_for_authorized_capabilities(&decision.allowed_capabilities, false)
        .into_iter()
        .filter(|tool| tool.name == "read_note")
        .collect::<Vec<_>>();
    let requirements = crate::ai_runtime::provider_router::ProviderRequirements {
        endpoint_family: None,
        streaming: true,
        tools: true,
        vision: false,
        reasoning: false,
        min_input_budget_tokens: crate::ai_runtime::text_support::estimate_tokens(execution_report),
        min_output_budget_tokens: 1,
        security_domain: crate::ai_runtime::provider_router::SecurityDomain::External,
    };
    let route = build_normal_route(
        &db,
        &context,
        requirements.min_input_budget_tokens,
        false,
        true,
    )?;
    let provider =
        FailoverStreamingProvider::new(route, requirements, &db, &accepted.session, sink);
    #[cfg(test)]
    let provider = if let Some(client) = state.test_streaming_client() {
        provider.with_test_streaming_client(client)
    } else {
        provider
    };
    let executor = NormalRunToolExecutor::new(
        &state,
        None,
        &accepted,
        &context,
        decision.allowed_capabilities,
        budget,
        sink,
        Vec::new(),
    )
    .with_verification_targets(targets);
    let message = |role, content| LlmMessage {
        role,
        content: MessageContent::Text(content),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    };
    let messages = vec![
        message(MessageRole::System, "你正在核对已经执行的变更。只能使用 read_note 读取下列已冻结目标；不得搜索、联网、读取其他文件或提出/执行额外修改。完成后简洁说明核对结果。".to_string()),
        message(MessageRole::User, format!("{execution_report}\n冻结目标：{}", targets.join(", "))),
    ];
    RunEngine::execute_post_confirmation_verification_with_sink(
        &db,
        &accepted.session,
        &accepted.run_id,
        messages,
        tools,
        &provider,
        &executor,
        sink,
    )
    .await
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
    let current_state = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .ok()
        .flatten()
        .map(|response| response.run.state);
    if !matches!(current_state, Some(RunState::Running))
        && RunEngine::mark_preparing_with_sink(&db, &accepted.session, &accepted.run_id, sink)
            .is_err()
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
    if is_retired_current_fact_run(&context) {
        let _ = RunEngine::fail_before_dispatch_with_sink(
            &db,
            &accepted.session,
            &accepted.run_id,
            SafeRunErrorCode::FinalizationProtocolInvalid,
            sink,
        );
        return;
    }
    let material_plan = context.context_material_plan();
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
    // The immutable capability snapshot determines the complete tool surface;
    // Web, local and external read tools all use the same bounded loop.
    let execution = dispatch_normal_run_after_context(
        &state,
        app_handle,
        &db,
        &accepted,
        &context,
        &material_plan,
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
        if still_active {
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

/// Legacy current-fact envelopes may be read for history, but never resumed:
/// their retired tool contract cannot be reconstructed safely through the
/// generic loop.
fn is_retired_current_fact_run(context: &crate::ai_runtime::run_context::RunContext) -> bool {
    context.envelope.fresh_fact.domain != crate::ai_runtime::run_contract::FreshFactDomain::None
        || context.envelope.verification_requirement == VerificationRequirement::CurrentRunDomain
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
    _authorized_capabilities: &[crate::ai_runtime::run_contract::CapabilityId],
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
    let intent = skill_intent_for_run(context);
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

fn skill_intent_for_run(context: &crate::ai_runtime::run_context::RunContext) -> AgentIntent {
    use crate::ai_runtime::run_contract::Effect;

    match context.envelope.effect {
        Effect::Draft | Effect::Apply => AgentIntent::Write,
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
    material_plan: &crate::ai_runtime::context_materials::ContextMaterialPlan,
    registered_evidence_ids: &[i64],
    authorized_capabilities: &[crate::ai_runtime::run_contract::CapabilityId],
    budget_policy: &RunBudgetPolicy,
    vault: Option<&std::path::Path>,
    sink: &impl RunEventSink,
    telemetry: Option<&crate::ai_runtime::agent_capacity_eval::EvaluationTelemetryTap>,
) -> AppResult<()> {
    let active_skills =
        build_cached_skill_activation(state, vault, context, authorized_capabilities)?;
    let messages = context.messages_with_context_material_plan_and_skills(
        material_plan,
        &active_skills.prompt_overlay,
    );
    let routing_prompt = context.prompt_with_context_material_plan(material_plan);
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
        matches!(context.envelope.effort, Effort::ToolLoop | Effort::Durable)
            || context.envelope.verification_requirement == VerificationRequirement::CurrentRunWeb;
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
        let mut tool_surface_plan = plan_tool_surface(context, authorized_capabilities);
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
        // Evidence obligation and structured terminalization are independent.
        // Ordinary WebRequired answers remain natural; only the narrow strict
        // contracts frozen above receive the reserved final-submission tool.
        if requires_structured_finalization(context) {
            tools.push(crate::ai_runtime::final_answer_submission::tool_spec());
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
        let executor = NormalRunToolExecutor::new(
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
        return if let Some(telemetry) = telemetry {
            RunEngine::execute_tool_loop_with_eval_telemetry(
                db,
                &accepted.session,
                &accepted.run_id,
                messages,
                tools,
                &evidence_ids,
                Some(material_plan),
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
                Some(material_plan),
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
        RunEngine::execute_direct_streaming_with_messages_evidence_and_context_material_plan_with_eval_telemetry(
            db,
            &accepted.session,
            &accepted.run_id,
            &messages,
            &evidence_ids,
            material_plan,
            &provider,
            sink,
            telemetry,
        )
        .await
    } else {
        RunEngine::execute_direct_streaming_with_messages_evidence_and_context_material_plan_with_sink(
            db,
            &accepted.session,
            &accepted.run_id,
            &messages,
            &evidence_ids,
            material_plan,
            &provider,
            sink,
        )
        .await
    }
}

/// Resolve one Provider route without widening the frozen Run capability set.
#[allow(clippy::too_many_arguments)]
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
