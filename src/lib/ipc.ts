import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type {
  AiScene,
  AiSendMessageResult,
  AssembledContext,
  AssistantExecuteRequest,
  AssistantExecuteResponse,
  ChapterInfo,
  ChapterWritingExecuteResult,
  CitationCheckResult,
  ContextPacket,
  ContextScope,
  DocumentCheckResult,
  OrganizeExecuteResult,
  ResearchExecuteResult,
  ResearchProgressEvent,
  WritingExecuteResult,
} from "@/types/ai";
import type {
  AiCacheClearResult,
  AppExitResult,
  BacklinkEntry,
  FileChangedEvent,
  FileEntry,
  FileListItem,
  FileReadResult,
  GraphData,
  InboxItem,
  KeywordHit,
  LlmGenerateParams,
  LlmProviderInfo,
  LlmTokenEvent,
  ProfileEntry,
  RecycleBinItem,
  SemanticHit,
  SessionMessageRecord,
  SessionSummary,
  TagGroup,
  VersionEntry,
  VersionSaveCompleteEvent,
} from "@/types/ipc";

export interface SettingsMap {
  theme: "dark" | "light";
  llm_custom_base_url: string | null;
  /** 底栏「联网」开关，跨会话保持 */
  web_search_enabled: boolean;
}

export async function settingsGet<K extends keyof SettingsMap>(
  key: K,
): Promise<SettingsMap[K] | null>;
export async function settingsGet<T>(key: string): Promise<T | null>;
export async function settingsGet<T>(key: string): Promise<T | null> {
  return invoke<T | null>("settings_get", { key });
}

export async function settingsSet<K extends keyof SettingsMap>(
  key: K,
  value: SettingsMap[K],
): Promise<void>;
export async function settingsSet(key: string, value: unknown): Promise<void>;
export async function settingsSet(key: string, value: unknown): Promise<void> {
  return invoke("settings_set", { key, value });
}

export async function vaultSet(path: string): Promise<void> {
  return invoke("vault_set", { path });
}

export async function vaultGet(): Promise<string | null> {
  return invoke<string | null>("vault_get");
}

export async function fileList(): Promise<FileListItem[]> {
  return invoke<FileListItem[]>("file_list");
}

export async function folderList(): Promise<string[]> {
  return invoke<string[]>("folder_list");
}

export async function fileRead(path: string): Promise<FileReadResult> {
  return invoke<FileReadResult>("file_read", { path });
}

export async function fileSetLock(
  path: string,
  locked: boolean,
): Promise<void> {
  return invoke("file_set_lock", { path, locked });
}

export async function fileWrite(
  path: string,
  content: string,
): Promise<FileEntry> {
  return invoke<FileEntry>("file_write", { path, content });
}

/** Save a vault image under `assets/` (base64 body). Returns vault-relative path. */
export async function vaultAssetWrite(params: {
  path: string;
  dataBase64: string;
}): Promise<string> {
  return invoke<string>("vault_asset_write", params);
}

export async function fileCreate(
  path: string,
  content: string,
): Promise<FileEntry> {
  return invoke<FileEntry>("file_create", { path, content });
}

export async function fileDelete(path: string): Promise<void> {
  return invoke("file_delete", { path });
}

/** Permanently remove a blank note (not recycled). */
export async function fileDiscard(path: string): Promise<void> {
  return invoke("file_discard", { path });
}

export async function fileRename(
  path: string,
  newPath: string,
): Promise<FileEntry> {
  return invoke<FileEntry>("file_rename", { path, newPath });
}

export async function pathSyncSuggest(
  currentPath: string,
  title: string,
): Promise<{
  current_path: string;
  suggested_path: string;
  needs_sync: boolean;
  conflict_resolved: boolean;
}> {
  return invoke("path_sync_suggest", { currentPath, title });
}

export async function fileBacklinks(path: string): Promise<BacklinkEntry[]> {
  return invoke<BacklinkEntry[]>("file_backlinks", { path });
}

export async function folderCreate(path: string): Promise<void> {
  return invoke("folder_create", { path });
}

