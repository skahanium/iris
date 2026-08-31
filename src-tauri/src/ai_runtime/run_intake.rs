//! Deterministic, scene-free Request Intake for unified Agent Runs.

use crate::ai_runtime::agent_run_repository::{
    AcceptRunInput, AcceptRunOutcome, AgentRunRepository, FrozenConfirmationApproval,
    FrozenConfirmationRejection, RetryRunInput,
};
use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
use crate::ai_runtime::run_contract::{
    AssistantRunAccepted, AssistantRunControlRequest, AssistantRunGetResponse,
    AssistantRunRetryRequest, AssistantRunStartRequest, AssistantSessionRef, CapabilityId,
    ContextMode, Effect, Effort, ExecutionEnvelope, ExplicitConstraint, FreshFactPolicy, Freshness,
    MaterialNeed, Modality, RiskClass, RunControlAction, RunEventPayload, RunEventType,
    SafeRunErrorCode, SecurityDomain, VerificationRequirement, WebDecisionReason,
};
use crate::ai_runtime::run_engine::emit_durable_event_best_effort;
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

const MAX_CLIENT_REQUEST_ID_CHARS: usize = 160;
const MAX_USER_MESSAGE_CHARS: usize = 16_000;

/// Outcome of one normal-domain control request after its durable event is written.
/// Commands use this to start post-approval execution exactly once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NormalRunControlOutcome {
    Applied,
    ConfirmationApproved,
    ConfirmationRejected,
    RecoveryResumed { confirmation_id: String },
    InputProvided,
    Noop,
}

/// The only normal-domain request admission boundary.
pub(crate) struct RunIntake;

impl RunIntake {
    /// Resolve the immutable execution envelope from request facts only.
    pub(crate) fn resolve_envelope(
        request: &AssistantRunStartRequest,
    ) -> AppResult<ExecutionEnvelope> {
        validate_start_request(request)?;
        let message = request.turn.message.to_ascii_lowercase();
        let directive_text = strip_quoted_segments(&message);
        let child_run_requested = needs_child_run(&directive_text);
        let local_only = contains_any(
            &directive_text,
            &[
                "local only",
                "offline only",
                "do not use web",
                "without web",
                "stay offline",
                "use local material only",
                "do not browse",
                "only use the attachment",
                "conversation only",
                "do not search online",
                "supplied material only",
                "不要联网",
                "不联网",
                "离线完成",
                "只看当前对话",
                "\u{53ea}\u{7528}\u{672c}\u{5730}",
                "\u{4ec5}\u{7528}\u{672c}\u{5730}",
            ],
        );
        if local_only && !request.external_tool_grants.is_empty() {
            return Err(AppError::msg("agent_run_external_tool_local_only_conflict"));
        }
        let do_not_modify = contains_any(
            &directive_text,
            &[
                "do not modify",
                "don't modify",
                "rewrite only",
                "\u{4e0d}\u{8981}\u{4fee}\u{6539}",
                "\u{4e0d}\u{4fee}\u{6539}",
            ],
        );
        // New Runs deliberately do not write a domain plan. The legacy field
        // remains in the envelope solely so historical Runs can be resumed
        // without migration; task/risk signals below are the new authority.
        let fresh_fact = FreshFactPolicy::default();
        let web_decision =
            ExclusionClassifier::resolve(request, &message, &directive_text, local_only);
        let effect = if do_not_modify {
            Effect::Answer
        } else {
            request
                .explicit_action
                .as_ref()
                .map_or(Effect::Answer, |action| action.effect)
        };
        let has_explicit_materials_or_scope =
            !request.turn.explicit_references.is_empty() || has_retrieval_scope(request);
        let implicit_vault_required = !has_explicit_materials_or_scope
            && allow_implicit_vault_for_run(
                request.security_domain,
                &directive_text,
                has_explicit_materials_or_scope,
            );
        let context = if request.explicit_action.is_some() {
            ContextMode::ExplicitScope
        } else if !request.turn.explicit_references.is_empty() {
            ContextMode::ExplicitReferences
        } else if has_retrieval_scope(request) {
            ContextMode::ExplicitScope
        } else if implicit_vault_required {
            ContextMode::ImplicitVault
        } else if request.session.is_some() {
            ContextMode::Conversation
        } else {
            ContextMode::None
        };
        let freshness = web_decision.freshness;
        let has_images = request.turn.content_parts.as_ref().is_some_and(|parts| {
            parts
                .iter()
                .any(|part| matches!(part, crate::ai_types::ContentPart::ImageUrl { .. }))
        });
        let effort = match effect {
            Effect::Apply => Effort::Durable,
            _ if matches!(freshness, Freshness::WebPreferred | Freshness::WebRequired)
                || has_images
                || has_retrieval_scope(request)
                || !request.external_tool_grants.is_empty()
                || child_run_requested
                || is_high_stakes_current_request(&directive_text)
                || requires_multi_step_research(&directive_text)
                || needs_offline_vault_tool_loop(request, &directive_text) =>
            {
                Effort::ToolLoop
            }
            _ => Effort::Direct,
        };
        let risk = match effect {
            Effect::Apply => RiskClass::BoundedWrite,
            Effect::Answer | Effect::Draft => RiskClass::ReadOnly,
        };
        let mut material_needs = Vec::new();
        if !request.turn.explicit_references.is_empty() {
            material_needs.push(MaterialNeed::Reference);
        }
        if freshness != Freshness::Offline {
            material_needs.push(MaterialNeed::Web);
        }
        material_needs.sort_by_key(|need| match need {
            MaterialNeed::Reference => 0,
            MaterialNeed::Web => 1,
            MaterialNeed::Authority | MaterialNeed::Exemplar => 2,
        });
        material_needs.dedup();
        // The envelope is the only source of capabilities that may reach a
        // model-visible tool surface. Keep these concrete and auditable rather
        // than deriving permissions later from effort or access-level enums.
        let mut required_capabilities = vec![
            CapabilityId::new("model.text"),
            // Trusted runtime facts are always locally available; exposing
            // them in a ToolLoop never grants filesystem or network access.
            CapabilityId::new("runtime.read"),
        ];
        if has_images {
            required_capabilities.push(CapabilityId::new("model.vision"));
        }
        match effect {
            Effect::Draft => required_capabilities.push(CapabilityId::new("note.propose_patch")),
            Effect::Apply => required_capabilities.push(CapabilityId::new("note.apply_patch")),
            Effect::Answer => {}
        }
        if matches!(effort, Effort::ToolLoop | Effort::Durable) {
            required_capabilities.push(CapabilityId::new("context.read"));
            if allow_implicit_vault_for_run(
                request.security_domain,
                &directive_text,
                has_explicit_materials_or_scope,
            ) {
                required_capabilities.push(CapabilityId::new("vault.read"));
            }
        }
        // The user-controlled Web toggle is the sole authority that can add
        // Web capability. Freshness only describes evidence obligation; it
        // must never be a second permission switch.
        if request.web_enabled
            && request.security_domain == SecurityDomain::Normal
            && !local_only
            && freshness != Freshness::Offline
        {
            required_capabilities.push(CapabilityId::new("web.search"));
        }
        if child_run_requested {
            required_capabilities.push(CapabilityId::new("harness.child_run"));
        }
        if !request.external_tool_grants.is_empty() {
            required_capabilities.push(CapabilityId::new("external.read"));
        }
        let mut explicit_constraints = Vec::new();
        if local_only {
            explicit_constraints.push(ExplicitConstraint {
                kind: "local_only".into(),
                value: None,
            });
        }
        if do_not_modify {
            explicit_constraints.push(ExplicitConstraint {
                kind: "do_not_modify".into(),
                value: None,
            });
        }
        if let Some(model_override) = request.model_override.as_ref() {
            explicit_constraints.push(ExplicitConstraint {
                kind: "model_override".into(),
                value: Some(serde_json::to_string(model_override)?),
            });
        }
        Ok(ExecutionEnvelope {
            effect,
            context,
            freshness,
            web_reason: web_decision.reason,
            verification_requirement: web_decision.verification_requirement,
            effort,
            security_domain: request.security_domain,
            risk,
            modalities: if has_images {
                vec![Modality::Text, Modality::Image]
            } else {
                vec![Modality::Text]
            },
            material_needs,
            required_capabilities,
            explicit_constraints,
            fresh_fact,
        })
    }

