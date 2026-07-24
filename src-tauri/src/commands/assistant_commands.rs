//! Unified Agent Run and domain-routed session IPC commands.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::ai_runtime::run_contract::{
    AssistantRunAccepted, AssistantRunControlRequest, AssistantRunEvent, AssistantRunGetRequest,
    AssistantRunGetResponse, AssistantRunRetryRequest, AssistantRunStartRequest,
    AssistantSessionRef, Effect, Effort, Freshness, SecurityDomain,
};
use crate::ai_runtime::run_engine::{
    ModelGatewayStreamingDirectAnswerProvider, RunEngine, RunEventSink,
    StreamingDirectAnswerProvider, TauriRunEventSink,
};
use crate::ai_runtime::run_intake::{NormalRunControlOutcome, RunIntake};
use crate::ai_runtime::run_tool_loop::NormalRunToolExecutor;
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
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Safe, replayable process events for one historical assistant message only.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub process_events: Vec<AssistantRunEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_parts: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub explicit_references: Vec<serde_json::Value>,
    pub context_scope: serde_json::Value,
    pub display_mentions: Vec<serde_json::Value>,
    pub created_at: String,
}

/// Request the one-time retrieval of an in-memory classified answer.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifiedRunResultRequest {
    pub run_id: String,
    pub context_ref: String,
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
            // New classified conversations are deliberately volatile. Existing
            // CEF history is left untouched but is never loaded by this API.
            Ok(Vec::new())
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
            let items = crate::ai_runtime::normal_session_repository::NormalSessionRepository::load_messages(
                &state.db,
                &request.session.session_key,
                request.limit,
            )?;
            let turn_ids = items
                .iter()
                .filter(|item| item.role == "assistant")
                .filter_map(|item| item.turn_id.clone())
                .collect::<Vec<_>>();
            let process_by_turn = crate::ai_runtime::agent_run_repository::AgentRunRepository::process_events_for_session_turns(
                &state.db,
                &request.session.session_key,
                &turn_ids,
            )?;
            Ok(items
                .into_iter()
                .map(|item| {
                    let process = (item.role == "assistant")
                        .then_some(item.turn_id.as_deref())
                        .flatten()
                        .and_then(|turn_id| process_by_turn.get(turn_id));
                    AssistantSessionMessage {
                        seq: item.seq,
                        role: item.role,
                        content: item.content,
                        run_id: process.map(|value| value.run_id.clone()),
                        turn_id: item.turn_id,
                        process_events: process
                            .map(|value| value.events.clone())
                            .unwrap_or_default(),
                        content_parts: item
                            .content_parts
                            .and_then(|value| serde_json::from_str(&value).ok()),
                        tool_calls: item.tool_calls,
                        explicit_references: Vec::new(),
                        context_scope: item.context_scope,
                        display_mentions: item.display_mentions,
                        created_at: item.created_at,
                    }
                })
                .collect())
        }
        SecurityDomain::Classified => {
            let _ = request;
            Err(AppError::msg("agent_run_classified_history_disabled"))
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
            let _ = request;
            Err(AppError::msg("agent_run_classified_history_disabled"))
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
            let _ = request;
            Err(AppError::msg("agent_run_classified_history_disabled"))
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
            let _ = request;
            Err(AppError::msg("agent_run_classified_history_disabled"))
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
            if request.session.is_some()
                || request.web_enabled
                || !request.turn.explicit_references.is_empty()
                || !request.turn.retrieval_scope.paths.is_empty()
                || !request.turn.retrieval_scope.path_prefixes.is_empty()
                || !request.turn.retrieval_scope.corpus_ids.is_empty()
                || !request.turn.retrieval_scope.required_tags.is_empty()
                || !request.turn.display_mentions.is_empty()
                || request.turn.content_parts.is_some()
                || request.explicit_action.is_some()
            {
                return Err(AppError::msg("agent_run_invalid_request"));
            }
            let context_ref = request
                .classified_context_ref
                .as_deref()
                .ok_or_else(|| AppError::msg("agent_run_classified_context_required"))?;
            if request.model_override.as_ref().is_some_and(|override_| {
                override_.provider_id.trim().is_empty() || override_.model_id.trim().is_empty()
            }) {
                return Err(AppError::msg("agent_run_invalid_request"));
            }
            let model_override = request.model_override.clone();
            let accepted = state
                .ai
                .classified_ephemeral
                .lock()
                .map_err(|_| AppError::msg("agent_run_persistence_failed"))?
                .accept(
                    &vault,
                    &request.client_request_id,
                    request.turn.message,
                    context_ref,
                )?;
            let event = state
                .ai
                .classified_ephemeral
                .lock()
                .map_err(|_| AppError::msg("agent_run_persistence_failed"))?
                .get(&accepted.run_id)?
                .and_then(|response| response.events.into_iter().next())
                .ok_or_else(|| AppError::msg("agent_run_accepted_event_missing"))?;
            sink.emit(&event)?;
            spawn_classified_direct_run(
                Arc::clone(&state),
                vault,
                app_handle,
                accepted.clone(),
                model_override,
            );
            Ok(accepted)
        }
    }
}

