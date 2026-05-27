import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type {
  BacklinkEntry,
  FileChangedEvent,
  FileEntry,
  FileListItem,
  RecycleBinItem,
  GraphData,
  KeywordHit,
  LlmGenerateParams,
  LlmProviderInfo,
  LlmTokenEvent,
  SemanticHit,
  TagGroup,
  VersionEntry,
} from "@/types/ipc";

export interface SettingsMap {
  theme: "dark" | "light";
  llm_custom_base_url: string | null;
}

export async function settingsGet<K extends keyof SettingsMap>(key: K): Promise<SettingsMap[K] | null>;
export async function settingsGet<T>(key: string): Promise<T | null>;
export async function settingsGet<T>(key: string): Promise<T | null> {
  return invoke<T | null>("settings_get", { key });
}

export async function settingsSet<K extends keyof SettingsMap>(key: K, value: SettingsMap[K]): Promise<void>;
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

export async function fileRead(path: string): Promise<string> {
  return invoke<string>("file_read", { path });
}

export async function fileWrite(
  path: string,
  content: string,
): Promise<FileEntry> {
  return invoke<FileEntry>("file_write", { path, content });
}

export async function fileCreate(
  path: string,
  content?: string,
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

export async function fileBacklinks(path: string): Promise<BacklinkEntry[]> {
  return invoke<BacklinkEntry[]>("file_backlinks", { path });
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

export async function versionSaveManual(
  path: string,
  content: string,
): Promise<VersionEntry | null> {
  return invoke<VersionEntry | null>("version_save_manual_cmd", {
    path,
    content,
  });
}

export async function versionSaveIdle(
  path: string,
  content: string,
): Promise<VersionEntry | null> {
  return invoke<VersionEntry | null>("version_save_idle_cmd", {
    path,
    content,
  });
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

// ─── AI Runtime IPC ───

import type { AiScene, AssembledContext, ContextPacket } from "@/types/ai";

export async function contextAssemble(params: {
  scene: AiScene;
  note_path: string | null;
  note_content_hash: string | null;
  query: string;
  session_id: number | null;
}): Promise<AssembledContext> {
  return invoke<AssembledContext>("context_assemble", {
    scene: params.scene,
    notePath: params.note_path,
    noteContentHash: params.note_content_hash,
    query: params.query,
    sessionId: params.session_id,
  });
}

export async function aiSendMessage(params: {
  scene: AiScene;
  session_id: number | null;
  message: string;
  selected_packet_ids?: string[];
}): Promise<{
  request_id: string;
  session_id: number;
  status: string;
  content?: string;
  tool_calls?: Array<{ id: string; function: { name: string; arguments: string } }>;
  tool_results?: Array<{ tool_call_id: string; status: string; result?: unknown }>;
  usage?: { prompt_tokens: number; completion_tokens: number; total_tokens: number };
  citation_valid?: boolean;
}> {
  return invoke("ai_send_message", {
    scene: params.scene,
    sessionId: params.session_id,
    message: params.message,
    selectedPacketIds: params.selected_packet_ids ?? null,
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

export async function aiListTools(
  scene: AiScene,
): Promise<{ name: string; description: string; requires_confirmation: boolean; access_level: string }[]> {
  return invoke("ai_list_tools", { scene });
}

// ─── Knowledge Index IPC ───

export async function knowledgeReindex(): Promise<{ anchors: number; regulations: number }> {
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
}): Promise<{
  request_id: string;
  topic: string;
  rounds: number;
  evidence_matrix: {
    topic: string;
    propositions: Array<{
      id: string;
      statement: string;
      evidence: ContextPacket[];
      gaps: string[];
    }>;
    global_gaps: string[];
    total_evidence_count: number;
    coverage_score: number;
  };
  argument_chain: {
    links: Array<{
      from_proposition_id: string;
      to_proposition_id: string;
      link_type: string;
      strength: number;
    }>;
    has_contradictions: boolean;
    chain_strength: number;
  };
  summary: string;
  total_tokens: { prompt_tokens: number; completion_tokens: number; total_tokens: number };
}> {
  return invoke("research_execute", {
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

// ─── Personalization IPC (E) ───

export async function profileList(params: {
  include_inactive?: boolean;
}): Promise<
  Array<{
    key: string;
    value: unknown;
    source: string;
    confidence: number;
    is_active: boolean;
    updated_at: string;
  }>
> {
  return invoke("profile_list", {
    includeInactive: params.include_inactive ?? false,
  });
}

export async function profileGet(params: {
  key: string;
}): Promise<{
  key: string;
  value: unknown;
  source: string;
  confidence: number;
  is_active: boolean;
  updated_at: string;
} | null> {
  return invoke("profile_get", { key: params.key });
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

export async function profileDeactivate(params: {
  key: string;
}): Promise<void> {
  return invoke("profile_deactivate", { key: params.key });
}

export async function profileDelete(params: {
  key: string;
}): Promise<void> {
  return invoke("profile_delete", { key: params.key });
}

export async function inboxList(params: {
  status?: string;
}): Promise<
  Array<{
    id: number;
    session_id: number | null;
    source_note: string | null;
    deposit_type: string;
    content: string;
    status: string;
    target_path: string | null;
    created_at: string;
    updated_at: string;
  }>
> {
  return invoke("inbox_list", { status: params.status ?? "inbox" });
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