    /// Atomically accept a normal-domain Run before routing or context assembly.
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "unit and evaluation fixtures accept without an event sink"
        )
    )]
    pub(crate) fn start(
        db: &Database,
        request: AssistantRunStartRequest,
    ) -> AppResult<AssistantRunAccepted> {
        Self::start_outcome(db, request).map(|outcome| outcome.accepted)
    }

    fn start_outcome(
        db: &Database,
        mut request: AssistantRunStartRequest,
    ) -> AppResult<AcceptRunOutcome> {
        request.turn.retrieval_scope = crate::ai_runtime::retrieval_scope::normalize_context_scope(
            &request.turn.retrieval_scope,
        )?;
        for reference in &mut request.turn.explicit_references {
            if let Some(path) = reference.file_path.as_mut() {
                *path = crate::ai_runtime::retrieval_scope::normalize_note_path(path)
                    .map_err(|_| AppError::run(SafeRunErrorCode::InvalidExplicitReference))?;
            }
        }
        let envelope = Self::resolve_envelope(&request)?;
        if envelope.security_domain != SecurityDomain::Normal {
            return Err(AppError::run(
                SafeRunErrorCode::ClassifiedDomainNotSupported,
            ));
        }
        let session = request
            .session
            .as_ref()
            .map(|session| resolve_existing_normal_session(db, session))
            .transpose()?;
        let create_session = session.is_none();
        let session_id = session.as_ref().map_or(0, |session| session.session_id);
        let session_key = session
            .as_ref()
            .map_or_else(String::new, |session| session.session_key.clone());
        let external_tool_grants = request.external_tool_grants.clone();
        AgentRunRepository::accept_with_external_grants_outcome(
            db,
            AcceptRunInput {
                session_id,
                session_key,
                client_request_id: request.client_request_id,
                run_id: uuid::Uuid::new_v4().to_string(),
                turn_id: uuid::Uuid::new_v4().to_string(),
                message: request.turn.message,
                content_parts: request.turn.content_parts,
                explicit_references: request.turn.explicit_references,
                context_scope: request.turn.retrieval_scope,
                display_mentions: request.turn.display_mentions,
                explicit_action: request.explicit_action,
                envelope,
            },
            &external_tool_grants,
            create_session,
        )
    }

    /// Accept a classified Run in CEF only; classified execution remains direct and offline.
    ///
    /// Production classified assistant runs use [`crate::ai_runtime::classified_ephemeral`]
    /// instead of this intake path. Retained for contract tests and CEF migration coverage.
    #[cfg(test)]
    pub(crate) fn start_classified(
        vault: &std::path::Path,
        request: AssistantRunStartRequest,
    ) -> AppResult<AssistantRunAccepted> {
        let envelope = Self::resolve_envelope(&request)?;
        if envelope.security_domain != SecurityDomain::Classified {
            return Err(AppError::run(SafeRunErrorCode::InvalidRequest));
        }
        if envelope.freshness != Freshness::Offline
            || envelope.effort != Effort::Direct
            || envelope.effect != Effect::Answer
        {
            return Err(AppError::run(SafeRunErrorCode::PermissionDenied));
        }
        let session_key = match request.session.as_ref() {
            Some(session) if session.domain != SecurityDomain::Classified => {
                return Err(AppError::run(SafeRunErrorCode::SessionNotFound))
            }
            Some(session) => Some(session.session_key.clone()),
            None => None,
        };
        let effect = serde_json::to_value(envelope.effect)?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidRequest))?;
        crate::ai_runtime::classified_session::classified_run_accept(
            vault,
            crate::ai_runtime::classified_session::ClassifiedRunAcceptInput {
                client_request_id: request.client_request_id,
                session_key,
                run_id: uuid::Uuid::new_v4().to_string(),
                turn_id: uuid::Uuid::new_v4().to_string(),
                message: request.turn.message,
                content_parts: request
                    .turn
                    .content_parts
                    .map(serde_json::to_value)
                    .transpose()?,
                explicit_references: request
                    .turn
                    .explicit_references
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<Result<Vec<_>, _>>()?,
                explicit_action: request
                    .explicit_action
                    .map(serde_json::to_value)
                    .transpose()?,
                envelope: serde_json::to_value(envelope)?,
                effect,
            },
        )
    }

    /// Accept and emit the already durable accepted event.
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "unit and evaluation fixtures inspect the accepted identity directly"
        )
    )]
    pub(crate) fn start_with_sink(
        db: &Database,
        request: AssistantRunStartRequest,
        sink: &impl crate::ai_runtime::run_engine::RunEventSink,
    ) -> AppResult<AssistantRunAccepted> {
        Self::start_with_sink_outcome(db, request, sink).map(|outcome| outcome.accepted)
    }

    /// Accept and notify once; idempotent replays return the original identity
    /// without emitting or scheduling the same Run again.
    pub(crate) fn start_with_sink_outcome(
        db: &Database,
        request: AssistantRunStartRequest,
        sink: &impl crate::ai_runtime::run_engine::RunEventSink,
    ) -> AppResult<AcceptRunOutcome> {
        let outcome = Self::start_outcome(db, request)?;
        if !outcome.is_new {
            return Ok(outcome);
        }
        let event = AgentRunRepository::get_for_session(
            db,
            &outcome.accepted.session.session_key,
            &outcome.accepted.run_id,
        )?
        .and_then(|response| response.events.into_iter().next())
        .ok_or_else(|| AppError::run(SafeRunErrorCode::AcceptedEventMissing))?;
        // The durable event is authoritative. A transient IPC notification
        // failure must not strand a newly accepted Run before execution.
        emit_durable_event_best_effort(sink, &event);
        Ok(outcome)
    }

    /// Accept a retry and report whether this call created the Run.
    ///
    /// Only `is_new=true` may start an executor; idempotent replays return the
    /// original identity and do not emit a second accepted notification.
    pub(crate) fn retry_with_sink_outcome(
        db: &Database,
        request: AssistantRunRetryRequest,
        sink: &impl crate::ai_runtime::run_engine::RunEventSink,
    ) -> AppResult<AcceptRunOutcome> {
        if request.session.domain != SecurityDomain::Normal
            || request.source_run_id.trim().is_empty()
            || request.client_request_id.trim().is_empty()
            || request.client_request_id.chars().count() > MAX_CLIENT_REQUEST_ID_CHARS
        {
            return Err(AppError::run(SafeRunErrorCode::InvalidRequest));
        }
        let outcome = AgentRunRepository::accept_retry_outcome(
            db,
            RetryRunInput {
                session_key: request.session.session_key,
                source_run_id: request.source_run_id,
                client_request_id: request.client_request_id,
                run_id: uuid::Uuid::new_v4().to_string(),
            },
        )?;
        if !outcome.is_new {
            return Ok(outcome);
        }
        let event = AgentRunRepository::get_for_session(
            db,
            &outcome.accepted.session.session_key,
            &outcome.accepted.run_id,
        )?
        .and_then(|response| response.events.into_iter().next())
        .ok_or_else(|| AppError::run(SafeRunErrorCode::AcceptedEventMissing))?;
        // The durable event is authoritative. A transient IPC notification
        // failure must not strand a newly accepted retry before execution.
        emit_durable_event_best_effort(sink, &event);
        Ok(outcome)
    }

    /// Read only through the owning normal-domain session reference.
    pub(crate) fn get(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
    ) -> AppResult<Option<AssistantRunGetResponse>> {
        if session.domain != SecurityDomain::Normal {
            return Err(AppError::run(
                SafeRunErrorCode::ClassifiedDomainNotSupported,
            ));
        }
        AgentRunRepository::get_for_session(db, &session.session_key, run_id)
    }

    /// Return the latest recoverable Run for one normal-domain session.
    pub(crate) fn get_latest_active(
        db: &Database,
        session: &AssistantSessionRef,
    ) -> AppResult<Option<AssistantRunGetResponse>> {
        if session.domain != SecurityDomain::Normal {
            return Err(AppError::run(
                SafeRunErrorCode::ClassifiedDomainNotSupported,
            ));
        }
        AgentRunRepository::latest_active_for_session(db, &session.session_key)
    }

    /// Apply an explicit lifecycle control without legacy task state.
    #[cfg(test)]
    pub(crate) fn control(db: &Database, request: AssistantRunControlRequest) -> AppResult<()> {
        let _ = Self::control_event(db, request)?;
        Ok(())
    }

    /// Apply and emit a durable lifecycle event.
    pub(crate) fn control_with_sink(
        db: &Database,
        request: AssistantRunControlRequest,
        sink: &impl crate::ai_runtime::run_engine::RunEventSink,
    ) -> AppResult<NormalRunControlOutcome> {
        let (outcome, event) = Self::control_event(db, request)?;
        if let Some(event) = event {
            emit_durable_event_best_effort(sink, &event);
        }
        Ok(outcome)
    }

    fn control_event(
        db: &Database,
        request: AssistantRunControlRequest,
    ) -> AppResult<(
        NormalRunControlOutcome,
        Option<crate::ai_runtime::run_contract::AssistantRunEvent>,
    )> {
        if request.session.domain != SecurityDomain::Normal {
            return Err(AppError::run(
                SafeRunErrorCode::ClassifiedDomainNotSupported,
            ));
        }
        let snapshot =
            AgentRunRepository::get_for_session(db, &request.session.session_key, &request.run_id)?
                .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
        if snapshot.run.state == crate::ai_runtime::run_contract::RunState::Cancelled
            && matches!(&request.action, RunControlAction::Cancel)
        {
            return Ok((NormalRunControlOutcome::Noop, None));
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
                            reason: "user_cancelled".into(),
                        },
                    },
                )?;
                crate::ai_runtime::model_gateway::request_abort(&request.run_id);
                Ok((NormalRunControlOutcome::Applied, Some(event)))
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
                FrozenConfirmationApproval::Resumed(event) => {
                    Ok((NormalRunControlOutcome::ConfirmationApproved, Some(event)))
                }
                FrozenConfirmationApproval::AlreadyApplied => {
                    Ok((NormalRunControlOutcome::Noop, None))
                }
            },
            RunControlAction::RejectChange { confirmation_id } => {
                match AgentRunRepository::reject_frozen_confirmation(
                    db,
                    &request.session.session_key,
                    &request.run_id,
                    &confirmation_id,
                    request.expected_state_version,
                    chrono::Utc::now().timestamp_millis(),
                )? {
                    FrozenConfirmationRejection::Cancelled(event) => {
                        Ok((NormalRunControlOutcome::ConfirmationRejected, Some(event)))
                    }
                    FrozenConfirmationRejection::AlreadyRejected => {
                        Ok((NormalRunControlOutcome::Noop, None))
                    }
                }
            }
            RunControlAction::SubmitInput { input_id, values } => {
                if snapshot.run.state == crate::ai_runtime::run_contract::RunState::Preparing
                    && snapshot.events.iter().any(|event| {
                        matches!(
                            event.payload(),
                            RunEventPayload::InputProvided { input_id: provided, .. }
                                if provided == &input_id
                        )
                    })
                {
                    return Ok((NormalRunControlOutcome::Noop, None));
                }
                validate_input_submission(&snapshot, &input_id, &values)?;
                let event = AgentRunRepository::append_event(
                    db,
                    crate::ai_runtime::agent_run_repository::AppendRunEventInput {
                        run_id: request.run_id.clone(),
                        state_version: request.expected_state_version,
                        event_type: RunEventType::InputProvided,
                        payload: RunEventPayload::InputProvided { input_id, values },
                    },
                )?;
                Ok((NormalRunControlOutcome::InputProvided, Some(event)))
            }
            RunControlAction::Resume => {
                let (event, confirmation_id) = AgentRunRepository::resume_durable_apply(
                    db,
                    &request.session.session_key,
                    &request.run_id,
                    request.expected_state_version,
                )?;
                Ok((
                    NormalRunControlOutcome::RecoveryResumed { confirmation_id },
                    Some(event),
                ))
            }
        }
    }
}