/// Retry one terminal web-verification failure without duplicating its user turn.
#[tauri::command]
pub async fn assistant_run_retry(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    request: AssistantRunRetryRequest,
) -> AppResult<AssistantRunAccepted> {
    let sink = TauriRunEventSink::new(&app_handle);
    let accepted = RunIntake::retry_with_sink(&state.db, request, &sink)?;
    spawn_normal_direct_run(
        Arc::clone(&state),
        app_handle,
        accepted.clone(),
        state.vault_path().ok(),
    );
    Ok(accepted)
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
            let event = state
                .ai
                .classified_ephemeral
                .lock()
                .map_err(|_| AppError::msg("agent_run_persistence_failed"))?
                .cancel(&request.run_id)?;
            sink.emit(&event)?;
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
        SecurityDomain::Classified => match request.run_id.as_deref() {
            Some(run_id) => state
                .ai
                .classified_ephemeral
                .lock()
                .map_err(|_| AppError::msg("agent_run_persistence_failed"))?
                .get(run_id),
            None => Ok(None),
        },
    }
}

/// Mint a short-lived capability for the currently open classified document.
#[tauri::command]
pub async fn assistant_classified_context_open(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> AppResult<crate::ai_runtime::classified_ephemeral::ClassifiedDocumentContext> {
    let vault = state.vault_path()?;
    state
        .ai
        .classified_ephemeral
        .lock()
        .map_err(|_| AppError::msg("agent_run_persistence_failed"))?
        .open_context(&vault, &path)
}

/// Clear all volatile classified prompt, context, and result state.
#[tauri::command]
pub async fn assistant_classified_context_clear(state: State<'_, Arc<AppState>>) -> AppResult<()> {
    state
        .ai
        .classified_ephemeral
        .lock()
        .map_err(|_| AppError::msg("agent_run_persistence_failed"))?
        .clear();
    Ok(())
}

/// Consume a classified answer once, while the same document context is active.
#[tauri::command]
pub async fn assistant_classified_run_take_result(
    state: State<'_, Arc<AppState>>,
    request: ClassifiedRunResultRequest,
) -> AppResult<String> {
    state
        .ai
        .classified_ephemeral
        .lock()
        .map_err(|_| AppError::msg("agent_run_persistence_failed"))?
        .take_result(&request.run_id, &request.context_ref)
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
                client_request_id: String::new(),
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
            client_request_id: String::new(),
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
            allow_implicit_vault: crate::ai_runtime::run_intake::allow_implicit_vault_for_run(
                context.envelope.security_domain,
                &context.user_message,
                !context.materials.is_empty() || !context.retrieval_scope.is_unrestricted(),
            ),
        };
        let executor = NormalRunToolExecutor::new(
            &state,
            Some(app_handle.clone()),
            &accepted,
            &context,
            tool_policy,
            &sink,
            None,
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
async fn dispatch_normal_run_service<'a, R, S, Execute, Execution>(
    state: Arc<AppState>,
    accepted: AssistantRunAccepted,
    vault: Option<std::path::PathBuf>,
    app_handle: AppHandle<R>,
    sink: &'a S,
    execute: Execute,
) where
    R: tauri::Runtime,
    S: RunEventSink,
    Execute: FnOnce(
        Arc<AppState>,
        AssistantRunAccepted,
        Option<std::path::PathBuf>,
        Option<AppHandle<R>>,
        &'a S,
    ) -> Execution,
    Execution: std::future::Future<Output = ()> + 'a,
{
    execute(state, accepted, vault, Some(app_handle), sink).await;
}

fn spawn_normal_direct_run(
    state: Arc<AppState>,
    app_handle: AppHandle,
    accepted: AssistantRunAccepted,
    vault: Option<std::path::PathBuf>,
) {
    tauri::async_runtime::spawn(async move {
        let sink = TauriRunEventSink::new(&app_handle);
        dispatch_normal_run_service(
            state,
            accepted,
            vault,
            app_handle.clone(),
            &sink,
            crate::ai_runtime::normal_run_service::execute_normal_run,
        )
        .await;
    });
}

/// Start a volatile, single-document classified execution after acceptance.
fn spawn_classified_direct_run(
    state: Arc<AppState>,
    vault: std::path::PathBuf,
    app_handle: AppHandle,
    accepted: AssistantRunAccepted,
    model_override: Option<crate::ai_runtime::run_contract::ModelOverride>,
) {
    tauri::async_runtime::spawn(async move {
        let sink = TauriRunEventSink::new(&app_handle);
        let route_result = crate::llm::config::resolve_model_pool_for_requirements_without_secret(
            &state.db,
            crate::llm::config::ModelPoolRequirements {
                context_tokens: 0,
                has_images: false,
                needs_tools: false,
                needs_reasoning: false,
            },
        )
        .and_then(
            crate::ai_runtime::direct_provider_route::DirectProviderRoute::from_secret_free_route,
        )
        .map(|route| {
            model_override.as_ref().map_or(route.clone(), |override_| {
                route.with_model_override(override_.provider_id.clone(), override_.model_id.clone())
            })
        })
        .and_then(|route| {
            route.hydrate_selected_streaming_dispatch(
                crate::ai_runtime::provider_router::ProviderRequirements {
                    endpoint_family: None,
                    streaming: true,
                    tools: false,
                    vision: false,
                    reasoning: false,
                    min_input_budget_tokens: 0,
                    min_output_budget_tokens: 1,
                    security_domain: crate::ai_runtime::provider_router::SecurityDomain::External,
                },
                0,
            )
        });
        let dispatch = match route_result {
            Ok(dispatch) => dispatch,
            Err(_) => {
                fail_ephemeral_classified_run(
                    &state,
                    &accepted.run_id,
                    crate::ai_runtime::run_contract::SafeRunErrorCode::NoCapableModel,
                    &sink,
                );
                return;
            }
        };
        let provider_config = dispatch.provider;
        let gateway = match crate::ai_runtime::model_gateway::ModelGateway::with_defaults(vec![
            provider_config.clone(),
        ]) {
            Ok(gateway) => gateway,
            Err(_) => {
                fail_ephemeral_classified_run(
                    &state,
                    &accepted.run_id,
                    crate::ai_runtime::run_contract::SafeRunErrorCode::ProviderUnavailable,
                    &sink,
                );
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
                fail_ephemeral_classified_run(
                    &state,
                    &accepted.run_id,
                    crate::ai_runtime::run_contract::SafeRunErrorCode::ProviderUnavailable,
                    &sink,
                );
                return;
            }
        };
        let _ = vault; // The context was decrypted server-side before dispatch.
        let preparing = state
            .ai
            .classified_ephemeral
            .lock()
            .ok()
            .and_then(|mut store| {
                store
                    .transition(
                        &accepted.run_id,
                        crate::ai_runtime::run_contract::RunState::Preparing,
                        "preparing_classified_document",
                    )
                    .ok()
            });
        if let Some(event) = preparing {
            let _ = sink.emit(&event);
        }
        let running = state
            .ai
            .classified_ephemeral
            .lock()
            .ok()
            .and_then(|mut store| {
                store
                    .transition(
                        &accepted.run_id,
                        crate::ai_runtime::run_contract::RunState::Running,
                        "analyzing_current_classified_document",
                    )
                    .ok()
            });
        if let Some(event) = running {
            let _ = sink.emit(&event);
        }
        let prompt = state
            .ai
            .classified_ephemeral
            .lock()
            .ok()
            .and_then(|store| store.prompt(&accepted.run_id).ok());
        let Some((user_message, document)) = prompt else {
            fail_ephemeral_classified_run(
                &state,
                &accepted.run_id,
                crate::ai_runtime::run_contract::SafeRunErrorCode::ClassifiedContextExpired,
                &sink,
            );
            return;
        };
        let messages = [crate::ai_runtime::LlmMessage {
            role: crate::ai_runtime::MessageRole::User,
            content: crate::ai_types::MessageContent::Text(format!(
                "You may analyze only the explicitly attached current classified document. Do not claim access to other documents, tools, Web, or history.\\n\\n<current_classified_document>\\n{document}\\n</current_classified_document>\\n\\nUser request: {user_message}"
            )),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        }];
        struct SilentObserver;
        impl crate::ai_runtime::model_gateway::StreamEventObserver for SilentObserver {
            fn observe(
                &mut self,
                _: &crate::ai_runtime::model_gateway::StreamEvent,
                _: u32,
            ) -> AppResult<()> {
                Ok(())
            }
        }
        let response = provider
            .answer_streaming(&accepted.run_id, &messages, &mut SilentObserver)
            .await;
        match response {
            Ok(response)
                if response.tool_calls.is_empty()
                    && response
                        .content
                        .as_deref()
                        .is_some_and(|content| !content.is_empty()) =>
            {
                let event = state
                    .ai
                    .classified_ephemeral
                    .lock()
                    .ok()
                    .and_then(|mut store| {
                        store
                            .complete(
                                &accepted.run_id,
                                response.content.expect("checked classified response"),
                            )
                            .ok()
                    });
                if let Some(event) = event {
                    let _ = sink.emit(&event);
                }
            }
            Ok(_) => fail_ephemeral_classified_run(
                &state,
                &accepted.run_id,
                crate::ai_runtime::run_contract::SafeRunErrorCode::InvalidRequest,
                &sink,
            ),
            Err(error) => {
                let code = if error.to_string().to_ascii_lowercase().contains("timeout") {
                    crate::ai_runtime::run_contract::SafeRunErrorCode::ProviderTimeout
                } else {
                    crate::ai_runtime::run_contract::SafeRunErrorCode::ProviderUnavailable
                };
                fail_ephemeral_classified_run(&state, &accepted.run_id, code, &sink);
            }
        }
        if crate::ai_runtime::model_gateway::is_abort_requested(&accepted.run_id) {
            crate::ai_runtime::model_gateway::clear_abort(&accepted.run_id);
        }
    });
}

fn fail_ephemeral_classified_run(
    state: &AppState,
    run_id: &str,
    code: crate::ai_runtime::run_contract::SafeRunErrorCode,
    sink: &impl crate::ai_runtime::run_engine::RunEventSink,
) {
    if let Ok(mut store) = state.ai.classified_ephemeral.lock() {
        let failed = store.fail(run_id, code);
        if let Ok(failed) = failed {
            let _ = sink.emit(&failed);
        }
    }
}

#[cfg(test)]
mod normal_run_desktop_adapter_tests {
    use std::cell::Cell;
    use std::sync::Arc;

    use super::dispatch_normal_run_service;
    use crate::ai_runtime::run_contract::{
        AssistantRunEvent, AssistantRunStartRequest, AssistantTurnDraft, SecurityDomain,
    };
    use crate::ai_runtime::run_engine::RunEventSink;
    use crate::ai_runtime::run_intake::RunIntake;
    use crate::app::AppState;
    use crate::error::AppResult;

    struct NoopSink;

    impl RunEventSink for NoopSink {
        fn emit(&self, _event: &AssistantRunEvent) -> AppResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn production_service_dispatch_receives_a_present_desktop_app_handle() {
        let app = tauri::test::mock_app();
        let directory = tempfile::tempdir().expect("temporary app directory");
        let state = AppState::new(directory.path().join("data")).expect("application state");
        let accepted = RunIntake::start(
            &state.db,
            AssistantRunStartRequest {
                client_request_id: "desktop-service-dispatch".to_string(),
                session: None,
                turn: AssistantTurnDraft {
                    message: "请回答".to_string(),
                    content_parts: None,
                    explicit_references: vec![],
                    retrieval_scope: Default::default(),
                    display_mentions: vec![],
                },
                explicit_action: None,
                web_enabled: false,
                model_override: None,
                security_domain: SecurityDomain::Normal,
                classified_context_ref: None,
            },
        )
        .expect("accepted run");
        let observed_present = Cell::new(false);

        dispatch_normal_run_service(
            Arc::clone(&state),
            accepted,
            None,
            app.handle().clone(),
            &NoopSink,
            |_, _, _, app_handle: Option<tauri::AppHandle<tauri::test::MockRuntime>>, _| {
                observed_present.set(app_handle.is_some());
                std::future::ready(())
            },
        )
        .await;

        assert!(observed_present.get());
    }
}
