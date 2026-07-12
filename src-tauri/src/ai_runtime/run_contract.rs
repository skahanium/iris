//! Shared, scene-free contracts for the unified Agent Run control plane.

use crate::ai_types::{ContentPart, ContextReferenceWire, SourceSpan};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable capability identifier requested by an executor or the Run Engine.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct CapabilityId(String);

impl CapabilityId {
    /// Create a stable capability identifier.
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// User-visible effect the current Run may produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Effect {
    /// Answer without producing a persistent draft or changing a document.
    Answer,
    /// Produce a draft or preview without changing a document.
    Draft,
    /// Apply a confirmed document change.
    Apply,
}

/// Boundary from which the Run may assemble context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ContextMode {
    /// No contextual material beyond the current user message.
    None,
    /// Conversation history only.
    Conversation,
    /// Only references made explicit in this Run.
    ExplicitReferences,
    /// An explicit action target or bounded scope supplied for this Run.
    ExplicitScope,
}

/// Whether a Run may use Web capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Freshness {
    /// Web access is forbidden.
    Offline,
    /// Web evidence is preferred for externally verifiable facts.
    WebPreferred,
    /// Web evidence is required to substantiate the result.
    WebRequired,
}

/// Amount of coordinated work the Run may perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Effort {
    /// A single direct model invocation using already assembled context.
    Direct,
    /// A bounded loop of model and read-only capability calls.
    ToolLoop,
    /// A checkpointable, recoverable multi-step Run.
    Durable,
}

/// Physical storage and capability isolation boundary for a Run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SecurityDomain {
    /// Normal-domain data and storage.
    Normal,
    /// Classified-domain data and CEF-only storage.
    Classified,
}

/// Risk classification used by policy and confirmation decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RiskClass {
    /// Read-only work with no external effect.
    ReadOnly,
    /// A confirmed, bounded document modification.
    BoundedWrite,
    /// Destructive local modification.
    Destructive,
    /// An external or otherwise irreversible side effect.
    ExternalSideEffect,
}

/// Input/output modality needed by the Run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Modality {
    /// Text content.
    Text,
    /// Image input or output.
    Image,
}

/// The role of material a Run may request from its authorized context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MaterialNeed {
    /// A writing exemplar used only for form and style.
    Exemplar,
    /// An authority source used to constrain substantive claims.
    Authority,
    /// A supplementary reference source.
    Reference,
    /// Web evidence.
    Web,
}

/// A deterministic user or UI constraint preserved in the resolved envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ExplicitConstraint {
    /// Stable constraint category, such as `local_only` or `do_not_modify`.
    pub(crate) kind: String,
    /// Safe value needed to enforce the constraint.
    pub(crate) value: Option<String>,
}

/// The orthogonal execution boundary resolved for exactly one Agent Run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExecutionEnvelope {
    /// Result effect the Run may produce.
    pub(crate) effect: Effect,
    /// Context boundary for this Run.
    pub(crate) context: ContextMode,
    /// Web freshness requirement.
    pub(crate) freshness: Freshness,
    /// Allowed execution depth.
    pub(crate) effort: Effort,
    /// Physical security domain.
    pub(crate) security_domain: SecurityDomain,
    /// Maximum risk class requested by the Run.
    pub(crate) risk: RiskClass,
    /// Required modalities.
    pub(crate) modalities: Vec<Modality>,
    /// Authorized material roles that may be planned together.
    pub(crate) material_needs: Vec<MaterialNeed>,
    /// Stable capabilities required to execute the Run.
    pub(crate) required_capabilities: Vec<CapabilityId>,
    /// Explicit constraints that remain binding throughout the Run.
    pub(crate) explicit_constraints: Vec<ExplicitConstraint>,
}

/// Origin category of a registered evidence item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvidenceSourceKind {
    /// Evidence read from an authorized local vault resource.
    Local,
    /// Evidence fetched through a permitted Web capability.
    Web,
}

/// A safe, stable evidence reference shared with messages, Runs and the UI.
///
/// The evidence ledger owns source locations, hashes and bounded Web excerpts.
/// This DTO intentionally contains no source body or raw tool output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EvidenceRef {
    /// Stable evidence-ledger identifier.
    pub(crate) evidence_id: String,
    /// Origin category used for safe presentation.
    pub(crate) source_kind: EvidenceSourceKind,
    /// Optional safe display title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) title: Option<String>,
    /// Session-local citation label and safe source name for display.
    pub(crate) display_label: String,
    /// Whether source validation detected a changed local resource.
    pub(crate) stale: bool,
}

