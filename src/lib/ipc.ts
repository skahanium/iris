import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { IPC_EVENTS } from "@/lib/ipc-events";

import type {
  AgentIntent,
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
  ToolAccessLevel,
  WritingExecuteResult,
} from "@/types/ai";
import type {
  AiCacheClearResult,
  AgentTaskDto,
  AgentTaskEventDto,
  AgentTaskListParams,
  AgentTaskStepDto,
  AppExitResult,
  BacklinkEntry,
  ClassifiedFileTakenEvent,
  FileChangedEvent,
  FileEntry,
  FileLinkSummary,
  FileListItem,
  ClassifiedFileEntry,
  ClassifiedStatus,
  CredentialStatus,
  FileReadResult,
  FileSignatureResult,
  DocumentOpenScopeResult,
  DocumentOpenResult,
  EmbeddingIndexStatus,
  GraphData,
  ImageAttachmentDto,
  InboxItem,
  KeywordHit,
  LlmGenerateParams,
  LlmProviderInfo,
  LlmTokenEvent,
  MediaMetadata,
  MediaResolveResult,
  ProfileEntry,
  RecycleBinItem,
  SemanticHit,
  SessionEvidenceRecord,
  SessionEvidenceDetailRecord,
  SessionEvidenceRegisterPacket,
  SessionMessageRecord,
  SessionSummary,
  TagGroup,
  ToolConfirmRequestEvent,
  VersionEntry,
  VersionSaveCompleteEvent,
  WorkspaceItem,
} from "@/types/ipc";
import type {
  ConnectivityStatus,
  LlmConfigGetResponse,
  LlmConfigTestResult,
  LlmModelRegistryRefreshResult,
  LlmRoutingConfig,
  ModelCapabilityConfirmRequest,
  ModelRegistryEntry,
  ModelValidationKind,
} from "@/types/llm";

