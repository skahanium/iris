//! Explicit Request Intake for the scene-free normal-domain Run path.
//!
//! This minimal Phase 4 surface accepts and persists a Run before any routing,
//! context assembly, Skill activation, legacy Task, Trace, or Harness work.

use crate::ai_runtime::agent_run_repository::{
    AcceptRunInput, AgentRunRepository, FrozenConfirmationApproval,
};
use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
use crate::ai_runtime::run_contract::{
    AssistantRunAccepted, AssistantRunControlRequest, AssistantRunGetResponse,
    AssistantRunStartRequest, AssistantSessionRef, ContextMode, Effect, Effort, ExecutionEnvelope,
    Freshness, MaterialNeed, Modality, RiskClass, RunControlAction, RunEventPayload, RunEventType,
    SecurityDomain,
};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

const MAX_CLIENT_REQUEST_ID_CHARS: usize = 160;
const MAX_USER_MESSAGE_CHARS: usize = 16_000;

/// Scene-free normal-domain Request Intake.
pub(crate) struct RunIntake;

impl RunIntake {
    /// Resolve the deliberately small Phase 4 envelope without inferring UI state.
    pub(crate) fn resolve_minimal_envelope(
        request: &AssistantRunStartRequest,
    ) -> AppResult<ExecutionEnvelope> {
        validate_start_request(request)?;
        if request.web_enabled {
            return Err(AppError::msg("agent_run_web_not_available"));
        }
        if request
            .explicit_action
            .as_ref()
            .is_some_and(|action| action.effect != Effect::Answer)
        {
            return Err(AppError::msg("agent_run_effect_not_available"));
        }
        let context = if !request.explicit_references.is_empty() {
            ContextMode::ExplicitReferences
        } else if request.explicit_action.is_some() {
            ContextMode::ExplicitScope
        } else {
            ContextMode::None
        };
        Ok(ExecutionEnvelope {
            effect: Effect::Answer,
            context,
            freshness: Freshness::Offline,
            effort: Effort::Direct,
            security_domain: SecurityDomain::Normal,
            risk: RiskClass::ReadOnly,
            modalities: vec![Modality::Text],
            material_needs: if request.explicit_references.is_empty() {
                vec![]
            } else {
                vec![MaterialNeed::Reference]
            },
            required_capabilities: vec![],
            explicit_constraints: vec![],
        })
    }

    /// Atomically accept a new normal-domain Run before any execution work begins.
    pub(crate) fn start(
        db: &Database,
        request: AssistantRunStartRequest,
    ) -> AppResult<AssistantRunAccepted> {
        let envelope = Self::resolve_minimal_envelope(&request)?;
        let session = resolve_normal_session(db, request.session.as_ref())?;
        AgentRunRepository::accept(
            db,
            AcceptRunInput {
                session_id: session.session_id,
                session_key: session.session_key,
                client_request_id: request.client_request_id,
                run_id: uuid::Uuid::new_v4().to_string(),
                turn_id: uuid::Uuid::new_v4().to_string(),
                message: request.message,
                content_parts: request.content_parts,
                explicit_references: request.explicit_references,
                envelope,
            },
        )
    }

    /// Accept a Run then emit its already-committed accepted event on the shared sink.
    pub(crate) fn start_with_sink(
        db: &Database,
        request: AssistantRunStartRequest,
        sink: &impl crate::ai_runtime::run_engine::RunEventSink,
    ) -> AppResult<AssistantRunAccepted> {
        let accepted = Self::start(db, request)?;
        let event = AgentRunRepository::get_for_session(
            db,
            &accepted.session.session_key,
            &accepted.run_id,
        )?
        .and_then(|response| response.events.into_iter().next())
        .ok_or_else(|| AppError::msg("agent_run_accepted_event_missing"))?;
        sink.emit(&event)?;
        Ok(accepted)
    }

