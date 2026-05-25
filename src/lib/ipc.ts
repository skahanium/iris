import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type {
  FileEntry,
  FileListItem,
  KeywordHit,
  LlmGenerateParams,
  LlmProviderInfo,
  SemanticHit,
} from "@/types/ipc";

export async function settingsGet<T>(key: string): Promise<T | null> {
  return invoke<T | null>("settings_get", { key });
}

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

export async function fileRename(path: string, newPath: string): Promise<FileEntry> {
  return invoke<FileEntry>("file_rename", { path, newPath });
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
  handler: (payload: unknown) => void,
): Promise<() => void> {
  return listen("file:changed", (e) => handler(e.payload));
}

export async function listenLlmToken(
  handler: (payload: unknown) => void,
): Promise<() => void> {
  return listen("llm:token", (e) => handler(e.payload));
}

export async function listenLlmDone(
  handler: (payload: unknown) => void,
): Promise<() => void> {
  return listen("llm:done", (e) => handler(e.payload));
}

export async function listenLlmError(
  handler: (payload: unknown) => void,
): Promise<() => void> {
  return listen("llm:error", (e) => handler(e.payload));
}