/// Opaque session identity that keeps normal and classified storage separate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSessionRef {
    /// Declared physical storage and capability domain.
    pub(crate) domain: SecurityDomain,
    /// Domain-local opaque session key; never a SQLite primary key.
    pub(crate) session_key: String,
}

/// Explicit target selected by an editor action for exactly one Run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExplicitTarget {
    /// Stable explicit reference identifier.
    pub(crate) reference_id: String,
    /// Hash of the target content at action creation time.
    pub(crate) content_hash: String,
}

/// Immutable selection snapshot supplied by an explicit editor action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelectionSnapshot {
    /// Stable explicit reference identifier.
    pub(crate) reference_id: String,
    /// Hash of the document content at snapshot creation time.
    pub(crate) content_hash: String,
    /// UTF-8 byte range of the supplied snapshot.
    pub(crate) utf8_range: SourceSpan,
    /// Explicitly supplied selection text used only by this Run.
    pub(crate) text: String,
}

/// One explicit editor action that is scoped to a single Run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExplicitAction {
    /// Requested effect for this one action.
    pub(crate) effect: Effect,
    /// Optional explicit target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<ExplicitTarget>,
    /// Optional immutable selected-text snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) selection_snapshot: Option<SelectionSnapshot>,
}

/// Request accepted by `assistant_run_start`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantRunStartRequest {
    /// Idempotency key supplied by the client.
    pub(crate) client_request_id: String,
    /// Existing session to continue, when selected explicitly by the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) session: Option<AssistantSessionRef>,
    /// Current user message.
    pub(crate) message: String,
    /// Optional multimodal message parts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) content_parts: Option<Vec<ContentPart>>,
    /// Document references explicitly attached to this Run.
    pub(crate) explicit_references: Vec<ContextReferenceWire>,
    /// Editor action and snapshot explicitly supplied for this Run only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) explicit_action: Option<ExplicitAction>,
    /// User's Web toggle for this Run.
    pub(crate) web_enabled: bool,
    /// Domain in which this Run must execute and persist.
    pub(crate) security_domain: SecurityDomain,
}

/// Immediate accepted response returned by `assistant_run_start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantRunAccepted {
    /// Stable Run identifier.
    pub(crate) run_id: String,
    /// Stable logical turn identifier.
    pub(crate) turn_id: String,
    /// Opaque session reference resolved or created by Request Intake.
    pub(crate) session: AssistantSessionRef,
    /// Accepted initial state.
    pub(crate) state: RunState,
    /// Initial optimistic state version.
    pub(crate) state_version: u64,
}

/// Control request accepted by `assistant_run_control`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantRunControlRequest {
    /// Session that owns the Run.
    pub(crate) session: AssistantSessionRef,
    /// Stable Run identifier.
    pub(crate) run_id: String,
    /// Optimistic version observed by the client.
    pub(crate) expected_state_version: u64,
    /// Idempotent action requested by the user.
    pub(crate) action: RunControlAction,
}

/// Lookup request accepted by `assistant_run_get`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantRunGetRequest {
    /// Session that owns the Run.
    pub(crate) session: AssistantSessionRef,
    /// Stable Run identifier.
    pub(crate) run_id: String,
}

/// Pending confirmation summary safe to replay after reconnecting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingConfirmationSummary {
    /// Stable confirmation identifier.
    pub(crate) confirmation_id: String,
    /// Business-facing change summary.
    pub(crate) summary: String,
}

/// Safe recovery information returned by a Run snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeRunRecovery {
    /// Stable safe error code.
    pub(crate) code: SafeRunErrorCode,
    /// User-safe recovery message.
    pub(crate) message: String,
}

/// Safe persisted state returned by `assistant_run_get`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantRunSnapshot {
    /// Stable Run identifier.
    pub(crate) run_id: String,
    /// Stable logical turn identifier.
    pub(crate) turn_id: String,
    /// Owning opaque session reference.
    pub(crate) session: AssistantSessionRef,
    /// Current lifecycle state.
    pub(crate) state: RunState,
    /// Current optimistic state version.
    pub(crate) state_version: u64,
    /// Persisted final assistant message identifier, if terminal output exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) final_message_id: Option<String>,
    /// Current confirmation summary, if the Run is waiting for one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pending_confirmation: Option<PendingConfirmationSummary>,
    /// Safe recovery information, if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) recovery: Option<SafeRunRecovery>,
}

