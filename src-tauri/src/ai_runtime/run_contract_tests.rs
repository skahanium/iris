use super::run_contract::{
    transition_if_version, transition_to, AssistantRunAccepted, AssistantRunControlRequest,
    AssistantRunEvent, AssistantRunGetRequest, AssistantRunStartRequest, AssistantSessionRef,
    CapabilityId, ContextMode, Effect, Effort, EvidenceRef, EvidenceSourceKind, ExecutionEnvelope,
    Freshness, MaterialNeed, RiskClass, RunControlAction, RunEventPayload, RunEventType, RunState,
    RunStateTransitionError, SafeRunErrorCode, SecurityDomain, WebDecisionReason,
};

fn direct_answer_envelope() -> ExecutionEnvelope {
    ExecutionEnvelope {
        effect: Effect::Answer,
        context: ContextMode::Conversation,
        freshness: Freshness::Offline,
        web_reason: WebDecisionReason::LegacyUnknown,
        effort: Effort::Direct,
        security_domain: SecurityDomain::Normal,
        risk: RiskClass::ReadOnly,
        modalities: Vec::new(),
        material_needs: Vec::new(),
        required_capabilities: vec![CapabilityId::new("model.respond")],
        explicit_constraints: Vec::new(),
    }
}

#[test]
fn execution_envelope_keeps_orthogonal_execution_dimensions() {
    let envelope = direct_answer_envelope();

    assert_eq!(envelope.effect, Effect::Answer);
    assert_eq!(envelope.context, ContextMode::Conversation);
    assert_eq!(envelope.freshness, Freshness::Offline);
    assert_eq!(envelope.effort, Effort::Direct);
    assert_eq!(envelope.risk, RiskClass::ReadOnly);
    assert!(envelope.material_needs.is_empty());
}

#[test]
fn historical_web_verification_event_defaults_to_unknown_failure_reason() {
    let payload: RunEventPayload = serde_json::from_value(serde_json::json!({
        "kind": "web_verification_failed",
        "code": "agent_run_web_provider_timeout",
        "retryable": true,
        "attemptCount": 1,
        "durationBucket": "under_3s",
        "diagnosticId": "historic-run"
    }))
    .expect("historical payload remains replayable");

    assert!(matches!(
        payload,
        RunEventPayload::WebVerificationFailed {
            failure_reason: super::run_contract::WebEvidenceFailureReason::Unknown,
            ..
        }
    ));
}

#[test]
fn execution_envelope_allows_composable_material_needs() {
    let mut envelope = direct_answer_envelope();
    envelope.effect = Effect::Draft;
    envelope.effort = Effort::ToolLoop;
    envelope.material_needs = vec![MaterialNeed::Authority, MaterialNeed::Exemplar];

    assert_eq!(
        envelope.material_needs,
        vec![MaterialNeed::Authority, MaterialNeed::Exemplar]
    );
}

#[test]
fn terminal_states_cannot_be_left_and_duplicate_controls_are_idempotent() {
    assert_eq!(
        transition_to(RunState::Completed, RunState::Completed),
        Ok(RunState::Completed)
    );
    assert_eq!(
        transition_to(RunState::Completed, RunState::Running),
        Err(RunStateTransitionError::TerminalState)
    );
    assert_eq!(
        transition_to(RunState::Cancelled, RunState::Preparing),
        Err(RunStateTransitionError::TerminalState)
    );
}

#[test]
fn state_machine_allows_direct_completion_and_confirmation_resume() {
    assert_eq!(
        transition_to(RunState::Running, RunState::Completed),
        Ok(RunState::Completed)
    );
    assert_eq!(
        transition_to(RunState::Running, RunState::AwaitingConfirmation),
        Ok(RunState::AwaitingConfirmation)
    );
    assert_eq!(
        transition_to(RunState::AwaitingConfirmation, RunState::Running),
        Ok(RunState::Running)
    );
}

