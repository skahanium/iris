//! Unified Agent Run and domain-routed session IPC commands.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::ai_runtime::run_contract::{
    AssistantRunAccepted, AssistantRunControlRequest, AssistantRunGetRequest,
    AssistantRunGetResponse, AssistantRunStartRequest, AssistantSessionRef, Effect, Effort,
    Freshness, Modality, SecurityDomain,
};
use crate::ai_runtime::run_engine::{
    FailoverStreamingDirectAnswerProvider, FailoverStreamingToolLoopProvider,
    ModelGatewayStreamingDirectAnswerProvider, RunEngine, RunEventSink, TauriRunEventSink,
};
use crate::ai_runtime::run_intake::{NormalRunControlOutcome, RunIntake};
use crate::ai_runtime::run_tool_loop::NormalRunToolExecutor;
use crate::ai_runtime::tool_executor::ToolRegistry;
use crate::ai_runtime::tool_policy::ToolPolicyContext;
use crate::app::AppState;
use crate::error::{AppError, AppResult};
/// List request for the unified, domain-routed conversation history API.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSessionListRequest {
    pub domain: SecurityDomain,
    #[serde(default = "default_session_history_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

const fn default_session_history_limit() -> u32 {
    50
}

/// Request that addresses a conversation exclusively through its opaque ref.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSessionRefRequest {
    pub session: AssistantSessionRef,
}

/// Load request for a bounded history window.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSessionLoadRequest {
    pub session: AssistantSessionRef,
    #[serde(default = "default_session_history_limit")]
    pub limit: u32,
}

/// Rename request for a single opaque conversation.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSessionRenameRequest {
    pub session: AssistantSessionRef,
    pub title: String,
}

/// Retract request for a suffix of one opaque conversation.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSessionRetractRequest {
    pub session: AssistantSessionRef,
    pub from_seq: i64,
}

/// One domain-safe conversation history entry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSessionSummary {
    pub session: AssistantSessionRef,
    pub title: String,
    pub message_count: u32,
    pub created_at: String,
    pub updated_at: String,
}

/// One domain-safe message history entry. Database primary keys, legacy evidence
/// packet bodies and editor bindings never cross this API boundary.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSessionMessage {
    pub seq: i64,
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_parts: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub explicit_references: Vec<serde_json::Value>,
    pub created_at: String,
}
/// List conversation history through one domain-routed API.
#[tauri::command]
pub async fn assistant_session_list(
    state: State<'_, Arc<AppState>>,
    request: AssistantSessionListRequest,
) -> AppResult<Vec<AssistantSessionSummary>> {
    match request.domain {
        SecurityDomain::Normal => {
            crate::ai_runtime::normal_session_repository::NormalSessionRepository::list(
                &state.db,
                request.limit,
                request.offset,
            )
            .map(|items| {
                items
                    .into_iter()
                    .map(|item| AssistantSessionSummary {
                        session: AssistantSessionRef {
                            domain: SecurityDomain::Normal,
                            session_key: item.session_key,
                        },
                        title: item.title,
                        message_count: item.message_count,
                        created_at: item.created_at,
                        updated_at: item.updated_at,
                    })
                    .collect()
            })
        }
        SecurityDomain::Classified => {
            let vault = state.vault_path()?;
            crate::ai_runtime::classified_session::classified_ai_thread_list(&vault).map(|items| {
                items
                    .into_iter()
                    .skip(request.offset as usize)
                    .take(request.limit as usize)
                    .map(|item| AssistantSessionSummary {
                        session: AssistantSessionRef {
                            domain: SecurityDomain::Classified,
                            session_key: item.thread_id,
                        },
                        title: item.title,
                        message_count: item.message_count,
                        created_at: item.created_at,
                        updated_at: item.updated_at,
                    })
                    .collect()
            })
        }
    }
}

