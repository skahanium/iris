import {
  displayTitleForFileListItem,
  noteListSubtitle,
} from "@/lib/note-display";
import type { ContextScope } from "@/types/ai";
import type { FileListItem } from "@/types/ipc";

/** Token inserted in the composer: `@[path or prefix]` or `#[tag]` */
const MENTION_TOKEN_RE = /@\[([^\]]+)\]/g;
const TAG_TOKEN_RE = /#\[([^\]]+)\]/g;

export interface MentionToken {
  raw: string;
  /** Folder prefix (ends with `/`), file path, or tag name */
  value: string;
  kind: "folder" | "file" | "tag";
  label: string;
}

export function isFolderMention(value: string): boolean {
  const v = value.trim();
  return !v.toLowerCase().endsWith(".md");
}

export function normalizeFolderPrefix(value: string): string {
  const trimmed = value.trim().replace(/\\/g, "/");
  if (!trimmed) return "";
  return trimmed.endsWith("/") ? trimmed : `${trimmed}/`;
}

export function parseMentionTokens(text: string): MentionToken[] {
  const tokens: MentionToken[] = [];
  for (const match of text.matchAll(MENTION_TOKEN_RE)) {
    const value = match[1]?.trim() ?? "";
    if (!value) continue;
    const kind = isFolderMention(value) ? "folder" : "file";
    tokens.push({
      raw: match[0] ?? "",
      value:
        kind === "folder"
          ? normalizeFolderPrefix(value)
          : value.replace(/\\/g, "/"),
      kind,
      label: value.replace(/\\/g, "/").replace(/\/$/, "") || value,
    });
  }
  // Parse #tag tokens
  for (const match of text.matchAll(TAG_TOKEN_RE)) {
    const value = match[1]?.trim() ?? "";
    if (!value) continue;
    tokens.push({
      raw: match[0] ?? "",
      value: value.toLowerCase(),
      kind: "tag",
      label: value,
    });
  }
  return tokens;
}

export function tokensToContextScope(tokens: MentionToken[]): ContextScope {
  const paths: string[] = [];
  const pathPrefixes: string[] = [];
  const requiredTags: string[] = [];
  for (const t of tokens) {
    if (t.kind === "file") {
      if (!paths.includes(t.value)) paths.push(t.value);
    } else if (t.kind === "folder") {
      if (!pathPrefixes.includes(t.value)) pathPrefixes.push(t.value);
    } else if (t.kind === "tag") {
      if (!requiredTags.includes(t.value)) requiredTags.push(t.value);
    }
  }
  return { paths, pathPrefixes, requiredTags };
}

/** User-visible message with `@[...]` and `#[...]` tokens rendered as readable text. */
export function stripMentionTokensForDisplay(text: string): string {
  return text
    .replace(MENTION_TOKEN_RE, (_raw, value: string) => {
      const label = value.replace(/\\/g, "/").replace(/\/$/, "").trim();
      return label ? `@${label}` : "";
    })
    .replace(TAG_TOKEN_RE, (_raw, value: string) => {
      const label = value.trim();
      return label ? `#${label}` : "";
    })
    .replace(/[ \t]{2,}/g, " ")
    .replace(/[ \t]+\n/g, "\n")
    .replace(/\n[ \t]+/g, "\n")
    .trim();
}

export interface MentionCandidate {
  id: string;
  kind: "folder" | "file" | "tag";
  label: string;
  subtitle?: string;
  value: string;
}

export function collectFolderPrefixes(files: FileListItem[]): string[] {
  const prefixes = new Set<string>();
  for (const f of files) {
    const parts = f.path.replace(/\\/g, "/").split("/");
    if (parts.length <= 1) continue;
    let acc = "";
    for (let i = 0; i < parts.length - 1; i += 1) {
      acc += `${parts[i]}/`;
      prefixes.add(acc);
    }
  }
  return [...prefixes].sort();
}

export function buildMentionCandidates(
  files: FileListItem[],
  query: string,
): MentionCandidate[] {
  const q = query.trim().toLowerCase();
  const folders = collectFolderPrefixes(files)
    .map((prefix) => ({
      id: `folder:${prefix}`,
      kind: "folder" as const,
      label: prefix.replace(/\/$/, "") || prefix,
      subtitle: prefix,
      value: prefix,
    }))
    .filter(
      (item) =>
        !q ||
        item.label.toLowerCase().includes(q) ||
        item.value.toLowerCase().includes(q),
    );

  const docs = files
    .map((f) => ({
      id: `file:${f.path}`,
      kind: "file" as const,
      label: displayTitleForFileListItem(f),
      subtitle: noteListSubtitle(f.path),
      value: f.path,
    }))
    .filter(
      (item) =>
        !q ||
        item.label.toLowerCase().includes(q) ||
        item.value.toLowerCase().includes(q),
    );

  return [...folders, ...docs].slice(0, 40);
}

export function findActiveMentionQuery(
  text: string,
  cursor: number,
): { start: number; query: string; prefix: "@" | "#" } | null {
  const before = text.slice(0, cursor);
  // Check @ mention
  const at = before.lastIndexOf("@");
  const hash = before.lastIndexOf("#");
  const latest = Math.max(at, hash);
  if (latest < 0) return null;
  const prefix: "@" | "#" = latest === at ? "@" : "#";
  const segment = before.slice(latest + 1);
  if (segment.includes("\n") || segment.includes(" ")) return null;
  if (latest > 0 && !/[\s([{「]/.test(before[latest - 1] ?? "")) {
    return null;
  }
  return { start: latest, query: segment, prefix };
}

export function insertMentionToken(
  text: string,
  cursor: number,
  mentionStart: number,
  candidate: MentionCandidate,
): { text: string; cursor: number } {
  const tokenValue =
    candidate.kind === "folder"
      ? normalizeFolderPrefix(candidate.value)
      : candidate.value;
  const bracket = candidate.kind === "tag" ? "#" : "@";
  const token = `${bracket}[${tokenValue}]`;
  const next = `${text.slice(0, mentionStart)}${token} ${text.slice(cursor)}`;
  const nextCursor = mentionStart + token.length + 1;
  return { text: next, cursor: nextCursor };
}