export async function folderRename(
  oldPath: string,
  newPath: string,
): Promise<void> {
  return invoke("folder_rename", { oldPath, newPath });
}

export async function folderDelete(path: string): Promise<void> {
  return invoke("folder_delete", { path });
}

export async function recycleList(): Promise<RecycleBinItem[]> {
  return invoke<RecycleBinItem[]>("recycle_list_cmd");
}

export async function recycleRestore(id: string): Promise<string> {
  return invoke<string>("recycle_restore_cmd", { id });
}

export async function recyclePurge(id: string): Promise<void> {
  return invoke("recycle_purge_cmd", { id });
}

/** Rescan vault `.md` files into SQLite (titles, tags, FTS, chunks). */
export async function indexRescan(): Promise<FileEntry[]> {
  return invoke<FileEntry[]>("index_rescan");
}

export async function tagList(): Promise<TagGroup[]> {
  return invoke<TagGroup[]>("tag_list");
}

export async function graphData(): Promise<GraphData> {
  return invoke<GraphData>("graph_data");
}

export async function versionList(path: string): Promise<VersionEntry[]> {
  return invoke<VersionEntry[]>("version_list_cmd", { path });
}

export async function versionPreview(versionId: number): Promise<string> {
  return invoke<string>("version_preview_cmd", { versionId });
}

export async function versionRestore(
  versionId: number,
  currentContent: string,
): Promise<{ content: string }> {
  return invoke<{ content: string }>("version_restore_cmd", {
    versionId,
    currentContent,
  });
}

export async function versionDelete(versionId: number): Promise<void> {
  return invoke("version_delete_cmd", { versionId });
}

export async function versionFinalizeCurrent(
  path: string,
  content: string,
  label: string | null,
): Promise<VersionEntry | null> {
  return invoke<VersionEntry | null>("version_finalize_current_cmd", {
    path,
    content,
    label,
  });
}

/** Enqueues manual snapshot; completes on `version:save_complete`. */
export async function versionSaveManual(
  path: string,
  content: string,
): Promise<void> {
  await invoke<void>("version_save_manual_cmd", { path, content });
}

/** Enqueues idle snapshot; completes on `version:save_complete`. */
export async function versionSaveIdle(
  path: string,
  content: string,
): Promise<void> {
  await invoke<void>("version_save_idle_cmd", { path, content });
}

export function listenVersionSaveComplete(
  handler: (payload: VersionSaveCompleteEvent) => void,
): Promise<() => void> {
  return listen<VersionSaveCompleteEvent>("version:save_complete", (event) =>
    handler(event.payload),
  );
}

export async function templateList(): Promise<{ name: string }[]> {
  return invoke<{ name: string }[]>("template_list");
}

export async function templateCreate(
  path: string,
  templateName: string,
): Promise<FileEntry> {
  return invoke<FileEntry>("template_create", { path, templateName });
}

export async function templateRead(name: string): Promise<string> {
  return invoke<string>("template_read", { name });
}

export async function templateSave(
  name: string,
  content: string,
): Promise<void> {
  return invoke("template_save", { name, content });
}

export async function templateDelete(name: string): Promise<void> {
  return invoke("template_delete", { name });
}

export async function exportFile(
  destPath: string,
  content: string,
): Promise<void> {
  return invoke("export_file", { destPath, content });
}

export async function searchKeyword(
  query: string,
  limit?: number,
): Promise<KeywordHit[]> {
  return invoke<KeywordHit[]>("search_keyword", { query, limit });
}

export async function searchSemantic(
  query: string,
  limit?: number,
): Promise<SemanticHit[]> {
  return invoke<SemanticHit[]>("search_semantic", { query, limit });
}

export async function searchReindex(): Promise<number> {
  return invoke<number>("search_reindex");
}

export async function llmProviders(): Promise<LlmProviderInfo[]> {
  return invoke<LlmProviderInfo[]>("llm_providers");
}

export async function llmGenerate(params: LlmGenerateParams): Promise<string> {
  return invoke<string>("llm_generate", { params });
}

