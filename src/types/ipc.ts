import type {
  ContextPacket,
  DeliberationState,
  PermissionEffectSummary,
  VerificationSummary,
} from "./ai";

export interface FileListItem {
  path: string;
  title: string;
  updatedAt: string;
  isLocked: boolean;
}

export type WorkspaceItemKind = "note" | "media" | "unsupported";
export type WorkspaceMediaKind = "image" | "pdf" | "video" | null;
export type AttachmentRole = "attachment" | "formal";

export interface WorkspaceItem {
  attachmentRole: AttachmentRole;
  isLocked: boolean;
  kind: WorkspaceItemKind;
  mediaKind: WorkspaceMediaKind;
  mimeType: string | null;
  path: string;
  sizeBytes: number | null;
  title: string;
  updatedAt: string | null;
}

export interface MediaMetadata {
  mediaKind: Exclude<WorkspaceMediaKind, null>;
  mimeType: string;
  path: string;
  sizeBytes: number;
  updatedAt: string | null;
}

export interface MediaResolveResult extends MediaMetadata {
  handle: string;
  url: string;
}

export interface FileReadResult {
  content: string;
  isLocked: boolean;
}

export interface FileSignatureResult {
  byteLength: number;
  contentHash: string;
  isLocked: boolean;
  modifiedMs: number | null;
}

export interface DocumentOpenScopeResult {
  token: string;
}

/** Merged document open response: token, content, and lock status in a single IPC. */
export interface DocumentOpenResult {
  token: string;
  content: string;
  isLocked: boolean;
}

export interface ClassifiedFileEntry {
  path: string;
  isDir: boolean;
}

export type ClassifiedStatus = "needs_setup" | "locked" | "unlocked";

export interface CorpusListItem {
  id: string;
  name: string;
  pathPrefix: string;
  kind: string;
  scenes: string[];
}

export interface FileEntry {
  id: number;
  path: string;
  title: string;
  updated_at: string;
  word_count: number;
}

export interface ChatMessage {
  role: string;
  content: string;
}

export interface LlmGenerateParams {
  provider: string;
  model?: string;
  messages: ChatMessage[];
  system?: string;
  stream?: boolean;
  custom_base_url?: string;
  web_search?: boolean;
}

export interface LlmProviderInfo {
  id: string;
  name: string;
  default_model: string;
}

export type AppExitResult = void;

export interface KeywordHit {
  path: string;
  title: string;
  snippet: string;
}

export interface SemanticHit {
  chunk_id: number;
  path: string;
  title: string;
  snippet: string;
  score: number;
}

export interface FileChangedEvent {
  path: string;
  hash?: string;
  event_type: string;
}

export interface ClassifiedFileTakenEvent {
  path: string;
}

export type PermissionExecutionDecision =
  | "auto_allowed"
  | "requires_confirmation"
  | "denied";

export interface PermissionPreflightSummary {
  toolName: string;
  decision:
    | "allow"
    | "allow_once"
    | "allow_for_session"
    | "deny_once"
    | "deny_always_for_this_skill"
    | "open_settings";
  effects: PermissionEffectSummary[];
  blocked: boolean;
}

export interface PermissionDecisionOutcome {
  toolName: string;
  decision: PermissionExecutionDecision;
  preflight: PermissionPreflightSummary;
  deniedReason?: string | null;
  grantedBy?: PermissionPreflightSummary["decision"] | null;
}

export interface SandboxProfileSummary {
  id: string;
  level: "l0_app_boundary" | "l1_subprocess" | "l2_os_boundary";
  support: "supported" | "unsupported";
  summary: string;
  constraints: string[];
  limitations: string[];
}

export interface ToolConfirmRequestEvent {
  request_id: string;
  tool_call_id: string;
  tool_name: string;
  arguments: Record<string, string | number | boolean | null | undefined>;
  permissionEffects?: PermissionEffectSummary[];
  permissionDecision?: PermissionDecisionOutcome;
  sandboxProfile?: SandboxProfileSummary;
  pendingConfirmationIndex?: number;
  pendingConfirmationCount?: number;
  preview?: Record<string, unknown>;
}

export interface LlmTokenEvent {
  request_id: string;
  token: string;
  index: number;
}

export interface AiRetryStatusEvent {
  request_id: string;
  attempt: number;
  max_attempts: number;
  delay_ms: number;
}

/** Harness agent loop tool execution trace (backend `ai:harness_trace`). */
export interface HarnessTraceEvent {
  request_id: string;
  round: number;
  phase?: string;
  tool_name: string;
  status: string;
  message?: string | null;
  output_preview?: string | null;
}

export interface SessionSummary {
  id: number;
  title: string;
  scene: string;
  note_path: string | null;
  message_count: number;
  created_at: string;
  updated_at: string;
}

export interface SessionMessageRecord {
  id: number;
  session_id: number;
  seq: number;
  role: string;
  content: string;
  content_parts?: string | null;
  tool_calls?: unknown;
  evidence_packets?: ContextPacket[] | null;
  content_hash?: string | null;
  created_at: string;
}

export type SessionEvidenceSourceType = "local" | "web";

