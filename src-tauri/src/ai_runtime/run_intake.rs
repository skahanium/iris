//! Deterministic, scene-free Request Intake for unified Agent Runs.

use crate::ai_runtime::agent_run_repository::{
    AcceptRunInput, AgentRunRepository, FrozenConfirmationApproval, FrozenConfirmationRejection,
};
use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
use crate::ai_runtime::run_contract::{
    AssistantRunAccepted, AssistantRunControlRequest, AssistantRunGetResponse,
    AssistantRunStartRequest, AssistantSessionRef, CapabilityId, ContextMode, Effect, Effort,
    ExecutionEnvelope, ExplicitConstraint, Freshness, MaterialNeed, Modality, RiskClass,
    RunControlAction, RunEventPayload, RunEventType, SecurityDomain,
};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

const MAX_CLIENT_REQUEST_ID_CHARS: usize = 160;
const MAX_USER_MESSAGE_CHARS: usize = 16_000;

/// Outcome of one normal-domain control request after its durable event is written.
/// Commands use this to start post-approval execution exactly once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NormalRunControlOutcome {
    Applied,
    ConfirmationApproved,
    ConfirmationRejected,
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
        let message = request.message.to_ascii_lowercase();
        let local_only = contains_any(
            &message,
            &[
                "local only",
                "offline only",
                "do not use web",
                "\u{53ea}\u{7528}\u{672c}\u{5730}",
                "\u{4ec5}\u{7528}\u{672c}\u{5730}",
            ],
        );
        let do_not_modify = contains_any(
            &message,
            &[
                "do not modify",
                "don't modify",
                "rewrite only",
                "\u{4e0d}\u{8981}\u{4fee}\u{6539}",
                "\u{4e0d}\u{4fee}\u{6539}",
            ],
        );
        let explicit_web_instruction = has_explicit_web_instruction(&message);
        let effect = if do_not_modify {
            Effect::Answer
        } else {
            request
                .explicit_action
                .as_ref()
                .map_or(Effect::Answer, |action| action.effect)
        };
        let context = if request.explicit_action.is_some() {
            ContextMode::ExplicitScope
        } else if !request.explicit_references.is_empty() {
            ContextMode::ExplicitReferences
        } else if is_novel_writing_request(&message) {
            ContextMode::Conversation
        } else {
            ContextMode::None
        };
        let freshness = if !request.web_enabled
            || local_only
            || is_novel_writing_request(&message)
            || (is_local_transformation_request(&message) && !explicit_web_instruction)
            || is_short_greeting(&message)
        {
            Freshness::Offline
        } else {
            Freshness::WebRequired
        };
        let has_images = request.content_parts.as_ref().is_some_and(|parts| {
            parts
                .iter()
                .any(|part| matches!(part, crate::ai_types::ContentPart::ImageUrl { .. }))
        });
        let effort = match effect {
            Effect::Apply => Effort::Durable,
            _ if freshness != Freshness::Offline || has_images => Effort::ToolLoop,
            _ => Effort::Direct,
        };
        let risk = match effect {
            Effect::Apply => RiskClass::BoundedWrite,
            Effect::Answer | Effect::Draft => RiskClass::ReadOnly,
        };
        let mut material_needs = Vec::new();
        if !request.explicit_references.is_empty() {
            material_needs.push(MaterialNeed::Reference);
        }
        if is_official_writing_request(&message) {
            material_needs.push(MaterialNeed::Exemplar);
        }
        if needs_authority_material(&message) {
            material_needs.push(MaterialNeed::Authority);
        }
        if freshness != Freshness::Offline {
            material_needs.push(MaterialNeed::Web);
        }
        material_needs.sort_by_key(|need| match need {
            MaterialNeed::Exemplar => 0,
            MaterialNeed::Authority => 1,
            MaterialNeed::Reference => 2,
            MaterialNeed::Web => 3,
        });
        material_needs.dedup();
        let mut required_capabilities = vec![CapabilityId::new("model.text")];
        if has_images {
            required_capabilities.push(CapabilityId::new("model.vision"));
        }
        if freshness != Freshness::Offline {
            required_capabilities.push(CapabilityId::new("web.search"));
        }
        match effect {
            Effect::Draft => required_capabilities.push(CapabilityId::new("note.propose_patch")),
            Effect::Apply => required_capabilities.push(CapabilityId::new("note.apply_patch")),
            Effect::Answer => {}
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
        })
    }

    /// Atomically accept a normal-domain Run before routing or context assembly.
    pub(crate) fn start(
        db: &Database,
        request: AssistantRunStartRequest,
    ) -> AppResult<AssistantRunAccepted> {
        let envelope = Self::resolve_envelope(&request)?;
        if envelope.security_domain != SecurityDomain::Normal {
            return Err(AppError::msg("agent_run_classified_domain_not_supported"));
        }
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
                explicit_action: request.explicit_action,
                envelope,
            },
        )
    }

    /// Accept a classified Run in CEF only; classified execution remains direct and offline.
    pub(crate) fn start_classified(
        vault: &std::path::Path,
        request: AssistantRunStartRequest,
    ) -> AppResult<AssistantRunAccepted> {
        let envelope = Self::resolve_envelope(&request)?;
        if envelope.security_domain != SecurityDomain::Classified {
            return Err(AppError::msg("agent_run_invalid_request"));
        }
        if envelope.freshness != Freshness::Offline
            || envelope.effort != Effort::Direct
            || envelope.effect != Effect::Answer
        {
            return Err(AppError::msg("agent_run_permission_denied"));
        }
        let session_key = match request.session.as_ref() {
            Some(session) if session.domain != SecurityDomain::Classified => {
                return Err(AppError::msg("agent_run_session_not_found"))
            }
            Some(session) => Some(session.session_key.clone()),
            None => None,
        };
        let effect = serde_json::to_value(envelope.effect)?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| AppError::msg("agent_run_invalid_request"))?;
        crate::ai_runtime::classified_session::classified_run_accept(
            vault,
            crate::ai_runtime::classified_session::ClassifiedRunAcceptInput {
                client_request_id: request.client_request_id,
                session_key,
                run_id: uuid::Uuid::new_v4().to_string(),
                turn_id: uuid::Uuid::new_v4().to_string(),
                message: request.message,
                content_parts: request
                    .content_parts
                    .map(serde_json::to_value)
                    .transpose()?,
                explicit_references: request
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

    /// Read only through the owning normal-domain session reference.
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

    /// Return the latest recoverable Run for one normal-domain session.
    pub(crate) fn get_latest_active(
        db: &Database,
        session: &AssistantSessionRef,
    ) -> AppResult<Option<AssistantRunGetResponse>> {
        if session.domain != SecurityDomain::Normal {
            return Err(AppError::msg("agent_run_classified_domain_not_supported"));
        }
        AgentRunRepository::latest_active_for_session(db, &session.session_key)
    }

    /// Apply an explicit lifecycle control without legacy task state.
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
            sink.emit(&event)?;
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
            return Err(AppError::msg("agent_run_classified_domain_not_supported"));
        }
        let snapshot =
            AgentRunRepository::get_for_session(db, &request.session.session_key, &request.run_id)?
                .ok_or_else(|| AppError::msg("agent_run_not_found"))?;
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
                    FrozenConfirmationRejection::Resumed(event) => {
                        Ok((NormalRunControlOutcome::ConfirmationRejected, Some(event)))
                    }
                    FrozenConfirmationRejection::AlreadyRejected => {
                        Ok((NormalRunControlOutcome::Noop, None))
                    }
                }
            }
            RunControlAction::Resume => Err(AppError::msg("agent_run_control_not_available")),
        }
    }
}

