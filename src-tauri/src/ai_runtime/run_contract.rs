//! Shared, scene-free contracts for the unified Agent Run control plane.

use crate::ai_runtime::retrieval_scope::ContextScopeDto;
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
    /// Return the stable capability identifier without exposing storage internals.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
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
    /// A request explicitly depends on authorized vault knowledge but names no
    /// individual note or bounded scope. The runtime must retrieve eligible
    /// local context before any model turn.
    ImplicitVault,
    /// Only references made explicit in this Run.
    ExplicitReferences,
    /// An explicit action target or bounded scope supplied for this Run.
    ExplicitScope,
}

/// Whether a Run may use Web capabilities.
///
/// Explicit Web contract. `WebRequired` means the Run cannot produce a final
/// answer until usable Web evidence has entered the evidence ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Freshness {
    /// Web access is forbidden; `web_search` is not registered.
    Offline,
    /// Web is available to the model but local completion remains valid.
    #[serde(alias = "online")]
    WebPreferred,
    /// Web evidence is mandatory before finalization.
    WebRequired,
}

/// Evidence that must be bound to the current Run before it may state an
/// external fact. This is intentionally separate from `Freshness`: Web access
/// is an authorization decision, while verification is an answer-safety rule.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VerificationRequirement {
    /// The Run may complete without external evidence.
    #[default]
    None,
    /// A successful `web_search` in this exact Run must register usable HTTPS
    /// evidence before a final answer is accepted.
    CurrentRunWeb,
    /// A successful explicitly granted `external.read` call in this exact Run
    /// must register usable evidence before a final answer is accepted.
    CurrentRunExternal,
    /// A successful current-fact domain operation in this exact Run must
    /// register validated Appendix-D evidence before a final answer is
    /// accepted. The domain operation may be served by a frozen `web.domain.read`
    /// MCP mapping or by the generic Web evidence fallback.
    CurrentRunDomain,
}

/// Deterministic current-fact domain frozen into an accepted Run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FreshFactDomain {
    #[default]
    None,
    Runtime,
    Weather,
    News,
    Finance,
    Entertainment,
    Sports,
    GenericWeb,
}

/// Minimum location context a current-fact domain needs before Web research.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LocationRequirement {
    #[default]
    None,
    Country,
    City,
}

/// Frozen, backward-compatible current-fact policy attached to one accepted Run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FreshFactPolicy {
    pub(crate) schema_version: u8,
    pub(crate) domain: FreshFactDomain,
    pub(crate) window_start: Option<String>,
    pub(crate) window_end: Option<String>,
    pub(crate) location_requirement: LocationRequirement,
}

impl Default for FreshFactPolicy {
    fn default() -> Self {
        Self {
            schema_version: 1,
            domain: FreshFactDomain::None,
            window_start: None,
            window_end: None,
            location_requirement: LocationRequirement::None,
        }
    }
}

/// Stable explanation for the deterministic Web decision attached to a Run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WebDecisionReason {
    /// A historical envelope predates explicit Web decision reasons.
    #[default]
    LegacyUnknown,
    /// The user disabled Web access for this Run.
    UserDisabled,
    /// The security domain forbids network access.
    SecurityDomainOffline,
    /// The user explicitly required local-only execution.
    ExplicitLocalOnly,
    /// Trusted local runtime facts can answer the request.
    TrustedRuntimeFact,
    /// The request discusses this assistant, its tools, or a previous Run.
    ConversationMeta,
    /// The request transforms only supplied or authorized text.
    LocalTransformation,
    /// The request is creative and has no explicit external-fact requirement.
    CreativeGeneration,
    /// The user explicitly instructed the assistant to search or verify online.
    ExplicitWebRequest,
    /// The user supplied a URL that should be fetched through the Web boundary.
    ExplicitUrl,
    /// The answer depends on volatile external facts.
    VolatileExternalFact,
    /// Strict online verification is required for an external factual request.
    StrictExternalFact,
    /// A current medical, legal, financial, or compliance fact has elevated stakes.
    HighStakesCurrentFact,
    /// Web is available by default after exclusion checks; the model decides whether to search.
    #[serde(alias = "general_question")]
    DefaultOnline,
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

/// Frozen execution-budget profile selected once during Request Intake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RunBudgetProfile {
    /// One tool-free model turn.
    Direct,
    /// The ordinary bounded model/tool loop.
    Standard,
    /// The ordinary parent loop plus explicitly authorized depth-one ChildRuns.
    Delegated,
    /// A bounded pre-confirmation loop followed by model-free confirmed execution.
    DurableApply,
}

