//! Deterministic, scene-free Request Intake for unified Agent Runs.

use crate::ai_runtime::agent_run_repository::{
    AcceptRunInput, AgentRunRepository, FrozenConfirmationApproval, FrozenConfirmationRejection,
    RetryRunInput,
};
use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
use crate::ai_runtime::run_contract::{
    AssistantRunAccepted, AssistantRunControlRequest, AssistantRunGetResponse,
    AssistantRunRetryRequest, AssistantRunStartRequest, AssistantSessionRef, CapabilityId,
    ContextMode, Effect, Effort, ExecutionEnvelope, ExplicitConstraint, Freshness, MaterialNeed,
    Modality, RiskClass, RunControlAction, RunEventPayload, RunEventType, SecurityDomain,
    VerificationRequirement, WebDecisionReason,
};
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
        let context = if request.explicit_action.is_some() {
            ContextMode::ExplicitScope
        } else if !request.turn.explicit_references.is_empty() {
            ContextMode::ExplicitReferences
        } else if has_retrieval_scope(request) {
            ContextMode::ExplicitScope
        } else if is_novel_writing_request(&message) || request.session.is_some() {
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
            _ if freshness != Freshness::Offline
                || has_images
                || has_retrieval_scope(request)
                || child_run_requested
                || needs_offline_vault_tool_loop(request, &message) =>
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
                &message,
                !request.turn.explicit_references.is_empty() || has_retrieval_scope(request),
            ) {
                required_capabilities.push(CapabilityId::new("vault.read"));
            }
        }
        // The user-controlled Web toggle is the sole authority that can add
        // Web capability. Freshness only describes evidence obligation; it
        // must never be a second permission switch.
        if request.web_enabled && request.security_domain == SecurityDomain::Normal && !local_only {
            required_capabilities.push(CapabilityId::new("web.search"));
        }
        if child_run_requested {
            required_capabilities.push(CapabilityId::new("harness.child_run"));
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
        })
    }

    /// Atomically accept a normal-domain Run before routing or context assembly.
    pub(crate) fn start(
        db: &Database,
        mut request: AssistantRunStartRequest,
    ) -> AppResult<AssistantRunAccepted> {
        request.turn.retrieval_scope = crate::ai_runtime::retrieval_scope::normalize_context_scope(
            &request.turn.retrieval_scope,
        )?;
        for reference in &mut request.turn.explicit_references {
            if let Some(path) = reference.file_path.as_mut() {
                *path = crate::ai_runtime::retrieval_scope::normalize_note_path(path)
                    .map_err(|_| AppError::msg("agent_run_invalid_explicit_reference"))?;
            }
        }
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
                message: request.turn.message,
                content_parts: request.turn.content_parts,
                explicit_references: request.turn.explicit_references,
                context_scope: request.turn.retrieval_scope,
                display_mentions: request.turn.display_mentions,
                explicit_action: request.explicit_action,
                envelope,
            },
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

    /// Accept a fresh retry for the latest failed user turn without duplicating it.
    pub(crate) fn retry_with_sink(
        db: &Database,
        request: AssistantRunRetryRequest,
        sink: &impl crate::ai_runtime::run_engine::RunEventSink,
    ) -> AppResult<AssistantRunAccepted> {
        if request.session.domain != SecurityDomain::Normal
            || request.source_run_id.trim().is_empty()
            || request.client_request_id.trim().is_empty()
            || request.client_request_id.chars().count() > MAX_CLIENT_REQUEST_ID_CHARS
        {
            return Err(AppError::msg("agent_run_invalid_request"));
        }
        let accepted = AgentRunRepository::accept_retry(
            db,
            RetryRunInput {
                session_key: request.session.session_key,
                source_run_id: request.source_run_id,
                client_request_id: request.client_request_id,
                run_id: uuid::Uuid::new_v4().to_string(),
            },
        )?;
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

fn validate_start_request(request: &AssistantRunStartRequest) -> AppResult<()> {
    if request.client_request_id.trim().is_empty()
        || request.client_request_id.chars().count() > MAX_CLIENT_REQUEST_ID_CHARS
        || request.turn.message.trim().is_empty()
        || request.turn.message.chars().count() > MAX_USER_MESSAGE_CHARS
    {
        return Err(AppError::msg("agent_run_invalid_request"));
    }
    if request.model_override.as_ref().is_some_and(|override_| {
        override_.provider_id.trim().is_empty() || override_.model_id.trim().is_empty()
    }) {
        return Err(AppError::msg("agent_run_invalid_request"));
    }
    if request.security_domain == SecurityDomain::Normal && request.classified_context_ref.is_some()
    {
        return Err(AppError::msg("agent_run_invalid_request"));
    }
    if request.security_domain == SecurityDomain::Classified
        && (!request.turn.explicit_references.is_empty()
            || has_retrieval_scope(request)
            || !request.turn.display_mentions.is_empty()
            || request.turn.content_parts.is_some())
    {
        return Err(AppError::msg("agent_run_invalid_request"));
    }
    crate::ai_runtime::retrieval_scope::normalize_context_scope(&request.turn.retrieval_scope)?;
    for reference in &request.turn.explicit_references {
        if let Some(path) = reference.file_path.as_deref() {
            crate::ai_runtime::retrieval_scope::normalize_note_path(path)
                .map_err(|_| AppError::msg("agent_run_invalid_explicit_reference"))?;
        }
    }
    validate_display_mentions(request)?;
    let user_disables_apply = contains_any(
        &request.turn.message.to_ascii_lowercase(),
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

fn validate_display_mentions(request: &AssistantRunStartRequest) -> AppResult<()> {
    let message_len = request.turn.message.encode_utf16().count();
    if request.turn.display_mentions.iter().any(|mention| {
        mention.label.trim().is_empty()
            || mention.value.trim().is_empty()
            || mention.range.from >= mention.range.to
            || mention.range.to > message_len
    }) {
        return Err(AppError::msg("agent_run_invalid_request"));
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
        return Err(AppError::msg("agent_run_invalid_request"));
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
            return Err(AppError::msg("agent_run_invalid_request"));
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
        message: &str,
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

        // Soft exclusions. Conversation-meta must be evaluated even when the
        // message mentions past browsing ("browse the web", "联网查"), otherwise
        // those keywords false-positive as ExplicitWebRequest.
        if is_short_greeting(directive_text) {
            return offline(WebDecisionReason::ConversationMeta);
        }
        if is_conversation_meta_request(directive_text) {
            return offline(WebDecisionReason::ConversationMeta);
        }
        if !explicit_web {
            if is_trusted_runtime_request(directive_text) {
                return offline(WebDecisionReason::TrustedRuntimeFact);
            }
            if is_local_transformation_request(message)
                && is_material_bound_transformation(request, message, directive_text)
            {
                return offline(WebDecisionReason::LocalTransformation);
            }
            if is_material_bound_evidence_request(request, directive_text)
                && !is_volatile_external_request(directive_text)
                && !is_high_stakes_current_request(directive_text)
                && !requests_external_evidence(directive_text)
            {
                return offline(WebDecisionReason::LocalTransformation);
            }
            if looks_like_strong_vault_dependency(directive_text)
                && !is_volatile_external_request(directive_text)
                && !is_high_stakes_current_request(directive_text)
                && !requests_external_evidence(directive_text)
                && !rejects_local_material_as_factual_source(directive_text)
            {
                return offline(WebDecisionReason::LocalTransformation);
            }
            if is_creative_generation_request(directive_text) {
                return offline(WebDecisionReason::CreativeGeneration);
            }
        }

        // Every non-excluded factual request is strict. Do not try to infer
        // freshness from keywords such as "latest" or "World Cup": those
        // heuristics are exactly how recently completed events were missed.
        if !request.web_enabled {
            return offline_requires_web(WebDecisionReason::UserDisabled);
        }
        if contains_any(directive_text, &["http://", "https://"]) {
            return required(WebDecisionReason::ExplicitUrl);
        }
        if explicit_web {
            return required(WebDecisionReason::ExplicitWebRequest);
        }
        if is_high_stakes_current_request(directive_text) {
            return required(WebDecisionReason::HighStakesCurrentFact);
        }
        if is_volatile_external_request(directive_text) {
            return required(WebDecisionReason::VolatileExternalFact);
        }
        required(WebDecisionReason::StrictExternalFact)
    }
}

fn is_material_bound_transformation(
    request: &AssistantRunStartRequest,
    message: &str,
    directive_text: &str,
) -> bool {
    request.explicit_action.is_some()
        || !request.turn.explicit_references.is_empty()
        || has_quoted_material(message)
        || contains_any(
            directive_text,
            &[
                "provided material",
                "provided text",
                "supplied",
                "attached material",
                "attachment",
                "the text above",
                "text above",
                "this text",
                "this sentence",
                "this paragraph",
                "my draft",
                "my text",
                "provided draft",
                "我提供的材料",
                "上面的材料",
                "上面的段落",
                "上面的",
                "附件",
                "这段",
                "这句话",
                "这段话",
                "这段文字",
            ],
        )
}

/// User-provided material is authoritative for a request that explicitly asks
/// to summarize, compare, or extract only that material. This is deliberately
/// narrower than merely attaching a file: an attachment must not suppress the
/// Web requirement for a question that also seeks current external facts.
fn is_material_bound_evidence_request(
    request: &AssistantRunStartRequest,
    directive_text: &str,
) -> bool {
    !request.turn.explicit_references.is_empty()
        && contains_any(
            directive_text,
            &[
                "based on",
                "according to",
                "summarize",
                "compare",
                "extract",
                "only use",
                "仅根据",
                "根据",
                "总结",
                "概括",
                "提炼",
                "对比",
                "列出",
            ],
        )
}

/// Signals that an otherwise material-bound comparison also asks for external
/// proof. This is intentionally used only to *veto* a local-only exemption:
/// the default for anything not clearly a material transformation remains
/// strict current-Run Web verification, so an unrecognised phrasing fails
/// toward verification rather than a factual answer from local context alone.
fn requests_external_evidence(message: &str) -> bool {
    contains_any(
        message,
        &[
            "web evidence",
            "online evidence",
            "public evidence",
            "public information",
            "public status",
            "current public",
            "external evidence",
            "external source",
            "external",
            "on the web",
            "on the internet",
            "联网搜索",
            "联网检索",
            "联网核验",
            "联网查证",
            "联网查询",
            "网页证据",
            "公开证据",
            "公开信息",
            "公开状态",
            "当前公开",
            "外部证据",
            "外部来源",
            "外部",
        ],
    )
}

/// A request that rejects local notes as proof cannot enter the implicit-vault
/// shortcut merely because it names those notes; it remains a strict factual
/// request and therefore follows the normal Web decision path.
fn rejects_local_material_as_factual_source(message: &str) -> bool {
    contains_any(
        message,
        &[
            "不使用本地",
            "不用本地",
            "不得使用本地",
            "不要用本地",
            "do not use local",
            "don't use local",
            "without local",
        ],
    )
}

fn has_quoted_material(message: &str) -> bool {
    message.chars().any(|character| {
        matches!(
            character,
            '"' | '\'' | '“' | '”' | '‘' | '’' | '「' | '」' | '『' | '』' | '`'
        )
    })
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

fn required(reason: WebDecisionReason) -> WebIntentDecision {
    WebIntentDecision {
        freshness: Freshness::WebRequired,
        reason,
        verification_requirement: VerificationRequirement::CurrentRunWeb,
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
            "今天星期几",
            "今天几号",
            "当前日期",
            "本机日期",
            "现在几点",
            "当前时间",
            "本机时间",
            "应用版本",
            "iris 版本",
            "联网是否开启",
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

fn is_conversation_meta_request(message: &str) -> bool {
    if contains_any(
        message,
        &[
            "previous run",
            "tool called",
            "browsing fail",
            "what did you do",
        ],
    ) {
        return true;
    }
    let has_prior_reference = contains_any(
        message,
        &[
            "刚才",
            "刚刚",
            "上一条",
            "上一个",
            "之前",
            "previous",
            "earlier",
            "prior",
            "previous run",
        ],
    );
    let has_assistant_reference = contains_any(
        message,
        &["你", "助手", "模型", "harness", "you", "assistant"],
    );
    let has_behavior_reference = contains_any(
        message,
        &[
            "联网", "搜索", "工具", "调用", "报错", "出错", "错误", "失败", "坏掉", "罢工",
            "browse", "search", "tool", "error", "failed",
        ],
    );
    (has_prior_reference && (has_assistant_reference || has_behavior_reference))
        || (has_assistant_reference
            && has_behavior_reference
            && contains_any(
                message,
                &["为什么", "为何", "怎么", "还联网", "why", "how come"],
            ))
}

fn is_volatile_external_request(message: &str) -> bool {
    contains_any(
        message,
        &[
            "最新",
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
            | "你还在吗"
            | "早上好"
            | "晚上好"
            | "谢谢"
    )
}

fn is_creative_generation_request(message: &str) -> bool {
    contains_any(
        message,
        &[
            "write a poem",
            "write a story",
            "brainstorm",
            "fictional",
            "fantasy scene",
            "invent a character",
            "invent a",
            "short story",
            "write a song",
            "creative writing",
            "opening remarks",
            "launch opening",
            "opening line",
            "without external research",
            "do not research",
            "\u{5199}\u{4e00}\u{9996}\u{8bd7}",
            "\u{5199}\u{4e00}\u{4e2a}\u{5f00}\u{573a}\u{767d}",
            "\u{5f00}\u{573a}\u{767d}",
            "\u{5ba3}\u{4f20}\u{6587}\u{6848}",
            "\u{53e3}\u{53f7}",
            "\u{4e0d}\u{4f9d}\u{8d56}\u{5916}\u{90e8}\u{8d44}\u{6599}",
            "\u{4e0d}\u{8981}\u{68c0}\u{7d22}",
            "\u{521b}\u{4f5c}",
            "\u{865a}\u{6784}",
        ],
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
    if is_novel_writing_request(message)
        || is_short_greeting(message)
        || is_conversation_meta_request(message)
    {
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
/// - Ordinary/work task with clear local dependency → allow full vault
/// - Creative / rewrite / novel / classified / no local dependency → deny
pub(crate) fn allow_implicit_vault_for_run(
    security_domain: SecurityDomain,
    user_message: &str,
    has_explicit_materials_or_scope: bool,
) -> bool {
    if has_explicit_materials_or_scope {
        return true;
    }
    if security_domain == SecurityDomain::Classified {
        return false;
    }
    if is_novel_writing_request(user_message)
        || is_short_greeting(user_message)
        || is_conversation_meta_request(user_message)
    {
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
            "授权",
            "材料",
            "会议记录",
            "项目资料",
            "项目笔记",
            "vault",
            "note",
            "notes",
            "authorized",
            "local project",
            "local note",
            "local material",
            "local meeting",
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