fn validate_input_submission(
    snapshot: &AssistantRunGetResponse,
    input_id: &str,
    values: &std::collections::BTreeMap<String, String>,
) -> AppResult<()> {
    if snapshot.run.state != crate::ai_runtime::run_contract::RunState::AwaitingInput
        || input_id.trim().is_empty()
        || values.len() != 1
        || values
            .get("city")
            .is_none_or(|city| city.trim().is_empty() || city.chars().count() > 128)
    {
        return Err(AppError::run(SafeRunErrorCode::InputInvalid));
    }
    let pending = snapshot
        .run
        .pending_input
        .as_ref()
        .filter(|pending| pending.input_id == input_id && pending.kind == "location")
        .ok_or_else(|| AppError::run(SafeRunErrorCode::InputInvalid))?;
    if pending.fields != ["city".to_string()] {
        return Err(AppError::run(SafeRunErrorCode::InputInvalid));
    }
    Ok(())
}

/// Keep ordinary externally verifiable facts on the strict one-search Direct
/// route. ToolLoop is reserved for requests that explicitly ask the model to
/// conduct an investigation across multiple sources or steps.
fn requires_multi_step_research(message: &str) -> bool {
    contains_any(
        message,
        &[
            "compare sources",
            "compare and contrast",
            "investigate",
            "cross-check",
            "deep research",
            "multi-source",
            "多来源",
            "多资料",
            "对比研究",
            "交叉核验",
            "调研",
            "调查",
        ],
    )
}

