//! Unified Agent Run and domain-routed session IPC commands.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::ai_runtime::run_contract::{
    AssistantRunAccepted, AssistantRunControlRequest, AssistantRunGetRequest,
    AssistantRunGetResponse, AssistantRunStartRequest, AssistantSessionRef, SecurityDomain,
};
use crate::ai_runtime::run_engine::{
    ModelGatewayStreamingDirectAnswerProvider, RunEngine, RunEventSink, TauriRunEventSink,
};
use crate::ai_runtime::run_intake::RunIntake;
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
/// Start the isolated normal-domain Agent Run development path.
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
                Arc::clone(&state.db),
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
        SecurityDomain::Normal => RunIntake::control_with_sink(&state.db, request, &sink),
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
        SecurityDomain::Normal => RunIntake::get(&state.db, &request.session, &request.run_id),
        SecurityDomain::Classified => {
            let vault = state.vault_path()?;
            crate::ai_runtime::classified_session::classified_run_get(
                &vault,
                &request.session,
                &request.run_id,
            )
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
/// Start the minimal normal-domain direct execution after its accepted event exists.
///
/// This development entry deliberately bypasses the legacy Harness and does not
/// expose tools, context assembly, web, Skills, or scene routing.
fn spawn_normal_direct_run(
    db: Arc<crate::storage::db::Database>,
    app_handle: AppHandle,
    accepted: AssistantRunAccepted,
    vault: Option<std::path::PathBuf>,
) {
    tauri::async_runtime::spawn(async move {
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
        let mut prompt = context.prompt_with_domain_plan(&domain_plan);
        let mut evidence_ids =
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
        let web_evidence = match crate::ai_runtime::run_tool_loop::collect_web_evidence_with_broker(
            &db, &accepted, &context, &sink,
        )
        .await
        {
            Ok(result) => result,
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
        evidence_ids.extend(web_evidence.evidence_ids);
        prompt.push_str(&web_evidence.prompt_addendum);
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
        let provider_config = dispatch.provider;
        let gateway = match crate::ai_runtime::model_gateway::ModelGateway::with_defaults(
            app_handle.clone(),
            vec![provider_config.clone()],
        ) {
            Ok(gateway) => gateway,
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
        let provider = match ModelGatewayStreamingDirectAnswerProvider::new(
            &gateway,
            provider_config,
            dispatch.max_output_tokens,
        ) {
            Ok(provider) => provider,
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
        let _ = RunEngine::execute_direct_streaming_with_prompt_evidence_and_domain_plan_with_sink(
            &db,
            &accepted.session,
            &accepted.run_id,
            &prompt,
            &evidence_ids,
            &domain_plan,
            &provider,
            &sink,
        )
        .await;

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
        let _ = crate::ai_runtime::classified_run_engine::execute_classified_direct_streaming_with_sink(
            &vault,
            &accepted.session,
            &accepted.run_id,
            &provider,
            &sink,
        )
        .await;
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