/// Load messages through one domain-routed API without exposing normal SQLite IDs.
#[tauri::command]
pub async fn assistant_session_load(
    state: State<'_, Arc<AppState>>,
    request: AssistantSessionLoadRequest,
) -> AppResult<Vec<AssistantSessionMessage>> {
    match request.session.domain {
        SecurityDomain::Normal => {
            crate::ai_runtime::normal_session_repository::NormalSessionRepository::load_messages(
                &state.db,
                &request.session.session_key,
                request.limit,
            )
            .map(|items| {
                items
                    .into_iter()
                    .map(|item| AssistantSessionMessage {
                        seq: item.seq,
                        role: item.role,
                        content: item.content,
                        content_parts: item
                            .content_parts
                            .and_then(|value| serde_json::from_str(&value).ok()),
                        tool_calls: item.tool_calls,
                        explicit_references: Vec::new(),
                        created_at: item.created_at,
                    })
                    .collect()
            })
        }
        SecurityDomain::Classified => {
            let vault = state.vault_path()?;
            crate::ai_runtime::classified_session::classified_ai_thread_load(
                &vault,
                request.session.session_key,
            )
            .map(|thread| {
                thread
                    .messages
                    .into_iter()
                    .rev()
                    .take(request.limit as usize)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .map(|item| AssistantSessionMessage {
                        seq: item.seq,
                        role: item.role,
                        content: item.content,
                        content_parts: item.content_parts,
                        tool_calls: item.tool_calls,
                        explicit_references: item.explicit_references,
                        created_at: item.created_at,
                    })
                    .collect()
            })
        }
    }
}

/// Rename one conversation through its declared storage domain.
#[tauri::command]
pub async fn assistant_session_rename(
    state: State<'_, Arc<AppState>>,
    request: AssistantSessionRenameRequest,
) -> AppResult<()> {
    match request.session.domain {
        SecurityDomain::Normal => {
            crate::ai_runtime::normal_session_repository::NormalSessionRepository::rename(
                &state.db,
                &request.session.session_key,
                &request.title,
            )
        }
        SecurityDomain::Classified => {
            let vault = state.vault_path()?;
            crate::ai_runtime::classified_session::classified_ai_thread_rename(
                &vault,
                request.session.session_key,
                request.title,
            )
        }
    }
}

/// Delete one conversation through its declared storage domain.
#[tauri::command]
pub async fn assistant_session_delete(
    state: State<'_, Arc<AppState>>,
    request: AssistantSessionRefRequest,
) -> AppResult<bool> {
    match request.session.domain {
        SecurityDomain::Normal => {
            crate::ai_runtime::normal_session_repository::NormalSessionRepository::delete(
                &state.db,
                &request.session.session_key,
            )
        }
        SecurityDomain::Classified => {
            let vault = state.vault_path()?;
            crate::ai_runtime::classified_session::classified_ai_thread_delete(
                &vault,
                request.session.session_key,
            )?;
            Ok(true)
        }
    }
}

/// Retract a suffix through its declared storage domain.
#[tauri::command]
pub async fn assistant_session_retract(
    state: State<'_, Arc<AppState>>,
    request: AssistantSessionRetractRequest,
) -> AppResult<u32> {
    match request.session.domain {
        SecurityDomain::Normal => {
            crate::ai_runtime::normal_session_repository::NormalSessionRepository::retract(
                &state.db,
                &request.session.session_key,
                request.from_seq,
            )
        }
        SecurityDomain::Classified => {
            let vault = state.vault_path()?;
            crate::ai_runtime::classified_session::classified_ai_thread_retract(
                &vault,
                request.session.session_key,
                request.from_seq,
            )
        }
    }
}
/// Accept and start one normal-domain Agent Run.
#[tauri::command]
pub async fn assistant_run_start(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    request: AssistantRunStartRequest,
) -> AppResult<AssistantRunAccepted> {
    let sink = TauriRunEventSink::new(&app_handle);
    match request.security_domain {
        SecurityDomain::Normal => {
            let accepted = RunIntake::start_with_sink(&state.db, request, &sink)?;
            spawn_normal_direct_run(
                Arc::clone(&state),
                app_handle,
                accepted.clone(),
                state.vault_path().ok(),
            );
            Ok(accepted)
        }
        SecurityDomain::Classified => {
            let vault = state.vault_path()?;
            let accepted = RunIntake::start_classified(&vault, request)?;
            let event = crate::ai_runtime::classified_session::classified_run_get(
                &vault,
                &accepted.session,
                &accepted.run_id,
            )?
            .and_then(|response| response.events.into_iter().next())
            .ok_or_else(|| AppError::msg("agent_run_accepted_event_missing"))?;
            sink.emit(&event)?;
            spawn_classified_direct_run(Arc::clone(&state.db), vault, app_handle, accepted.clone());
            Ok(accepted)
        }
    }
}