export async function llmAbort(requestId: string): Promise<void> {
  return invoke("llm_abort_cmd", { requestId });
}

export {
  connectivityStatus,
  llmConfigApplyDeepseekDefaults,
  llmConfigGet,
  llmConfigSet,
  llmConfigTest,
  LLM_CONFIG_CHANGED_EVENT,
  notifyLlmConfigChanged,
} from "@/lib/llm-ipc";

export async function credentialSet(
  service: string,
  value: string,
): Promise<void> {
  return invoke("credential_set", { service, value });
}

export async function credentialHas(service: string): Promise<boolean> {
  return invoke<boolean>("credential_has", { service });
}

export async function credentialDelete(service: string): Promise<void> {
  return invoke("credential_delete", { service });
}

export async function listenFileChanged(
  handler: (payload: FileChangedEvent) => void,
): Promise<() => void> {
  return listen<FileChangedEvent>("file:changed", (e) => handler(e.payload));
}

export async function listenLlmToken(
  handler: (payload: LlmTokenEvent) => void,
): Promise<() => void> {
  return listen<LlmTokenEvent>("llm:token", (e) => handler(e.payload));
}

export async function listenLlmDone(
  handler: (payload: { request_id?: string }) => void,
): Promise<() => void> {
  return listen<{ request_id?: string }>("llm:done", (e) => handler(e.payload));
}

export async function listenLlmError(
  handler: (payload: { request_id?: string; error?: string }) => void,
): Promise<() => void> {
  return listen<{ request_id?: string; error?: string }>("llm:error", (e) =>
    handler(e.payload),
  );
}

export async function listenHarnessTrace(
  handler: (payload: import("@/types/ipc").HarnessTraceEvent) => void,
): Promise<() => void> {
  return listen<import("@/types/ipc").HarnessTraceEvent>(
    "ai:harness_trace",
    (e) => handler(e.payload),
  );
}

export interface AiThinkingEvent {
  request_id: string;
  round: number;
  content: string;
}

export async function listenAiThinking(
  handler: (payload: AiThinkingEvent) => void,
): Promise<() => void> {
  return listen<AiThinkingEvent>("ai:thinking", (e) => handler(e.payload));
}

export async function listenAiRequestStarted(
  handler: (payload: { request_id: string }) => void,
): Promise<() => void> {
  return listen<{ request_id: string }>("ai:request_started", (e) =>
    handler(e.payload),
  );
}

// ─── AI Runtime IPC ───

import type { CorpusListItem } from "@/types/ipc";

export async function corpusList(): Promise<CorpusListItem[]> {
  return invoke<CorpusListItem[]>("corpus_list");
}

export async function corpusUpsert(entry: {
  id: string;
  name: string;
  pathPrefix: string;
  kind: string;
  scenes: string[];
}): Promise<void> {
  return invoke("corpus_upsert", { entry });
}

export async function sessionList(params?: {
  scene?: string;
  note_path?: string | null;
  limit?: number;
  offset?: number;
}): Promise<SessionSummary[]> {
  return invoke<SessionSummary[]>("session_list", {
    scene: params?.scene ?? null,
    notePath: params?.note_path ?? null,
    limit: params?.limit ?? 50,
    offset: params?.offset ?? 0,
  });
}

export async function sessionDelete(sessionId: number): Promise<boolean> {
  return invoke<boolean>("session_delete", { sessionId });
}

export async function sessionRename(
  sessionId: number,
  title: string,
): Promise<void> {
  return invoke("session_rename", { sessionId, title });
}

export async function sessionRetract(
  sessionId: number,
  fromSeq: number,
): Promise<number> {
  return invoke<number>("session_retract", { sessionId, fromSeq });
}

export interface SkillEntryDto {
  name: string;
  description: string;
  license?: string | null;
  compatibility?: string | null;
  metadata: Record<string, unknown>;
  allowed_tools: string[];
  content: string;
  scope: string;
  source_url?: string | null;
  enabled: boolean;
  file_path: string;
  legacy_trigger?: string | null;
  content_hash?: string;
}