/// Immutable model, tool and ChildRun limits persisted with one accepted Run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RunBudgetPolicy {
    pub(crate) schema_version: u8,
    pub(crate) profile: RunBudgetProfile,
    /// Maximum prompt tokens accepted for one model turn.
    pub(crate) max_prompt_tokens: u32,
    /// Maximum completion tokens consumed across the entire Run.
    pub(crate) max_completion_tokens: u32,
    /// Maximum completion tokens accepted from one model turn.
    pub(crate) max_turn_output_tokens: u32,
    pub(crate) max_model_turns: u32,
    pub(crate) max_tool_calls: u32,
    pub(crate) max_child_runs: u32,
    pub(crate) child_max_model_turns: u32,
    pub(crate) child_max_tool_calls: u32,
    pub(crate) child_input_tokens_per_turn: u32,
    pub(crate) child_output_tokens_per_turn: u32,
    pub(crate) post_confirmation_max_model_turns: u32,
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
    /// Deterministic, content-safe explanation for the Web freshness decision.
    #[serde(default)]
    pub(crate) web_reason: WebDecisionReason,
    /// Evidence that must be available before this Run can state an external fact.
    #[serde(default)]
    pub(crate) verification_requirement: VerificationRequirement,
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
    /// Frozen current-fact policy; defaults to a no-op for historical envelopes.
    #[serde(default)]
    pub(crate) fresh_fact: FreshFactPolicy,
}

impl RunBudgetPolicy {
    /// Resolve the only supported frozen budget profile from an accepted envelope.
    pub(crate) fn for_envelope(envelope: &ExecutionEnvelope) -> Self {
        let profile = if envelope.effect == Effect::Apply && envelope.effort == Effort::Durable {
            RunBudgetProfile::DurableApply
        } else if envelope
            .required_capabilities
            .iter()
            .any(|capability| capability.as_str() == "harness.child_run")
        {
            RunBudgetProfile::Delegated
        } else if envelope.effort == Effort::Direct {
            RunBudgetProfile::Direct
        } else {
            RunBudgetProfile::Standard
        };
        Self::for_profile(profile)
    }

    /// Standard bounded policy for isolated evaluation harnesses without an accepted Run row.
    #[cfg(test)]
    pub(crate) fn standard() -> Self {
        Self::for_profile(RunBudgetProfile::Standard)
    }

    fn for_profile(profile: RunBudgetProfile) -> Self {
        match profile {
            RunBudgetProfile::Direct => Self {
                schema_version: 1,
                profile,
                max_prompt_tokens: 64_000,
                max_completion_tokens: 8_000,
                max_turn_output_tokens: 8_000,
                max_model_turns: 1,
                max_tool_calls: 0,
                max_child_runs: 0,
                child_max_model_turns: 0,
                child_max_tool_calls: 0,
                child_input_tokens_per_turn: 0,
                child_output_tokens_per_turn: 0,
                post_confirmation_max_model_turns: 0,
            },
            RunBudgetProfile::Standard => Self {
                schema_version: 1,
                profile,
                max_prompt_tokens: 128_000,
                max_completion_tokens: 16_000,
                max_turn_output_tokens: 4_000,
                max_model_turns: 8,
                max_tool_calls: 24,
                max_child_runs: 0,
                child_max_model_turns: 0,
                child_max_tool_calls: 0,
                child_input_tokens_per_turn: 0,
                child_output_tokens_per_turn: 0,
                post_confirmation_max_model_turns: 0,
            },
            RunBudgetProfile::Delegated => Self {
                schema_version: 1,
                profile,
                max_prompt_tokens: 96_000,
                max_completion_tokens: 12_000,
                max_turn_output_tokens: 4_000,
                max_model_turns: 8,
                max_tool_calls: 24,
                max_child_runs: 3,
                child_max_model_turns: 2,
                child_max_tool_calls: 6,
                child_input_tokens_per_turn: 2_000,
                child_output_tokens_per_turn: 1_024,
                post_confirmation_max_model_turns: 0,
            },
            RunBudgetProfile::DurableApply => Self {
                schema_version: 1,
                profile,
                max_prompt_tokens: 128_000,
                max_completion_tokens: 16_000,
                max_turn_output_tokens: 4_000,
                max_model_turns: 8,
                max_tool_calls: 24,
                max_child_runs: 0,
                child_max_model_turns: 0,
                child_max_tool_calls: 0,
                child_input_tokens_per_turn: 0,
                child_output_tokens_per_turn: 0,
                post_confirmation_max_model_turns: 0,
            },
        }
    }
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
    /// Legacy client field retained only for source compatibility. It is never
    /// deserialized, persisted, or trusted; the backend rereads the byte range.
    #[serde(skip)]
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

/// Explicit per-Run model choice. It is accepted only if the model still
/// satisfies every hard capability requirement at dispatch time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelOverride {
    pub(crate) provider_id: String,
    pub(crate) model_id: String,
}

/// Kind of inline display annotation attached to plain user-visible text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DisplayMentionKind {
    /// One normal-domain Markdown note.
    File,
    /// One normal-domain folder prefix.
    Folder,
    /// One indexed normal-domain tag.
    Tag,
}

/// UTF-16 code-unit range used by the browser textarea and history renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DisplayMentionRange {
    pub(crate) from: usize,
    pub(crate) to: usize,
}

/// Inline display metadata kept separate from model input and retrieval facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DisplayMention {
    pub(crate) kind: DisplayMentionKind,
    pub(crate) value: String,
    pub(crate) label: String,
    pub(crate) range: DisplayMentionRange,
}