/// Apply one explicit control action to an isolated Agent Run.
#[tauri::command]
pub async fn assistant_run_control(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    request: AssistantRunControlRequest,
) -> AppResult<()> {
    let sink = TauriRunEventSink::new(&app_handle);
    match request.session.domain {
        SecurityDomain::Normal => {
            let session = request.session.clone();
            let run_id = request.run_id.clone();
            let action = request.action.clone();
            let outcome = RunIntake::control_with_sink(&state.db, request, &sink)?;
            match (outcome, action) {
                (
                    NormalRunControlOutcome::ConfirmationApproved,
                    crate::ai_runtime::run_contract::RunControlAction::ApproveChange {
                        confirmation_id,
                        ..
                    },
                ) => spawn_confirmed_change_execution(
                    Arc::clone(&state),
                    app_handle,
                    session,
                    run_id,
                    confirmation_id,
                    state.vault_path().ok(),
                ),
                (
                    NormalRunControlOutcome::ConfirmationRejected,
                    crate::ai_runtime::run_contract::RunControlAction::RejectChange { .. },
                ) => spawn_rejected_change_finalization(
                    Arc::clone(&state.db),
                    app_handle,
                    session,
                    run_id,
                ),
                _ => {}
            }
            Ok(())
        }
        SecurityDomain::Classified => {
            if !matches!(
                &request.action,
                crate::ai_runtime::run_contract::RunControlAction::Cancel
            ) {
                return Err(AppError::msg("agent_run_control_not_available"));
            }
            let vault = state.vault_path()?;
            if let Some(event) = crate::ai_runtime::classified_session::classified_run_cancel(
                &vault,
                &request.session,
                &request.run_id,
                request.expected_state_version,
            )? {
                sink.emit(&event)?;
            }
            crate::ai_runtime::model_gateway::request_abort(&request.run_id);
            Ok(())
        }
    }
}

/// Replay one isolated Agent Run through its owning session reference.
#[tauri::command]
pub async fn assistant_run_get(
    state: State<'_, Arc<AppState>>,
    request: AssistantRunGetRequest,
) -> AppResult<Option<AssistantRunGetResponse>> {
    match request.session.domain {
        SecurityDomain::Normal => match request.run_id.as_deref() {
            Some(run_id) => RunIntake::get(&state.db, &request.session, run_id),
            None => RunIntake::get_latest_active(&state.db, &request.session),
        },
        SecurityDomain::Classified => {
            let vault = state.vault_path()?;
            match request.run_id.as_deref() {
                Some(run_id) => crate::ai_runtime::classified_session::classified_run_get(
                    &vault,
                    &request.session,
                    run_id,
                ),
                None => crate::ai_runtime::classified_session::classified_latest_active_run_get(
                    &vault,
                    &request.session,
                ),
            }
        }
    }
}

