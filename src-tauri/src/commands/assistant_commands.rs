//! Unified Agent Run and domain-routed session IPC commands.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::ai_runtime::agent_tool_loop::ToolLoopProvider;
use crate::ai_runtime::run_contract::{
    AssistantRunAccepted, AssistantRunControlRequest, AssistantRunEvent, AssistantRunGetRequest,
    AssistantRunGetResponse, AssistantRunRetryRequest, AssistantRunStartRequest,
    AssistantSessionRef, Effect, Effort, SafeRunErrorCode, SecurityDomain,
};
use crate::ai_runtime::run_engine::{
    ModelGatewayStreamingDirectAnswerProvider, RunEngine, RunEventSink, TauriRunEventSink,
};
use crate::ai_runtime::run_intake::{NormalRunControlOutcome, RunIntake};
use crate::ai_runtime::run_tool_loop::NormalRunToolExecutor;
use crate::app::AppState;
use crate::error::{AppError, AppResult};

/// Runtime adapter used by the start command to preserve the real desktop
/// handle in production while allowing the same IPC path to run headlessly.
pub trait AssistantRunRuntime: tauri::Runtime {
    /// Return the concrete desktop handle used by normal-domain tool dispatch.
    fn normal_run_app_handle(app_handle: &AppHandle<Self>) -> Option<AppHandle>;
}

impl AssistantRunRuntime for tauri::Wry {
    fn normal_run_app_handle(app_handle: &AppHandle<Self>) -> Option<AppHandle> {
        Some(app_handle.clone())
    }
}

#[cfg(test)]
impl AssistantRunRuntime for tauri::test::MockRuntime {
    fn normal_run_app_handle(_app_handle: &AppHandle<Self>) -> Option<AppHandle> {
        None
    }
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_state: Option<String>,
    pub retryable: bool,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub web_citations: Vec<crate::ai_types::WebCitationEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citation_binding: Option<crate::ai_types::CitationBinding>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub source_summary: Vec<crate::ai_runtime::provenance::SourceSummaryEntry>,
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
                    let web_citations = historical_web_citations_for_run(
                        &state.db,
                        process.map(|value| value.run_id.as_str()),
                        item.web_citations,
                    );
                    let citation_binding = item.citation_binding.or_else(|| {
                        (!web_citations.is_empty()).then_some(crate::ai_types::CitationBinding {
                            mode: crate::ai_types::CitationBindingMode::SourceGroupFallback,
                            referenced_indices: Vec::new(),
                            fallback_reason: Some("legacy_binding_unavailable".to_string()),
                        })
                    });
                    AssistantSessionMessage {
                        seq: item.seq,
                        role: item.role,
                        content: item.content,
                        run_id: item
                            .run_id
                            .or_else(|| process.map(|value| value.run_id.clone())),
                        turn_id: item.turn_id,
                        turn_state: item.turn_state,
                        retryable: item.retryable,
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
                        web_citations,
                        citation_binding,
                        source_summary: item.source_summary,
                        created_at: item.created_at,
                    }
                })
                .collect())
        }
        SecurityDomain::Classified => {
            let _ = request;
            Err(AppError::run(SafeRunErrorCode::ClassifiedHistoryDisabled))
        }
    }
}

/// Rebuild an assistant turn's Web citations from the immutable Run ledger when
/// available. This makes old sessions written with session-global indices
/// render against the same Run-local `[Wn]` projection as the answer body,
/// without mutating existing SQLite rows.
fn historical_web_citations_for_run(
    db: &crate::storage::db::Database,
    run_id: Option<&str>,
    persisted: Vec<crate::ai_types::WebCitationEntry>,
) -> Vec<crate::ai_types::WebCitationEntry> {
    let Some(run_id) = run_id else {
        return persisted;
    };
    match crate::ai_runtime::agent_evidence_repository::AgentEvidenceRepository::list_current_run_web_citation_links(db, run_id) {
        Ok(run_local) if !run_local.is_empty() => run_local
            .into_iter()
            .map(|citation| crate::ai_types::WebCitationEntry {
                index: citation.index,
                title: citation.title,
                url: citation.url,
            })
            .collect(),
        Ok(_) | Err(_) => persisted,
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
            Err(AppError::run(SafeRunErrorCode::ClassifiedHistoryDisabled))
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
            Err(AppError::run(SafeRunErrorCode::ClassifiedHistoryDisabled))
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
            Err(AppError::run(SafeRunErrorCode::ClassifiedHistoryDisabled))
        }
    }
}
/// Accept and start one normal-domain Agent Run.
#[tauri::command]
pub async fn assistant_run_start<R: AssistantRunRuntime>(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle<R>,
    request: AssistantRunStartRequest,
) -> AppResult<AssistantRunAccepted> {
    let sink = TauriRunEventSink::new(&app_handle);
    match request.security_domain {
        SecurityDomain::Normal => {
            let outcome = RunIntake::start_with_sink_outcome(&state.db, request, &sink)?;
            if outcome.is_new {
                spawn_normal_direct_run(
                    Arc::clone(&state),
                    app_handle,
                    outcome.accepted.clone(),
                    state.vault_path().ok(),
                );
            }
            Ok(outcome.accepted)
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
                || !request.external_tool_grants.is_empty()
            {
                return Err(AppError::run(SafeRunErrorCode::InvalidRequest));
            }
            let context_ref = request
                .classified_context_ref
                .as_deref()
                .ok_or_else(|| AppError::run(SafeRunErrorCode::ClassifiedContextRequired))?;
            if request.model_override.as_ref().is_some_and(|override_| {
                override_.provider_id.trim().is_empty() || override_.model_id.trim().is_empty()
            }) {
                return Err(AppError::run(SafeRunErrorCode::InvalidRequest));
            }
            let model_override = request.model_override.clone();
            let outcome = state
                .ai
                .classified_ephemeral
                .lock()
                .map_err(|_| AppError::run(SafeRunErrorCode::PersistenceFailed))?
                .accept_outcome(
                    &vault,
                    &request.client_request_id,
                    request.turn.message,
                    context_ref,
                    model_override.as_ref(),
                )?;
            if outcome.is_new {
                let event = state
                    .ai
                    .classified_ephemeral
                    .lock()
                    .map_err(|_| AppError::run(SafeRunErrorCode::PersistenceFailed))?
                    .get(&outcome.accepted.run_id)?
                    .and_then(|response| response.events.into_iter().next())
                    .ok_or_else(|| AppError::run(SafeRunErrorCode::AcceptedEventMissing))?;
                let _ = sink.emit(&event);
                spawn_classified_direct_run(
                    Arc::clone(&state),
                    vault,
                    app_handle,
                    outcome.accepted.clone(),
                    model_override,
                );
            }
            Ok(outcome.accepted)
        }
    }
}

