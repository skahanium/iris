export interface FileListItem {
  path: string;
  title: string;
  updated_at: string;
}

export interface FileReadResult {
  content: string;
  isLocked: boolean;
}

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

export interface LlmTokenEvent {
  request_id: string;
  token: string;
  index: number;
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
  tool_calls?: unknown;
  content_hash?: string | null;
  created_at: string;
}

export interface BacklinkEntry {
  source_path: string;
  source_title: string;
  context: string | null;
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

// ─── AI Runtime IPC types ───

/** `ai_cache_clear` 返回值：清空会话、checkpoint、追踪记录与知识沉淀缓存。 */
export interface AiCacheClearResult {
  sessions_deleted: number;
  checkpoints_cleared: number;
  deposits_deleted: number;
  traces_deleted: number;
  web_pages_cleared: number;
  searches_cleared: number;
}

/** 用户画像条目（`profile_list` / `profile_get` 返回） */
export interface ProfileEntry {
  key: string;
  value: unknown;
  source: string;
  confidence: number;
  is_active: boolean;
  updated_at: string;
}

/** 收件箱条目（`inbox_list` 返回） */
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

export type {
  AiScene,
  AssembledContext,
  ContextPacket,
  ContextStatus,
  ToolSpec,
} from "./ai";
