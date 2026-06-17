import { joinVaultChildPath } from "@/lib/vault-tree";
import type { FileListItem } from "@/types/ipc";

export type CorpusKind = "authority" | "exemplar" | "reference" | "lookup";
export type StoredCorpusKind = CorpusKind | "regulation" | "general";

export type RenameTarget =
  | { kind: "file"; file: FileListItem }
  | { kind: "folder"; path: string };

export type MoveTarget =
  | { kind: "file"; file: FileListItem }
  | { kind: "files"; files: FileListItem[] }
  | { kind: "folder"; path: string };

export function slugFromPath(prefix: string): string {
  return prefix
    .replace(/\\/g, "/")
    .replace(/\/$/, "")
    .split("/")
    .filter(Boolean)
    .join("_")
    .replace(/[^a-zA-Z0-9_\u4e00-\u9fff-]/g, "_")
    .toLowerCase();
}

export function canonicalCorpusKind(kind: string): CorpusKind {
  switch (kind) {
    case "authority":
    case "regulation":
      return "authority";
    case "exemplar":
      return "exemplar";
    case "reference":
      return "reference";
    case "lookup":
    case "general":
      return "lookup";
    default:
      return "authority";
  }
}

export function defaultScenesForKind(kind: StoredCorpusKind): string[] {
  switch (canonicalCorpusKind(kind)) {
    case "authority":
      return ["knowledge_lookup", "research_synthesis", "drafting_assist"];
    case "exemplar":
      return ["exemplar_learning", "drafting_assist"];
    case "reference":
    case "lookup":
      return ["knowledge_lookup", "research_synthesis"];
    default:
      return [];
  }
}

export function isInvalidFolderName(name: string): boolean {
  return /[\\/:*?"<>|]/.test(name) || name === "." || name === "..";
}

export function normalizeFolderPrefix(path: string): string {
  const normalized = path.replace(/\\/g, "/").replace(/^\/+/, "");
  if (!normalized) return "";
  return normalized.endsWith("/") ? normalized : `${normalized}/`;
}

export function displayFolderPath(path: string): string {
  return path ? normalizeFolderPrefix(path) : "全部笔记";
}

export function folderNameFromPath(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/$/, "").split("/").pop() ?? "";
}

export function fileNameFromPath(path: string): string {
  return path.replace(/\\/g, "/").split("/").pop() ?? path;
}

export function fileParentPath(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const index = normalized.lastIndexOf("/");
  return index >= 0 ? normalized.slice(0, index + 1) : "";
}

export function normalizeDocumentName(name: string): string {
  const trimmed = name.trim();
  if (!trimmed) return "";
  return trimmed.toLowerCase().endsWith(".md") ? trimmed : `${trimmed}.md`;
}

export function isInvalidLeafName(name: string): boolean {
  return isInvalidFolderName(name) || name.includes("/") || name.includes("\\");
}

export function buildFolderPath(parentPath: string, name: string): string {
  return joinVaultChildPath(parentPath, name);
}

export function buildFolderPrefix(parentPath: string, name: string): string {
  return normalizeFolderPrefix(buildFolderPath(parentPath, name));
}

export function availableMoveFolders(
  folders: string[],
  target: MoveTarget | null,
): string[] {
  const normalized = Array.from(
    new Set(folders.map(normalizeFolderPrefix).filter(Boolean)),
  ).sort((a, b) => a.localeCompare(b, "zh-Hans-CN"));
  if (!target || target.kind === "file" || target.kind === "files") {
    return ["", ...normalized];
  }
  const current = normalizeFolderPrefix(target.path);
  return [
    "",
    ...normalized.filter(
      (folder) => folder !== current && !folder.startsWith(current),
    ),
  ];
}