fn validate_start_request(request: &AssistantRunStartRequest) -> AppResult<()> {
    if request.client_request_id.trim().is_empty()
        || request.client_request_id.chars().count() > MAX_CLIENT_REQUEST_ID_CHARS
        || request.turn.message.trim().is_empty()
        || request.turn.message.chars().count() > MAX_USER_MESSAGE_CHARS
    {
        return Err(AppError::run(SafeRunErrorCode::InvalidRequest));
    }
    if request.model_override.as_ref().is_some_and(|override_| {
        override_.provider_id.trim().is_empty() || override_.model_id.trim().is_empty()
    }) {
        return Err(AppError::run(SafeRunErrorCode::InvalidRequest));
    }
    validate_context_references(&request.turn.explicit_references)?;
    if request.security_domain == SecurityDomain::Normal && request.classified_context_ref.is_some()
    {
        return Err(AppError::run(SafeRunErrorCode::InvalidRequest));
    }
    if request.security_domain == SecurityDomain::Classified
        && (!request.turn.explicit_references.is_empty()
            || has_retrieval_scope(request)
            || !request.turn.display_mentions.is_empty()
            || request.turn.content_parts.is_some()
            || !request.external_tool_grants.is_empty())
    {
        return Err(AppError::run(SafeRunErrorCode::InvalidRequest));
    }
    crate::ai_runtime::retrieval_scope::normalize_context_scope(&request.turn.retrieval_scope)?;
    for reference in &request.turn.explicit_references {
        if let Some(path) = reference.file_path.as_deref() {
            crate::ai_runtime::retrieval_scope::normalize_note_path(path)
                .map_err(|_| AppError::run(SafeRunErrorCode::InvalidExplicitReference))?;
        }
    }
    validate_display_mentions(request)?;
    let user_disables_apply = contains_any(
        &strip_quoted_segments(&request.turn.message.to_ascii_lowercase()),
        &[
            "do not modify",
            "don't modify",
            "rewrite only",
            "\u{4e0d}\u{8981}\u{4fee}\u{6539}",
            "\u{4e0d}\u{4fee}\u{6539}",
        ],
    );
    if request.security_domain == SecurityDomain::Classified || user_disables_apply {
        Ok(())
    } else {
        validate_explicit_action(request)
    }
}