/// Immutable, structured input for exactly one accepted assistant turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssistantTurnDraft {
    /// Plain user-visible and model-facing message text.
    pub(crate) message: String,
    /// Optional multimodal message parts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) content_parts: Option<Vec<ContentPart>>,
    /// Document references explicitly attached to this Run.
    #[serde(default)]
    pub(crate) explicit_references: Vec<ContextReferenceWire>,
    /// Hard local retrieval boundary for this Run.
    #[serde(default)]
    pub(crate) retrieval_scope: ContextScopeDto,
    /// User-visible inline annotations; never model instructions.
    #[serde(default)]
    pub(crate) display_mentions: Vec<DisplayMention>,
}

/// One explicit, normal-domain grant for a reviewed read-only MCP binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ExternalToolGrantRef {
    pub(crate) binding_id: String,
    pub(crate) binding_config_hash: String,
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
    /// Immutable structured facts for the accepted user turn.
    pub(crate) turn: AssistantTurnDraft,
    /// Editor action and snapshot explicitly supplied for this Run only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) explicit_action: Option<ExplicitAction>,
    /// User's Web toggle for this Run.
    pub(crate) web_enabled: bool,
    /// Optional provider/model override, revalidated against the Run route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) model_override: Option<ModelOverride>,
    /// Reviewed read-only MCP bindings explicitly granted for this Run only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) external_tool_grants: Vec<ExternalToolGrantRef>,
    /// Domain in which this Run must execute and persist.
    pub(crate) security_domain: SecurityDomain,
    /// Opaque, one-document classified context capability. It is required only
    /// for classified Runs and is never a filesystem path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) classified_context_ref: Option<String>,
}

/// Immediate accepted response returned by `assistant_run_start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantRunAccepted {
    /// Idempotency key whose acceptance produced this identity.
    pub(crate) client_request_id: String,
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
    /// Stable Run identifier. Omit only for a normal-domain session to recover
    /// its latest non-terminal Run after a frontend reconnect; classified
    /// sessions require a Run ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) run_id: Option<String>,
}

/// Start a fresh attempt from the latest terminal failed Run without
/// duplicating the user turn in the persisted conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantRunRetryRequest {
    /// Session that owns the failed Run.
    pub(crate) session: AssistantSessionRef,
    /// Latest terminal failed Run for the persisted user turn.
    pub(crate) source_run_id: String,
    /// Fresh idempotency key for this retry attempt.
    pub(crate) client_request_id: String,
}

/// Pending confirmation summary safe to replay after reconnecting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingConfirmationSummary {
    /// Stable confirmation identifier.
    pub(crate) confirmation_id: String,
    /// Business-facing change summary.
    pub(crate) summary: String,
    /// Safe effect category projected from the immutable change plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) effect: Option<Effect>,
    /// Bounded, normalized change targets and no raw tool arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) targets: Option<Vec<ConfirmationTargetSummary>>,
    /// RFC 3339 expiry of the immutable approval window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) expires_at: Option<String>,
}

/// Bounded target metadata shown before approving a frozen change plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfirmationTargetSummary {
    /// Broad target kind.
    pub(crate) kind: String,
    /// Normalized target identity so the user can approve the actual scope.
    pub(crate) label: String,
    /// Maximum risk class of the planned effect.
    pub(crate) risk: RiskClass,
}

/// Safe recovery classification returned by a paused Durable Apply Run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunRecoveryKind {
    /// The exact consumed plan may be revalidated and resumed without a model turn.
    ResumeAvailable,
    /// The target diverged or cannot be classified safely; automatic replay is forbidden.
    ManualReviewRequired,
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
    /// Current deterministic input request, if this Run is waiting for a user value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pending_input: Option<PendingRunInput>,
    /// Safe recovery information, if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) recovery: Option<RunRecoveryKind>,
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
    /// The Run is waiting for a bounded user-provided value before continuing.
    AwaitingInput,
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
    #[cfg(test)]
    #[error("agent_run_state_version_conflict")]
    StateVersionConflict,
}

/// Lifecycle state paired with the optimistic version stored by the Run repository.
#[cfg(test)]
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
    /// A provider explicitly supplied a safe, user-visible reasoning summary.
    ReasoningSummary,
    /// A safe streamed content fragment arrived.
    ContentDelta,
    /// A capability call started.
    ToolStarted,
    /// A capability call completed.
    ToolCompleted,
    /// A recoverable capability failure occurred without terminating the Run.
    CapabilityDegraded,
    /// Required Web verification exhausted its bounded recovery path.
    WebVerificationFailed,
    /// A frozen change plan needs user confirmation.
    ConfirmationRequired,
    /// A bounded input is required to continue this Run.
    InputRequired,
    /// The required input was supplied and this Run may resume.
    InputProvided,
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