#[test]
fn illegal_state_transitions_return_a_stable_error() {
    assert_eq!(
        transition_to(RunState::Accepted, RunState::Completed),
        Err(RunStateTransitionError::IllegalTransition)
    );
    assert_eq!(
        transition_to(RunState::Paused, RunState::Verifying),
        Err(RunStateTransitionError::IllegalTransition)
    );
}

#[test]
fn state_machine_exhaustively_classifies_every_state_pair() {
    let states = [
        RunState::Accepted,
        RunState::Preparing,
        RunState::Running,
        RunState::AwaitingConfirmation,
        RunState::Paused,
        RunState::Verifying,
        RunState::Completed,
        RunState::Failed,
        RunState::Cancelled,
    ];

    for current in states {
        for next in states {
            let result = transition_to(current, next);
            if current == next {
                assert_eq!(result, Ok(current));
            } else if current.is_terminal() {
                assert_eq!(result, Err(RunStateTransitionError::TerminalState));
            } else if matches!(
                (current, next),
                (RunState::Accepted, RunState::Preparing)
                    | (RunState::Accepted, RunState::Cancelled)
                    | (
                        RunState::Preparing,
                        RunState::Running | RunState::Failed | RunState::Cancelled
                    )
                    | (
                        RunState::Running,
                        RunState::AwaitingConfirmation
                            | RunState::Paused
                            | RunState::Verifying
                            | RunState::Completed
                            | RunState::Failed
                            | RunState::Cancelled
                    )
                    | (RunState::AwaitingConfirmation, RunState::Running)
                    | (RunState::Paused, RunState::Running)
                    | (
                        RunState::Verifying,
                        RunState::Paused
                            | RunState::Completed
                            | RunState::Failed
                            | RunState::Cancelled
                    )
            ) {
                assert_eq!(result, Ok(next));
            } else {
                assert_eq!(result, Err(RunStateTransitionError::IllegalTransition));
            }
        }
    }
}

#[test]
fn optimistic_state_versions_reject_conflicts_and_accept_duplicate_controls_idempotently() {
    let running = transition_if_version(RunState::Running, 3, 3, RunState::Cancelled).unwrap();
    assert_eq!(running.state, RunState::Cancelled);
    assert_eq!(running.state_version, 4);

    let duplicate = transition_if_version(
        RunState::Cancelled,
        running.state_version,
        3,
        RunState::Cancelled,
    )
    .unwrap();
    assert_eq!(duplicate, running);

    assert_eq!(
        transition_if_version(RunState::Running, 4, 3, RunState::Paused),
        Err(RunStateTransitionError::StateVersionConflict)
    );
}

#[test]
fn run_events_serialize_the_shared_wire_envelope_without_internal_details() {
    let event = AssistantRunEvent::new(
        "run-1",
        1,
        0,
        RunEventType::Accepted,
        "2026-07-13T00:00:00Z",
        RunEventPayload::Accepted {
            turn_id: "turn-1".into(),
            session_key: "session-1".into(),
        },
    )
    .unwrap();

    assert_eq!(
        serde_json::to_value(event).unwrap(),
        serde_json::json!({
            "runId": "run-1",
            "seq": 1,
            "stateVersion": 0,
            "type": "accepted",
            "timestamp": "2026-07-13T00:00:00Z",
            "payload": { "kind": "accepted", "turnId": "turn-1", "sessionKey": "session-1" },
        })
    );
}

#[test]
fn execution_envelope_uses_the_same_camel_case_wire_fields_as_typescript() {
    let envelope = direct_answer_envelope();

    assert_eq!(
        serde_json::to_value(envelope).unwrap(),
        serde_json::json!({
            "effect": "answer",
            "context": "conversation",
            "freshness": "offline",
            "webReason": "legacy_unknown",
            "effort": "direct",
            "securityDomain": "normal",
            "risk": "read_only",
            "modalities": [],
            "materialNeeds": [],
            "requiredCapabilities": ["model.respond"],
            "explicitConstraints": [],
        })
    );
}

