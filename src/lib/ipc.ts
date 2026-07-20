import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { IPC_EVENTS } from "@/lib/ipc-events";

import type {
  AssistantRunAccepted,
  AssistantRunControlRequest,
  AssistantRunEvent,
  AssistantRunGetRequest,
  AssistantRunGetResponse,
  AssistantRunRetryRequest,
  AssistantRunStartRequest,
  ClassifiedDocumentContext,
  ClassifiedRunResultRequest,
  AssistantSessionListRequest,
  AssistantSessionLoadRequest,
  AssistantSessionMessage,
  AssistantSessionRef,
  AssistantSessionRenameRequest,
  AssistantSessionRetractRequest,
  AssistantSessionSummary,
} from "@/types/ai";

export type {
  AssistantRunAccepted,
  AssistantRunControlRequest,
  AssistantRunEvent,
  AssistantRunGetRequest,
  AssistantRunGetResponse,
  AssistantRunRetryRequest,
  AssistantRunStartRequest,
  ClassifiedDocumentContext,
  ClassifiedRunResultRequest,
  AssistantSessionListRequest,
  AssistantSessionLoadRequest,
  AssistantSessionMessage,
  AssistantSessionRef,
  AssistantSessionRenameRequest,
  AssistantSessionRetractRequest,
  AssistantSessionSummary,
} from "@/types/ai";
import type {
  AiCacheClearResult,
  AppUpdateInfo,
  AppUpdatePreflightResult,
  AppUpdateProgressEvent,
  AppUpdateStateEvent,
  AppExitResult,
  BacklinkEntry,
  ClassifiedFileTakenEvent,
  ClassifiedFileEntry,
  ClassifiedStatus,
  DocumentRecoveryAudit,
  CredentialStatus,
  DocumentOpenResult,
  DocumentTitleAuditItem,
  DocumentOpenScopeResult,
  EmbeddingIndexStatus,
  EmbeddingSchedulerStartResult,
  FileChangedEvent,
  FileEntry,
  FileWriteIndexStatus,
  FileWriteResult,
  FileLinkSummary,
  FileListItem,
  FileReadResult,
  FileSignatureResult,
  GraphData,
  InboxItem,
  KeywordHit,
  LlmProviderInfo,
  MediaMetadata,
  MediaResolveResult,
  ProfileEntry,
  RecycleBinItem,
  SemanticHit,
  TagGroup,
  VersionEntry,
  VersionSaveOutcome,
  VersionSaveCompleteEvent,
  WorkspaceItem,
} from "@/types/ipc";

import type {
  ConnectivityStatus,
  LlmConfigGetResponse,
  LlmConfigTestResult,
  LlmModelRegistryRefreshResult,
  LlmRoutingConfig,
  ModelValidationKind,
} from "@/types/llm";