fn validate_start_request(request: &AssistantRunStartRequest) -> AppResult<()> {
    if request.client_request_id.trim().is_empty()
        || request.client_request_id.chars().count() > MAX_CLIENT_REQUEST_ID_CHARS
        || request.message.trim().is_empty()
        || request.message.chars().count() > MAX_USER_MESSAGE_CHARS
    {
        return Err(AppError::msg("agent_run_invalid_request"));
    }
    if request.model_override.as_ref().is_some_and(|override_| {
        override_.provider_id.trim().is_empty() || override_.model_id.trim().is_empty()
    }) {
        return Err(AppError::msg("agent_run_invalid_request"));
    }
    validate_explicit_action(request)
}

fn validate_explicit_action(request: &AssistantRunStartRequest) -> AppResult<()> {
    let Some(action) = request.explicit_action.as_ref() else {
        return Ok(());
    };
    let valid_reference = |id: &str, hash: &str| {
        request.explicit_references.iter().any(|reference| {
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
            return Err(AppError::msg("agent_run_invalid_request"));
        }
    }
    if let Some(snapshot) = action.selection_snapshot.as_ref() {
        let length = snapshot
            .utf8_range
            .end
            .checked_sub(snapshot.utf8_range.start)
            .ok_or_else(|| AppError::msg("agent_run_invalid_request"))?;
        let range_matches = request.explicit_references.iter().any(|reference| {
            reference.id == snapshot.reference_id
                && reference.content_hash.as_deref() == Some(snapshot.content_hash.as_str())
                && reference.utf8_range.as_ref().is_some_and(|range| {
                    range.start == snapshot.utf8_range.start && range.end == snapshot.utf8_range.end
                })
        });
        if snapshot.reference_id.trim().is_empty()
            || snapshot.content_hash.trim().is_empty()
            || snapshot.text.is_empty()
            || snapshot.text.chars().count() > MAX_USER_MESSAGE_CHARS
            || length != snapshot.text.len()
            || !valid_reference(&snapshot.reference_id, &snapshot.content_hash)
            || !range_matches
        {
            return Err(AppError::msg("agent_run_invalid_request"));
        }
        if let Some(target) = action.target.as_ref() {
            if target.reference_id != snapshot.reference_id
                || target.content_hash != snapshot.content_hash
            {
                return Err(AppError::msg("agent_run_invalid_request"));
            }
        }
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
fn contains_any(message: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| message.contains(marker))
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
            "改写",
            "润色",
            "翻译",
            "校对",
            "续写",
        ],
    )
}