#[test]
fn historical_envelope_without_web_reason_deserializes_safely() {
    let envelope: ExecutionEnvelope = serde_json::from_value(serde_json::json!({
        "effect": "answer",
        "context": "conversation",
        "freshness": "web_required",
        "effort": "tool_loop",
        "securityDomain": "normal",
        "risk": "read_only",
        "modalities": [],
        "materialNeeds": ["web"],
        "requiredCapabilities": ["web.search"],
        "explicitConstraints": []
    }))
    .expect("legacy envelope remains readable");

    assert_eq!(envelope.web_reason, WebDecisionReason::LegacyUnknown);
}

#[test]
fn web_preferred_and_reason_use_stable_wire_values() {
    let mut envelope = direct_answer_envelope();
    envelope.freshness = Freshness::WebPreferred;
    envelope.web_reason = WebDecisionReason::GeneralQuestion;

    let json = serde_json::to_value(envelope).expect("serialize envelope");
    assert_eq!(json["freshness"], "web_preferred");
    assert_eq!(json["webReason"], "general_question");
}

#[test]
fn safe_run_errors_serialize_as_stable_agent_run_codes() {
    for code in [
        SafeRunErrorCode::InvalidRequest,
        SafeRunErrorCode::InvalidExplicitReference,
        SafeRunErrorCode::ExplicitReferenceChanged,
        SafeRunErrorCode::InvalidRetrievalScope,
        SafeRunErrorCode::LocalReferenceIndexUnavailable,
        SafeRunErrorCode::SessionNotFound,
        SafeRunErrorCode::RunNotFound,
        SafeRunErrorCode::IllegalTransition,
        SafeRunErrorCode::StateVersionConflict,
        SafeRunErrorCode::PermissionDenied,
        SafeRunErrorCode::ConfirmationExpired,
        SafeRunErrorCode::ProviderUnavailable,
        SafeRunErrorCode::ProviderTimeout,
        SafeRunErrorCode::NoCapableModel,
        SafeRunErrorCode::WebProviderUnavailable,
        SafeRunErrorCode::WebProviderTimeout,
        SafeRunErrorCode::WebProviderAuthFailed,
        SafeRunErrorCode::WebProviderFailed,
        SafeRunErrorCode::WebEvidenceInvalid,
        SafeRunErrorCode::PersistenceFailed,
        SafeRunErrorCode::Cancelled,
        SafeRunErrorCode::ClassifiedContextRequired,
        SafeRunErrorCode::ClassifiedContextExpired,
        SafeRunErrorCode::ClassifiedVaultLocked,
    ] {
        assert_eq!(
            serde_json::to_value(code).unwrap(),
            serde_json::json!(code.as_str())
        );
    }
}

#[test]
fn finalization_failures_deserialize_to_distinct_stable_safe_codes() {
    for stable_code in [
        "agent_run_empty_output",
        "agent_run_output_too_long",
        "agent_run_evidence_invalid",
        "agent_run_event_delivery_failed",
    ] {
        let code: SafeRunErrorCode = serde_json::from_value(serde_json::json!(stable_code))
            .expect("finalization failure code must remain wire-compatible");
        assert_eq!(code.as_str(), stable_code);
    }
}

#[test]
fn evidence_refs_omit_an_absent_optional_title() {
    let reference = EvidenceRef {
        evidence_id: "evidence-2".into(),
        source_kind: EvidenceSourceKind::Local,
        title: None,
        display_label: "[2] 本地资料".into(),
        stale: false,
    };

    let json = serde_json::to_value(reference).unwrap();
    assert!(json.get("title").is_none());
}

#[test]
fn run_event_rejects_mismatched_type_and_payload_at_the_rust_boundary() {
    let error = AssistantRunEvent::new(
        "run-1",
        1,
        0,
        RunEventType::ToolStarted,
        "2026-07-13T00:00:00Z",
        RunEventPayload::ContentDelta {
            delta: "must not be accepted".into(),
        },
    )
    .unwrap_err();

    assert_eq!(error, "agent_run_event_type_payload_mismatch");
    let result = serde_json::from_value::<AssistantRunEvent>(serde_json::json!({
        "runId": "run-1",
        "seq": 1,
        "stateVersion": 0,
        "type": "tool_started",
        "timestamp": "2026-07-13T00:00:00Z",
        "payload": { "kind": "content_delta", "delta": "must not be accepted" },
    }));
    assert!(result.is_err());
}