/// Retry the latest terminal failed Run without duplicating its user turn.
#[tauri::command]
pub async fn assistant_run_retry(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    request: AssistantRunRetryRequest,
) -> AppResult<AssistantRunAccepted> {
    let sink = TauriRunEventSink::new(&app_handle);
    let outcome = RunIntake::retry_with_sink_outcome(&state.db, request, &sink)?;
    if outcome.is_new {
        spawn_normal_direct_run(
            Arc::clone(&state),
            app_handle,
            outcome.accepted.clone(),
            state.vault_path().ok(),
        );
    }
    Ok(outcome.accepted)
}

/// Apply one explicit control action to an isolated Agent Run.
#[tauri::command]
pub async fn assistant_run_control<R: AssistantRunRuntime>(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle<R>,
    request: AssistantRunControlRequest,
) -> AppResult<()> {
    assistant_run_control_inner(Arc::clone(&state), app_handle, request).await
}

async fn assistant_run_control_inner<R: AssistantRunRuntime>(
    state: Arc<AppState>,
    app_handle: AppHandle<R>,
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
                    NormalRunControlOutcome::RecoveryResumed { confirmation_id },
                    crate::ai_runtime::run_contract::RunControlAction::Resume,
                ) => spawn_confirmed_change_execution(
                    Arc::clone(&state),
                    app_handle,
                    session,
                    run_id,
                    confirmation_id,
                    state.vault_path().ok(),
                ),
                (
                    NormalRunControlOutcome::InputProvided,
                    crate::ai_runtime::run_contract::RunControlAction::SubmitInput { .. },
                ) => spawn_normal_direct_run(
                    Arc::clone(&state),
                    app_handle,
                    crate::ai_runtime::run_contract::AssistantRunAccepted {
                        client_request_id: String::new(),
                        run_id: run_id.clone(),
                        turn_id: crate::ai_runtime::run_intake::RunIntake::get(
                            &state.db, &session, &run_id,
                        )?
                        .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?
                        .run
                        .turn_id,
                        session: session.clone(),
                        state: crate::ai_runtime::run_contract::RunState::AwaitingInput,
                        state_version: 0,
                    },
                    state.vault_path().ok(),
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
                return Err(AppError::run(SafeRunErrorCode::ControlNotAvailable));
            }
            let event = state
                .ai
                .classified_ephemeral
                .lock()
                .map_err(|_| AppError::run(SafeRunErrorCode::PersistenceFailed))?
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
                .map_err(|_| AppError::run(SafeRunErrorCode::PersistenceFailed))?
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
        .map_err(|_| AppError::run(SafeRunErrorCode::PersistenceFailed))?
        .open_context(&vault, &path)
}

/// Clear all volatile classified prompt, context, and result state.
#[tauri::command]
pub async fn assistant_classified_context_clear(state: State<'_, Arc<AppState>>) -> AppResult<()> {
    state
        .ai
        .classified_ephemeral
        .lock()
        .map_err(|_| AppError::run(SafeRunErrorCode::PersistenceFailed))?
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
        .map_err(|_| AppError::run(SafeRunErrorCode::PersistenceFailed))?
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
        .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
    let engine = crate::ai_runtime::document_policy_repository::load_policy_decision_engine(db)?;
    Ok(engine.evaluate_run(request))
}