/// Rebuild and evaluate the persisted normal Run policy before Provider routing.
fn evaluate_normal_run_policy(
    db: &crate::storage::db::Database,
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

/// Resume exactly one consumed frozen change plan. This path intentionally has
/// no Provider construction or model invocation: approval authorizes the
/// immutable arguments that were already produced during the original Run.
fn spawn_confirmed_change_execution(
    state: Arc<AppState>,
    app_handle: AppHandle,
    session: AssistantSessionRef,
    run_id: String,
    confirmation_id: String,
    vault: Option<std::path::PathBuf>,
) {
    tauri::async_runtime::spawn(async move {
        let sink = TauriRunEventSink::new(&app_handle);
        let db = Arc::clone(&state.db);
        let fail = || {
            RunEngine::fail_active_with_sink(&db, &session, &run_id, &sink)
                .map(|_| ())
                .ok();
        };
        let consumed = match crate::ai_runtime::agent_run_repository::AgentRunRepository::consumed_frozen_confirmation_for_session(
            &db,
            &session.session_key,
            &run_id,
            &confirmation_id,
        ) {
            Ok(plan) => plan,
            Err(_) => {
                fail();
                return;
            }
        };
        let plan =
            match crate::ai_runtime::frozen_change_plan::FrozenChangePlan::from_persisted_plan_json(
                &consumed.plan_json,
            ) {
                Ok(plan) if plan.plan_hash() == consumed.plan_hash => plan,
                _ => {
                    fail();
                    return;
                }
            };
        if plan.confirmation_id() != confirmation_id || plan.run_id() != run_id {
            fail();
            return;
        }
        let policy = match evaluate_normal_run_policy(
            &db,
            &AssistantRunAccepted {
                run_id: run_id.clone(),
                turn_id: String::new(),
                session: session.clone(),
                state: crate::ai_runtime::run_contract::RunState::Running,
                state_version: 0,
            },
        ) {
            Ok(policy) if policy.denial_code.is_none() => policy,
            _ => {
                fail();
                return;
            }
        };
        let _ = policy;
        let context = match crate::ai_runtime::run_context::RunContextAssembler::assemble(
            &db,
            vault.as_deref(),
            &session.session_key,
            &run_id,
        ) {
            Ok(context)
                if context.envelope.effort == Effort::Durable
                    && context.envelope.effect == Effect::Apply =>
            {
                context
            }
            _ => {
                fail();
                return;
            }
        };
        let accepted = AssistantRunAccepted {
            run_id: run_id.clone(),
            turn_id: String::new(),
            session: session.clone(),
            state: crate::ai_runtime::run_contract::RunState::Running,
            state_version: 0,
        };
        let tool_policy = ToolPolicyContext {
            autonomy_level: crate::ai_runtime::AutonomyLevel::L2,
            web_search_enabled: context.envelope.freshness != Freshness::Offline,
            allow_writes: true,
            allow_research: context.envelope.freshness != Freshness::Offline,
            allow_skill_management: false,
        };
        let executor = NormalRunToolExecutor::new(
            &state,
            app_handle.clone(),
            &accepted,
            &context,
            tool_policy,
            &sink,
        );
        match executor.execute_confirmed_frozen_change(&plan).await {
            Ok(result) if result.success => {
                if RunEngine::finalize_confirmed_change_with_sink(
                    &db, &session, &run_id, &sink, true,
                )
                .is_err()
                {
                    fail();
                }
            }
            Ok(_) | Err(_) => fail(),
        }
    });
}

/// A rejected frozen plan ends the Run without dispatching a tool or calling a model.
fn spawn_rejected_change_finalization(
    db: Arc<crate::storage::db::Database>,
    app_handle: AppHandle,
    session: AssistantSessionRef,
    run_id: String,
) {
    tauri::async_runtime::spawn(async move {
        let sink = TauriRunEventSink::new(&app_handle);
        if RunEngine::finalize_confirmed_change_with_sink(&db, &session, &run_id, &sink, false)
            .is_err()
        {
            let _ = RunEngine::fail_active_with_sink(&db, &session, &run_id, &sink);
        }
    });
}
/// Start normal-domain execution after its accepted event exists.
///
/// Context, policy and bounded Web evidence are prepared from persisted Run
/// facts before the streaming Provider is dispatched. The Run Engine remains
/// the sole owner of lifecycle persistence and terminalization.
fn spawn_normal_direct_run(
    state: Arc<AppState>,
    app_handle: AppHandle,
    accepted: AssistantRunAccepted,
    vault: Option<std::path::PathBuf>,
) {
    tauri::async_runtime::spawn(async move {
        let db = Arc::clone(&state.db);
        let sink = TauriRunEventSink::new(&app_handle);
        let policy = match evaluate_normal_run_policy(&db, &accepted) {
            Ok(policy) => policy,
            Err(_) => {
                let _ = RunEngine::fail_before_dispatch_with_sink(
                    &db,
                    &accepted.session,
                    &accepted.run_id,
                    crate::ai_runtime::run_contract::SafeRunErrorCode::PersistenceFailed,
                    &sink,
                );
                return;
            }
        };
        match RunEngine::enforce_policy_before_dispatch_with_sink(
            &db,
            &accepted.session,
            &accepted.run_id,
            &policy,
            &sink,
        ) {
            Ok(true) => {}
            Ok(false) | Err(_) => return,
        }
        let context = match crate::ai_runtime::run_context::RunContextAssembler::assemble(
            &db,
            vault.as_deref(),
            &accepted.session.session_key,
            &accepted.run_id,
        ) {
            Ok(context) => context,
            Err(_) => {
                let _ = RunEngine::fail_before_dispatch_with_sink(
                    &db,
                    &accepted.session,
                    &accepted.run_id,
                    crate::ai_runtime::run_contract::SafeRunErrorCode::InvalidRequest,
                    &sink,
                );
                return;
            }
        };
        let domain_plan = context.domain_plan();
        let prompt = context.prompt_with_domain_plan(&domain_plan);
        let evidence_ids =
            match crate::ai_runtime::run_context::RunContextAssembler::register_evidence(
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
                        crate::ai_runtime::run_contract::SafeRunErrorCode::PersistenceFailed,
                        &sink,
                    );
                    return;
                }
            };
        if matches!(context.envelope.effort, Effort::ToolLoop | Effort::Durable) {
            let tool_policy = ToolPolicyContext {
                autonomy_level: crate::ai_runtime::AutonomyLevel::L2,
                web_search_enabled: context.envelope.freshness != Freshness::Offline,
                allow_writes: context.envelope.effort == Effort::Durable,
                allow_research: context.envelope.freshness != Freshness::Offline,
                allow_skill_management: false,
            };
            let tools = ToolRegistry::new()
                .tools_for_policy_surface(&tool_policy, context.envelope.effort != Effort::Durable);
            let route_result = crate::llm::config::resolve_capability_route_for_requirements_without_secret(
                &db,
                crate::llm::config::CapabilityRouteRequirements {
                    preferred_slot: crate::ai_types::CapabilitySlot::AgentTools,
                    context_tokens: crate::ai_runtime::text_support::estimate_tokens(&prompt),
                    has_images: context.envelope.modalities.contains(&Modality::Image),
                    needs_tools: true,
                    needs_reasoning: false,
                    privacy_preference: crate::llm::config::PrivacyPreference::ExternalAllowed,
                },
            )
            .and_then(crate::ai_runtime::direct_provider_route::DirectProviderRoute::from_secret_free_route)
            .map(|route| {
                let route = context
                    .routing_policy()
                    .map_or(route.clone(), |policy| route.with_routing_policy(policy));
                context.model_override().map_or(route.clone(), |override_| {
                    route.with_model_override(override_.provider_id, override_.model_id)
                })
            });
            let route = match route_result {
                Ok(route) => route,
                Err(_) => {
                    let _ = RunEngine::fail_before_dispatch_with_sink(
                        &db,
                        &accepted.session,
                        &accepted.run_id,
                        crate::ai_runtime::run_contract::SafeRunErrorCode::ProviderUnavailable,
                        &sink,
                    );
                    return;
                }
            };
            let requirements = crate::ai_runtime::provider_router::ProviderRequirements {
                endpoint_family: None,
                streaming: true,
                tools: true,
                vision: context.envelope.modalities.contains(&Modality::Image),
                reasoning: false,
                min_input_budget_tokens: crate::ai_runtime::text_support::estimate_tokens(&prompt),
                min_output_budget_tokens: 1,
                security_domain: crate::ai_runtime::provider_router::SecurityDomain::External,
            };
            let provider = FailoverStreamingToolLoopProvider::new(
                route,
                requirements,
                app_handle.clone(),
                &db,
                &accepted.session,
                &sink,
            );
            let executor = NormalRunToolExecutor::new(
                &state,
                app_handle.clone(),
                &accepted,
                &context,
                tool_policy,
                &sink,
            );
            let messages = context.messages_with_domain_plan(&domain_plan);
            if RunEngine::execute_tool_loop_with_sink(
                &db,
                &accepted.session,
                &accepted.run_id,
                messages,
                tools,
                &evidence_ids,
                context.envelope.freshness
                    == crate::ai_runtime::run_contract::Freshness::WebRequired,
                Some(&domain_plan),
                &provider,
                &executor,
                &sink,
            )
            .await
            .is_err()
            {
                tracing::warn!(
                    run_id = %accepted.run_id,
                    "normal Agent tool loop exited without a successful result"
                );
                let _ = RunEngine::fail_active_with_sink(
                    &db,
                    &accepted.session,
                    &accepted.run_id,
                    &sink,
                );
            }
            if crate::ai_runtime::model_gateway::is_abort_requested(&accepted.run_id) {
                crate::ai_runtime::model_gateway::clear_abort(&accepted.run_id);
            }
            return;
        }
        let route_result = crate::llm::config::resolve_capability_route_for_requirements_without_secret(
            &db,
            crate::llm::config::CapabilityRouteRequirements {
                preferred_slot: crate::ai_types::CapabilitySlot::Fast,
                context_tokens: 0,
                has_images: false,
                needs_tools: false,
                needs_reasoning: false,
                privacy_preference: crate::llm::config::PrivacyPreference::ExternalAllowed,
            },
        )
        .and_then(|route| {
            let endpoint_family = route.resolved.endpoint_family;
            crate::ai_runtime::direct_provider_route::DirectProviderRoute::from_secret_free_route(
                route,
            )
            .map(|route| {
                let route = context
                    .routing_policy()
                    .map_or(route.clone(), |policy| route.with_routing_policy(policy));
                let route = context.model_override().map_or(route.clone(), |override_| {
                    route.with_model_override(override_.provider_id, override_.model_id)
                });
                (route, endpoint_family)
            })
        });

        let (route, endpoint_family) = match route_result {
            Ok(route) => route,
            Err(_) => {
                let _ = RunEngine::fail_before_dispatch_with_sink(
                    &db,
                    &accepted.session,
                    &accepted.run_id,
                    crate::ai_runtime::run_contract::SafeRunErrorCode::ProviderUnavailable,
                    &sink,
                );
                return;
            }
        };
        let provider = FailoverStreamingDirectAnswerProvider::new(
            route,
            endpoint_family,
            app_handle.clone(),
            &db,
            &accepted.session,
            &sink,
        );
        if RunEngine::execute_direct_streaming_with_prompt_evidence_and_domain_plan_with_sink(
            &db,
            &accepted.session,
            &accepted.run_id,
            &prompt,
            &evidence_ids,
            &domain_plan,
            &provider,
            &sink,
        )
        .await
        .is_err()
        {
            tracing::warn!(
                run_id = %accepted.run_id,
                "normal Agent Run exited without a successful result"
            );
            let _ =
                RunEngine::fail_active_with_sink(&db, &accepted.session, &accepted.run_id, &sink);
        }

        if crate::ai_runtime::model_gateway::is_abort_requested(&accepted.run_id) {
            // The gateway normally clears the marker. This defensive cleanup only
            // covers a provider implementation that exited during cancellation.
            crate::ai_runtime::model_gateway::clear_abort(&accepted.run_id);
        }
    });
}

