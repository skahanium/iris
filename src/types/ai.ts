/** Frontend contracts for the unified, scene-free Agent Run control plane. */

export type AgentPermissionAtom =
  | "vault.read"
  | "vault.search"
  | "vault.write.patch"
  | "vault.create_note"
  | "vault.rename_move"
  | "vault.delete_to_trash"
  | "vault.assets.read"
  | "vault.assets.write"
  | "vault.versioning"
  | "fs.pick_file"
  | "fs.pick_folder"
  | "fs.import_to_vault"
  | "fs.export"
  | "fs.read_authorized_folder"
  | "fs.write_authorized_export"
  | "doc.convert"
  | "doc.ocr"
  | "doc.extract_pdf"
  | "doc.extract_table"
  | "doc.normalize_markdown"
  | "doc.fix_links"
  | "doc.extract_citations"
  | "web.search"
  | "web.fetch"
  | "web.to_markdown"
  | "web.download_to_assets"
  | "web.citation_extract"
  | "net.localhost"
  | "process.run_markdown_tool"
  | "process.run_readonly"
  | "process.run_mutating"
  | "process.run_network"
  | "process.long_running"
  | "process.kill_owned"
  | "git.read_status"
  | "git.read_diff"
  | "git.read_log"
  | "git.write_commit"
  | "clipboard.write"
  | "clipboard.read"
  | "browser.read_page"
  | "browser.screenshot"
  | "browser.control_page"
  | "secret.exists"
  | "secret.create_update"
  | "secret.read_plaintext"
  | "app_state.read"
  | "app_state.write";

export type PermissionRiskLevel = "low" | "medium" | "high" | "critical";
export type PermissionScopeKind =
  | "request"
  | "session"
  | "vault"
  | "folder"
  | "skill"
  | "global";
export type PermissionDecision =
  | "allow"
  | "allow_once"
  | "allow_for_session"
  | "deny_once"
  | "deny_always_for_this_skill"
  | "open_settings";

export interface PermissionEffectSummary {
  permissionName: AgentPermissionAtom;
  scopeKind: PermissionScopeKind;
  scopeSummary: string;
  riskLevel: PermissionRiskLevel;
  reversibleBy: string;
  blockedReason?: string | null;
}

/** UTF-8 byte offsets into a Markdown source string. */
export interface SourceSpan {
  start: number;
  end: number;
}

/** Explicit user-authorized reference supplied to exactly one Run. */
export interface ContextReference {
  id: string;
  kind: "selection" | "paragraph" | "heading" | "note";
  filePath: string | null;
  contentHash: string | null;
  utf8Range: SourceSpan | null;
  editorRange: { from: number; to: number } | null;
  excerpt: string;
  headingPath?: string | null;
  anchor?: string | null;
  stale: boolean;
  invalidReason?: string | null;
}

export type SecurityDomain = "normal" | "classified";
/** Backward-compatible UI name for the same security boundary, not a scene. */
export type AiDomain = SecurityDomain;
export type Effect = "answer" | "draft" | "apply";
export type ContextMode =
  | "none"
  | "conversation"
  | "explicit_references"
  | "explicit_scope";
export type Freshness = "offline" | "web_preferred" | "web_required";
export type WebDecisionReason =
  | "legacy_unknown"
  | "user_disabled"
  | "security_domain_offline"
  | "explicit_local_only"
  | "trusted_runtime_fact"
  | "conversation_meta"
  | "local_transformation"
  | "creative_generation"
  | "explicit_web_request"
  | "explicit_url"
  | "volatile_external_fact"
  | "high_stakes_current_fact"
  | "general_question";
export type Effort = "direct" | "tool_loop" | "durable";
/** An explicit, one-Run model choice. The backend must reject an incapable override. */
export interface AgentModelOverride {
  providerId: string;
  modelId: string;
}

export type RiskClass =
  | "read_only"
  | "bounded_write"
  | "destructive"
  | "external_side_effect";
export type Modality = "text" | "image";
export type MaterialNeed = "exemplar" | "authority" | "reference" | "web";
export type CapabilityId = string;

export interface ExplicitConstraint {
  kind: string;
  value: string | null;
}

export interface ExecutionEnvelope {
  effect: Effect;
  context: ContextMode;
  freshness: Freshness;
  webReason: WebDecisionReason;
  effort: Effort;
  securityDomain: SecurityDomain;
  risk: RiskClass;
  modalities: Modality[];
  materialNeeds: MaterialNeed[];
  requiredCapabilities: CapabilityId[];
  explicitConstraints: ExplicitConstraint[];
}

export interface AssistantSessionRef {
  domain: SecurityDomain;
  sessionKey: string;
}

export interface AssistantSessionListRequest {
  domain: SecurityDomain;
  limit?: number;
  offset?: number;
}