export interface SettingsMap {
  theme: "dark" | "light";
  llm_custom_base_url: string | null;
  /** Enables approved web-search providers for a Run. */
  web_search_enabled: boolean;
  /** Enables automatic version snapshots. */
  auto_version_enabled: boolean;
  /** Idle-minute threshold before an automatic version snapshot. */
  auto_version_idle_minutes: number;
  /** Follow OS system proxy / HTTP(S)_PROXY for HTTPS exits (updates, LLM, fetch). */
  follow_system_proxy: boolean;
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

export interface NetworkProxyStatus {
  follow: boolean;
  label: string;
}

export async function networkProxyStatus(): Promise<NetworkProxyStatus> {
  return invoke<NetworkProxyStatus>("network_proxy_status");
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
): Promise<FileWriteResult> {
  return invoke<FileWriteResult>("file_write", { path, content });
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
): Promise<FileWriteResult> {
  return invoke<FileWriteResult>("file_rename", { path, newPath });
}

/** Atomically allocate and move a note to the basename entered inline. */
export async function documentRenameByTitle(
  path: string,
  title: string,
): Promise<FileWriteResult> {
  return invoke<FileWriteResult>("document_rename_by_title", { path, title });
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
): Promise<FileWriteIndexStatus> {
  return invoke<FileWriteIndexStatus>("folder_rename", { oldPath, newPath });
}

export async function folderDelete(path: string): Promise<void> {
  return invoke("folder_delete", { path });
}

export async function recycleList(): Promise<RecycleBinItem[]> {
  return invoke<RecycleBinItem[]>("recycle_list_cmd");
}

export async function recycleRestore(id: string): Promise<FileWriteResult> {
  return invoke<FileWriteResult>("recycle_restore_cmd", { id });
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

/** Creates a manual snapshot and resolves only after the durable result is known. */
export async function versionSaveManual(
  path: string,
  content: string,
): Promise<VersionSaveOutcome> {
  return invoke<VersionSaveOutcome>("version_save_manual_cmd", {
    path,
    content,
  });
}

/** Creates an idle snapshot and resolves only after the durable result is known. */
export async function versionSaveIdle(
  path: string,
  content: string,
): Promise<VersionSaveOutcome> {
  return invoke<VersionSaveOutcome>("version_save_idle_cmd", {
    path,
    content,
  });
}

/** Find title corruption candidates without modifying Markdown. */
export async function documentTitleAudit(): Promise<DocumentTitleAuditItem[]> {
  return invoke<DocumentTitleAuditItem[]>("document_title_audit_cmd");
}

/** Audits title corruption, missing Markdown, and recoverable orphan CAS blobs. */
export async function documentRecoveryAudit(): Promise<DocumentRecoveryAudit> {
  return invoke<DocumentRecoveryAudit>("document_recovery_audit_cmd");
}

/** Recreates a missing indexed note from one audited version snapshot. */
export async function documentRecoveryRestoreMissing(
  path: string,
  versionId: number,
  expectedContentHash: string,
): Promise<FileWriteResult> {
  return invoke<FileWriteResult>("document_recovery_restore_missing_cmd", {
    path,
    versionId,
    expectedContentHash,
    confirmed: true,
  });
}

/** Recreates an audited orphan CAS Markdown blob at a new user-selected path. */
export async function documentRecoveryRestoreOrphan(
  objectHash: string,
  targetPath: string,
): Promise<FileWriteResult> {
  return invoke<FileWriteResult>("document_recovery_restore_orphan_cmd", {
    objectHash,
    targetPath,
    confirmed: true,
  });
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

/** Reads the single scheduler-owned embedding generation state. */
export async function embeddingSchedulerStatus(): Promise<EmbeddingIndexStatus> {
  return invoke<EmbeddingIndexStatus>("embedding_scheduler_status");
}

/** Starts background embedding work, or reports that its single worker already runs. */
export async function embeddingSchedulerStart(): Promise<EmbeddingSchedulerStartResult> {
  return invoke<EmbeddingSchedulerStartResult>("embedding_scheduler_start");
}

/** Requests a pause or resume at the scheduler's next batch boundary. */
export async function embeddingSchedulerSetPaused(
  paused: boolean,
): Promise<void> {
  return invoke("embedding_scheduler_set_paused", { paused });
}

/** Reports foreground editing activity to the scheduler's idle policy. */
export async function embeddingSchedulerSetForegroundBusy(
  busy: boolean,
): Promise<void> {
  return invoke("embedding_scheduler_set_foreground_busy", { busy });
}

/** Subscribes the central scheduler hook to complete status snapshots. */
export async function listenEmbeddingSchedulerStatus(
  handler: (payload: EmbeddingIndexStatus) => void,
): Promise<() => void> {
  return listen<EmbeddingIndexStatus>(
    IPC_EVENTS.EMBEDDING_INDEX_PROGRESS,
    (e) => handler(e.payload),
  );
}

export async function llmProviders(): Promise<LlmProviderInfo[]> {
  return invoke<LlmProviderInfo[]>("llm_providers");
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

export async function connectivityStatus(): Promise<ConnectivityStatus> {
  return invoke<ConnectivityStatus>("connectivity_status");
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

export async function appUpdateCheck(): Promise<AppUpdateStateEvent> {
  return invoke<AppUpdateStateEvent>("app_update_check_cmd");
}

export async function appUpdateDownload(): Promise<AppUpdateStateEvent> {
  return invoke<AppUpdateStateEvent>("app_update_download_cmd");
}

export async function appUpdatePreflight(): Promise<AppUpdatePreflightResult> {
  return invoke<AppUpdatePreflightResult>("app_update_preflight_cmd");
}

export async function appUpdateInstall(): Promise<void> {
  return invoke("app_update_install_cmd");
}

export async function listenAppUpdateStatus(
  handler: (payload: AppUpdateStateEvent) => void,
): Promise<() => void> {
  return listen<AppUpdateStateEvent>(IPC_EVENTS.APP_UPDATE_STATUS, (e) =>
    handler(e.payload),
  );
}

export async function listenAppUpdateProgress(
  handler: (payload: AppUpdateProgressEvent) => void,
): Promise<() => void> {
  return listen<AppUpdateProgressEvent>(IPC_EVENTS.APP_UPDATE_PROGRESS, (e) =>
    handler(e.payload),
  );
}

export type { AppUpdateInfo };

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

export async function listenAssistantRunEvent(
  handler: (event: AssistantRunEvent) => void,
): Promise<() => void> {
  return listen<AssistantRunEvent>(IPC_EVENTS.ASSISTANT_RUN_EVENT, (event) =>
    handler(event.payload),
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
  isRuntimeSelected: boolean;
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
  providerId: string,
): Promise<WebEvidenceProviderDiagnostics> {
  return invoke<WebEvidenceProviderDiagnostics>(
    "web_evidence_provider_diagnostics",
    { providerId },
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

export async function skillsList(): Promise<SkillListEntryDto[]> {
  return invoke<SkillListEntryDto[]>("skills_list");
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

export async function aiCacheClear(): Promise<AiCacheClearResult> {
  return invoke<AiCacheClearResult>("ai_cache_clear");
}

export async function assistantRunStart(
  request: AssistantRunStartRequest,
): Promise<AssistantRunAccepted> {
  return invoke<AssistantRunAccepted>("assistant_run_start", { request });
}

export async function assistantRunRetry(
  request: AssistantRunRetryRequest,
): Promise<AssistantRunAccepted> {
  return invoke<AssistantRunAccepted>("assistant_run_retry", { request });
}

export async function assistantRunControl(
  request: AssistantRunControlRequest,
): Promise<void> {
  return invoke<void>("assistant_run_control", { request });
}

export async function assistantRunGet(
  request: AssistantRunGetRequest,
): Promise<AssistantRunGetResponse | null> {
  return invoke<AssistantRunGetResponse | null>("assistant_run_get", {
    request,
  });
}

export async function assistantClassifiedContextOpen(
  path: string,
): Promise<ClassifiedDocumentContext> {
  return invoke<ClassifiedDocumentContext>(
    "assistant_classified_context_open",
    {
      path,
    },
  );
}

export async function assistantClassifiedContextClear(): Promise<void> {
  return invoke<void>("assistant_classified_context_clear");
}

export async function assistantClassifiedRunTakeResult(
  request: ClassifiedRunResultRequest,
): Promise<string> {
  return invoke<string>("assistant_classified_run_take_result", { request });
}

/** List conversations through the only domain-routed history API. */
export async function assistantSessionList(
  request: AssistantSessionListRequest,
): Promise<AssistantSessionSummary[]> {
  return invoke<AssistantSessionSummary[]>("assistant_session_list", {
    request,
  });
}

export async function assistantSessionLoad(
  request: AssistantSessionLoadRequest,
): Promise<AssistantSessionMessage[]> {
  return invoke<AssistantSessionMessage[]>("assistant_session_load", {
    request,
  });
}

export async function assistantSessionRename(
  request: AssistantSessionRenameRequest,
): Promise<void> {
  return invoke<void>("assistant_session_rename", { request });
}

export async function assistantSessionDelete(
  session: AssistantSessionRef,
): Promise<boolean> {
  return invoke<boolean>("assistant_session_delete", { request: { session } });
}

export async function assistantSessionRetract(
  request: AssistantSessionRetractRequest,
): Promise<number> {
  return invoke<number>("assistant_session_retract", { request });
}
// Knowledge Index IPC

export async function knowledgeReindex(): Promise<{
  anchors: number;
  regulations: number;
}> {
  return invoke("knowledge_reindex");
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

/** Persist a user-managed AI rule. */
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

/** Desktop chrome metrics returned by the Rust `chrome_metrics` command. */
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

/** Fetch desktop chrome metrics to synchronize CSS variables. */
export async function getDesktopChromeMetrics(): Promise<DesktopChromeMetrics> {
  return invoke<DesktopChromeMetrics>("get_desktop_chrome_metrics");
}

/** Reapply native window chrome after a platform-level window change. */
export async function reapplyWindowChrome(): Promise<void> {
  return invoke("reapply_window_chrome");
}

// Classified AI cache IPC.
export async function classifiedAiCacheClear(): Promise<void> {
  return invoke("classified_ai_cache_clear");
}

/** Clear the in-memory classified retrieval chunk index. */
export async function classifiedAiRetrievalClear(): Promise<void> {
  return invoke("classified_ai_retrieval_clear");
}