/// Start a CEF-only direct execution after its accepted event exists.
fn spawn_classified_direct_run(
    db: Arc<crate::storage::db::Database>,
    vault: std::path::PathBuf,
    app_handle: AppHandle,
    accepted: AssistantRunAccepted,
) {
    tauri::async_runtime::spawn(async move {
        let sink = TauriRunEventSink::new(&app_handle);
        let route_result = crate::llm::config::resolve_capability_route_for_requirements_without_secret(
            &db,
            crate::llm::config::CapabilityRouteRequirements {
                preferred_slot: crate::ai_types::CapabilitySlot::Fast,
                context_tokens: 0,
                has_images: false,
                needs_tools: false,
                needs_reasoning: false,
                privacy_preference: crate::llm::config::PrivacyPreference::ExternalAllowed,
            },
        )
        .and_then(|route| {
            let endpoint_family = route.resolved.endpoint_family;
            crate::ai_runtime::direct_provider_route::DirectProviderRoute::from_secret_free_route(
                route,
            )
            .and_then(|route| {
                route.hydrate_selected_text_streaming_no_tools_as_fast_dispatch(endpoint_family, 0)
            })
        });
        let dispatch = match route_result {
            Ok(dispatch) => dispatch,
            Err(_) => {
                fail_classified_before_dispatch(&vault, &accepted, &sink);
                return;
            }
        };
        let provider_config = dispatch.provider;
        let gateway = match crate::ai_runtime::model_gateway::ModelGateway::with_defaults(
            app_handle.clone(),
            vec![provider_config.clone()],
        ) {
            Ok(gateway) => gateway,
            Err(_) => {
                fail_classified_before_dispatch(&vault, &accepted, &sink);
                return;
            }
        };
        let provider = match ModelGatewayStreamingDirectAnswerProvider::new(
            &gateway,
            provider_config,
            dispatch.max_output_tokens,
        ) {
            Ok(provider) => provider,
            Err(_) => {
                fail_classified_before_dispatch(&vault, &accepted, &sink);
                return;
            }
        };
        if crate::ai_runtime::classified_run_engine::execute_classified_direct_streaming_with_sink(
            &vault,
            &accepted.session,
            &accepted.run_id,
            &provider,
            &sink,
        )
        .await
        .is_err()
        {
            tracing::warn!(
                run_id = %accepted.run_id,
                "classified Agent Run exited without a successful result"
            );
            if let Ok(events) =
                crate::ai_runtime::classified_session::classified_run_fail_unfinished(
                    &vault,
                    &accepted.session,
                    &accepted.run_id,
                    crate::ai_runtime::run_contract::SafeRunErrorCode::PersistenceFailed,
                )
            {
                for event in events {
                    let _ = sink.emit(&event);
                }
            }
        }
        if crate::ai_runtime::model_gateway::is_abort_requested(&accepted.run_id) {
            crate::ai_runtime::model_gateway::clear_abort(&accepted.run_id);
        }
    });
}

fn fail_classified_before_dispatch(
    vault: &std::path::Path,
    accepted: &AssistantRunAccepted,
    sink: &impl crate::ai_runtime::run_engine::RunEventSink,
) {
    let Ok(preparing) = crate::ai_runtime::classified_session::classified_run_mark_preparing(
        vault,
        &accepted.session,
        &accepted.run_id,
    ) else {
        return;
    };
    if sink.emit(&preparing).is_err() {
        return;
    }
    if let Ok(Some(failed)) = crate::ai_runtime::classified_session::classified_run_fail(
        vault,
        &accepted.session,
        &accepted.run_id,
        1,
        crate::ai_runtime::run_contract::SafeRunErrorCode::ProviderUnavailable,
    ) {
        let _ = sink.emit(&failed);
    }
}