/// Snapshot plus persisted events returned by `assistant_run_get`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantRunGetResponse {
    /// Current safe Run snapshot.
    pub(crate) run: AssistantRunSnapshot,
    /// Persisted ordered events available for replay.
    pub(crate) events: Vec<AssistantRunEvent>,
}

/// Unified lifecycle state of an Agent Run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RunState {
    /// Request Intake atomically accepted the request.
    Accepted,
    /// The Run is resolving its policy, context and route.
    Preparing,
    /// The Run is dispatching model or capability work.
    Running,
    /// The Run is waiting for a user confirmation.
    AwaitingConfirmation,
    /// The Run is durably paused and may later resume.
    Paused,
    /// The Run is validating an output before completion.
    Verifying,
    /// The Run completed successfully.
    Completed,
    /// The Run reached a safe failure terminal state.
    Failed,
    /// The Run was cancelled.
    Cancelled,
}

impl RunState {
    /// Return whether no further lifecycle transition is permitted.
    pub(crate) fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// Stable errors returned for an invalid Run lifecycle transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub(crate) enum RunStateTransitionError {
    /// A terminal state cannot transition to a distinct state.
    #[error("agent_run_terminal_state")]
    TerminalState,
    /// The requested state is not a legal successor.
    #[error("agent_run_illegal_transition")]
    IllegalTransition,
    /// The client attempted a control action against a stale state version.
    #[error("agent_run_state_version_conflict")]
    StateVersionConflict,
}

/// Lifecycle state paired with the optimistic version stored by the Run repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VersionedRunState {
    /// Current lifecycle state.
    pub(crate) state: RunState,
    /// Version incremented only when the lifecycle state changes.
    pub(crate) state_version: u64,
}

/// Stable event kinds emitted by the unified Run Engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RunEventType {
    /// Request Intake accepted the Run.
    Accepted,
    /// A user-visible execution stage changed.
    StageChanged,
    /// A safe streamed content fragment arrived.
    ContentDelta,
    /// A capability call started.
    ToolStarted,
    /// A capability call completed.
    ToolCompleted,
    /// A frozen change plan needs user confirmation.
    ConfirmationRequired,
    /// Policy denied an action.
    PermissionDenied,
    /// The Provider Router selected a permitted fallback candidate.
    ProviderSwitched,
    /// Evidence was registered for later citation.
    EvidenceRegistered,
    /// A durable Run paused.
    Paused,
    /// A paused Run resumed.
    Resumed,
    /// The Run completed successfully.
    Completed,
    /// The Run reached a safe failure terminal state.
    Failed,
    /// The Run was cancelled.
    Cancelled,
}

/// Safe, UI-oriented payloads carried by a Run event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub(crate) enum RunEventPayload {
    /// Accepted identity facts that allow the UI to associate this Run with a turn.
    Accepted {
        /// Logical turn identifier.
        turn_id: String,
        /// Opaque session key.
        session_key: String,
    },
    /// A short display stage.
    StageChanged {
        /// Exact lifecycle state after this transition; reducers must not infer it from text.
        state: RunState,
        /// User-visible status text without internal planning details.
        stage: String,
    },
    /// A safely buffered visible content fragment.
    ContentDelta {
        /// Streamed response content.
        delta: String,
    },
    /// A capability started using stable identifiers only.
    ToolStarted {
        /// Stable capability name.
        capability: String,
        /// Provider tool-call identifier unique within the Run.
        tool_call_id: String,
    },
    /// A capability completed with a safe summary.
    ToolCompleted {
        /// Stable capability name.
        capability: String,
        /// Provider tool-call identifier unique within the Run.
        tool_call_id: String,
        /// User-safe completion summary.
        summary: String,
    },
    /// A frozen confirmation summary.
    ConfirmationRequired {
        /// Stable confirmation identifier.
        confirmation_id: String,
        /// Frozen plan hash.
        plan_hash: String,
        /// Business-facing description of the intended change.
        summary: String,
    },
    /// A safe policy denial.
    PermissionDenied {
        /// Stable denial code.
        code: SafeRunErrorCode,
        /// User-safe denial explanation.
        message: String,
    },
    /// A safe Provider fallback summary.
    ProviderSwitched {
        /// Actual provider identifier, never an endpoint or credential.
        provider_id: String,
        /// Stable failure classification for the previous candidate.
        reason: String,
    },
    /// Evidence registration metadata.
    EvidenceRegistered {
        /// Stable evidence identifier.
        evidence_id: String,
    },
    /// A pause summary.
    Paused {
        /// User-safe reason for pausing.
        reason: String,
    },
    /// A resume summary.
    Resumed {
        /// User-safe reason for resuming.
        reason: String,
    },
    /// Completion metadata.
    Completed {
        /// Stable final assistant message identifier when one was persisted.
        message_id: Option<String>,
    },
    /// Safe terminal failure metadata.
    Failed {
        /// Stable failure code.
        code: SafeRunErrorCode,
        /// User-safe recovery text.
        message: String,
    },
    /// Safe cancellation metadata.
    Cancelled {
        /// User-safe cancellation reason.
        reason: String,
    },
}