/// Stable, locale-independent code for common Run progress stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RunStageCode {
    Preparing,
    PreparingTools,
    Recovering,
    ModelAndTools,
    GeneratingAnswer,
    ClassifiedPreparing,
    ClassifiedAnalyzing,
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
        /// Exclusion-classifier Web mode for this Run. Absent on historical events.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        freshness: Option<Freshness>,
        /// Deterministic explanation for the Web mode. Absent on historical events.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        web_reason: Option<WebDecisionReason>,
    },
    /// A short display stage.
    StageChanged {
        /// Exact lifecycle state after this transition; reducers must not infer it from text.
        state: RunState,
        /// User-visible status text without internal planning details.
        stage: String,
        /// Optional stable presentation key. Historical events contain only `stage`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stage_code: Option<RunStageCode>,
    },
    /// A bounded provider-generated summary that is safe to show and replay.
    ///
    /// This is never a raw reasoning channel or an inferred chain of thought.
    ReasoningSummary {
        /// Stable identifier for one provider model turn's summary stream.
        summary_id: String,
        /// Sanitized, user-visible summary text.
        text: String,
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
        /// Measured execution duration for faithful historical process playback.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
        /// Whether the capability completed successfully; absent on historical events.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        success: Option<bool>,
        /// Optional safe, bounded ChildRun report for durable replay.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subagent_batch_report: Option<crate::ai_runtime::subagent_coordinator::SubagentBatchReport>,
    },
    /// A recoverable capability failure that allows the Run to continue.
    CapabilityDegraded {
        /// Stable capability name.
        capability: String,
        /// Stable sanitized failure code.
        code: SafeRunErrorCode,
        /// Whether a later user retry may succeed.
        retryable: bool,
        /// Number of attempts already consumed during this Run.
        attempt_count: u32,
        /// User-safe explanation without raw provider output.
        message: String,
    },
    /// WebRequired could not obtain usable evidence after every permitted attempt.
    WebVerificationFailed {
        /// Stable sanitized failure code.
        code: SafeRunErrorCode,
        /// Structured, provider-content-free explanation for the failed evidence stage.
        #[serde(default)]
        failure_reason: WebEvidenceFailureReason,
        /// Whether retrying the same selected provider may succeed.
        retryable: bool,
        /// Total evidence attempts across the initial and recovery stages.
        attempt_count: u32,
        /// Bounded duration classification, never a raw provider diagnostic.
        duration_bucket: String,
        /// Opaque support identifier; equal to the owning Run identifier.
        diagnostic_id: String,
    },
    /// A frozen confirmation summary.
    ConfirmationRequired {
        /// Stable confirmation identifier.
        confirmation_id: String,
        /// Frozen plan hash.
        plan_hash: String,
        /// Business-facing description of the intended change.
        summary: String,
        /// Safe effect category projected from the frozen plan.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        effect: Option<Effect>,
        /// Counted and redacted change targets; never paths or arguments.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        targets: Option<Vec<ConfirmationTargetSummary>>,
        /// RFC 3339 expiry of the frozen approval window.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires_at: Option<String>,
    },
    /// A bounded user input request with no raw provider data.
    InputRequired {
        /// Stable input request identity.
        input_id: String,
        /// Input category, such as `location`.
        input_kind: String,
        /// Required bounded field names.
        fields: Vec<String>,
        /// Safe prompt shown to the user.
        prompt: String,
    },
    /// Sanitized values supplied for a previously requested input.
    InputProvided {
        /// Stable input request identity.
        input_id: String,
        /// Bounded, validated values keyed by field name.
        values: std::collections::BTreeMap<String, String>,
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
        /// Capability whose execution route changed.
        #[serde(default)]
        capability: String,
        /// Previous provider identifier, never an endpoint or credential.
        #[serde(default)]
        from_provider_id: String,
        /// Actual fallback provider identifier, never an endpoint or credential.
        provider_id: String,
        /// Actual fallback model identifier.
        #[serde(default)]
        model_id: String,
        /// Stable failure classification for the previous candidate.
        #[serde(default, alias = "reason")]
        reason_code: String,
        /// One-based fallback attempt number within this Run.
        #[serde(default)]
        attempt: u32,
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
        /// Durable Apply recovery classification, absent for historical pauses.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        recovery: Option<RunRecoveryKind>,
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
        /// Minimal source-origin category counts for the final answer.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        source_summary: Vec<crate::ai_runtime::provenance::SourceSummaryEntry>,
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

    /// Return the optimistic state version recorded by this durable event.
    pub(crate) const fn state_version(&self) -> u64 {
        self.state_version
    }

    /// Return the owning opaque Run identity for in-process event routing.
    pub(crate) fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Return this Run-local durable event sequence.
    pub(crate) const fn seq(&self) -> u64 {
        self.seq
    }

    /// Return the safe payload for in-process presentation projection.
    pub(crate) const fn payload(&self) -> &RunEventPayload {
        &self.payload
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

/// Ephemeral event kinds used only by the live presentation channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RunPresentationEventType {
    ProcessStarted,
    ProcessUpdated,
    ProcessFinished,
    AnswerDelta,
    AnswerReset,
    AnswerComplete,
}

/// Safe process item category rendered by the presentation timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PresentationProcessKind {
    Stage,
    ReasoningSummary,
    Tool,
}

/// Safe terminal visual state for one process item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PresentationProcessStatus {
    Completed,
    Failed,
}