/// Resume exactly one consumed frozen change plan. This path intentionally has
/// no Provider construction or model invocation: approval authorizes the
/// immutable arguments that were already produced during the original Run.
fn spawn_confirmed_change_execution<R: tauri::Runtime>(
    state: Arc<AppState>,
    app_handle: AppHandle<R>,
    session: AssistantSessionRef,
    run_id: String,
    confirmation_id: String,
    vault: Option<std::path::PathBuf>,
) {
    tauri::async_runtime::spawn(async move {
        let sink = TauriRunEventSink::new(&app_handle);
        execute_confirmed_change_with_sink(state, session, run_id, confirmation_id, vault, &sink)
            .await;
    });
}

async fn execute_confirmed_change_with_sink(
    state: Arc<AppState>,
    session: AssistantSessionRef,
    run_id: String,
    confirmation_id: String,
    vault: Option<std::path::PathBuf>,
    sink: &impl RunEventSink,
) {
    let db = Arc::clone(&state.db);
    let fail = || {
        RunEngine::fail_active_with_sink(&db, &session, &run_id, sink)
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
    let authorized_capabilities = match crate::ai_runtime::agent_run_repository::AgentRunRepository::persist_authorization_snapshot(
            &db,
            &session.session_key,
            &run_id,
            &policy.allowed_capabilities,
        ) {
        Ok(snapshot) => snapshot,
        Err(_) => {
            fail();
            return;
        }
    };
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
    let budget_policy =
        match crate::ai_runtime::agent_run_repository::AgentRunRepository::budget_policy_for_session(
            &db,
            &session.session_key,
            &run_id,
        ) {
            Ok(Some(policy)) => policy,
            Ok(None) | Err(_) => {
                fail();
                return;
            }
        };
    let executor = NormalRunToolExecutor::new(
        &state,
        None,
        &accepted,
        &context,
        authorized_capabilities,
        budget_policy,
        sink,
        Vec::new(),
    );
    match executor.execute_confirmed_frozen_change(&plan).await {
        Ok(result) if result.success => {
            if RunEngine::finalize_confirmed_change_with_sink(&db, &session, &run_id, sink).is_err()
            {
                fail();
            }
        }
        Ok(_) | Err(_) => fail(),
    }
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

fn spawn_normal_direct_run<R: AssistantRunRuntime>(
    state: Arc<AppState>,
    app_handle: AppHandle<R>,
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
            |state, accepted, vault, _, sink| {
                crate::ai_runtime::normal_run_service::execute_normal_run(
                    state,
                    accepted,
                    vault,
                    R::normal_run_app_handle(&app_handle),
                    sink,
                )
            },
        )
        .await;
    });
}