export interface SessionEvidenceRecord {
  id: number;
  sessionId: number;
  citationIndex: number;
  citationLabel: string;
  packetKey: string;
  messageSeqFirst: number;
  sourceType: SessionEvidenceSourceType;
  title: string;
  sourcePath?: string | null;
  sourceSpanStart?: number | null;
  sourceSpanEnd?: number | null;
  headingPath?: string | null;
  contentHash?: string | null;
  retrievalReason?: string | null;
  score?: number | null;
  confidence?: string | null;
  url?: string | null;
  normalizedUrl?: string | null;
  domain?: string | null;
  retrievedAt?: string | null;
  searchBackend?: string | null;
  sourceRank?: number | null;
  failureReason?: string | null;
  retiredAt?: string | null;
  createdAt: string;
  detailStatus?: string | null;
  liveExcerpt?: string | null;
}

export interface SessionEvidenceRegisterPacket {
  sourceType: SessionEvidenceSourceType;
  title: string;
  sourcePath?: string | null;
  sourceSpanStart?: number | null;
  sourceSpanEnd?: number | null;
  headingPath?: string | null;
  contentHash?: string | null;
  retrievalReason?: string | null;
  score?: number | null;
  confidence?: string | null;
  url?: string | null;
  normalizedUrl?: string | null;
  domain?: string | null;
  retrievedAt?: string | null;
  searchBackend?: string | null;
  sourceRank?: number | null;
  failureReason?: string | null;
}

export interface BacklinkEntry {
  source_path: string;
  source_title: string;
  context: string | null;
}

export interface FileLinkPreview {
  path: string;
  title: string;
  context: string | null;
}

export interface FileLinkSummary {
  inboundCount: number;
  outboundCount: number;
  inbound: FileLinkPreview[];
  outbound: FileLinkPreview[];
}

export interface GraphNode {
  id: number;
  path: string;
  title: string;
  link_count: number;
}

export interface GraphEdge {
  source: number;
  target: number;
}

export interface GraphData {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface TagGroup {
  name: string;
  files: FileListItem[];
}

export interface RecycleBinItem {
  id: string;
  original_path: string;
  title: string;
  deleted_at: string;
  expires_at: string;
  version_count: number;
}

export type VersionKind =
  | "auto_idle"
  | "manual"
  | "pre_restore"
  | "finalize"
  | "pre_close";

export interface VersionEntry {
  id: number;
  file_id: number;
  version_no: string;
  label: string | null;
  content_hash: string;
  word_count: number;
  is_finalized: boolean;
  kind: VersionKind;
  created_at: string;
}

/** Emitted on `version:save_complete` after async manual/idle snapshot IPC. */
export interface VersionSaveCompleteEvent {
  path: string;
  kind: VersionKind | "manual" | "auto_idle";
  created: boolean;
  versionId: number | null;
  skipReason?:
    | "duplicate_hash"
    | "auto_idle_any_snapshot_cooldown"
    | "auto_idle_interval_cooldown"
    | null;
  error: string | null;
}

// AI Runtime IPC types

/** `ai_cache_clear` return value: cleared sessions, checkpoints, traces, and caches. */
export interface AiCacheClearResult {
  sessions_deleted: number;
  aborted_tasks: number;
  checkpoints_cleared: number;
  deposits_deleted: number;
  traces_deleted: number;
  web_pages_cleared: number;
  searches_cleared: number;
}

export type AgentTaskKind = "lightweight" | "complex";

export type AgentTaskStatus =
  | "queued"
  | "running"
  | "awaiting_confirmation"
  | "paused_budget"
  | "paused_recoverable"
  | "completed"
  | "failed_safe"
  | "aborted";

export interface AgentTaskDto {
  task_id: string;
  request_id: string;
  session_id: number;
  kind: AgentTaskKind;
  status: AgentTaskStatus;
  user_goal_summary: string;
  budget_policy: unknown;
  created_at: string;
  updated_at: string;
  completed_at?: string | null;
  error_code?: string | null;
  error_message?: string | null;
  deliberation_state?: DeliberationState | null;
  verification_summary?: VerificationSummary | null;
}

export interface AgentTaskStepDto {
  id: number;
  task_id: string;
  step_seq: number;
  kind: string;
  status: AgentTaskStatus;
  input_summary: string;
  output_summary: string;
  evidence_packet_ids: string[];
  created_at: string;
  updated_at: string;
}

export interface AgentTaskEventDto {
  id: number;
  task_id: string;
  event_type: string;
  message: string;
  created_at: string;
}

export interface AgentTaskListParams {
  sessionId?: number | null;
  status?: AgentTaskStatus | null;
}

/** User profile entry returned by `profile_list` / `profile_get`. */
export interface ProfileEntry {
  key: string;
  value: unknown;
  source: string;
  confidence: number;
  is_active: boolean;
  updated_at: string;
}

/** Inbox item returned by `inbox_list`. */
export interface InboxItem {
  id: number;
  session_id: number | null;
  source_note: string | null;
  deposit_type: string;
  content: string;
  status: string;
  target_path: string | null;
  created_at: string;
  updated_at: string;
}

/** Image attachment DTO passed from the frontend. */
export interface ImageAttachmentDto {
  id: string;
  dataBase64: string;
  mimeType: string;
  fileName?: string;
  sizeBytes: number;
}

export type {
  AiScene,
  AssembledContext,
  ContextPacket,
  ContextStatus,
  ToolSpec,
} from "./ai";