/// Non-persisted, presentation-only payloads. They never include raw tool data,
/// provider-private reasoning, source content, credentials, or URLs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub(crate) enum RunPresentationPayload {
    ProcessStarted {
        item_id: String,
        item_kind: PresentationProcessKind,
        label: String,
    },
    ProcessUpdated {
        item_id: String,
        label: String,
    },
    ProcessFinished {
        item_id: String,
        status: PresentationProcessStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    AnswerDelta {
        delta: String,
    },
    AnswerReset,
    AnswerComplete,
}

impl RunPresentationPayload {
    const fn event_type(&self) -> RunPresentationEventType {
        match self {
            Self::ProcessStarted { .. } => RunPresentationEventType::ProcessStarted,
            Self::ProcessUpdated { .. } => RunPresentationEventType::ProcessUpdated,
            Self::ProcessFinished { .. } => RunPresentationEventType::ProcessFinished,
            Self::AnswerDelta { .. } => RunPresentationEventType::AnswerDelta,
            Self::AnswerReset => RunPresentationEventType::AnswerReset,
            Self::AnswerComplete => RunPresentationEventType::AnswerComplete,
        }
    }
}

/// Strictly ordered, non-replayable live event. The durable Run event log is
/// still authoritative after reconnect or presentation delivery failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunPresentationEvent {
    run_id: String,
    presentation_seq: u64,
    elapsed_ms: u64,
    #[serde(rename = "type")]
    event_type: RunPresentationEventType,
    payload: RunPresentationPayload,
}

impl RunPresentationEvent {
    /// Construct one safe live presentation event with matching type and payload.
    pub(crate) fn new(
        run_id: impl Into<String>,
        presentation_seq: u64,
        elapsed_ms: u64,
        payload: RunPresentationPayload,
    ) -> Result<Self, &'static str> {
        let run_id = run_id.into();
        if run_id.trim().is_empty() || presentation_seq == 0 {
            return Err("agent_run_invalid_presentation_event");
        }
        Ok(Self {
            run_id,
            presentation_seq,
            elapsed_ms,
            event_type: payload.event_type(),
            payload,
        })
    }
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
    /// Provide the bounded value requested by an `InputRequired` event.
    SubmitInput {
        /// Stable input request identity.
        input_id: String,
        /// Validated values for the requested fields.
        values: std::collections::BTreeMap<String, String>,
    },
    /// Resume a valid paused or confirmation-blocked Run.
    Resume,
    /// Cancel an active Run.
    Cancel,
}

/// A safe, replayable input request exposed while a Run is awaiting user data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRunInput {
    /// Stable input request identity.
    pub(crate) input_id: String,
    /// Input category.
    pub(crate) kind: String,
    /// Required bounded field names.
    pub(crate) fields: Vec<String>,
    /// User-facing prompt.
    pub(crate) prompt: String,
}

/// Safe, bounded reason for a Web evidence failure. These values never contain provider output,
/// request arguments, credentials, URLs, or user content.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WebEvidenceFailureReason {
    ProviderUnavailable,
    ProviderTransport,
    ProviderTimeout,
    ProviderAuthentication,
    ProviderOutputTooLarge,
    ProviderRateLimited,
    ProviderQuotaExhausted,
    ProviderInvalidArguments,
    SearchResultUnparseable,
    SearchResultNoUsableHttps,
    EvidenceContentEmpty,
    /// A query would disclose text from user-authorized local material to a Web provider.
    LocalMaterialQueryBlocked,
    #[default]
    Unknown,
}