    /// Read a persisted Run only through its explicit owning session.
    pub(crate) fn get(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
    ) -> AppResult<Option<AssistantRunGetResponse>> {
        if session.domain != SecurityDomain::Normal {
            return Err(AppError::msg("agent_run_classified_domain_not_supported"));
        }
        AgentRunRepository::get_for_session(db, &session.session_key, run_id)
    }

    /// Handle a control action without touching legacy Harness state.
    pub(crate) fn control(db: &Database, request: AssistantRunControlRequest) -> AppResult<()> {
        let _ = Self::control_event(db, request)?;
        Ok(())
    }

    /// Handle a control action and emit its durable event when it changes state.
    pub(crate) fn control_with_sink(
        db: &Database,
        request: AssistantRunControlRequest,
        sink: &impl crate::ai_runtime::run_engine::RunEventSink,
    ) -> AppResult<()> {
        if let Some(event) = Self::control_event(db, request)? {
            sink.emit(&event)?;
        }
        Ok(())
    }

    fn control_event(
        db: &Database,
        request: AssistantRunControlRequest,
    ) -> AppResult<Option<crate::ai_runtime::run_contract::AssistantRunEvent>> {
        if request.session.domain != SecurityDomain::Normal {
            return Err(AppError::msg("agent_run_classified_domain_not_supported"));
        }
        let snapshot =
            AgentRunRepository::get_for_session(db, &request.session.session_key, &request.run_id)?
                .ok_or_else(|| AppError::msg("agent_run_not_found"))?;
        if snapshot.run.state == crate::ai_runtime::run_contract::RunState::Cancelled
            && matches!(&request.action, RunControlAction::Cancel)
        {
            return Ok(None);
        }
        match request.action {
            RunControlAction::Cancel => {
                let event = AgentRunRepository::append_event(
                    db,
                    crate::ai_runtime::agent_run_repository::AppendRunEventInput {
                        run_id: request.run_id.clone(),
                        state_version: request.expected_state_version,
                        event_type: RunEventType::Cancelled,
                        payload: RunEventPayload::Cancelled {
                            reason: "用户取消运行".to_string(),
                        },
                    },
                )?;
                crate::ai_runtime::model_gateway::request_abort(&request.run_id);
                Ok(Some(event))
            }
            RunControlAction::ApproveChange {
                confirmation_id,
                plan_hash,
            } => match AgentRunRepository::approve_frozen_confirmation(
                db,
                &request.session.session_key,
                &request.run_id,
                &confirmation_id,
                &plan_hash,
                request.expected_state_version,
                chrono::Utc::now().timestamp_millis(),
            )? {
                FrozenConfirmationApproval::Resumed(event) => Ok(Some(event)),
                FrozenConfirmationApproval::AlreadyApplied => Ok(None),
            },
            RunControlAction::RejectChange { .. } | RunControlAction::Resume => {
                Err(AppError::msg("agent_run_control_not_available"))
            }
        }
    }
}

fn validate_start_request(request: &AssistantRunStartRequest) -> AppResult<()> {
    if request.security_domain != SecurityDomain::Normal {
        return Err(AppError::msg("agent_run_classified_domain_not_supported"));
    }
    if request.client_request_id.trim().is_empty()
        || request.client_request_id.chars().count() > MAX_CLIENT_REQUEST_ID_CHARS
        || request.message.trim().is_empty()
        || request.message.chars().count() > MAX_USER_MESSAGE_CHARS
    {
        return Err(AppError::msg("agent_run_invalid_request"));
    }
    Ok(())
}

fn resolve_normal_session(
    db: &Database,
    requested: Option<&AssistantSessionRef>,
) -> AppResult<crate::ai_runtime::normal_session_repository::NormalSession> {
    match requested {
        Some(session) if session.domain != SecurityDomain::Normal => {
            Err(AppError::msg("agent_run_classified_domain_not_supported"))
        }
        Some(session) => NormalSessionRepository::get(db, &session.session_key)?
            .ok_or_else(|| AppError::msg("agent_run_session_not_found")),
        None => NormalSessionRepository::create(db),
    }
}