export interface SettingsMap {
  theme: "dark" | "light";
  llm_custom_base_url: string | null;
  /** 底栏“联网”开关，跨会话保持。 */
  web_search_enabled: boolean;
  /** 自动版本追踪总开关，默认开启。 */
  auto_version_enabled: boolean;
  /** 自动版本追踪空闲间隔，单位分钟。 */
  auto_version_idle_minutes: number;
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

export async function settingsReset<K extends keyof SettingsMap>(
  key: K,
): Promise<void>;
export async function settingsReset(key: string): Promise<void>;
export async function settingsReset(key: string): Promise<void> {
  return invoke("settings_reset", { key });
}

export async function vaultSet(path: string): Promise<void> {
  return invoke("vault_set", { path });
}

export async function vaultGet(): Promise<string | null> {
  return invoke<string | null>("vault_get");
}

export async function fileList(opts?: {
  limit?: number;
  offset?: number;
}): Promise<FileListItem[]> {
  return invoke<FileListItem[]>("file_list", {
    limit: opts?.limit ?? null,
    offset: opts?.offset ?? null,
  });
}

export async function workspaceList(opts?: {
  limit?: number;
  offset?: number;
}): Promise<WorkspaceItem[]> {
  return invoke<WorkspaceItem[]>("workspace_list", {
    limit: opts?.limit ?? null,
    offset: opts?.offset ?? null,
  });
}

export async function mediaMetadata(path: string): Promise<MediaMetadata> {
  return invoke<MediaMetadata>("media_metadata", { path });
}

export async function mediaResolve(path: string): Promise<MediaResolveResult> {
  return invoke<MediaResolveResult>("media_resolve", { path });
}

export async function mediaRelease(handle: string): Promise<void> {
  return invoke("media_release", { handle });
}

export async function folderList(): Promise<string[]> {
  return invoke<string[]>("folder_list");
}

export async function fileRead(
  path: string,
  options?: { allowClassified?: boolean },
): Promise<FileReadResult> {
  return invoke<FileReadResult>("file_read", {
    path,
    allowClassified: options?.allowClassified === true,
  });
}

export async function fileSignature(
  path: string,
  options?: { allowClassified?: boolean },
): Promise<FileSignatureResult> {
  return invoke<FileSignatureResult>("file_signature", {
    path,
    allowClassified: options?.allowClassified === true,
  });
}

export async function documentOpenBegin(): Promise<DocumentOpenScopeResult> {
  return invoke<DocumentOpenScopeResult>("document_open_begin");
}

export async function documentOpenEnd(token: string): Promise<void> {
  return invoke("document_open_end", { token });
}

export async function documentOpen(
  path: string,
  allowClassified?: boolean,
): Promise<DocumentOpenResult> {
  return invoke<DocumentOpenResult>("document_open", {
    path,
    allowClassified: allowClassified ?? false,
  });
}

export async function fileSetLock(
  path: string,
  locked: boolean,
): Promise<void> {
  return invoke("file_set_lock", { path, locked });
}

export async function classifiedSetup(password: string): Promise<void> {
  return invoke("classified_setup", { password });
}

export async function classifiedUnlock(password: string): Promise<void> {
  return invoke("classified_unlock", { password });
}

export async function classifiedLock(): Promise<void> {
  return invoke("classified_lock");
}

export async function classifiedStatus(): Promise<ClassifiedStatus> {
  return invoke<ClassifiedStatus>("classified_status");
}

export async function classifiedFiles(
  folder?: string,
): Promise<ClassifiedFileEntry[]> {
  return invoke<ClassifiedFileEntry[]>("classified_files", {
    folder: folder ?? null,
  });
}

export async function classifiedImport(
  path: string,
  targetFolder: string,
): Promise<void> {
  return invoke("classified_import", { path, targetFolder });
}

export async function classifiedExport(
  path: string,
  targetFolder: string,
  overwrite = false,
): Promise<void> {
  return invoke("classified_export", { path, targetFolder, overwrite });
}

export async function classifiedDelete(path: string): Promise<void> {
  return invoke("classified_delete", { path });
}

export async function classifiedMkdir(folder: string): Promise<void> {
  return invoke("classified_mkdir", { folder });
}

export async function classifiedRename(
  path: string,
  newPath: string,
): Promise<void> {
  return invoke("classified_rename", { path, newPath });
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

export async function fileLinkSummary(path: string): Promise<FileLinkSummary> {
  return invoke<FileLinkSummary>("file_link_summary", { path });
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

export async function versionCleanup(): Promise<number> {
  return invoke<number>("version_cleanup_cmd");
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
  return listen<VersionSaveCompleteEvent>(
    IPC_EVENTS.VERSION_SAVE_COMPLETE,
    (event) => handler(event.payload),
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

export async function searchEmbeddingStatus(): Promise<EmbeddingIndexStatus> {
  return invoke<EmbeddingIndexStatus>("search_embedding_status");
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

export async function llmConfigGet(): Promise<LlmConfigGetResponse> {
  return invoke<LlmConfigGetResponse>("llm_config_get");
}

export async function llmConfigSet(routing: LlmRoutingConfig): Promise<void> {
  return invoke("llm_config_set", { routing });
}

export async function llmConfigApplyDeepseekDefaults(): Promise<LlmRoutingConfig> {
  return invoke<LlmRoutingConfig>("llm_config_apply_deepseek_defaults");
}

export async function llmConfigDeleteProvider(
  providerId: string,
  deleteCredential = false,
): Promise<LlmRoutingConfig> {
  return invoke<LlmRoutingConfig>("llm_config_delete_provider", {
    providerId,
    deleteCredential,
  });
}

export async function connectivityStatus(
  scene?: string,
): Promise<ConnectivityStatus> {
  return invoke<ConnectivityStatus>("connectivity_status", { scene });
}

export async function llmConfigTest(
  providerId: string,
  model?: string,
  apiKeyOverride?: string,
): Promise<LlmConfigTestResult> {
  return invoke<LlmConfigTestResult>("llm_config_test", {
    providerId,
    model,
    apiKeyOverride,
  });
}

export async function llmConfigTestProvider(
  providerId: string,
  apiKeyOverride?: string,
): Promise<LlmConfigTestResult> {
  return invoke<LlmConfigTestResult>("llm_config_test_provider", {
    providerId,
    apiKeyOverride,
  });
}

export async function llmModelRegistryRefresh(
  providerId: string,
  apiKeyOverride?: string,
): Promise<LlmModelRegistryRefreshResult> {
  return invoke<LlmModelRegistryRefreshResult>("llm_model_registry_refresh", {
    providerId,
    apiKeyOverride,
  });
}

export async function llmModelValidate(
  providerId: string,
  modelId: string,
  kind: ModelValidationKind = "text",
  apiKeyOverride?: string,
): Promise<LlmConfigTestResult> {
  return invoke<LlmConfigTestResult>("llm_model_validate", {
    providerId,
    modelId,
    kind,
    apiKeyOverride,
  });
}

export async function llmModelConfirmCapability(
  request: ModelCapabilityConfirmRequest,
): Promise<ModelRegistryEntry> {
  return invoke<ModelRegistryEntry>("llm_model_confirm_capability", {
    request,
  });
}

export async function credentialSet(
  service: string,
  value: string,
): Promise<CredentialStatus> {
  return invoke<CredentialStatus>("credential_set", { service, value });
}

export async function credentialHas(service: string): Promise<boolean> {
  return invoke<boolean>("credential_has", { service });
}

export async function credentialStatus(
  service: string,
): Promise<CredentialStatus> {
  return invoke<CredentialStatus>("credential_status", { service });
}

export async function credentialDelete(
  service: string,
): Promise<CredentialStatus> {
  return invoke<CredentialStatus>("credential_delete", { service });
}

export async function credentialLockSession(): Promise<void> {
  return invoke("credential_lock_session");
}

export async function listenFileChanged(
  handler: (payload: FileChangedEvent) => void,
): Promise<() => void> {
  return listen<FileChangedEvent>(IPC_EVENTS.FILE_CHANGED, (e) =>
    handler(e.payload),
  );
}

export async function listenClassifiedFileTaken(
  handler: (payload: ClassifiedFileTakenEvent) => void,
): Promise<() => void> {
  return listen<ClassifiedFileTakenEvent>(
    IPC_EVENTS.CLASSIFIED_FILE_TAKEN,
    (e) => handler(e.payload),
  );
}

export async function listenSkillsChanged(
  handler: () => void,
): Promise<() => void> {
  return listen(IPC_EVENTS.SKILLS_CHANGED, () => handler());
}

export async function listenToolConfirmRequest(
  handler: (payload: ToolConfirmRequestEvent) => void,
): Promise<() => void> {
  return listen<ToolConfirmRequestEvent>(IPC_EVENTS.TOOL_CONFIRM_REQUEST, (e) =>
    handler(e.payload),
  );
}

export async function listenLlmToken(
  handler: (payload: LlmTokenEvent) => void,
): Promise<() => void> {
  return listen<LlmTokenEvent>(IPC_EVENTS.LLM_TOKEN, (e) => handler(e.payload));
}

export async function listenLlmDone(
  handler: (payload: import("@/types/ipc").LlmDoneEvent) => void,
): Promise<() => void> {
  return listen<import("@/types/ipc").LlmDoneEvent>(IPC_EVENTS.LLM_DONE, (e) =>
    handler(e.payload),
  );
}

export async function listenLlmError(
  handler: (payload: import("@/types/ipc").LlmErrorEvent) => void,
): Promise<() => void> {
  return listen<import("@/types/ipc").LlmErrorEvent>(
    IPC_EVENTS.LLM_ERROR,
    (e) => handler(e.payload),
  );
}

export async function listenLlmReset(
  handler: (payload: import("@/types/ipc").LlmResetEvent) => void,
): Promise<() => void> {
  return listen<import("@/types/ipc").LlmResetEvent>(
    IPC_EVENTS.LLM_RESET,
    (e) => handler(e.payload),
  );
}

export async function listenAiRetryStatus(
  handler: (payload: import("@/types/ipc").AiRetryStatusEvent) => void,
): Promise<() => void> {
  return listen<import("@/types/ipc").AiRetryStatusEvent>(
    IPC_EVENTS.AI_RETRY_STATUS,
    (e) => handler(e.payload),
  );
}

export async function listenHarnessTrace(
  handler: (payload: import("@/types/ipc").HarnessTraceEvent) => void,
): Promise<() => void> {
  return listen<import("@/types/ipc").HarnessTraceEvent>(
    IPC_EVENTS.HARNESS_TRACE,
    (e) => handler(e.payload),
  );
}

export async function listenAiThinking(
  handler: (payload: import("@/types/ipc").AiThinkingEvent) => void,
): Promise<() => void> {
  return listen<import("@/types/ipc").AiThinkingEvent>(
    IPC_EVENTS.AI_THINKING,
    (e) => handler(e.payload),
  );
}

export interface AiRequestStartedEvent {
  request_id: string;
  session_id?: number;
  task_id?: string | null;
  intent?: string;
  scene?: string;
  domain?: string;
  classified?: boolean;
}

export async function listenAiRequestStarted(
  handler: (payload: AiRequestStartedEvent) => void,
): Promise<() => void> {
  return listen<AiRequestStartedEvent>(IPC_EVENTS.AI_REQUEST_STARTED, (e) =>
    handler(e.payload),
  );
}

// AI Runtime IPC

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
  content: string;
  scope: string;
  enabled: boolean;
  file_path: string;
  scope_rules: Array<{ kind: string; pattern: string }>;
  confirmed_hash?: string | null;
  confirmation_status: "confirmed" | "needs_confirmation";
  legacy_trigger?: string | null;
  content_hash: string;
}

export type SkillValidationStatus = "valid" | "legacy" | { invalid: string };

export type SkillManifestKind = "legacy_prompt_only" | "prompt_only";

export interface SkillListEntryDto {
  name: string;
  description: string;
  license?: string | null;
  compatibility?: string | null;
  metadata: Record<string, unknown>;
  content: string;
  scope: string;
  enabled: boolean;
  file_path: string;
  scope_rules: Array<{ kind: string; pattern: string }>;
  content_hash: string;
  confirmed_hash?: string | null;
  confirmation_status: "confirmed" | "needs_confirmation";
  legacy_trigger?: string | null;
  validation: SkillValidationStatus;
  task_active?: boolean;
  task_score?: number;
  lastMatchedAt?: string | null;
  lastUsedAt?: string | null;
  lastActivationScore?: number | null;
  lastBlockedReason?: string | null;
  lastResourceStatus?: string | null;
  kind: SkillManifestKind;
  activation_ready: boolean;
}

export interface WebEvidenceProviderInput {
  id: string;
  name: string;
  providerKind: "native" | "mcp" | string;
  enabled: boolean;
  transportKind?: "stdio" | "https" | string | null;
  transportConfigJson: string;
  credentialRefsJson: string;
  searchMapping?: string | null;
  fetchMapping?: string | null;
}

export interface WebEvidenceProviderSummary {
  id: string;
  name: string;
  providerKind: "native" | "mcp" | string;
  enabled: boolean;
  transportKind: "native" | "stdio" | "https" | string;
  transportConfigJson: string;
  credentialRefsJson: string;
  searchMapping?: string | null;
  fetchMapping?: string | null;
  mappingStatus: "complete" | "partial" | "missing" | string;
  diagnosticStatus: "ready" | "needs_mapping" | "disabled" | string;
  isNative: boolean;
  editable: boolean;
  hasSearchMapping: boolean;
  hasFetchMapping: boolean;
}

export interface WebEvidenceProviderDiagnosticCheck {
  label: string;
  status: "pass" | "fail" | "warn" | string;
  message: string;
}

export interface WebEvidenceProviderDiagnostics {
  providerId?: string | null;
  status: string;
  failures: string[];
  checks: WebEvidenceProviderDiagnosticCheck[];
  canUseForSearch: boolean;
  canUseForFetch: boolean;
}

export interface SkillDraftScopeRule {
  kind: string;
  pattern: string;
}

export interface SkillCreateDraftRequest {
  name: string;
  description?: string | null;
  body?: string | null;
  scopeRules: SkillDraftScopeRule[];
}

export interface SkillDraft {
  name: string;
  markdown: string;
  scopeRules: SkillDraftScopeRule[];
  contentHash: string;
  targetPath: string;
}

export async function webEvidenceProviderUpsert(
  input: WebEvidenceProviderInput,
): Promise<void> {
  return invoke("web_evidence_provider_upsert", { input });
}

export async function webEvidenceProvidersList(): Promise<
  WebEvidenceProviderSummary[]
> {
  return invoke<WebEvidenceProviderSummary[]>("web_evidence_providers_list");
}

export async function webEvidenceProviderToggle(
  providerId: string,
  enabled: boolean,
): Promise<void> {
  return invoke("web_evidence_provider_toggle", { providerId, enabled });
}

export async function webEvidenceProviderDelete(
  providerId: string,
): Promise<void> {
  return invoke("web_evidence_provider_delete", { providerId });
}

export async function webEvidenceProviderDiagnostics(
  providerId?: string,
  liveCheck?: boolean,
): Promise<WebEvidenceProviderDiagnostics> {
  return invoke<WebEvidenceProviderDiagnostics>(
    "web_evidence_provider_diagnostics",
    { providerId: providerId ?? null, liveCheck: liveCheck ?? false },
  );
}

export async function skillsCreateDraft(
  request: SkillCreateDraftRequest,
): Promise<SkillDraft> {
  return invoke<SkillDraft>("skills_create_draft", { request });
}

export async function skillsConfirm(draft: SkillDraft): Promise<void> {
  return invoke("skills_confirm", { draft });
}
export interface PromptProfileDto {
  display_name: string;
  avatar_emoji: string | null;
  persona: string;
  writing_style: string;
  custom_rules: string[];
  language: string;
}

export interface SkillsPathsDto {
  global: string;
  vault: string;
}

export async function skillsList(
  scene?: AiScene,
): Promise<SkillListEntryDto[]> {
  return invoke<SkillListEntryDto[]>("skills_list", { scene: scene ?? null });
}

export async function skillsPaths(): Promise<SkillsPathsDto> {
  return invoke<SkillsPathsDto>("skills_paths");
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

export async function agentTaskGet(
  taskId: string,
): Promise<AgentTaskDto | null> {
  return invoke<AgentTaskDto | null>("agent_task_get", { taskId });
}

export async function agentTaskList(
  params: AgentTaskListParams = {},
): Promise<AgentTaskDto[]> {
  return invoke<AgentTaskDto[]>("agent_task_list", {
    sessionId: params.sessionId ?? null,
    status: params.status ?? null,
  });
}

export async function agentTaskSteps(
  taskId: string,
): Promise<AgentTaskStepDto[]> {
  return invoke<AgentTaskStepDto[]>("agent_task_steps", { taskId });
}

export async function agentTaskEvents(
  taskId: string,
): Promise<AgentTaskEventDto[]> {
  return invoke<AgentTaskEventDto[]>("agent_task_events", { taskId });
}

export async function agentTaskResume(
  taskId: string,
): Promise<AiSendMessageResult> {
  return invoke<AiSendMessageResult>("agent_task_resume", { taskId });
}

export async function agentTaskAbort(taskId: string): Promise<void> {
  return invoke("agent_task_abort", { taskId });
}

export async function harnessResume(
  requestId: string,
): Promise<AiSendMessageResult> {
  return invoke<AiSendMessageResult>("harness_resume", { requestId });
}

export async function harnessAbort(requestId: string): Promise<void> {
  return invoke("harness_abort", { requestId });
}

// Tool Audit

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

export async function sessionEvidenceList(
  sessionId: number,
): Promise<SessionEvidenceRecord[]> {
  return invoke<SessionEvidenceRecord[]>("session_evidence_list", {
    sessionId,
  });
}

export async function sessionEvidenceDetail(
  sessionId: number,
): Promise<SessionEvidenceDetailRecord[]> {
  return invoke<SessionEvidenceDetailRecord[]>("session_evidence_detail", {
    sessionId,
  });
}

export async function sessionEvidenceRegister(
  sessionId: number,
  messageSeq: number,
  packets: SessionEvidenceRegisterPacket[],
): Promise<SessionEvidenceRecord[]> {
  return invoke<SessionEvidenceRecord[]>("session_evidence_register", {
    sessionId,
    messageSeq,
    packets,
  });
}

export async function contextAssemble(params: {
  scene: AiScene;
  agent_intent?: AgentIntent;
  note_path: string | null;
  note_content_hash: string | null;
  query: string;
  session_id: number | null;
  context_scope?: ContextScope | null;
  runtime_documents?: import("@/types/ai").RuntimeDocumentSnapshot[] | null;
  webSearch?: boolean;
}): Promise<AssembledContext> {
  return invoke<AssembledContext>("context_assemble", {
    scene: params.scene,
    agentIntent: params.agent_intent ?? null,
    notePath: params.note_path,
    noteContentHash: params.note_content_hash,
    query: params.query,
    sessionId: params.session_id,
    contextScope: params.context_scope ?? null,
    runtimeDocuments: params.runtime_documents ?? null,
    webSearch: params.webSearch ?? false,
  });
}

/** 统一助手执行门面：按 intent 路由到既有工作流。 */
export async function assistantExecute(
  request: AssistantExecuteRequest,
): Promise<AssistantExecuteResponse> {
  return invoke<AssistantExecuteResponse>("assistant_execute", { request });
}

export async function aiSendMessage(params: {
  scene: AiScene;
  agent_intent?: AgentIntent;
  session_id: number | null;
  message: string;
  images?: ImageAttachmentDto[];
  note_path?: string | null;
  selected_packet_ids?: string[];
  context_scope?: ContextScope | null;
  runtime_documents?: import("@/types/ai").RuntimeDocumentSnapshot[] | null;
  /** 为 true 时通过 WebEvidenceBroker 收集联网证据。 */
  webSearch?: boolean;
  /** 为 true 时创建新会话，而不是续写当前 scene/note 会话。 */
  new_session?: boolean;
}): Promise<AiSendMessageResult> {
  return invoke<AiSendMessageResult>("ai_send_message", {
    scene: params.scene,
    agentIntent: params.agent_intent ?? null,
    sessionId: params.session_id,
    message: params.message,
    images: params.images ?? null,
    notePath: params.note_path ?? null,
    selectedPacketIds: params.selected_packet_ids ?? null,
    contextScope: params.context_scope ?? null,
    runtimeDocuments: params.runtime_documents ?? null,
    webSearch: params.webSearch ?? false,
    newSession: params.new_session ?? false,
  });
}

export async function toolConfirm(params: {
  request_id: string;
  tool_call_id: string;
  decision: "approve" | "reject" | "modify";
  modified_args?: unknown;
}): Promise<AiSendMessageResult> {
  return invoke<AiSendMessageResult>("tool_confirm", {
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
    access_level: ToolAccessLevel;
  }[]
> {
  return invoke("ai_list_tools", { scene });
}

// Knowledge Index IPC

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

// Research Workflow IPC (D)

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
  return listen<ResearchProgressEvent>(IPC_EVENTS.RESEARCH_PROGRESS, (e) =>
    handler(e.payload),
  );
}

// Writing Workflow IPC (Phase 1)

export async function writingExecute(params: {
  target_path: string;
  base_content_hash: string;
  selection?: string;
  cursor_context: string;
  writing_goal: string;
  web_authorized?: boolean;
  edit_target?: import("@/types/ai").EditTarget | null;
}): Promise<WritingExecuteResult> {
  return invoke<WritingExecuteResult>("writing_execute", {
    input: {
      target_path: params.target_path,
      base_content_hash: params.base_content_hash,
      selection: params.selection ?? null,
      cursor_context: params.cursor_context,
      writing_goal: params.writing_goal,
      web_authorized: params.web_authorized ?? false,
      edit_target: params.edit_target ?? null,
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

// Citation Check IPC (Phase 1)

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

// Organize Workflow IPC (Phase 2)

type OrganizeScopeInput = {
  paths?: string[];
  pathPrefixes?: string[];
  corpusIds?: string[];
  path_prefixes?: string[];
  corpus_ids?: string[];
};

export async function organizeExecute(params: {
  scope?: OrganizeScopeInput;
  task_type: string;
}): Promise<OrganizeExecuteResult> {
  const scope = params.scope
    ? {
        paths: params.scope.paths ?? [],
        pathPrefixes:
          params.scope.pathPrefixes ?? params.scope.path_prefixes ?? [],
        corpusIds: params.scope.corpusIds ?? params.scope.corpus_ids ?? [],
      }
    : null;

  return invoke<OrganizeExecuteResult>("organize_execute", {
    input: {
      scope,
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

// Chapter & Document Writing IPC (Phase 3)

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

// Personalization IPC (E)

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

/** 以纯文本保存用户确认的规则（Phase 5）。 */
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

/** 桌面顶栏指标（逻辑像素），与 Rust `chrome_metrics` 一致。 */
export interface DesktopChromeMetrics {
  titlebarHeightLogical: number;
  trafficInsetLogical: number;
  scaleFactor: number;
}

/** Exit the desktop app after close guards have finished. */
export async function appExit(): Promise<AppExitResult> {
  return invoke<AppExitResult>("app_exit");
}

/** Show the hidden desktop window after the startup splash has mounted. */
export async function showMainWindowWhenReady(): Promise<void> {
  return invoke("show_main_window_when_ready");
}

/** 读取当前平台顶栏指标并用于 CSS 变量同步。 */
export async function getDesktopChromeMetrics(): Promise<DesktopChromeMetrics> {
  return invoke<DesktopChromeMetrics>("get_desktop_chrome_metrics");
}

/** 重新应用无边框窗口标题与平台圆角。 */
export async function reapplyWindowChrome(): Promise<void> {
  return invoke("reapply_window_chrome");
}

// ── Classified AI Thread IPC ─────────────────────────────────────────────────

/** List classified AI threads, optionally filtered by document path. */
export async function classifiedAiThreadList(
  documentPath?: string,
): Promise<import("@/types/ipc").ClassifiedAiThreadSummary[]> {
  return invoke("classified_ai_thread_list", { documentPath });
}

/** Load a classified AI thread by id. */
export async function classifiedAiThreadLoad(
  threadId: string,
): Promise<import("@/types/ipc").ClassifiedAiThread> {
  return invoke("classified_ai_thread_load", { threadId });
}

/** Save a classified AI thread. */
export async function classifiedAiThreadSave(
  thread: import("@/types/ipc").ClassifiedAiThread,
): Promise<void> {
  return invoke("classified_ai_thread_save", { thread });
}

/** Delete a classified AI thread. */
export async function classifiedAiThreadDelete(
  threadId: string,
): Promise<void> {
  return invoke("classified_ai_thread_delete", { threadId });
}

/** Clear the in-memory classified AI thread index cache. */
export async function classifiedAiCacheClear(): Promise<void> {
  return invoke("classified_ai_cache_clear");
}

/** Search classified documents using the in-memory retrieval index. */
export async function classifiedAiContextSearch(
  query: string,
  currentDocument?: string,
  scopePaths?: string[],
  limit?: number,
): Promise<import("@/types/ipc").ClassifiedSearchHit[]> {
  return invoke("classified_ai_context_search", {
    query,
    currentDocument: currentDocument ?? null,
    scopePaths: scopePaths ?? null,
    limit: limit ?? null,
  });
}

/** Clear the in-memory classified retrieval chunk index. */
export async function classifiedAiRetrievalClear(): Promise<void> {
  return invoke("classified_ai_retrieval_clear");
}