/// Stable, safe error codes exposed across the Rust/TypeScript boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafeRunErrorCode {
    /// Input did not satisfy the Run contract.
    #[serde(rename = "agent_run_invalid_request")]
    InvalidRequest,
    /// The tool-loop budget (model turns or tool calls) was exhausted.
    #[serde(rename = "agent_run_tool_loop_limit")]
    ToolLoopLimit,
    /// Provider output contained no user-visible final answer.
    #[serde(rename = "agent_run_empty_output")]
    EmptyOutput,
    /// Provider output exceeded the bounded final-answer size.
    #[serde(rename = "agent_run_output_too_long")]
    OutputTooLong,
    /// Provider stopped before producing a complete visible answer.
    #[serde(rename = "agent_run_incomplete_output")]
    IncompleteOutput,
    /// Final evidence ownership or citation association was invalid.
    #[serde(rename = "agent_run_evidence_invalid")]
    EvidenceInvalid,
    /// A calibrated model did not complete the required structured finalization protocol.
    #[serde(rename = "agent_run_finalization_protocol_invalid")]
    FinalizationProtocolInvalid,
    /// A committed Run event could not be delivered to the active renderer.
    #[serde(rename = "agent_run_event_delivery_failed")]
    EventDeliveryFailed,
    /// An explicit local reference is missing required immutable metadata or has an invalid range.
    #[serde(rename = "agent_run_invalid_explicit_reference")]
    InvalidExplicitReference,
    /// An explicit local reference changed after the Run was accepted.
    #[serde(rename = "agent_run_explicit_reference_changed")]
    ExplicitReferenceChanged,
    /// The persisted local retrieval boundary is invalid or cannot be resolved safely.
    #[serde(rename = "agent_run_invalid_retrieval_scope")]
    InvalidRetrievalScope,
    /// A required long-reference or scoped lookup cannot be served by the local index.
    #[serde(rename = "agent_run_local_reference_index_unavailable")]
    LocalReferenceIndexUnavailable,
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
    /// The Provider did not establish or maintain a response within the Run deadline.
    #[serde(rename = "agent_run_provider_timeout")]
    ProviderTimeout,
    /// No enabled model satisfies the Run's hard requirements.
    #[serde(rename = "agent_run_no_capable_model")]
    NoCapableModel,
    /// No selected Web evidence provider can perform the requested search.
    #[serde(rename = "agent_run_mcp_unavailable")]
    WebProviderUnavailable,
    /// The selected Web evidence provider exceeded the bounded evidence-stage deadline.
    #[serde(rename = "agent_run_web_provider_timeout")]
    WebProviderTimeout,
    /// The selected Web evidence provider rejected the configured credential.
    #[serde(rename = "agent_run_web_provider_auth_failed")]
    WebProviderAuthFailed,
    /// The selected Web evidence provider failed while executing a search request.
    #[serde(rename = "agent_run_web_provider_failed")]
    WebProviderFailed,
    /// The Web evidence provider returned no safely parseable evidence.
    #[serde(rename = "agent_run_web_evidence_invalid")]
    WebEvidenceInvalid,
    /// The request needs Web verification, but the Run is not authorized to search.
    #[serde(rename = "agent_run_web_verification_required")]
    WebVerificationRequired,
    /// A required persistence operation failed safely.
    #[serde(rename = "agent_run_persistence_failed")]
    PersistenceFailed,
    /// The Run was cancelled before completion.
    #[serde(rename = "agent_run_cancelled")]
    Cancelled,
    /// No explicit current classified document was attached to this Run.
    #[serde(rename = "agent_run_classified_context_required")]
    ClassifiedContextRequired,
    /// The active classified document changed, closed, or its short-lived scope expired.
    #[serde(rename = "agent_run_classified_context_expired")]
    ClassifiedContextExpired,
    /// The classified vault was locked before the in-memory Run could complete.
    #[serde(rename = "agent_run_classified_vault_locked")]
    ClassifiedVaultLocked,
    /// The frozen change plan expired, diverged, or no longer matches the Run.
    #[serde(rename = "agent_run_invalid_change_plan")]
    InvalidChangePlan,
    /// The provider continuation state lock could not be acquired.
    #[serde(rename = "agent_run_continuation_lock_failed")]
    ContinuationLockFailed,
    /// The Run control channel is not available for the requested transition.
    #[serde(rename = "agent_run_control_not_available")]
    ControlNotAvailable,
    /// The Run already reached a terminal state.
    #[serde(rename = "agent_run_terminal_state")]
    TerminalState,
    /// A classified-domain Run was started from the normal domain (or vice versa).
    #[serde(rename = "agent_run_classified_domain_not_supported")]
    ClassifiedDomainNotSupported,
    /// The Run evidence registry lock could not be acquired.
    #[serde(rename = "agent_run_evidence_lock_failed")]
    EvidenceLockFailed,
    /// The persisted budget policy is missing or inconsistent with the envelope.
    #[serde(rename = "agent_run_invalid_budget_policy")]
    InvalidBudgetPolicy,
    /// The Run event payload failed schema validation.
    #[serde(rename = "agent_run_invalid_event")]
    InvalidEvent,
    /// The persisted local evidence reference is invalid or unresolved.
    #[serde(rename = "agent_run_local_evidence_invalid")]
    LocalEvidenceInvalid,
    /// The explicit action is not permitted for this Run.
    #[serde(rename = "agent_run_invalid_explicit_action")]
    InvalidExplicitAction,
    /// Classified history is disabled for this session.
    #[serde(rename = "agent_run_classified_history_disabled")]
    ClassifiedHistoryDisabled,
    /// The durable checkpoint stage transition conflicts with the current stage.
    #[serde(rename = "agent_run_checkpoint_stage_conflict")]
    CheckpointStageConflict,
    /// The accepted event could not be replayed for this Run.
    #[serde(rename = "agent_run_accepted_event_missing")]
    AcceptedEventMissing,
    /// The final answer submission failed structural validation.
    #[serde(rename = "agent_run_final_submission_invalid")]
    FinalSubmissionInvalid,
    /// The model attempted an effect outside the permitted write target.
    #[serde(rename = "agent_run_write_target_violation")]
    WriteTargetViolation,
    /// The persisted document policy is invalid.
    #[serde(rename = "agent_run_invalid_document_policy")]
    InvalidDocumentPolicy,
    /// The durable checkpoint payload failed schema validation.
    #[serde(rename = "agent_run_checkpoint_invalid_schema")]
    CheckpointInvalidSchema,
    /// The final answer output failed structural validation.
    #[serde(rename = "agent_run_invalid_final_output")]
    InvalidFinalOutput,
    /// The change plan is still awaiting user confirmation.
    #[serde(rename = "agent_run_confirmation_pending")]
    ConfirmationPending,
    /// The change plan confirmation is missing.
    #[serde(rename = "agent_run_confirmation_missing")]
    ConfirmationMissing,
    /// The sub-agent lifecycle transition is illegal.
    #[serde(rename = "agent_run_invalid_subagent_lifecycle")]
    InvalidSubagentLifecycle,
    /// The sub-agent batch report failed schema validation.
    #[serde(rename = "agent_run_invalid_subagent_batch_report")]
    InvalidSubagentBatchReport,
    /// The terminal Run cannot be retried.
    #[serde(rename = "agent_run_retry_not_available")]
    RetryNotAvailable,
    /// The same client request id was replayed with a different payload.
    #[serde(rename = "agent_run_idempotency_conflict")]
    IdempotencyConflict,
    /// The normal session already owns another non-terminal top-level Run.
    #[serde(rename = "agent_run_active_run_exists")]
    ActiveRunExists,
    /// The model referenced a tool call id that this Run never issued.
    #[serde(rename = "agent_run_unknown_tool_call_id")]
    UnknownToolCallId,
    /// A cited web source cannot be verified against the Run's evidence.
    #[serde(rename = "agent_run_unverified_web_citation")]
    UnverifiedWebCitation,
    /// Web evidence is required but the Run has none registered.
    #[serde(rename = "agent_run_web_evidence_required")]
    WebEvidenceRequired,
    /// The model route cannot expose the grounded finalization protocol required by a current-fact Run.
    #[serde(rename = "agent_run_grounded_finalization_unavailable")]
    GroundedFinalizationUnavailable,
    /// A current-fact Run could not collect enough evidence to support its final answer.
    #[serde(rename = "agent_run_fresh_evidence_insufficient")]
    FreshEvidenceInsufficient,
    /// A current-fact Run requires an explicit location that is not available.
    #[serde(rename = "agent_run_location_required")]
    LocationRequired,
    /// A submitted Run input does not match the pending request.
    #[serde(rename = "agent_run_input_invalid")]
    InputInvalid,
}

