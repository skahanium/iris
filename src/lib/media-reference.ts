export type WorkspaceItemKind = "note" | "media" | "unsupported";
export type MediaKind = "image" | "pdf" | "video";
export type AttachmentRole = "attachment" | "formal";

export interface WikiMediaReference {
  alias: string | null;
  embed: boolean;
  raw: string;
  target: string;
}

export interface WorkspacePathClassification {
  kind: WorkspaceItemKind;
  mediaKind: MediaKind | null;
}

const IMAGE_EXTENSIONS = new Set(["avif", "gif", "jpeg", "jpg", "png", "webp"]);
const VIDEO_EXTENSIONS = new Set(["m4v", "mov", "mp4", "webm"]);

function normalizedPath(path: string): string {
  return path.trim().replace(/\\/g, "/").replace(/^\/+/, "");
}

function extensionOf(path: string): string {
  const clean = normalizedPath(path).split(/[?#]/, 1)[0] ?? "";
  const fileName = clean.split("/").pop() ?? clean;
  const dot = fileName.lastIndexOf(".");
  if (dot < 0 || dot === fileName.length - 1) return "";
  return fileName.slice(dot + 1).toLowerCase();
}

export function parseWikiMediaReference(
  raw: string,
): WikiMediaReference | null {
  const trimmed = raw.trim();
  const match = /^(!)?\[\[([^\]\n]+)\]\]$/.exec(trimmed);
  if (!match) return null;

  const body = match[2]!.trim();
  if (!body) return null;

  const separator = body.indexOf("|");
  const target = (separator >= 0 ? body.slice(0, separator) : body).trim();
  if (!target) return null;

  const alias =
    separator >= 0 ? body.slice(separator + 1).trim() || null : null;
  return {
    alias,
    embed: match[1] === "!",
    raw: trimmed,
    target,
  };
}

export function classifyWorkspacePath(
  path: string,
): WorkspacePathClassification {
  const normalized = normalizedPath(path);
  const ext = extensionOf(normalized);
  if (ext === "md") return { kind: "note", mediaKind: null };
  if (IMAGE_EXTENSIONS.has(ext)) return { kind: "media", mediaKind: "image" };
  if (VIDEO_EXTENSIONS.has(ext)) return { kind: "media", mediaKind: "video" };
  if (ext === "pdf") return { kind: "media", mediaKind: "pdf" };
  return { kind: "unsupported", mediaKind: null };
}

export function resolveAttachmentRole(
  path: string,
  attachmentRoots: readonly string[],
): AttachmentRole {
  const normalized = normalizedPath(path);
  for (const root of attachmentRoots) {
    const prefix = normalizedPath(root).replace(/\/+$/, "");
    if (!prefix) continue;
    if (normalized === prefix || normalized.startsWith(`${prefix}/`)) {
      return "attachment";
    }
  }
  return "formal";
}