export type SkillValidationStatus = "valid" | "legacy" | { invalid: string };

export interface SkillListEntryDto {
  /** Embedded skill fields (flattened via serde). */
  name: string;
  description: string;
  license?: string | null;
  compatibility?: string | null;
  metadata: Record<string, unknown>;
  allowed_tools: string[];
  content: string;
  scope: string;
  source_url?: string | null;
  enabled: boolean;
  file_path: string;
  legacy_trigger?: string | null;
  /** Computed fields. */
  validation: SkillValidationStatus;
  unrecognized_tools: string[];
  missing_deps: string[];
  /** Present when `skillsList` was called with a scene. */
  scene_active?: boolean;
  scene_score?: number;
  /** Subset of allowed_tools that require harness confirmation. */
  confirmation_required_tools: string[];
  content_hash?: string;
  capability_preview?: {
    requested_tools?: string[];
    confirmation_required_tools?: string[];
    unrecognized_tools?: string[];
    missing_deps?: string[];
    allows_script_execution?: boolean;
    script_policy?: string;
    [key: string]: unknown;
  };
  availability: "available" | "partial" | "unavailable" | "disabled" | string;
}

export interface PromptProfileDto {
  display_name: string;
  avatar_emoji: string | null;
  persona: string;
  writing_style: string;
  custom_rules: string[];
  language: string;
}

export async function skillsList(
  scene?: AiScene,
): Promise<SkillListEntryDto[]> {
  return invoke<SkillListEntryDto[]>("skills_list", { scene: scene ?? null });
}

export async function skillsReadResource(request: {
  name: string;
  scope: string;
  relative_path: string;
}): Promise<string> {
  return invoke<string>("skills_read_resource", { request });
}

export async function skillsInstall(request: {
  source: string;
  path_or_url: string;
  scope: string;
  subpath?: string;
  registry?: string;
  expected_sha256?: string;
}): Promise<unknown> {
  return invoke("skills_install", { request });
}

export async function skillsUninstall(
  name: string,
  scope: string,
): Promise<void> {
  return invoke("skills_uninstall", { name, scope });
}

export async function skillsUpdate(
  name: string,
  scope: string,
): Promise<unknown> {
  return invoke("skills_update", { name, scope });
}

export async function skillsToggle(
  name: string,
  scope: string,
  enabled: boolean,
): Promise<void> {
  return invoke("skills_toggle", { name, scope, enabled });
}

export async function promptProfileGet(): Promise<PromptProfileDto> {
  return invoke<PromptProfileDto>("prompt_profile_get");
}

export async function promptProfileSet(
  profile: PromptProfileDto,
): Promise<void> {
  return invoke("prompt_profile_set", { profile });
}

export async function promptProfilePresets(): Promise<
  { label: string; profile: PromptProfileDto }[]
> {
  return invoke("prompt_profile_presets");
}

export async function sessionClearAll(params?: {
  scene?: string;
  note_path?: string | null;
}): Promise<number> {
  return invoke<number>("session_clear_all", {
    scene: params?.scene ?? null,
    notePath: params?.note_path ?? null,
  });
}

/** 清空 AI 运行时持久化缓存（会话、harness checkpoint、知识沉淀）。 */
export async function aiCacheClear(): Promise<AiCacheClearResult> {
  return invoke<AiCacheClearResult>("ai_cache_clear");
}

export async function harnessResume(requestId: string): Promise<unknown> {
  return invoke("harness_resume", { requestId });
}

export async function harnessAbort(requestId: string): Promise<void> {
  return invoke("harness_abort", { requestId });
}

export async function skillsRead(filePath: string): Promise<string> {
  return invoke<string>("skills_read", {
    request: { file_path: filePath },
  });
}

export async function skillsWrite(
  filePath: string,
  scope: string,
  content: string,
): Promise<SkillEntryDto> {
  return invoke<SkillEntryDto>("skills_write", {
    request: { file_path: filePath, scope, content },
  });
}

export async function skillsMigrateLegacy(
  filePath: string,
  scope: string,
): Promise<SkillEntryDto> {
  return invoke<SkillEntryDto>("skills_migrate_legacy", {
    filePath,
    scope,
  });
}

