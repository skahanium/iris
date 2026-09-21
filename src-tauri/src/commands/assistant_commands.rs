//! Unified Agent Run and domain-routed session IPC commands.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::ai_runtime::agent_tool_loop::ToolLoopProvider;
use crate::ai_runtime::diagnostic_query::{diagnose_run, DiagnosticReport};
use crate::ai_runtime::run_contract::{
    AssistantRunAccepted, AssistantRunControlRequest, AssistantRunDiagnoseRequest,
    AssistantRunEvent, AssistantRunGetRequest, AssistantRunGetResponse, AssistantRunRetryRequest,
    AssistantRunStartRequest, AssistantSessionRef, Effect, Effort, SafeRunErrorCode,
    SecurityDomain,
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
                    let evidence_refs = item.evidence_refs.clone();
                    let web_citations = historical_web_citations_for_run(
                        &state.db,
                        item.run_id
                            .as_deref()
                            .or_else(|| process.map(|value| value.run_id.as_str())),
                        evidence_refs.as_deref(),
                        item.web_citations,
                    );
                    let citation_binding = item.citation_binding.or_else(|| {
                        (evidence_refs.is_none() && !web_citations.is_empty()).then_some(
                            crate::ai_types::CitationBinding {
                                mode: crate::ai_types::CitationBindingMode::SourceGroupFallback,
                                referenced_indices: Vec::new(),
                                fallback_reason: Some("legacy_binding_unavailable".to_string()),
                            },
                        )
                    });
                    let source_summary = historical_source_summary_for_run(
                        &state.db,
                        item.run_id
                            .as_deref()
                            .or_else(|| process.map(|value| value.run_id.as_str())),
                        evidence_refs.as_deref(),
                        item.source_summary,
                    );
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
                        source_summary,
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
    evidence_refs: Option<&[i64]>,
    persisted: Vec<crate::ai_types::WebCitationEntry>,
) -> Vec<crate::ai_types::WebCitationEntry> {
    let Some(run_id) = run_id else {
        return persisted;
    };
    if let Some(evidence_refs) = evidence_refs {
        return match crate::ai_runtime::agent_evidence_repository::AgentEvidenceRepository::list_selected_current_run_web_citation_links(
            db,
            run_id,
            evidence_refs,
        ) {
            Ok(selected) => selected
                .into_iter()
                .map(|citation| crate::ai_types::WebCitationEntry {
                    index: citation.index,
                    title: citation.title,
                    url: citation.url,
                })
                .collect(),
            // A modern message has an explicit evidence selection. If that
            // selection cannot be resolved, showing no source is safer than
            // reviving a stale persisted or whole-Run source group.
            Err(_) => Vec::new(),
        };
    }
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

fn historical_source_summary_for_run(
    db: &crate::storage::db::Database,
    run_id: Option<&str>,
    evidence_refs: Option<&[i64]>,
    persisted: Vec<crate::ai_runtime::provenance::SourceSummaryEntry>,
) -> Vec<crate::ai_runtime::provenance::SourceSummaryEntry> {
    let (Some(run_id), Some(evidence_refs)) = (run_id, evidence_refs) else {
        return persisted;
    };
    crate::ai_runtime::agent_evidence_repository::AgentEvidenceRepository::source_summary_for_current_run(
        db,
        run_id,
        evidence_refs,
    )
    .map(|summary| summary.entries())
    // Modern messages own an explicit selection. Never revive a persisted
    // whole-Run summary when the selected rows cannot be resolved.
    .unwrap_or_default()
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
                ) => {
                    let resumed = crate::ai_runtime::run_intake::RunIntake::get(
                        &state.db, &session, &run_id,
                    )?
                    .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?
                    .run;
                    spawn_normal_direct_run(
                        Arc::clone(&state),
                        app_handle,
                        crate::ai_runtime::run_contract::AssistantRunAccepted {
                            client_request_id: String::new(),
                            run_id: run_id.clone(),
                            turn_id: resumed.turn_id,
                            session: session.clone(),
                            state: resumed.state,
                            state_version: resumed.state_version,
                        },
                        state.vault_path().ok(),
                    );
                }
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

/// Explain one Run from C26 records. Query failure is an error, not an empty pass.
#[tauri::command]
pub async fn assistant_run_diagnose(
    state: State<'_, Arc<AppState>>,
    request: AssistantRunDiagnoseRequest,
) -> AppResult<DiagnosticReport> {
    diagnose_run(&state.db, &request.session, &request.run_id)
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

/// Resume exactly one consumed frozen change set. Approval authorizes only the
/// immutable arguments produced during the original Run; a successful full
/// execution may subsequently use the separately bounded, read-only verifier.
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
    let _inflight = crate::ai_runtime::run_inflight::InflightGuard::new(run_id.clone());
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
    let resumed = crate::ai_runtime::agent_run_repository::AgentRunRepository::get_for_session(
        &db,
        &session.session_key,
        &run_id,
    )
    .ok()
    .flatten()
    .is_some_and(|snapshot| {
        snapshot.events.iter().any(|event| {
            matches!(
                event.payload(),
                crate::ai_runtime::run_contract::RunEventPayload::Paused {
                    recovery: Some(
                        crate::ai_runtime::run_contract::RunRecoveryKind::ResumeAvailable
                    ),
                    ..
                }
            )
        })
    });
    let (applied_prefix, boundary_hashes) = if resumed {
        let Some(live_vault) = state.vault_path().ok() else {
            fail();
            return;
        };
        match RunEngine::confirmed_apply_boundary(
            &db,
            &session.session_key,
            &run_id,
            &plan,
            &live_vault,
        ) {
            Ok(boundary) => boundary,
            Err(_) => {
                fail();
                return;
            }
        }
    } else {
        (0, Vec::new())
    };
    let context =
        match crate::ai_runtime::run_context::RunContextAssembler::assemble_at_confirmed_boundary(
            &db,
            vault.as_deref(),
            &session.session_key,
            &run_id,
            &boundary_hashes,
            &plan.operations()[..applied_prefix],
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
    match executor.execute_confirmed_frozen_change_set(&plan).await {
        Ok(results) if !results.is_empty() => {
            let applied = results.iter().filter(|result| result.success).count();
            let executed_operations = plan
                .operations()
                .iter()
                .zip(results.iter())
                .filter(|(_, result)| result.success)
                .map(|(operation, _)| {
                    format!(
                        "{}（{}）",
                        operation.operation(),
                        operation.relative_paths().join(", ")
                    )
                })
                .collect::<Vec<_>>()
                .join("；");
            let applied_paths: std::collections::BTreeSet<&str> = plan
                .operations()
                .iter()
                .zip(results.iter())
                .filter(|(_, result)| result.success)
                .flat_map(|(operation, _)| operation.relative_paths().iter().map(String::as_str))
                .collect();
            let frozen_paths: std::collections::BTreeSet<&str> =
                plan.relative_paths().iter().map(String::as_str).collect();
            let complete = applied == plan.operations().len() && applied_paths == frozen_paths;
            let content = if complete {
                format!(
                    "已按确认顺序执行 {applied}/{} 项变更。\n已执行操作：{executed_operations}",
                    plan.operations().len(),
                )
            } else {
                format!(
                    "已执行 {applied}/{} 项变更；后续操作未执行，因为目标已变化或执行条件不再满足。\n已执行操作：{executed_operations}",
                    plan.operations().len(),
                )
            };
            if complete
                && crate::ai_runtime::normal_run_service::execute_post_confirmation_verification(
                    Arc::clone(&state),
                    accepted.clone(),
                    vault.clone(),
                    &plan,
                    &content,
                    sink,
                )
                .await
                .is_ok()
            {
                return;
            }
            let fallback = if complete {
                format!("{content} 未进行模型复核。")
            } else {
                content
            };
            if RunEngine::finalize_confirmed_change_report_with_sink(
                &db, &session, &run_id, &fallback, complete, sink,
            )
            .is_err()
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
            None,
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
        let _budget = crate::ai_runtime::model_turn_ledger::BindGuard::new(&accepted.run_id, 1);
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
#[path = "assistant_commands_tests.rs"]
mod normal_run_desktop_adapter_tests;