/// Persisted, ordered and replayable event emitted for an Agent Run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AssistantRunEvent {
    /// Stable Run identifier.
    run_id: String,
    /// Strictly increasing sequence number within the Run.
    seq: u64,
    /// Optimistic-concurrency version after this event.
    state_version: u64,
    /// Stable event kind.
    event_type: RunEventType,
    /// RFC 3339 event timestamp.
    timestamp: String,
    /// Safe UI payload.
    payload: RunEventPayload,
}

impl AssistantRunEvent {
    /// Build a validated event whose outer type matches its payload discriminator.
    pub(crate) fn new(
        run_id: impl Into<String>,
        seq: u64,
        state_version: u64,
        event_type: RunEventType,
        timestamp: impl Into<String>,
        payload: RunEventPayload,
    ) -> Result<Self, &'static str> {
        if event_type != payload.event_type() {
            return Err("agent_run_event_type_payload_mismatch");
        }
        Ok(Self {
            run_id: run_id.into(),
            seq,
            state_version,
            event_type,
            timestamp: timestamp.into(),
            payload,
        })
    }
}

impl Serialize for AssistantRunEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if self.event_type != self.payload.event_type() {
            return Err(serde::ser::Error::custom(
                "agent_run_event_type_payload_mismatch",
            ));
        }
        AssistantRunEventWireRef {
            run_id: &self.run_id,
            seq: self.seq,
            state_version: self.state_version,
            event_type: self.event_type,
            timestamp: &self.timestamp,
            payload: &self.payload,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AssistantRunEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = AssistantRunEventWire::deserialize(deserializer)?;
        Self::new(
            wire.run_id,
            wire.seq,
            wire.state_version,
            wire.event_type,
            wire.timestamp,
            wire.payload,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AssistantRunEventWireRef<'a> {
    run_id: &'a str,
    seq: u64,
    state_version: u64,
    #[serde(rename = "type")]
    event_type: RunEventType,
    timestamp: &'a str,
    payload: &'a RunEventPayload,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssistantRunEventWire {
    run_id: String,
    seq: u64,
    state_version: u64,
    #[serde(rename = "type")]
    event_type: RunEventType,
    timestamp: String,
    payload: RunEventPayload,
}

/// A user control request that may advance an Agent Run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RunControlAction {
    /// Approve one unchanged, unexpired change plan.
    ApproveChange {
        /// Stable confirmation identifier.
        confirmation_id: String,
        /// Hash of the plan shown to the user.
        plan_hash: String,
    },
    /// Reject one pending change plan.
    RejectChange {
        /// Stable confirmation identifier.
        confirmation_id: String,
    },
    /// Resume a valid paused or confirmation-blocked Run.
    Resume,
    /// Cancel an active Run.
    Cancel,
}

/// Stable, safe error codes exposed across the Rust/TypeScript boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SafeRunErrorCode {
    /// Input did not satisfy the Run contract.
    #[serde(rename = "agent_run_invalid_request")]
    InvalidRequest,
    /// The opaque session reference was not found in its declared domain.
    #[serde(rename = "agent_run_session_not_found")]
    SessionNotFound,
    /// The requested Run was not found for the supplied session.
    #[serde(rename = "agent_run_not_found")]
    RunNotFound,
    /// A requested state transition is illegal.
    #[serde(rename = "agent_run_illegal_transition")]
    IllegalTransition,
    /// The control request's state version is stale.
    #[serde(rename = "agent_run_state_version_conflict")]
    StateVersionConflict,
    /// Policy denied an attempted effect or capability.
    #[serde(rename = "agent_run_permission_denied")]
    PermissionDenied,
    /// The pending change plan expired or no longer matches.
    #[serde(rename = "agent_run_confirmation_expired")]
    ConfirmationExpired,
    /// No suitable Provider can complete the permitted route.
    #[serde(rename = "agent_run_provider_unavailable")]
    ProviderUnavailable,
    /// A required persistence operation failed safely.
    #[serde(rename = "agent_run_persistence_failed")]
    PersistenceFailed,
    /// The Run was cancelled before completion.
    #[serde(rename = "agent_run_cancelled")]
    Cancelled,
}