// ─── Tool Audit ────────────────────────────────────────

export interface ToolAuditEntry {
  id: number;
  request_id: string;
  harness_round: number;
  tool_name: string;
  arguments_summary: string | null;
  result_summary: string | null;
  success: boolean;
  duration_ms: number | null;
  scene: string | null;
  subagent_depth: number;
  created_at: string;
}

export async function toolAuditQuery(
  requestId: string,
): Promise<ToolAuditEntry[]> {
  return invoke<ToolAuditEntry[]>("tool_audit_query", { requestId });
}

export async function sessionLoad(
  sessionId: number,
  limit?: number,
): Promise<SessionMessageRecord[]> {
  return invoke<SessionMessageRecord[]>("session_load", {
    sessionId,
    limit: limit ?? 50,
  });
}

export async function contextAssemble(params: {
  scene: AiScene;
  note_path: string | null;
  note_content_hash: string | null;
  query: string;
  session_id: number | null;
  context_scope?: ContextScope | null;
  web_search?: boolean;
}): Promise<AssembledContext> {
  return invoke<AssembledContext>("context_assemble", {
    scene: params.scene,
    notePath: params.note_path,
    noteContentHash: params.note_content_hash,
    query: params.query,
    sessionId: params.session_id,
    contextScope: params.context_scope ?? null,
    webSearch: params.web_search ?? false,
  });
}

/** 统一助手执行门面 — 按 intent 路由到既有工作流 */
export async function assistantExecute(
  request: AssistantExecuteRequest,
): Promise<AssistantExecuteResponse> {
  return invoke<AssistantExecuteResponse>("assistant_execute", { request });
}

export async function aiSendMessage(params: {
  scene: AiScene;
  session_id: number | null;
  message: string;
  note_path?: string | null;
  selected_packet_ids?: string[];
  context_scope?: ContextScope | null;
  /** 为 true 时在发送前注入 MiniMax / DuckDuckGo 网页检索摘要 */
  web_search?: boolean;
}): Promise<AiSendMessageResult> {
  return invoke<AiSendMessageResult>("ai_send_message", {
    scene: params.scene,
    sessionId: params.session_id,
    message: params.message,
    notePath: params.note_path ?? null,
    selectedPacketIds: params.selected_packet_ids ?? null,
    contextScope: params.context_scope ?? null,
    webSearch: params.web_search ?? false,
  });
}

export async function toolConfirm(params: {
  request_id: string;
  tool_call_id: string;
  decision: "approve" | "reject" | "modify";
  modified_args?: unknown;
}): Promise<{ request_id: string; tool_call_id: string; status: string }> {
  return invoke("tool_confirm", {
    requestId: params.request_id,
    toolCallId: params.tool_call_id,
    decision: params.decision,
    modifiedArgs: params.modified_args ?? null,
  });
}

export async function aiListTools(scene: AiScene): Promise<
  {
    name: string;
    description: string;
    requires_confirmation: boolean;
    access_level: string;
  }[]
> {
  return invoke("ai_list_tools", { scene });
}

// ─── Knowledge Index IPC ───

export async function knowledgeReindex(): Promise<{
  anchors: number;
  regulations: number;
}> {
  return invoke("knowledge_reindex");
}

export async function searchHybrid(params: {
  query: string;
  scene?: string;
  note_path?: string | null;
  limit?: number;
}): Promise<ContextPacket[]> {
  return invoke("search_hybrid", {
    query: params.query,
    scene: params.scene ?? null,
    notePath: params.note_path ?? null,
    limit: params.limit ?? null,
  });
}

// ─── Research Workflow IPC (D) ───

export async function researchExecute(params: {
  topic: string;
  web_authorized?: boolean;
}): Promise<ResearchExecuteResult> {
  return invoke<ResearchExecuteResult>("research_execute", {
    topic: params.topic,
    webAuthorized: params.web_authorized ?? false,
  });
}