#[test]
fn run_event_exposes_its_persisted_state_version_without_serialization() {
    let event = AssistantRunEvent::new(
        "run-1",
        3,
        7,
        RunEventType::StageChanged,
        "2026-07-13T00:00:00Z",
        RunEventPayload::StageChanged {
            state: RunState::Running,
            stage: "正在生成答复".into(),
        },
    )
    .expect("valid event");

    assert_eq!(event.state_version(), 7);
}

#[test]
fn run_event_types_match_the_stable_event_contract() {
    let event_types = [
        RunEventType::Accepted,
        RunEventType::StageChanged,
        RunEventType::ContentDelta,
        RunEventType::ToolStarted,
        RunEventType::ToolCompleted,
        RunEventType::CapabilityDegraded,
        RunEventType::ConfirmationRequired,
        RunEventType::PermissionDenied,
        RunEventType::ProviderSwitched,
        RunEventType::EvidenceRegistered,
        RunEventType::Paused,
        RunEventType::Resumed,
        RunEventType::Completed,
        RunEventType::Failed,
        RunEventType::Cancelled,
    ];

    assert_eq!(event_types.len(), 15);
}

#[test]
fn web_provider_auth_failure_has_a_stable_wire_code() {
    assert_eq!(
        SafeRunErrorCode::WebProviderAuthFailed.as_str(),
        "agent_run_web_provider_auth_failed"
    );
    assert_eq!(
        serde_json::to_value(SafeRunErrorCode::WebProviderAuthFailed).unwrap(),
        serde_json::json!("agent_run_web_provider_auth_failed")
    );
}

#[test]
fn capability_degraded_is_nonterminal_and_serializes_only_safe_diagnostics() {
    let event = AssistantRunEvent::new(
        "run-1",
        4,
        2,
        RunEventType::CapabilityDegraded,
        "2026-07-15T00:00:00Z",
        RunEventPayload::CapabilityDegraded {
            capability: "web.search".into(),
            code: SafeRunErrorCode::WebProviderTimeout,
            retryable: true,
            attempt_count: 2,
            message: "联网核实暂不可用，已继续生成受约束答复。".into(),
        },
    )
    .expect("valid degradation event");

    let json = serde_json::to_value(event).expect("event JSON");
    assert_eq!(json["type"], "capability_degraded");
    assert_eq!(json["stateVersion"], 2);
    assert_eq!(json["payload"]["code"], "agent_run_web_provider_timeout");
    assert_eq!(json["payload"]["attemptCount"], 2);
    assert!(json.to_string().contains("web.search"));
    assert!(!json.to_string().contains("endpoint"));
}

#[test]
fn stage_changed_events_carry_the_exact_state_for_replay_without_guessing() {
    let payload = RunEventPayload::StageChanged {
        state: RunState::Preparing,
        stage: "正在准备上下文".into(),
    };

    assert_eq!(
        serde_json::to_value(payload).unwrap(),
        serde_json::json!({
            "kind": "stage_changed",
            "state": "preparing",
            "stage": "正在准备上下文",
        })
    );
}

#[test]
fn control_actions_and_safe_errors_never_need_legacy_request_or_task_ids() {
    let action = RunControlAction::ApproveChange {
        confirmation_id: "confirmation-1".into(),
        plan_hash: "plan-hash".into(),
    };

    assert_eq!(
        serde_json::to_value(action).unwrap(),
        serde_json::json!({
            "type": "approve_change",
            "confirmationId": "confirmation-1",
            "planHash": "plan-hash",
        })
    );
    assert_eq!(
        SafeRunErrorCode::IllegalTransition.as_str(),
        "agent_run_illegal_transition"
    );
}