impl std::fmt::Display for SafeRunErrorCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl SafeRunErrorCode {
    /// Extract the stable run error code carried by an `AppError`, when its
    /// message is a known wire code; otherwise the persistence fallback is
    /// returned. This is the single typed entry point replacing inline
    /// string-round-trip deserialization at call sites.
    pub(crate) fn from_app_error(error: &crate::error::AppError) -> Self {
        match error {
            crate::error::AppError::Message(message) => {
                serde_json::from_value::<Self>(serde_json::Value::String(message.clone()))
                    .unwrap_or(Self::PersistenceFailed)
            }
            crate::error::AppError::Run(code) => *code,
            _ => Self::PersistenceFailed,
        }
    }

    /// Return the stable wire code used in safe errors and audit records.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "agent_run_invalid_request",
            Self::ToolLoopLimit => "agent_run_tool_loop_limit",
            Self::EmptyOutput => "agent_run_empty_output",
            Self::OutputTooLong => "agent_run_output_too_long",
            Self::IncompleteOutput => "agent_run_incomplete_output",
            Self::EvidenceInvalid => "agent_run_evidence_invalid",
            Self::FinalizationProtocolInvalid => "agent_run_finalization_protocol_invalid",
            Self::EventDeliveryFailed => "agent_run_event_delivery_failed",
            Self::InvalidExplicitReference => "agent_run_invalid_explicit_reference",
            Self::ExplicitReferenceChanged => "agent_run_explicit_reference_changed",
            Self::InvalidRetrievalScope => "agent_run_invalid_retrieval_scope",
            Self::LocalReferenceIndexUnavailable => "agent_run_local_reference_index_unavailable",
            Self::SessionNotFound => "agent_run_session_not_found",
            Self::RunNotFound => "agent_run_not_found",
            Self::IllegalTransition => "agent_run_illegal_transition",
            Self::StateVersionConflict => "agent_run_state_version_conflict",
            Self::PermissionDenied => "agent_run_permission_denied",
            Self::ConfirmationExpired => "agent_run_confirmation_expired",
            Self::ProviderUnavailable => "agent_run_provider_unavailable",
            Self::ProviderTimeout => "agent_run_provider_timeout",
            Self::NoCapableModel => "agent_run_no_capable_model",
            Self::WebProviderUnavailable => "agent_run_mcp_unavailable",
            Self::WebProviderTimeout => "agent_run_web_provider_timeout",
            Self::WebProviderAuthFailed => "agent_run_web_provider_auth_failed",
            Self::WebProviderFailed => "agent_run_web_provider_failed",
            Self::WebEvidenceInvalid => "agent_run_web_evidence_invalid",
            Self::WebVerificationRequired => "agent_run_web_verification_required",
            Self::PersistenceFailed => "agent_run_persistence_failed",
            Self::Cancelled => "agent_run_cancelled",
            Self::ClassifiedContextRequired => "agent_run_classified_context_required",
            Self::ClassifiedContextExpired => "agent_run_classified_context_expired",
            Self::ClassifiedVaultLocked => "agent_run_classified_vault_locked",
            Self::InvalidChangePlan => "agent_run_invalid_change_plan",
            Self::ContinuationLockFailed => "agent_run_continuation_lock_failed",
            Self::ControlNotAvailable => "agent_run_control_not_available",
            Self::TerminalState => "agent_run_terminal_state",
            Self::ClassifiedDomainNotSupported => "agent_run_classified_domain_not_supported",
            Self::EvidenceLockFailed => "agent_run_evidence_lock_failed",
            Self::InvalidBudgetPolicy => "agent_run_invalid_budget_policy",
            Self::InvalidEvent => "agent_run_invalid_event",
            Self::LocalEvidenceInvalid => "agent_run_local_evidence_invalid",
            Self::InvalidExplicitAction => "agent_run_invalid_explicit_action",
            Self::ClassifiedHistoryDisabled => "agent_run_classified_history_disabled",
            Self::CheckpointStageConflict => "agent_run_checkpoint_stage_conflict",
            Self::AcceptedEventMissing => "agent_run_accepted_event_missing",
            Self::FinalSubmissionInvalid => "agent_run_final_submission_invalid",
            Self::WriteTargetViolation => "agent_run_write_target_violation",
            Self::InvalidDocumentPolicy => "agent_run_invalid_document_policy",
            Self::CheckpointInvalidSchema => "agent_run_checkpoint_invalid_schema",
            Self::InvalidFinalOutput => "agent_run_invalid_final_output",
            Self::ConfirmationPending => "agent_run_confirmation_pending",
            Self::ConfirmationMissing => "agent_run_confirmation_missing",
            Self::InvalidSubagentLifecycle => "agent_run_invalid_subagent_lifecycle",
            Self::InvalidSubagentBatchReport => "agent_run_invalid_subagent_batch_report",
            Self::RetryNotAvailable => "agent_run_retry_not_available",
            Self::IdempotencyConflict => "agent_run_idempotency_conflict",
            Self::ActiveRunExists => "agent_run_active_run_exists",
            Self::UnknownToolCallId => "agent_run_unknown_tool_call_id",
            Self::UnverifiedWebCitation => "agent_run_unverified_web_citation",
            Self::WebEvidenceRequired => "agent_run_web_evidence_required",
            Self::GroundedFinalizationUnavailable => "agent_run_grounded_finalization_unavailable",
            Self::FreshEvidenceInsufficient => "agent_run_fresh_evidence_insufficient",
            Self::LocationRequired => "agent_run_location_required",
            Self::InputInvalid => "agent_run_input_invalid",
        }
    }
}