impl SafeRunErrorCode {
    /// Return the stable wire code used in safe errors and audit records.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "agent_run_invalid_request",
            Self::SessionNotFound => "agent_run_session_not_found",
            Self::RunNotFound => "agent_run_not_found",
            Self::IllegalTransition => "agent_run_illegal_transition",
            Self::StateVersionConflict => "agent_run_state_version_conflict",
            Self::PermissionDenied => "agent_run_permission_denied",
            Self::ConfirmationExpired => "agent_run_confirmation_expired",
            Self::ProviderUnavailable => "agent_run_provider_unavailable",
            Self::PersistenceFailed => "agent_run_persistence_failed",
            Self::Cancelled => "agent_run_cancelled",
        }
    }
}

impl RunEventPayload {
    fn event_type(&self) -> RunEventType {
        match self {
            Self::Accepted { .. } => RunEventType::Accepted,
            Self::StageChanged { .. } => RunEventType::StageChanged,
            Self::ContentDelta { .. } => RunEventType::ContentDelta,
            Self::ToolStarted { .. } => RunEventType::ToolStarted,
            Self::ToolCompleted { .. } => RunEventType::ToolCompleted,
            Self::ConfirmationRequired { .. } => RunEventType::ConfirmationRequired,
            Self::PermissionDenied { .. } => RunEventType::PermissionDenied,
            Self::ProviderSwitched { .. } => RunEventType::ProviderSwitched,
            Self::EvidenceRegistered { .. } => RunEventType::EvidenceRegistered,
            Self::Paused { .. } => RunEventType::Paused,
            Self::Resumed { .. } => RunEventType::Resumed,
            Self::Completed { .. } => RunEventType::Completed,
            Self::Failed { .. } => RunEventType::Failed,
            Self::Cancelled { .. } => RunEventType::Cancelled,
        }
    }
}

/// Validate and return the next lifecycle state.
///
/// Repeating a control request for the current state is idempotent. A direct
/// answer may complete from `running` without entering `verifying`, because
/// verification is optional for low-risk work.
pub(crate) fn transition_to(
    current: RunState,
    next: RunState,
) -> Result<RunState, RunStateTransitionError> {
    if current == next {
        return Ok(current);
    }
    if current.is_terminal() {
        return Err(RunStateTransitionError::TerminalState);
    }

    let allowed = matches!(
        (current, next),
        (
            RunState::Accepted,
            RunState::Preparing | RunState::Cancelled
        ) | (
            RunState::Preparing,
            RunState::Running | RunState::Failed | RunState::Cancelled
        ) | (
            RunState::Running,
            RunState::AwaitingConfirmation
                | RunState::Paused
                | RunState::Verifying
                | RunState::Completed
                | RunState::Failed
                | RunState::Cancelled
        ) | (RunState::AwaitingConfirmation, RunState::Running)
            | (RunState::Paused, RunState::Running)
            | (
                RunState::Verifying,
                RunState::Completed | RunState::Failed | RunState::Cancelled
            )
    );

    if allowed {
        Ok(next)
    } else {
        Err(RunStateTransitionError::IllegalTransition)
    }
}

/// Validate an optimistic state version and apply one idempotent state transition.
///
/// When a repeated control request carries an older version but asks for the
/// already-observed state, it is treated as a successful no-op. Any other
/// stale or future version is rejected with a stable conflict error.
pub(crate) fn transition_if_version(
    current: RunState,
    state_version: u64,
    expected_state_version: u64,
    next: RunState,
) -> Result<VersionedRunState, RunStateTransitionError> {
    if expected_state_version != state_version {
        if expected_state_version < state_version && current == next {
            return Ok(VersionedRunState {
                state: current,
                state_version,
            });
        }
        return Err(RunStateTransitionError::StateVersionConflict);
    }

    let state = transition_to(current, next)?;
    Ok(VersionedRunState {
        state,
        state_version: if state == current {
            state_version
        } else {
            state_version + 1
        },
    })
}