export async function researchStatus(): Promise<{
  recent_research: Array<{
    request_id: string;
    status: string;
    latency_ms: number | null;
    created_at: string;
  }>;
}> {
  return invoke("research_status");
}

export async function researchAbort(requestId: string): Promise<void> {
  return invoke("research_abort", { requestId });
}

export async function researchActiveTasks(): Promise<string[]> {
  return invoke("research_active_tasks");
}

export async function researchGenerateNote(params: {
  topic: string;
  summary: string;
  evidence_count: number;
  coverage_score: number;
  target_path?: string;
}): Promise<{
  content: string;
  suggested_path: string;
  section_count: number;
}> {
  return invoke("research_generate_note", {
    request: {
      topic: params.topic,
      summary: params.summary,
      evidence_count: params.evidence_count,
      coverage_score: params.coverage_score,
      target_path: params.target_path ?? null,
    },
  });
}

export async function listenResearchProgress(
  handler: (payload: ResearchProgressEvent) => void,
): Promise<() => void> {
  return listen<ResearchProgressEvent>("ai:research_progress", (e) =>
    handler(e.payload),
  );
}

// ─── Writing Workflow IPC (Phase 1) ───

export async function writingExecute(params: {
  target_path: string;
  base_content_hash: string;
  selection?: string;
  cursor_context: string;
  writing_goal: string;
  web_authorized?: boolean;
}): Promise<WritingExecuteResult> {
  return invoke<WritingExecuteResult>("writing_execute", {
    input: {
      target_path: params.target_path,
      base_content_hash: params.base_content_hash,
      selection: params.selection ?? null,
      cursor_context: params.cursor_context,
      writing_goal: params.writing_goal,
      web_authorized: params.web_authorized ?? false,
    },
  });
}

export async function patchApply(patch: {
  id: string;
  target_path: string;
  base_content_hash: string;
  range: { start: number; end: number };
  original_text: string;
  replacement_text: string;
  evidence_packet_ids: string[];
  risk_level: string;
  warnings: string[];
  created_at: string;
}): Promise<{
  success: boolean;
  new_content_hash?: string;
  error?: string;
  warnings: string[];
}> {
  return invoke("patch_apply", { patch });
}

// ─── Citation Check IPC (Phase 1) ───

export async function citationCheck(params: {
  paragraph_text: string;
  document_path: string;
  scope?: {
    paths: string[];
    pathPrefixes: string[];
    corpusIds?: string[];
  };
  web_authorized?: boolean;
}): Promise<CitationCheckResult> {
  return invoke<CitationCheckResult>("citation_check", {
    input: {
      paragraph_text: params.paragraph_text,
      document_path: params.document_path,
      scope: params.scope ?? null,
      web_authorized: params.web_authorized ?? false,
    },
  });
}

// ─── Organize Workflow IPC (Phase 2) ───

export async function organizeExecute(params: {
  scope?: {
    paths?: string[];
    path_prefixes?: string[];
    corpus_ids?: string[];
  };
  task_type: string;
}): Promise<OrganizeExecuteResult> {
  return invoke<OrganizeExecuteResult>("organize_execute", {
    input: {
      scope: params.scope ?? null,
      task_type: params.task_type,
    },
  });
}

export async function organizeApply(
  suggestions: Array<{
    id: string;
    suggestion_type: string;
    target_path: string;
    current_value?: string;
    suggested_value: string;
    reason: string;
    source: string;
    confidence: number;
    evidence_packet_ids: string[];
  }>,
): Promise<{
  applied: string[];
  skipped: string[];
  errors: string[];
}> {
  return invoke("organize_apply", { request: { suggestions } });
}

// ─── Chapter & Document Writing IPC (Phase 3) ───

export async function chapterWritingExecute(params: {
  target_path: string;
  base_content_hash: string;
  chapter: ChapterInfo;
  writing_goal: string;
  web_authorized?: boolean;
}): Promise<ChapterWritingExecuteResult> {
  return invoke<ChapterWritingExecuteResult>("chapter_writing_execute", {
    input: {
      target_path: params.target_path,
      base_content_hash: params.base_content_hash,
      chapter: params.chapter,
      writing_goal: params.writing_goal,
      web_authorized: params.web_authorized ?? false,
    },
  });
}