fn has_explicit_web_instruction(message: &str) -> bool {
    contains_any(
        message,
        &[
            "search",
            "browse",
            "web",
            "联网",
            "搜索",
            "检索",
            "查一下",
            "查找",
        ],
    )
}

fn is_short_greeting(message: &str) -> bool {
    let normalized = message.trim_matches(|ch: char| {
        ch.is_whitespace() || ch.is_ascii_punctuation() || "，。！？；：、（）“”‘’".contains(ch)
    });
    matches!(
        normalized,
        "hi" | "hello"
            | "hey"
            | "thanks"
            | "thank you"
            | "你好"
            | "您好"
            | "嗨"
            | "哈喽"
            | "在吗"
            | "早上好"
            | "晚上好"
            | "谢谢"
    )
}
fn is_novel_writing_request(message: &str) -> bool {
    contains_any(
        message,
        &[
            "chapter",
            "novel",
            "fiction",
            "write a story",
            "\u{5c0f}\u{8bf4}",
        ],
    )
}
fn is_official_writing_request(message: &str) -> bool {
    contains_any(message, &["memo", "brief", "official notice"])
}
fn needs_authority_material(message: &str) -> bool {
    contains_any(
        message,
        &[
            "regulation",
            "compliance",
            "policy",
            "procedure",
            "responsibility",
        ],
    )
}