/// Start a volatile, single-document classified execution after acceptance.
fn spawn_classified_direct_run<R: tauri::Runtime>(
    state: Arc<AppState>,
    vault: std::path::PathBuf,
    app_handle: AppHandle<R>,
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
                        crate::ai_runtime::run_contract::RunStageCode::ClassifiedPreparing,
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
                        crate::ai_runtime::run_contract::RunStageCode::ClassifiedAnalyzing,
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
            .answer_turn(
                &accepted.run_id,
                &messages,
                &[],
                crate::ai_runtime::agent_tool_loop::AgentModelTurnBudget::default(),
                &mut SilentObserver,
            )
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
    use std::time::Duration;

    #[cfg(not(windows))]
    use super::assistant_run_start;
    use super::{
        assistant_run_control, dispatch_normal_run_service, evaluate_normal_run_policy,
        execute_confirmed_change_with_sink,
    };
    #[cfg(not(windows))]
    use crate::ai_runtime::agent_capacity_eval::{spawn_llm_protocol_double, HttpResponseScript};
    use crate::ai_runtime::agent_run_repository::{
        AgentRunRepository, AppendRunCheckpointInput, AppendRunEventInput, DurableApplyCheckpoint,
        DurableApplyCheckpointStage,
    };
    use crate::ai_runtime::frozen_change_plan::{FrozenChangePlan, FrozenChangePlanInput};
    #[cfg(not(windows))]
    use crate::ai_runtime::mcp_external_tools::{
        review_discovered_tool, upsert_binding, McpCapabilityBindingInput,
        McpCapabilityBindingSummary,
    };
    #[cfg(not(windows))]
    use crate::ai_runtime::mcp_host_runtime::{
        discover_provider_tools_without_recording_with_config_hash, McpHostRuntimeOptions,
        DEFAULT_STDIO_SESSION_IDLE_TIMEOUT,
    };
    #[cfg(not(windows))]
    use crate::ai_runtime::mcp_runtime_registry::{
        upsert_web_evidence_provider, WebEvidenceProviderInput,
    };
    #[cfg(not(windows))]
    use crate::ai_runtime::run_contract::ExternalToolGrantRef;
    use crate::ai_runtime::run_contract::{
        AssistantRunAccepted, AssistantRunControlRequest, AssistantRunEvent,
        AssistantRunStartRequest, AssistantTurnDraft, Effect, ExplicitAction, ExplicitTarget,
        RunControlAction, RunEventPayload, RunEventType, RunRecoveryKind, RunState, SecurityDomain,
    };
    use crate::ai_runtime::run_engine::RunEngine;
    use crate::ai_runtime::run_engine::RunEventSink;
    use crate::ai_runtime::run_intake::RunIntake;
    use crate::ai_types::{ContextReferenceKind, ContextReferenceWire};
    use crate::app::AppState;
    use crate::error::AppResult;
    #[cfg(not(windows))]
    use crate::llm::config::{LlmRoutingConfig, ModelReference, ProviderOverride};
    use tauri::webview::InvokeRequest;

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
                external_tool_grants: Vec::new(),
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

    #[cfg(not(windows))]
    async fn install_production_external_binding(state: &AppState) -> McpCapabilityBindingSummary {
        let fixture = format!(
            "{}/tests/fixtures/agent-capacity-mcp-stdio.sh",
            env!("CARGO_MANIFEST_DIR")
        );
        upsert_web_evidence_provider(
            &state.db,
            &WebEvidenceProviderInput {
                id: "assistant-start-external".into(),
                name: "Assistant Start External".into(),
                kind: "mcp".into(),
                enabled: true,
                transport_kind: "stdio".into(),
                transport_config_json: serde_json::json!({
                    "command": "/bin/sh",
                    "args": [fixture, "search-only"]
                })
                .to_string(),
                credential_refs_json: "{}".into(),
                web_search_mapping_json: None,
                web_fetch_mapping_json: None,
            },
        )
        .expect("external MCP provider");
        let (discovery, provider_config_hash) =
            discover_provider_tools_without_recording_with_config_hash(
                &state.db,
                "assistant-start-external",
                McpHostRuntimeOptions {
                    request_timeout: Duration::from_secs(5),
                    max_stdout_line_bytes: 64 * 1024,
                    max_stderr_bytes: 8 * 1024,
                    cwd: None,
                    stdio_session_pool: true,
                    stdio_session_idle_timeout: DEFAULT_STDIO_SESSION_IDLE_TIMEOUT,
                },
            )
            .await
            .expect("real stdio MCP discovery");
        let discovered = discovery
            .tools
            .into_iter()
            .find(|tool| tool.name == "search")
            .expect("search tool");
        let reviewed = review_discovered_tool(
            &discovered.name,
            &discovered.input_schema,
            discovered.read_only_hint,
        )
        .expect("read-only review");
        let input = McpCapabilityBindingInput {
            id: None,
            provider_id: "assistant-start-external".into(),
            mcp_tool_name: discovered.name,
            input_schema: reviewed.input_schema.clone(),
            argument_mapping: serde_json::json!({}),
            risk_class: "read_only".into(),
            read_only: true,
            user_trusted: true,
            attested_binding_config_hash: String::new(),
            domain_operation: None,
            output_mapping: None,
        };
        let attestation = crate::ai_runtime::mcp_external_tools::attest_reviewed_tool(
            &state.db,
            &input.provider_id,
            &reviewed,
            &provider_config_hash,
            &input.argument_mapping,
        )
        .expect("binding attestation");
        let input = McpCapabilityBindingInput {
            attested_binding_config_hash: attestation.binding_config_hash,
            ..input
        };
        upsert_binding(&state.db, &input, &reviewed, &provider_config_hash)
            .expect("trusted binding")
    }

    #[cfg(not(windows))]
    fn configure_test_llm(state: &AppState, base_url: String, model_id: &str) {
        let mut routing = LlmRoutingConfig::default();
        routing.providers.clear();
        routing.providers.insert(
            "custom".into(),
            ProviderOverride {
                base_url: Some(base_url),
                enabled_models: Some(vec![model_id.into()]),
                ..Default::default()
            },
        );
        routing.default_model = Some(ModelReference {
            provider_id: "custom".into(),
            model_id: model_id.into(),
        });
        crate::llm::config::save(&state.db, &routing).expect("normal service route");
        state.set_test_streaming_client(reqwest::Client::new());
    }

    #[cfg(not(windows))]
    fn invoke_start(
        webview: &tauri::WebviewWindow<tauri::test::MockRuntime>,
        request: AssistantRunStartRequest,
    ) -> AssistantRunAccepted {
        let response = tauri::test::get_ipc_response(
            webview,
            InvokeRequest {
                cmd: "assistant_run_start".into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: "tauri://localhost".parse().expect("invoke URL"),
                body: tauri::ipc::InvokeBody::Json(serde_json::json!({ "request": request })),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.into(),
            },
        )
        .expect("assistant_run_start IPC response");
        let tauri::ipc::InvokeResponseBody::Json(response) = response else {
            panic!("assistant_run_start must return JSON");
        };
        serde_json::from_str(&response).expect("accepted response")
    }

    #[cfg(not(windows))]
    async fn wait_for_terminal(state: &AppState, accepted: &AssistantRunAccepted) -> RunState {
        for _ in 0..200 {
            let current = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
                .expect("poll run")
                .expect("accepted run");
            if current.run.state.is_terminal() {
                return current.run.state;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("assistant run did not reach a terminal state");
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn assistant_run_start_reaches_frozen_stdio_external_tool_and_enforces_exact_run_grant_evidence(
    ) {
        let directory = tempfile::tempdir().expect("temporary app directory");
        let state = AppState::new(directory.path().join("data")).expect("application state");
        let binding = install_production_external_binding(&state).await;
        let first_tool_packet = serde_json::json!({
            "choices":[{
                "delta":{
                    "tool_calls":[{
                        "index":0,
                        "id":"assistant-start-external-call",
                        "type":"function",
                        "function":{
                            "name":binding.exposed_name,
                            "arguments":"{\"query\":\"synthetic\"}"
                        }
                    }]
                }
            }]
        });
        let first_tool_sse = format!("data: {first_tool_packet}\n\ndata: [DONE]\n\n");
        let llm = spawn_llm_protocol_double(vec![
            HttpResponseScript::sse(&first_tool_sse),
            HttpResponseScript::sse(
                "data: {\"choices\":[{\"delta\":{\"content\":\"外部工具事实已核实。\"}}]}\n\ndata: [DONE]\n\n",
            ),
        ])
        .await
        .expect("local LLM boundary");
        configure_test_llm(
            &state,
            llm.base_url.clone(),
            "iris-test-verified-tools-assistant-external",
        );
        let app = tauri::test::mock_builder()
            .manage(Arc::clone(&state))
            .invoke_handler(tauri::generate_handler![assistant_run_start])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock application");
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("mock webview");
        let granted_request = AssistantRunStartRequest {
            client_request_id: "assistant-start-external-granted".into(),
            session: None,
            turn: AssistantTurnDraft {
                message: "请用我授权的外部只读工具核实 synthetic 事实".into(),
                content_parts: None,
                explicit_references: Vec::new(),
                retrieval_scope: Default::default(),
                display_mentions: Vec::new(),
            },
            explicit_action: None,
            web_enabled: false,
            model_override: None,
            external_tool_grants: vec![ExternalToolGrantRef {
                binding_id: binding.id.clone(),
                binding_config_hash: binding.binding_config_hash.clone(),
            }],
            security_domain: SecurityDomain::Normal,
            classified_context_ref: None,
        };
        let granted = invoke_start(&webview, granted_request.clone());
        assert_eq!(
            wait_for_terminal(&state, &granted).await,
            RunState::Completed
        );
        let calls = llm.finish().await.expect("LLM completion");
        assert_eq!(calls.len(), 2);
        let granted_tools = calls[0].body["tools"]
            .as_array()
            .expect("granted model tool surface");
        assert!(granted_tools
            .iter()
            .any(|tool| tool["function"]["name"] == binding.exposed_name));
        assert!(!granted_tools
            .iter()
            .any(|tool| tool["function"]["name"] == "web_search"));
        let external_evidence_count = state
            .db
            .with_read_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM agent_run_evidence
                     WHERE run_id = ?1 AND registration_source = 'external_tool'",
                    [&granted.run_id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(Into::into)
            })
            .expect("external evidence count");
        assert_eq!(external_evidence_count, 1);

        let mut ungranted_request = granted_request.clone();
        ungranted_request.client_request_id = "assistant-start-external-ungranted".into();
        ungranted_request.external_tool_grants.clear();
        let ungranted = invoke_start(&webview, ungranted_request);
        assert_eq!(
            wait_for_terminal(&state, &ungranted).await,
            RunState::Failed
        );
        let ungranted_tools =
            crate::ai_runtime::tool_executor::ToolRegistry::for_run(&state.db, &ungranted.run_id)
                .expect("ungranted registry")
                .tools_for_authorized_capabilities(
                    &[crate::ai_runtime::run_contract::CapabilityId::new(
                        "external.read",
                    )],
                    true,
                );
        assert!(ungranted_tools
            .iter()
            .all(|tool| tool.name != binding.exposed_name));

        let bypass_llm = spawn_llm_protocol_double(vec![HttpResponseScript::sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"未经工具核实的事实。\"}}]}\n\ndata: [DONE]\n\n",
        )])
        .await
        .expect("bypass LLM boundary");
        configure_test_llm(
            &state,
            bypass_llm.base_url.clone(),
            "iris-test-verified-tools-assistant-external-bypass",
        );
        let mut bypass_request = granted_request;
        bypass_request.client_request_id = "assistant-start-external-bypass".into();
        let bypass = invoke_start(&webview, bypass_request);
        assert_eq!(wait_for_terminal(&state, &bypass).await, RunState::Failed);
        let bypass_calls = bypass_llm.finish().await.expect("bypass LLM completion");
        assert_eq!(bypass_calls.len(), 1);
        assert!(bypass_calls[0].body["tools"]
            .as_array()
            .expect("bypass tool surface")
            .iter()
            .any(|tool| tool["function"]["name"] == binding.exposed_name));
        let bypass_evidence_count = state
            .db
            .with_read_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM agent_run_evidence
                     WHERE run_id = ?1 AND registration_source = 'external_tool'",
                    [&bypass.run_id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(Into::into)
            })
            .expect("bypass evidence count");
        assert_eq!(bypass_evidence_count, 0);
    }

    fn durable_apply_fixture() -> (
        tempfile::TempDir,
        Arc<AppState>,
        AssistantRunAccepted,
        FrozenChangePlan,
    ) {
        let directory = tempfile::tempdir().expect("temporary app directory");
        let vault = directory.path().join("vault");
        std::fs::create_dir_all(&vault).expect("vault directory");
        std::fs::write(vault.join("note.md"), "base").expect("base note");
        let state = AppState::new(directory.path().join("data")).expect("application state");
        state.set_vault(vault.clone()).expect("activate vault");
        let base_hash = crate::cas::hash::content_hash_str("base");
        let accepted = RunIntake::start(
            &state.db,
            AssistantRunStartRequest {
                client_request_id: format!("durable-command-{}", uuid::Uuid::new_v4()),
                session: None,
                turn: AssistantTurnDraft {
                    message: "将已确认的修改应用到笔记".into(),
                    content_parts: None,
                    explicit_references: vec![ContextReferenceWire {
                        id: "target-note".into(),
                        kind: ContextReferenceKind::Note,
                        file_path: Some("note.md".into()),
                        content_hash: Some(base_hash.clone()),
                        utf8_range: None,
                        editor_range: None,
                        excerpt: String::new(),
                        heading_path: None,
                        anchor: None,
                        stale: false,
                        invalid_reason: None,
                    }],
                    retrieval_scope: Default::default(),
                    display_mentions: vec![],
                },
                explicit_action: Some(ExplicitAction {
                    effect: Effect::Apply,
                    target: Some(ExplicitTarget {
                        reference_id: "target-note".into(),
                        content_hash: base_hash.clone(),
                    }),
                    selection_snapshot: None,
                }),
                web_enabled: false,
                model_override: None,
                external_tool_grants: Vec::new(),
                security_domain: SecurityDomain::Normal,
                classified_context_ref: None,
            },
        )
        .expect("accepted durable apply");
        let active_vault = state.vault_path().expect("canonical active vault");
        let policy =
            evaluate_normal_run_policy(&state.db, &accepted).expect("current policy decision");
        AgentRunRepository::persist_authorization_snapshot(
            &state.db,
            &accepted.session.session_key,
            &accepted.run_id,
            &policy.allowed_capabilities,
        )
        .expect("immutable authorization snapshot");
        let session_id = state
            .db
            .with_read_conn(|conn| {
                conn.query_row(
                    "SELECT session_id FROM agent_runs WHERE run_id = ?1",
                    [&accepted.run_id],
                    |row| row.get(0),
                )
                .map_err(Into::into)
            })
            .expect("session id");
        let preparing = AgentRunRepository::append_event(
            &state.db,
            AppendRunEventInput {
                run_id: accepted.run_id.clone(),
                state_version: accepted.state_version,
                event_type: RunEventType::StageChanged,
                payload: RunEventPayload::StageChanged {
                    state: RunState::Preparing,
                    stage: "正在准备".into(),
                    stage_code: None,
                },
            },
        )
        .expect("preparing");
        let running = AgentRunRepository::append_event(
            &state.db,
            AppendRunEventInput {
                run_id: accepted.run_id.clone(),
                state_version: preparing.state_version(),
                event_type: RunEventType::StageChanged,
                payload: RunEventPayload::StageChanged {
                    state: RunState::Running,
                    stage: "正在生成变更预览".into(),
                    stage_code: None,
                },
            },
        )
        .expect("running");
        let tool_call_id = format!("tool-{}", accepted.run_id);
        let started = AgentRunRepository::append_event(
            &state.db,
            AppendRunEventInput {
                run_id: accepted.run_id.clone(),
                state_version: running.state_version(),
                event_type: RunEventType::ToolStarted,
                payload: RunEventPayload::ToolStarted {
                    capability: "replace_selection".into(),
                    tool_call_id: tool_call_id.clone(),
                },
            },
        )
        .expect("tool started");
        let plan = FrozenChangePlan::freeze(FrozenChangePlanInput {
            confirmation_id: format!("confirmation-{}", accepted.run_id),
            run_id: accepted.run_id.clone(),
            session_id,
            request_id: accepted.run_id.clone(),
            tool_call_id,
            vault_id: crate::cas::hash::content_hash_str(&active_vault.to_string_lossy()),
            relative_paths: vec!["note.md".into()],
            operation: "replace_selection".into(),
            base_content_hashes: vec![("note.md".into(), base_hash.clone())],
            expected_post_content_hashes: vec![(
                "note.md".into(),
                crate::cas::hash::content_hash_str("after"),
            )],
            change: serde_json::json!({
                "target_path": "note.md",
                "base_content_hash": base_hash,
                "range": { "start": 0, "end": 4 },
                "original_text": "base",
                "replacement": "after"
            }),
            affected_file_count: 1,
            rollback_summary: "可通过版本历史撤销".into(),
            expires_at_unix_ms: i64::MAX,
        })
        .expect("frozen plan");
        let awaiting = AgentRunRepository::request_frozen_confirmation(
            &state.db,
            &plan,
            started.state_version(),
            "等待确认：更新 1 个目标",
        )
        .expect("await confirmation");
        AgentRunRepository::approve_frozen_confirmation(
            &state.db,
            &accepted.session.session_key,
            &accepted.run_id,
            plan.confirmation_id(),
            plan.plan_hash(),
            awaiting.state_version(),
            0,
        )
        .expect("consume confirmation");
        (directory, state, accepted, plan)
    }

    fn tamper_consumed_confirmation(
        state: &AppState,
        original: &FrozenChangePlan,
    ) -> FrozenChangePlan {
        let tampered = FrozenChangePlan::freeze(FrozenChangePlanInput {
            confirmation_id: original.confirmation_id().into(),
            run_id: original.run_id().into(),
            session_id: original.session_id(),
            request_id: original.run_id().into(),
            tool_call_id: original.tool_call_id().into(),
            vault_id: original.vault_id().into(),
            relative_paths: original.relative_paths().to_vec(),
            operation: original.operation().into(),
            base_content_hashes: original.base_content_hashes().to_vec(),
            expected_post_content_hashes: vec![(
                "note.md".into(),
                crate::cas::hash::content_hash_str("tampered"),
            )],
            change: serde_json::json!({
                "target_path": "note.md",
                "base_content_hash": crate::cas::hash::content_hash_str("base"),
                "range": { "start": 0, "end": 4 },
                "original_text": "base",
                "replacement": "tampered"
            }),
            affected_file_count: 1,
            rollback_summary: "可通过版本历史撤销".into(),
            expires_at_unix_ms: i64::MAX,
        })
        .expect("tampered frozen plan");
        state
            .db
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE agent_run_confirmations
                     SET plan_hash = ?1, plan_json = ?2
                     WHERE confirmation_id = ?3 AND run_id = ?4 AND status = 'consumed'",
                    rusqlite::params![
                        tampered.plan_hash(),
                        tampered.persisted_plan_json()?,
                        tampered.confirmation_id(),
                        tampered.run_id(),
                    ],
                )?;
                Ok(())
            })
            .expect("tamper consumed confirmation row");
        tampered
    }

    fn assert_zero_write_and_dispatch(
        state: &AppState,
        accepted: &AssistantRunAccepted,
        directory: &tempfile::TempDir,
    ) {
        assert_eq!(
            std::fs::read_to_string(directory.path().join("vault/note.md"))
                .expect("read unchanged note"),
            "base"
        );
        assert_eq!(
            crate::ai_runtime::tool_audit::count_by_run(&state.db, &accepted.run_id)
                .expect("dispatch audit count"),
            0
        );
    }

    #[tokio::test]
    async fn approved_confirmation_tamper_before_async_executor_fails_closed_without_write() {
        let (directory, state, accepted, plan) = durable_apply_fixture();
        tamper_consumed_confirmation(&state, &plan);

        execute_confirmed_change_with_sink(
            Arc::clone(&state),
            accepted.session.clone(),
            accepted.run_id.clone(),
            plan.confirmation_id().into(),
            state.vault_path().ok(),
            &NoopSink,
        )
        .await;

        let replay = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
            .expect("replay")
            .expect("run");
        assert_eq!(replay.run.state, RunState::Failed);
        assert_zero_write_and_dispatch(&state, &accepted, &directory);
    }

    #[tokio::test]
    async fn tamper_after_startup_classification_before_resume_executor_fails_closed() {
        let (directory, state, accepted, plan) = durable_apply_fixture();
        RunEngine::recover_interrupted_runs(&state.db).expect("startup classification");
        let paused = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
            .expect("paused replay")
            .expect("run");
        assert_eq!(paused.run.state, RunState::Paused);
        assert_eq!(
            paused.run.recovery,
            Some(RunRecoveryKind::ResumeAvailable),
            "startup recovery must classify the intact fixture as resumable"
        );
        RunIntake::control(
            &state.db,
            AssistantRunControlRequest {
                session: accepted.session.clone(),
                run_id: accepted.run_id.clone(),
                expected_state_version: paused.run.state_version,
                action: RunControlAction::Resume,
            },
        )
        .expect("resume classified run");
        tamper_consumed_confirmation(&state, &plan);

        execute_confirmed_change_with_sink(
            Arc::clone(&state),
            accepted.session.clone(),
            accepted.run_id.clone(),
            plan.confirmation_id().into(),
            state.vault_path().ok(),
            &NoopSink,
        )
        .await;

        let replay = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
            .expect("replay")
            .expect("run");
        assert_eq!(replay.run.state, RunState::Failed);
        assert_zero_write_and_dispatch(&state, &accepted, &directory);
    }

    #[tokio::test]
    async fn resume_from_dispatching_checkpoint_replays_the_exact_plan_once() {
        let (directory, state, accepted, plan) = durable_apply_fixture();
        let running = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
            .expect("running replay")
            .expect("run");
        AgentRunRepository::append_checkpoint_step(
            &state.db,
            AppendRunCheckpointInput {
                run_id: accepted.run_id.clone(),
                state_version: running.run.state_version,
                checkpoint: DurableApplyCheckpoint::new(
                    plan.confirmation_id(),
                    plan.plan_hash(),
                    DurableApplyCheckpointStage::Dispatching,
                    plan.base_content_hashes()
                        .iter()
                        .map(|(_, hash)| hash.clone())
                        .collect(),
                    plan.expected_post_content_hashes()
                        .iter()
                        .map(|(_, hash)| hash.clone())
                        .collect(),
                    Vec::new(),
                )
                .expect("dispatching checkpoint"),
            },
        )
        .expect("simulate crash after dispatching checkpoint");
        RunEngine::recover_interrupted_runs(&state.db).expect("startup classification");
        let paused = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
            .expect("paused replay")
            .expect("run");
        assert_eq!(paused.run.recovery, Some(RunRecoveryKind::ResumeAvailable));
        RunIntake::control(
            &state.db,
            AssistantRunControlRequest {
                session: accepted.session.clone(),
                run_id: accepted.run_id.clone(),
                expected_state_version: paused.run.state_version,
                action: RunControlAction::Resume,
            },
        )
        .expect("resume dispatching checkpoint");

        execute_confirmed_change_with_sink(
            Arc::clone(&state),
            accepted.session.clone(),
            accepted.run_id.clone(),
            plan.confirmation_id().into(),
            state.vault_path().ok(),
            &NoopSink,
        )
        .await;

        let completed = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
            .expect("completed replay")
            .expect("run");
        assert_eq!(completed.run.state, RunState::Completed);
        assert_eq!(
            std::fs::read_to_string(directory.path().join("vault/note.md")).expect("applied note"),
            "after"
        );
        assert_eq!(
            crate::ai_runtime::tool_audit::count_by_run(&state.db, &accepted.run_id)
                .expect("single dispatch audit"),
            1
        );
    }

    fn invoke_control(
        webview: &tauri::WebviewWindow<tauri::test::MockRuntime>,
        request: AssistantRunControlRequest,
    ) -> Result<tauri::ipc::InvokeResponseBody, serde_json::Value> {
        tauri::test::get_ipc_response(
            webview,
            InvokeRequest {
                cmd: "assistant_run_control".into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                // Windows/Android 的 wry workaround 使用 http://tauri.localhost，
                // 其余平台才是 tauri://localhost；用错 URL 会被判定为 remote origin 并触发 ACL 拒绝。
                url: if cfg!(any(windows, target_os = "android")) {
                    "http://tauri.localhost"
                } else {
                    "tauri://localhost"
                }
                .parse()
                .expect("invoke URL"),
                body: tauri::ipc::InvokeBody::Json(serde_json::json!({ "request": request })),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.into(),
            },
        )
    }

    #[test]
    fn production_resume_command_completes_without_model_and_repeated_resume_does_not_dispatch() {
        let (directory, state, accepted, _plan) = durable_apply_fixture();
        let frozen_budget = AgentRunRepository::budget_policy_for_session(
            &state.db,
            &accepted.session.session_key,
            &accepted.run_id,
        )
        .expect("read frozen budget")
        .expect("frozen budget");
        assert_eq!(frozen_budget.post_confirmation_max_model_turns, 0);
        RunEngine::recover_interrupted_runs(&state.db).expect("startup classification");
        let paused = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
            .expect("paused replay")
            .expect("run");
        assert_eq!(
            paused.run.recovery,
            Some(RunRecoveryKind::ResumeAvailable),
            "startup recovery must classify the intact fixture as resumable"
        );
        let request = AssistantRunControlRequest {
            session: accepted.session.clone(),
            run_id: accepted.run_id.clone(),
            expected_state_version: paused.run.state_version,
            action: RunControlAction::Resume,
        };
        let app = tauri::test::mock_builder()
            .manage(Arc::clone(&state))
            .invoke_handler(tauri::generate_handler![assistant_run_control])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock application");
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("mock webview");

        let control_result = invoke_control(&webview, request.clone());
        assert!(
            control_result.is_ok(),
            "invoke_control failed: {control_result:?}"
        );
        for _ in 0..100 {
            let completed = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
                .expect("poll replay")
                .is_some_and(|replay| replay.run.state == RunState::Completed);
            if completed {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let completed = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
            .expect("completed replay")
            .expect("run");
        assert_eq!(completed.run.state, RunState::Completed);
        assert_eq!(
            std::fs::read_to_string(directory.path().join("vault/note.md")).expect("applied note"),
            "after"
        );
        assert_eq!(
            AgentRunRepository::latest_durable_apply_checkpoint(&state.db, &accepted.run_id)
                .expect("latest checkpoint")
                .expect("completed checkpoint")
                .stage(),
            DurableApplyCheckpointStage::Completed
        );
        assert_eq!(
            crate::ai_runtime::tool_audit::count_by_run(&state.db, &accepted.run_id)
                .expect("single dispatch audit"),
            1
        );
        let post_confirmation_model_event_count = completed
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event.payload(),
                    RunEventPayload::ProviderSwitched { .. }
                        | RunEventPayload::ReasoningSummary { .. }
                        | RunEventPayload::ContentDelta { .. }
                )
            })
            .count();
        assert_eq!(post_confirmation_model_event_count, 0);

        assert!(invoke_control(&webview, request).is_err());
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(
            crate::ai_runtime::tool_audit::count_by_run(&state.db, &accepted.run_id)
                .expect("dispatch count after repeated resume"),
            1
        );
    }
}