fn validate_context_references(
    references: &[crate::ai_types::ContextReferenceWire],
) -> AppResult<()> {
    let valid_hash = |hash: &str| {
        hash.len() == 64
            && hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    let invalid = references.len() > 12
        || references.iter().any(|reference| {
            reference.id.trim().is_empty()
                || reference.id.chars().count() > 160
                || reference.id.chars().any(char::is_control)
                || matches!(
                    reference.kind,
                    crate::ai_types::ContextReferenceKind::Artifact
                )
                || reference
                    .file_path
                    .as_deref()
                    .is_none_or(|value| value.trim().is_empty() || value.chars().count() > 1_024)
                || reference.excerpt.chars().count() > 512
                || reference
                    .heading_path
                    .as_ref()
                    .is_some_and(|value| value.chars().count() > 512)
                || reference
                    .anchor
                    .as_ref()
                    .is_some_and(|value| value.chars().count() > 256)
                || reference
                    .invalid_reason
                    .as_ref()
                    .is_some_and(|value| value.chars().count() > 256)
                || reference
                    .content_hash
                    .as_deref()
                    .is_none_or(|hash| !valid_hash(hash))
                || reference
                    .utf8_range
                    .as_ref()
                    .is_some_and(|range| range.start >= range.end)
                || reference
                    .editor_range
                    .as_ref()
                    .is_some_and(|range| range.from >= range.to)
        });
    if invalid {
        Err(AppError::run(SafeRunErrorCode::InvalidExplicitReference))
    } else {
        Ok(())
    }
}

fn validate_display_mentions(request: &AssistantRunStartRequest) -> AppResult<()> {
    let message_len = request.turn.message.encode_utf16().count();
    if request.turn.display_mentions.iter().any(|mention| {
        mention.label.trim().is_empty()
            || mention.value.trim().is_empty()
            || mention.range.from >= mention.range.to
            || mention.range.to > message_len
    }) {
        return Err(AppError::run(SafeRunErrorCode::InvalidRequest));
    }
    Ok(())
}

fn has_retrieval_scope(request: &AssistantRunStartRequest) -> bool {
    let scope = &request.turn.retrieval_scope;
    !scope.paths.is_empty()
        || !scope.path_prefixes.is_empty()
        || !scope.corpus_ids.is_empty()
        || !scope.required_tags.is_empty()
}

fn validate_explicit_action(request: &AssistantRunStartRequest) -> AppResult<()> {
    let Some(action) = request.explicit_action.as_ref() else {
        return Ok(());
    };
    if action.effect == Effect::Apply
        && action.target.is_none()
        && action.selection_snapshot.is_none()
    {
        return Err(AppError::run(SafeRunErrorCode::InvalidRequest));
    }
    let valid_reference = |id: &str, hash: &str| {
        request.turn.explicit_references.iter().any(|reference| {
            reference.id == id
                && reference.content_hash.as_deref() == Some(hash)
                && !reference.stale
                && reference.invalid_reason.is_none()
        })
    };
    if let Some(target) = action.target.as_ref() {
        if target.reference_id.trim().is_empty()
            || target.content_hash.trim().is_empty()
            || !valid_reference(&target.reference_id, &target.content_hash)
        {
            return Err(AppError::run(SafeRunErrorCode::InvalidRequest));
        }
    }
    if let Some(snapshot) = action.selection_snapshot.as_ref() {
        let range_matches = request.turn.explicit_references.iter().any(|reference| {
            reference.id == snapshot.reference_id
                && reference.content_hash.as_deref() == Some(snapshot.content_hash.as_str())
                && reference.utf8_range.as_ref().is_some_and(|range| {
                    range.start == snapshot.utf8_range.start && range.end == snapshot.utf8_range.end
                })
        });
        if snapshot.reference_id.trim().is_empty()
            || snapshot.content_hash.trim().is_empty()
            || snapshot.utf8_range.start >= snapshot.utf8_range.end
            || !valid_reference(&snapshot.reference_id, &snapshot.content_hash)
            || !range_matches
        {
            return Err(AppError::run(SafeRunErrorCode::InvalidRequest));
        }
        if let Some(target) = action.target.as_ref() {
            if target.reference_id != snapshot.reference_id
                || target.content_hash != snapshot.content_hash
            {
                return Err(AppError::run(SafeRunErrorCode::InvalidRequest));
            }
        }
    }
    Ok(())
}

fn resolve_existing_normal_session(
    db: &Database,
    requested: &AssistantSessionRef,
) -> AppResult<crate::ai_runtime::normal_session_repository::NormalSession> {
    match requested {
        session if session.domain != SecurityDomain::Normal => Err(AppError::run(
            SafeRunErrorCode::ClassifiedDomainNotSupported,
        )),
        session => NormalSessionRepository::get(db, &session.session_key)?
            .ok_or_else(|| AppError::run(SafeRunErrorCode::SessionNotFound)),
    }
}
/// Return whether the user explicitly requested delegated or parallel work.
/// ChildRun is intentionally opt-in at intake: a normal ToolLoop must not gain
/// an extra model invocation merely because it happens to have read tools.
fn needs_child_run(message: &str) -> bool {
    contains_any(
        message,
        &[
            "spawn subagent",
            "spawn_subagent",
            "subagent",
            "sub-agent",
            "child task",
            "child run",
            "delegate",
            "delegat",
            "parallel",
            "multi-agent",
            "子任务",
            "子 agent",
            "子agent",
            "委派",
            "分工",
            "并行",
            "交叉验证",
        ],
    )
}

fn contains_any(message: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| message.contains(marker))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WebIntentDecision {
    freshness: Freshness,
    reason: WebDecisionReason,
    verification_requirement: VerificationRequirement,
}

/// Exclusion-first Web classifier with explicit preferred/required outcomes.
struct ExclusionClassifier;

impl ExclusionClassifier {
    fn resolve(
        request: &AssistantRunStartRequest,
        _message: &str,
        directive_text: &str,
        local_only: bool,
    ) -> WebIntentDecision {
        // Hard exclusions — never overridden by an explicit web instruction.
        if request.security_domain == SecurityDomain::Classified {
            return offline(WebDecisionReason::SecurityDomainOffline);
        }
        // An explicit offline/local-only instruction is an authorization
        // boundary, not a factuality heuristic. Honour it before examining
        // Web intent so that a request such as "use local material only"
        // cannot accidentally enter a strict Web run merely because the
        // supplied material mentions an external event.
        if local_only {
            return offline(WebDecisionReason::ExplicitLocalOnly);
        }
        let explicit_web = has_explicit_web_instruction(directive_text);

        // Only trusted runtime facts bypass the Web surface.  Conversation
        // follow-ups, creative requests and local-material work are semantic
        // choices for the model, not Host-side capability revocations: a user
        // may challenge a prior factual answer or ask to verify it in any of
        // those forms.
        if !explicit_web && is_trusted_runtime_request(directive_text) {
            return offline(WebDecisionReason::TrustedRuntimeFact);
        }

        // An explicit external grant is the user's selected evidence source.
        // It must not silently expand into Web access, while finalization still
        // requires evidence from this exact Run.
        if !request.external_tool_grants.is_empty()
            && !explicit_web
            && !contains_any(directive_text, &["http://", "https://"])
        {
            return offline_requires_external(WebDecisionReason::DefaultOnline);
        }
        let strict_reason = if contains_any(directive_text, &["http://", "https://"]) {
            Some(WebDecisionReason::ExplicitUrl)
        } else if explicit_web {
            Some(WebDecisionReason::ExplicitWebRequest)
        } else if is_high_stakes_current_request(directive_text) {
            Some(WebDecisionReason::HighStakesCurrentFact)
        } else if is_volatile_external_request(directive_text) {
            Some(WebDecisionReason::VolatileExternalFact)
        } else {
            None
        };
        if let Some(reason) = strict_reason {
            return if request.web_enabled {
                required(reason)
            } else {
                offline_requires_web(WebDecisionReason::UserDisabled)
            };
        }
        if request.web_enabled {
            preferred(WebDecisionReason::DefaultOnline)
        } else {
            offline(WebDecisionReason::UserDisabled)
        }
    }
}

/// A request that rejects local notes as proof cannot enter the implicit-vault
/// shortcut merely because it names those notes; it remains a strict factual
/// request and therefore follows the normal Web decision path.
fn rejects_local_material_as_factual_source(message: &str) -> bool {
    contains_any(
        message,
        &[
            "不使用本地",
            "不使用本地材料",
            "不使用本地资料",
            "不用本地",
            "不用本地材料",
            "不得使用本地",
            "不要用本地",
            "不要使用本地",
            "不要使用本地材料",
            "不要使用本地资料",
            "do not use local",
            "don't use local",
            "without local",
        ],
    )
}

fn offline(reason: WebDecisionReason) -> WebIntentDecision {
    WebIntentDecision {
        freshness: Freshness::Offline,
        reason,
        verification_requirement: VerificationRequirement::None,
    }
}

fn offline_requires_web(reason: WebDecisionReason) -> WebIntentDecision {
    WebIntentDecision {
        freshness: Freshness::Offline,
        reason,
        verification_requirement: VerificationRequirement::CurrentRunWeb,
    }
}

fn offline_requires_external(reason: WebDecisionReason) -> WebIntentDecision {
    WebIntentDecision {
        freshness: Freshness::Offline,
        reason,
        verification_requirement: VerificationRequirement::CurrentRunExternal,
    }
}

fn required(reason: WebDecisionReason) -> WebIntentDecision {
    WebIntentDecision {
        freshness: Freshness::WebRequired,
        reason,
        verification_requirement: VerificationRequirement::CurrentRunWeb,
    }
}

fn preferred(reason: WebDecisionReason) -> WebIntentDecision {
    WebIntentDecision {
        freshness: Freshness::WebPreferred,
        reason,
        verification_requirement: VerificationRequirement::None,
    }
}

fn strip_quoted_segments(message: &str) -> String {
    let mut output = String::with_capacity(message.len());
    let mut closing_quote = None;
    let characters = message.chars().collect::<Vec<_>>();
    for (index, character) in characters.iter().copied().enumerate() {
        if let Some(expected) = closing_quote {
            if character == expected {
                closing_quote = None;
            }
            output.push(' ');
            continue;
        }
        closing_quote = match character {
            '“' => Some('”'),
            '‘' => Some('’'),
            '「' => Some('」'),
            '『' => Some('』'),
            '"' => Some('"'),
            '\'' if index == 0
                || !characters[index - 1].is_alphanumeric()
                    && characters[index + 1..].contains(&'\'') =>
            {
                Some('\'')
            }
            '`' => Some('`'),
            _ => None,
        };
        if closing_quote.is_some() {
            output.push(' ');
        } else {
            output.push(character);
        }
    }
    output
}

fn is_local_transformation_request(message: &str) -> bool {
    contains_any(
        message,
        &[
            "rewrite",
            "rephrase",
            "polish",
            "translate",
            "proofread",
            "summarize",
            "写得更",
            "礼貌",
            "校对",
            "改写",
            "润色",
            "翻译",
            "校对",
            "总结",
            "摘要",
        ],
    )
}

fn has_explicit_web_instruction(message: &str) -> bool {
    // Mentioning a previous search in an explanation question is not itself
    // an instruction to search again.  The model still receives the generic
    // WebPreferred surface when enabled and can decide to verify a disputed
    // factual claim from conversation context.
    if message.starts_with("why did ")
        || message.starts_with("why was ")
        || message.starts_with("为什么你")
        || message.starts_with("为什么刚才")
    {
        return false;
    }
    contains_any(
        message,
        &[
            "please search",
            "please browse",
            "search for",
            "browse for",
            "look up",
            "verify online",
            "browse the web",
            "search online",
            "find this online",
            "use online sources",
            "search the internet",
            "use web search",
            "on the internet",
            "请联网",
            "帮我联网",
            "请搜索",
            "帮我搜索",
            "联网查",
            "联网核实",
            "上网查证",
            "检索公开来源",
            "搜索一下",
            "检索一下",
            "查一下",
            "查找",
        ],
    )
}

fn is_trusted_runtime_request(message: &str) -> bool {
    contains_any(
        message,
        &[
            "今天是几月几日",
            "今天几月几日",
            "今天是几号",
            "今天几号",
            "当前日期",
            "本机日期",
            "现在几点",
            "当前时间",
            "本机时间",
            "应用版本",
            "iris 版本",
            "今天星期几",
            "what day of the week is it today",
            "which day of the week is it today",
            "what day is it",
            "what day of week is it",
            "what is today's weekday",
            "what is today's date",
            "current local time",
            "what is the local time",
            "what time is it locally",
            "show local date",
            "app version",
            "application version",
            "iris version",
        ],
    )
}

fn is_volatile_external_request(message: &str) -> bool {
    // Discourse about a prior turn may contain temporal words such as “just
    // now”, but it does not by itself assert a current external fact.  Keep
    // Web available as a preference; do not turn it into a strict factual
    // contract merely because the conversation is being discussed.
    if is_reflective_dialogue_request(message) {
        return false;
    }
    contains_any(
        message,
        &[
            "最新",
            "近期",
            "最近",
            "当前",
            "现在",
            "今日",
            "今天",
            "今晚",
            "本周",
            "实时",
            "现任",
            "截至",
            "当前赛",
            "今天的比赛",
            "今天比赛",
            "赛况",
            "战况",
            "比分",
            "股价",
            "价格",
            "天气",
            "新闻",
            "latest",
            "recent",
            "current",
            "today",
            "tonight",
            "this week",
            "now",
            "real-time",
            "realtime",
            "current score",
            "current price",
            "live score",
            "today's game",
            "stock price",
            "weather",
            "breaking news",
        ],
    )
}

fn is_reflective_dialogue_request(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    if lower.contains("what did you do") {
        return true;
    }
    (contains_any(
        message,
        &["刚才", "刚刚", "上一条", "之前", "你", "助手", "模型"],
    ) && contains_any(
        message,
        &["为什么", "怎么", "失败", "调用", "搜索", "工具", "报错"],
    )) || (lower.contains("previous") || lower.contains("earlier") || lower.contains("prior"))
        && (lower.contains("tool")
            || lower.contains("search")
            || lower.contains("browse")
            || lower.contains("fail")
            || lower.contains("error"))
}

fn is_high_stakes_current_request(message: &str) -> bool {
    let high_stakes = contains_any(
        message,
        &[
            "用药",
            "剂量",
            "诊断",
            "法律",
            "法规",
            "合规",
            "税务",
            "投资",
            "税",
            "签证",
            "监管",
            "medical",
            "dosage",
            "dose",
            "visa",
            "regulatory",
            "legal",
            "regulation",
            "compliance",
            "tax",
            "investment",
        ],
    );
    high_stakes
        && contains_any(
            message,
            &[
                "最新",
                "当前",
                "现行",
                "现在",
                "今天",
                "怎么做",
                "建议",
                "latest",
                "current",
                "today",
                "advice",
            ],
        )
}

/// Offline Answers without explicit `@`/`#` materials still need a tool loop when the
/// user clearly depends on vault notes; otherwise the model cannot call `read_note` /
/// `search_hybrid`. Creative, greeting, and pure rewrite paths stay Direct.
pub(crate) fn needs_offline_vault_tool_loop(
    request: &AssistantRunStartRequest,
    message: &str,
) -> bool {
    if !request.turn.explicit_references.is_empty() || has_retrieval_scope(request) {
        return false;
    }
    if request.security_domain == SecurityDomain::Classified {
        return false;
    }
    // 「只用本地资料改写/翻译」uses「本地」as an offline constraint, not a vault
    // read request. Stronger vault cues (笔记/授权/项目资料/…) still enter ToolLoop.
    if is_local_transformation_request(message) {
        return looks_like_strong_vault_dependency(message);
    }
    looks_like_local_vault_dependency(message)
}

/// Decide whether vault read/search tools may run for this Answer.
///
/// Decision table:
/// - Explicit `@`/`#` or folder/tag scope → allow (path scope enforces bounds)
/// - A request with a clear local dependency → allow the bounded vault surface
/// - Classified or no-local-dependency requests → deny
pub(crate) fn allow_implicit_vault_for_run(
    security_domain: SecurityDomain,
    user_message: &str,
    has_explicit_materials_or_scope: bool,
) -> bool {
    if has_explicit_materials_or_scope {
        return true;
    }
    if rejects_local_material_as_factual_source(user_message) {
        return false;
    }
    if security_domain == SecurityDomain::Classified {
        return false;
    }
    if is_local_transformation_request(user_message) {
        return looks_like_strong_vault_dependency(user_message);
    }
    looks_like_local_vault_dependency(user_message)
}

pub(crate) fn looks_like_local_vault_dependency(message: &str) -> bool {
    contains_any(message, &["本地"]) || looks_like_strong_vault_dependency(message)
}

/// Vault-source cues stronger than a bare offline「本地」constraint.
fn looks_like_strong_vault_dependency(message: &str) -> bool {
    contains_any(
        message,
        &[
            "笔记",
            "材料",
            "授权材料",
            "授权的材料",
            "已授权资料",
            "会议记录",
            "项目资料",
            "项目笔记",
            "note",
            "notes",
            "authorized material",
            "local project",
            "local note",
            "local material",
            "local meeting",
        ],
    ) || mentions_vault_as_material_source(message)
}

/// A bare `vault` token is not enough to request local material: it is also a
/// common part of Skill identifiers such as `vault-command-skill`. Requiring a
/// source-reading phrase preserves fail-closed implicit retrieval without
/// making a Skill name accidentally depend on the local retrieval index.
fn mentions_vault_as_material_source(message: &str) -> bool {
    contains_any(
        message,
        &[
            "vault material",
            "vault materials",
            "vault note",
            "vault notes",
            "from vault",
            "in vault",
            "read vault",
            "search vault",
            "vault 中",
            "vault里",
            "vault 的笔记",
            "vault的笔记",
            "vault 里的",
            "读取 vault",
            "搜索 vault",
        ],
    )
}