impl RunEventPayload {
    fn event_type(&self) -> RunEventType {
        match self {
            Self::Accepted { .. } => RunEventType::Accepted,
            Self::StageChanged { .. } => RunEventType::StageChanged,
            Self::ReasoningSummary { .. } => RunEventType::ReasoningSummary,
            Self::ContentDelta { .. } => RunEventType::ContentDelta,
            Self::ToolStarted { .. } => RunEventType::ToolStarted,
            Self::ToolCompleted { .. } => RunEventType::ToolCompleted,
            Self::CapabilityDegraded { .. } => RunEventType::CapabilityDegraded,
            Self::WebVerificationFailed { .. } => RunEventType::WebVerificationFailed,
            Self::ConfirmationRequired { .. } => RunEventType::ConfirmationRequired,
            Self::InputRequired { .. } => RunEventType::InputRequired,
            Self::InputProvided { .. } => RunEventType::InputProvided,
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
                | RunState::AwaitingInput
                | RunState::Paused
                | RunState::Verifying
                | RunState::Completed
                | RunState::Failed
                | RunState::Cancelled
        ) | (
            RunState::AwaitingConfirmation,
            RunState::Running | RunState::Cancelled
        ) | (RunState::Paused, RunState::Running)
        | (RunState::AwaitingInput, RunState::Running)
            | (
                RunState::Verifying,
                RunState::Paused | RunState::Completed | RunState::Failed | RunState::Cancelled
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
#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use super::SafeRunErrorCode;
    use crate::error::{AppError, ProviderErrorKind};

    #[test]
    fn from_app_error_maps_known_wire_codes_typed() {
        let error = AppError::msg("agent_run_tool_loop_limit");
        assert_eq!(
            SafeRunErrorCode::from_app_error(&error),
            SafeRunErrorCode::ToolLoopLimit
        );
    }

    #[test]
    fn from_app_error_falls_back_for_unknown_or_typed_errors() {
        let unknown = AppError::msg("some unclassified failure");
        assert_eq!(
            SafeRunErrorCode::from_app_error(&unknown),
            SafeRunErrorCode::PersistenceFailed
        );

        let structured = AppError::provider(ProviderErrorKind::Timeout, "upstream timeout");
        assert_eq!(
            SafeRunErrorCode::from_app_error(&structured),
            SafeRunErrorCode::PersistenceFailed
        );
    }
}
