import type { PermissionEffectSummary } from "./ai";

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
  intents: string[];
}

export interface FileEntry {
  id: number;
  path: string;
  title: string;
  updated_at: string;
  word_count: number;
}

export type FileWriteIndexStatus = "synced" | "degraded";

/** Receipt for the authoritative Markdown write and its derived-index refresh. */
export interface FileWriteResult {
  entry: FileEntry;
  contentHash: string;
  indexStatus: FileWriteIndexStatus;
}

export type CredentialState = "available" | "missing";

export interface CredentialStatus {
  service: string;
  state: CredentialState;
  configured: boolean;
  checkedAt: string;
}

export type AppUpdateStatus =
  | "idle"
  | "checking"
  | "up_to_date"
  | "available"
  | "downloading"
  | "downloaded"
  | "ready_to_install"
  | "unsupported"
  | "error";

export interface AppUpdateInfo {
  currentVersion: string;
  version: string;
  notes?: string | null;
  downloaded: boolean;
  preflightPassed: boolean;
  cachedBytes?: number | null;
}

export interface AppUpdateStateEvent {
  status: AppUpdateStatus;
  info?: AppUpdateInfo | null;
  message?: string | null;
}

export type AppUpdatePreflightCheckStatus = "passed" | "failed" | "warning";

export interface AppUpdatePreflightCheck {
  id: string;
  label: string;
  status: AppUpdatePreflightCheckStatus;
  message: string;
}

export interface AppUpdatePreflightResult {
  ok: boolean;
  checks: AppUpdatePreflightCheck[];
}

export interface AppUpdateProgressEvent {
  phase: "started" | "progress" | "finished";
  chunkLength: number;
  contentLength?: number | null;
  downloaded: number;
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

export type EmbeddingIndexPhase =
  | "legacy_ready"
  | "running"
  | "paused"
  | "ready"
  | "failed"
  | "disabled";

export type EmbeddingFailureCode =
  | "interrupted_migration"
  | "interrupted_restart"
  | "model_unavailable"
  | "scheduler_start_failed"
  | "embedding_failed"
  | "database_error";

export interface EmbeddingIndexStatus {
  activeModelId: string;
  targetModelId: string;
  dimension: number;
  phase: EmbeddingIndexPhase;
  indexedItems: number;
  totalItems: number;
  lastError: string | null;
  failureCode: EmbeddingFailureCode | null;
  automaticAttempted: boolean;
}

export type EmbeddingSchedulerStartResult =
  | "started"
  | "already_running"
  | "disabled";

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

/** Durable result of a manual or idle version-save command. */
export interface VersionSaveOutcome {
  created: boolean;
  versionId: number | null;
  skipReason?:
    | "duplicate_hash"
    | "auto_idle_any_snapshot_cooldown"
    | "auto_idle_interval_cooldown"
    | null;
}

/** Read-only finding produced by the document-title recovery audit. */
export interface DocumentTitleAuditItem {
  path: string;
  currentTitle: string;
  candidateTitle: string | null;
  candidateSource: "version" | "index" | "filename" | null;
  contentHash: string | null;
  reason:
    | "missing_markdown"
    | "missing_or_placeholder_title"
    | "index_title_mismatch";
}

/** A missing indexed document that can be recreated from a retained version. */
export interface MissingDocumentRecoveryItem {
  path: string;
  currentTitle: string;
  candidateTitle: string | null;
  versionId: number;
  contentHash: string;
  createdAt: string;
  preview: string;
}

/** An unattached Markdown CAS object that needs a new user-selected destination. */
export interface OrphanedDocumentRecoveryItem {
  objectHash: string;
  candidateTitle: string | null;
  suggestedPath: string;
  preview: string;
}

/** A missing indexed document for which no safe local recovery source remains. */
export interface UnavailableDocumentRecoveryItem {
  path: string;
  currentTitle: string;
  reason: "no_readable_version_snapshot";
}

/** Read-only recovery audit spanning title corruption, missing files, and CAS orphans. */
export interface DocumentRecoveryAudit {
  titleIssues: DocumentTitleAuditItem[];
  missingDocuments: MissingDocumentRecoveryItem[];
  orphanedDocuments: OrphanedDocumentRecoveryItem[];
  unavailableDocuments: UnavailableDocumentRecoveryItem[];
}

// AI Runtime IPC types

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

// ── RSS 订阅资料库 IPC 类型（阶段 3 冻结契约）───────────────
//
// 与 Rust `feed::model` 的 camelCase DTO 一一对应；`source_payload`
// 永不进入任何 IPC 类型；同步事件只投影 sourceId/类型/计数/错误码。

export type FeedView = "inbox" | "today" | "all" | "starred" | "archived";

/** 稳定 keyset 游标：按 `(receivedAt DESC, rowId DESC)` 排序。 */
export interface FeedPageCursor {
  sortAt: string;
  rowId: number;
}

/** 冻结的文章查询条件（列表与批量已读共用）。 */
export interface FeedItemQuery {
  view: FeedView;
  sourceId?: string | null;
  receivedAfter?: string | null;
  cursor?: FeedPageCursor | null;
  limit: number;
}

/** 订阅源列表摘要。 */
export interface FeedSourceSummary {
  id: string;
  title: string;
  feedUrl: string;
  siteUrl: string | null;
  folderPath: string;
  isEnabled: boolean;
  fetchIntervalMinutes: number;
  unreadCount: number;
  lastCheckedAt: string | null;
  lastSuccessAt: string | null;
  nextFetchAt: string | null;
  consecutiveFailures: number;
  lastErrorCode: string | null;
}

/** 文章列表摘要（rowId 用于构造 keyset 游标）。 */
export interface FeedItemSummary {
  rowId: number;
  id: string;
  sourceId: string;
  sourceTitle: string;
  title: string;
  authorName: string | null;
  canonicalUrl: string | null;
  publishedAt: string | null;
  receivedAt: string;
  excerpt: string;
  isRead: boolean;
  isStarred: boolean;
  isArchived: boolean;
  conversionStatus: string;
}

/** 文章详情：只含规范化 Markdown 与安全元数据，原始源载荷永不进 IPC。 */
export interface FeedItemDetail {
  summary: FeedItemSummary;
  contentMarkdown: string;
  summaryMarkdown: string;
}

/** 阅读状态补丁；至少一个字段为 true/false。 */
export interface FeedItemStatePatch {
  isRead?: boolean;
  isStarred?: boolean;
  isArchived?: boolean;
}

/** 发现的候选订阅源（不含 HTML）。 */
export interface FeedCandidate {
  url: string;
  title: string | null;
  format: string | null;
}

/** 添加订阅源输入。 */
export interface FeedSourceAddInput {
  url: string;
  title: string;
  titleOverride?: string | null;
  folderPath?: string | null;
  fetchIntervalMinutes?: number | null;
}

/** 编辑订阅源：`titleOverride: null` 清除覆盖标题，缺省字段不改动。 */
export interface FeedSourceUpdateInput {
  titleOverride?: string | null;
  folderPath?: string | null;
  fetchIntervalMinutes?: number | null;
  isEnabled?: boolean | null;
}

/** 同步结果（事件另发 `feed:changed`）。 */
export interface FeedSyncOutcome {
  status: "succeeded" | "not_modified" | "skipped" | "in_flight" | "failed";
  newItems: number;
  errorCode: string | null;
}

/** 同步事件投影：只提示 UI 重新查询，不含 URL/正文。 */
export interface FeedChangedEvent {
  sourceId: string;
  kind: "sync_succeeded" | "sync_failed" | "items_changed";
  newItems: number;
  errorCode: string | null;
}