export async function documentCheckExecute(params: {
  target_path: string;
  content: string;
  base_content_hash?: string;
  check_type: string;
  web_authorized?: boolean;
}): Promise<DocumentCheckResult> {
  return invoke<DocumentCheckResult>("document_check_execute", {
    input: {
      target_path: params.target_path,
      content: params.content,
      base_content_hash: params.base_content_hash ?? "",
      check_type: params.check_type,
      web_authorized: params.web_authorized ?? false,
    },
  });
}

export async function parseDocumentChapters(
  content: string,
): Promise<ChapterInfo[]> {
  return invoke<ChapterInfo[]>("parse_document_chapters", { content });
}

// ─── Personalization IPC (E) ───

export async function profileList(params: {
  include_inactive?: boolean;
}): Promise<ProfileEntry[]> {
  return invoke<ProfileEntry[]>("profile_list", {
    includeInactive: params.include_inactive ?? false,
  });
}

export async function profileGet(params: {
  key: string;
}): Promise<ProfileEntry | null> {
  return invoke<ProfileEntry | null>("profile_get", { key: params.key });
}

export async function profileSet(params: {
  key: string;
  value: unknown;
  source: string;
  confidence?: number;
}): Promise<void> {
  return invoke("profile_set", {
    key: params.key,
    value: params.value,
    source: params.source,
    confidence: params.confidence ?? 1.0,
  });
}

/** 以纯文本保存用户确认的规则（Phase 5） */
export async function profileSetRule(params: {
  key: string;
  description: string;
  source?: string;
}): Promise<void> {
  return invoke("profile_set_rule", {
    key: params.key,
    description: params.description,
    source: params.source ?? null,
  });
}

export async function profileDeactivate(params: {
  key: string;
}): Promise<void> {
  return invoke("profile_deactivate", { key: params.key });
}

export async function profileDelete(params: { key: string }): Promise<void> {
  return invoke("profile_delete", { key: params.key });
}

export async function inboxList(params: {
  status?: string;
}): Promise<InboxItem[]> {
  return invoke<InboxItem[]>("inbox_list", {
    status: params.status ?? "inbox",
  });
}

export async function inboxAdd(params: {
  deposit_type: string;
  content: string;
  source_note?: string;
  session_id?: number;
}): Promise<number> {
  return invoke("inbox_add", {
    depositType: params.deposit_type,
    content: params.content,
    sourceNote: params.source_note ?? null,
    sessionId: params.session_id ?? null,
  });
}

export async function inboxUpdateStatus(params: {
  deposit_id: number;
  new_status: string;
  target_path?: string;
}): Promise<void> {
  return invoke("inbox_update_status", {
    depositId: params.deposit_id,
    newStatus: params.new_status,
    targetPath: params.target_path ?? null,
  });
}

export async function inboxDelete(params: {
  deposit_id: number;
}): Promise<void> {
  return invoke("inbox_delete", { depositId: params.deposit_id });
}

export async function inboxCounts(): Promise<{
  inbox: number;
  archived: number;
  written: number;
}> {
  return invoke("inbox_counts");
}

/** 桌面顶栏与交通灯留白（逻辑像素），与 Rust `chrome_metrics` 一致 */
export interface DesktopChromeMetrics {
  titlebarHeightLogical: number;
  trafficInsetLogical: number;
  scaleFactor: number;
}

/** Exit the desktop app after close guards have finished. */
export async function appExit(): Promise<AppExitResult> {
  return invoke<AppExitResult>("app_exit");
}

/** 读取当前平台顶栏指标并用于 CSS 变量同步 */
export async function getDesktopChromeMetrics(): Promise<DesktopChromeMetrics> {
  return invoke<DesktopChromeMetrics>("get_desktop_chrome_metrics");
}

/** macOS：全屏/缩放后重新应用交通灯位置与窗口标题 */
export async function reapplyWindowChrome(): Promise<void> {
  return invoke("reapply_window_chrome");
}