#[test]
fn evidence_refs_are_stable_display_metadata_without_source_bodies() {
    let reference = EvidenceRef {
        evidence_id: "evidence-1".into(),
        source_kind: EvidenceSourceKind::Web,
        title: Some("公开来源".into()),
        display_label: "[1] 公开来源".into(),
        stale: false,
    };

    assert_eq!(
        serde_json::to_value(reference).unwrap(),
        serde_json::json!({
            "evidenceId": "evidence-1",
            "sourceKind": "web",
            "title": "公开来源",
            "displayLabel": "[1] 公开来源",
            "stale": false,
        })
    );
}

#[test]
fn run_ipc_dtos_keep_session_and_document_context_explicit() {
    let start: AssistantRunStartRequest = serde_json::from_value(serde_json::json!({
        "clientRequestId": "client-request-1",
        "session": { "domain": "normal", "sessionKey": "session-1" },
        "turn": {
            "message": "只根据明确资料回答",
            "explicitReferences": [],
            "retrievalScope": {},
            "displayMentions": []
        },
        "webEnabled": false,
        "securityDomain": "normal",
    }))
    .unwrap();

    assert_eq!(start.client_request_id, "client-request-1");
    assert_eq!(start.session.unwrap().session_key, "session-1");
    assert!(start.explicit_action.is_none());

    let accepted = AssistantRunAccepted {
        client_request_id: "client-request-1".into(),
        run_id: "run-1".into(),
        turn_id: "turn-1".into(),
        session: AssistantSessionRef {
            domain: SecurityDomain::Normal,
            session_key: "session-1".into(),
        },
        state: RunState::Accepted,
        state_version: 1,
    };
    let accepted = serde_json::to_value(accepted).unwrap();
    assert_eq!(accepted["state"], serde_json::json!("accepted"));
    assert_eq!(
        accepted["clientRequestId"],
        serde_json::json!("client-request-1")
    );
}

#[test]
fn run_start_deserializes_one_structured_turn_draft_without_legacy_top_level_fields() {
    let start: AssistantRunStartRequest = serde_json::from_value(serde_json::json!({
        "clientRequestId": "client-request-structured-turn",
        "turn": {
            "message": "分析 项目计划",
            "explicitReferences": [],
            "retrievalScope": {
                "paths": ["notes/project.md"],
                "pathPrefixes": ["notes/"],
                "corpusIds": [],
                "requiredTags": ["project"]
            },
            "displayMentions": [{
                "kind": "file",
                "value": "notes/project.md",
                "label": "项目计划",
                "range": { "from": 3, "to": 7 }
            }]
        },
        "webEnabled": false,
        "securityDomain": "normal"
    }))
    .expect("structured turn draft must be the only message input");

    let wire = serde_json::to_value(start).expect("serialize structured turn draft");
    assert_eq!(wire["turn"]["message"], "分析 项目计划");
    assert_eq!(
        wire["turn"]["retrievalScope"]["paths"][0],
        "notes/project.md"
    );
    assert_eq!(wire["turn"]["displayMentions"][0]["range"]["to"], 7);
    assert!(wire.get("message").is_none());
    assert!(wire.get("explicitReferences").is_none());
}

#[test]
fn run_control_and_get_dtos_use_only_run_and_session_identity() {
    let session = AssistantSessionRef {
        domain: SecurityDomain::Normal,
        session_key: "session-1".into(),
    };
    let control = AssistantRunControlRequest {
        session: session.clone(),
        run_id: "run-1".into(),
        expected_state_version: 4,
        action: RunControlAction::Cancel,
    };
    let get = AssistantRunGetRequest {
        session,
        run_id: Some("run-1".into()),
    };

    assert_eq!(
        serde_json::to_value(control).unwrap(),
        serde_json::json!({
            "session": { "domain": "normal", "sessionKey": "session-1" },
            "runId": "run-1",
            "expectedStateVersion": 4,
            "action": { "type": "cancel" },
        })
    );
    assert_eq!(serde_json::to_value(get).unwrap()["runId"], "run-1");
}
