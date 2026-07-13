/** Frontend contracts for the unified, scene-free Agent Run control plane. */

export type CapabilitySlot =
  | "fast"
  | "writer"
  | "reasoner"
  | "long_context"
  | "vision"
  | "agent_tools"
  | "embedding"
  | "reranker"
  | "local_private";

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
export type Effort = "direct" | "tool_loop" | "durable";
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
  text: string;
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

export interface AssistantRunStartRequest {
  clientRequestId: string;
  session?: AssistantSessionRef;
  message: string;
  contentParts?: ContentPart[];
  explicitReferences: ContextReference[];
  explicitAction?: {
    effect: Effect;
    target?: ExplicitTarget;
    selectionSnapshot?: SelectionSnapshot;
  };
  webEnabled: boolean;
  securityDomain: SecurityDomain;
}

export interface AssistantRunAccepted {
  runId: string;
  turnId: string;
  session: AssistantSessionRef;
  state: "accepted";
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
  | "agent_run_session_not_found"
  | "agent_run_illegal_transition"
  | "agent_run_state_version_conflict"
  | "agent_run_not_found"
  | "agent_run_permission_denied"
  | "agent_run_confirmation_expired"
  | "agent_run_persistence_failed"
  | "agent_run_provider_unavailable"
  | "agent_run_provider_timeout"
  | "agent_run_cancelled";

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
      kind: "confirmation_required";
      confirmationId: string;
      planHash: string;
      summary: string;
    }
  | {
      kind: "permission_denied";
      code: AssistantRunErrorCode;
      message: string;
    }
  | { kind: "provider_switched"; providerId: string; reason: string }
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

export interface AssistantRunSnapshot {
  runId: string;
  turnId: string;
  session: AssistantSessionRef;
  state: RunState;
  stateVersion: number;
  finalMessageId?: string | null;
  pendingConfirmation?: {
    confirmationId: string;
    summary: string;
  } | null;
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