export interface AssistantSessionSummary {
  session: AssistantSessionRef;
  title: string;
  messageCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface AssistantSessionMessage {
  seq: number;
  role: string;
  content: string;
  contentParts?: unknown;
  toolCalls?: unknown;
  explicitReferences: unknown[];
  contextScope: ContextScope | [];
  displayMentions: DisplayMention[];
  createdAt: string;
}

export interface AssistantSessionLoadRequest {
  session: AssistantSessionRef;
  limit?: number;
}

export interface AssistantSessionRenameRequest {
  session: AssistantSessionRef;
  title: string;
}

export interface AssistantSessionRetractRequest {
  session: AssistantSessionRef;
  fromSeq: number;
}

export interface ExplicitTarget {
  referenceId: string;
  contentHash: string;
}

export interface SelectionSnapshot {
  referenceId: string;
  contentHash: string;
  utf8Range: SourceSpan;
}

/** Safe ledger projection: never includes source body, location, or prompts. */
export interface EvidenceRef {
  evidenceId: string;
  sourceKind: "local" | "web";
  displayLabel: string;
  title?: string;
  stale: boolean;
}

export type ContentPart =
  | { type: "text"; text: string }
  | {
      type: "image_url";
      image_url: { url: string; detail?: "auto" | "low" | "high" };
    };

export type DisplayMentionKind = "file" | "folder" | "tag";

/** UTF-16 code-unit range into the plain user-visible message. */
export interface DisplayMentionRange {
  from: number;
  to: number;
}

/** Inline presentation metadata kept separate from model and retrieval input. */
export interface DisplayMention {
  kind: DisplayMentionKind;
  value: string;
  label: string;
  range: DisplayMentionRange;
}

/** Immutable structured input for one assistant turn. */
export interface AssistantTurnDraft {
  message: string;
  contentParts?: ContentPart[];
  explicitReferences: ContextReference[];
  retrievalScope: ContextScope;
  displayMentions: DisplayMention[];
}

export interface AssistantRunStartRequest {
  clientRequestId: string;
  session?: AssistantSessionRef;
  turn: AssistantTurnDraft;
  explicitAction?: {
    effect: Effect;
    target?: ExplicitTarget;
    selectionSnapshot?: SelectionSnapshot;
  };
  webEnabled: boolean;
  securityDomain: SecurityDomain;
  /** Opaque, current-document capability for one classified request only. */
  classifiedContextRef?: string;
  /** Optional until the backend accepts an explicit one-Run model override. */
  modelOverride?: AgentModelOverride;
}

export interface AssistantRunAccepted {
  clientRequestId: string;
  runId: string;
  turnId: string;
  session: AssistantSessionRef;
  state: RunState;
  stateVersion: number;
}

export type RunState =
  | "accepted"
  | "preparing"
  | "running"
  | "awaiting_confirmation"
  | "paused"
  | "verifying"
  | "completed"
  | "failed"
  | "cancelled";

export type RunEventType =
  | "accepted"
  | "stage_changed"
  | "content_delta"
  | "tool_started"
  | "tool_completed"
  | "capability_degraded"
  | "web_verification_failed"
  | "confirmation_required"
  | "permission_denied"
  | "provider_switched"
  | "evidence_registered"
  | "paused"
  | "resumed"
  | "completed"
  | "failed"
  | "cancelled";

export type AssistantRunErrorCode =
  | "agent_run_invalid_request"
  | "agent_run_empty_output"
  | "agent_run_output_too_long"
  | "agent_run_evidence_invalid"
  | "agent_run_event_delivery_failed"
  | "agent_run_invalid_explicit_reference"
  | "agent_run_explicit_reference_changed"
  | "agent_run_invalid_retrieval_scope"
  | "agent_run_local_reference_index_unavailable"
  | "agent_run_session_not_found"
  | "agent_run_illegal_transition"
  | "agent_run_state_version_conflict"
  | "agent_run_not_found"
  | "agent_run_permission_denied"
  | "agent_run_confirmation_expired"
  | "agent_run_persistence_failed"
  | "agent_run_provider_unavailable"
  | "agent_run_provider_timeout"
  | "agent_run_no_capable_model"
  | "agent_run_tool_loop_limit"
  | "agent_run_tool_invalid_arguments"
  | "agent_run_mcp_unavailable"
  | "agent_run_web_evidence_required"
  | "agent_run_web_provider_timeout"
  | "agent_run_web_provider_auth_failed"
  | "agent_run_web_provider_failed"
  | "agent_run_web_evidence_invalid"
  | "agent_run_cancelled"
  | "agent_run_classified_context_required"
  | "agent_run_classified_context_expired"
  | "agent_run_classified_vault_locked";

export interface ClassifiedDocumentContext {
  contextRef: string;
}

export interface ClassifiedRunResultRequest {
  runId: string;
  contextRef: string;
}

export type ProviderSwitchReasonCode =
  | "transient_failure"
  | "provider_timeout"
  | "rate_limited"
  | "health_circuit_open"
  | "capability_fallback"
  | "manual_override_rejected"
  | "unknown";

/** Safe confirmation target projection. It must never contain source body or tool arguments. */
export interface ConfirmationTargetSummary {
  kind: "note" | "file" | "external" | "process" | "other";
  label: string;
  risk: RiskClass;
  detail?: string | null;
}

export interface PendingConfirmation {
  confirmationId: string;
  planHash: string;
  summary: string;
  /** Absent on events emitted by pre-maturity backends. */
  effect?: Effect;
  /** Absent on events emitted by pre-maturity backends. */
  targets?: ConfirmationTargetSummary[];
  /** ISO 8601 timestamp, absent on events emitted by pre-maturity backends. */
  expiresAt?: string;
}

export type AssistantRunEventPayload =
  | { kind: "accepted"; turnId: string; sessionKey: string }
  | { kind: "stage_changed"; state: RunState; stage: string }
  | { kind: "content_delta"; delta: string }
  | { kind: "tool_started"; capability: string; toolCallId: string }
  | {
      kind: "tool_completed";
      capability: string;
      toolCallId: string;
      summary: string;
    }
  | {
      kind: "capability_degraded";
      capability: string;
      code: AssistantRunErrorCode;
      retryable: boolean;
      attemptCount: number;
      message: string;
    }
  | {
      kind: "web_verification_failed";
      code: AssistantRunErrorCode;
      failureReason:
        | "provider_unavailable"
        | "provider_transport"
        | "provider_timeout"
        | "provider_authentication"
        | "provider_output_too_large"
        | "provider_rate_limited"
        | "provider_quota_exhausted"
        | "provider_invalid_arguments"
        | "search_result_unparseable"
        | "search_result_no_usable_https"
        | "evidence_content_empty"
        | "unknown";
      retryable: boolean;
      attemptCount: number;
      durationBucket: string;
      diagnosticId: string;
    }
  | {
      kind: "confirmation_required";
      confirmationId: PendingConfirmation["confirmationId"];
      planHash: PendingConfirmation["planHash"];
      summary: PendingConfirmation["summary"];
      effect?: PendingConfirmation["effect"];
      targets?: PendingConfirmation["targets"];
      expiresAt?: PendingConfirmation["expiresAt"];
    }
  | {
      kind: "permission_denied";
      code: AssistantRunErrorCode;
      message: string;
    }
  | {
      kind: "provider_switched";
      providerId: string;
      /** Absent on events emitted by pre-maturity backends. */
      modelId?: string;
      /** Structured replacement for the legacy human-readable `reason`. */
      reasonCode?: ProviderSwitchReasonCode;
      /** Kept while older backends still emit unstructured switch reasons. */
      reason?: string;
    }
  | { kind: "evidence_registered"; evidenceId: string }
  | { kind: "paused"; reason: string }
  | { kind: "resumed"; reason: string }
  | { kind: "completed"; messageId: string | null }
  | { kind: "failed"; code: AssistantRunErrorCode; message: string }
  | { kind: "cancelled"; reason: string };

interface AssistantRunEventBase {
  runId: string;
  seq: number;
  stateVersion: number;
  timestamp: string;
}

export type AssistantRunEvent = {
  [Type in RunEventType]: AssistantRunEventBase & {
    type: Type;
    payload: Extract<AssistantRunEventPayload, { kind: Type }>;
  };
}[RunEventType];

export type RunControlAction =
  | { type: "approve_change"; confirmationId: string; planHash: string }
  | { type: "reject_change"; confirmationId: string }
  | { type: "resume" }
  | { type: "cancel" };

export interface AssistantRunControlRequest {
  session: AssistantSessionRef;
  runId: string;
  expectedStateVersion: number;
  action: RunControlAction;
}

export interface AssistantRunGetRequest {
  session: AssistantSessionRef;
  /** Omit to recover this session's latest non-terminal Run after reconnecting. */
  runId?: string;
}

export interface AssistantRunRetryRequest {
  session: AssistantSessionRef;
  sourceRunId: string;
  clientRequestId: string;
}

export interface AssistantRunSnapshot {
  runId: string;
  turnId: string;
  session: AssistantSessionRef;
  state: RunState;
  stateVersion: number;
  finalMessageId?: string | null;
  pendingConfirmation?: PendingConfirmation | null;
  recovery?: {
    code: AssistantRunErrorCode;
    message: string;
  } | null;
}

export interface AssistantRunGetResponse {
  run: AssistantRunSnapshot;
  events: AssistantRunEvent[];
}

export interface ContextScope {
  paths: string[];
  pathPrefixes: string[];
  corpusIds?: string[];
  requiredTags?: string[];
}

export type ToolCallStatus =
  | "pending"
  | "running"
  | "completed"
  | "failed"
  | "rejected";

export interface ToolCallInfo {
  id: string;
  name: string;
  arguments?: Record<string, unknown>;
  status: ToolCallStatus;
  result_summary?: string;
  error?: string;
  duration_ms?: number;
  tokens_used?: number;
}

export interface TokenUsage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  prompt_cache_hit_tokens?: number;
  prompt_cache_miss_tokens?: number;
}
