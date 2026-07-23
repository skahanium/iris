import {
  displayTitleForFileListItem,
  noteListSubtitle,
} from "@/lib/note-display";
import type { ContextScope, DisplayMention } from "@/types/ai";
import type { FileListItem, TagGroup } from "@/types/ipc";

export function normalizeFolderPrefix(value: string): string {
  const trimmed = value.trim().replace(/\\/g, "/");
  if (!trimmed) return "";
  return trimmed.endsWith("/") ? trimmed : `${trimmed}/`;
}

export interface MentionCandidate {
  id: string;
  kind: "folder" | "file" | "tag";
  label: string;
  subtitle?: string;
  value: string;
}

function hasControlCharacters(value: string): boolean {
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code < 32 || code === 127) return true;
  }
  return false;
}

export function displayMentionTooltip(mention: DisplayMention): string {
  const kind =
    mention.kind === "file"
      ? "文档"
      : mention.kind === "folder"
        ? "文件夹"
        : "标签";
  const value = mention.value.trim();
  if (mention.kind === "tag") {
    return value && !hasControlCharacters(value) ? `${kind}：${value}` : kind;
  }

  const normalized = value.replace(/\\/g, "/");
  const parts = normalized.split("/");
  const unsafe =
    !normalized ||
    normalized.startsWith("/") ||
    normalized.startsWith("//") ||
    /^[a-zA-Z]:\//.test(normalized) ||
    parts.includes("..") ||
    parts[0] === ".iris" ||
    parts[0] === ".classified" ||
    hasControlCharacters(normalized);
  return unsafe ? kind : `${kind}：${normalized}`;
}

interface MentionCandidateOptions {
  prefix?: "@" | "#";
  tags?: TagGroup[];
}

export function collectFolderPrefixes(files: FileListItem[]): string[] {
  const prefixes = new Set<string>();
  for (const file of files) {
    const parts = file.path.replace(/\\/g, "/").split("/");
    if (parts.length <= 1) continue;
    let prefix = "";
    for (let index = 0; index < parts.length - 1; index += 1) {
      prefix += `${parts[index]}/`;
      prefixes.add(prefix);
    }
  }
  return [...prefixes].sort();
}

function folderDisplayName(prefix: string): string {
  const withoutSlash = prefix.replace(/\/$/, "");
  return withoutSlash.split("/").at(-1) || withoutSlash;
}

export function buildMentionCandidates(
  files: FileListItem[],
  query: string,
  options: MentionCandidateOptions = {},
): MentionCandidate[] {
  const q = query.trim().toLowerCase();
  const prefix = options.prefix ?? "@";

  if (prefix === "#") {
    return (options.tags ?? [])
      .map((tag) => ({
        id: `tag:${tag.name}`,
        kind: "tag" as const,
        label: tag.name,
        value: tag.name,
      }))
      .filter((candidate) => !q || candidate.label.toLowerCase().includes(q))
      .slice(0, 40);
  }

  const folders = collectFolderPrefixes(files)
    .map((folderPrefix) => ({
      id: `folder:${folderPrefix}`,
      kind: "folder" as const,
      label: folderDisplayName(folderPrefix),
      subtitle: folderPrefix,
      value: folderPrefix,
    }))
    .filter(
      (candidate) =>
        !q ||
        candidate.label.toLowerCase().includes(q) ||
        candidate.value.toLowerCase().includes(q),
    );

  const documents = files
    .map((file) => ({
      id: `file:${file.path}`,
      kind: "file" as const,
      label: displayTitleForFileListItem(file),
      subtitle: noteListSubtitle(file.path),
      value: file.path.replace(/\\/g, "/"),
    }))
    .filter(
      (candidate) =>
        !q ||
        candidate.label.toLowerCase().includes(q) ||
        candidate.value.toLowerCase().includes(q),
    );

  return [...folders, ...documents].slice(0, 40);
}

export function findActiveMentionQuery(
  text: string,
  cursor: number,
): { start: number; query: string; prefix: "@" | "#" } | null {
  const before = text.slice(0, cursor);
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

export function insertDisplayMention(
  text: string,
  cursor: number,
  mentionStart: number,
  candidate: MentionCandidate,
): { text: string; cursor: number; mention: DisplayMention } {
  const before = text.slice(0, mentionStart);
  const after = text.slice(cursor);
  const separator = after.length === 0 || !/^\s/.test(after) ? " " : "";
  const nextText = `${before}${candidate.label}${separator}${after}`;
  const mention: DisplayMention = {
    kind: candidate.kind,
    value:
      candidate.kind === "folder"
        ? normalizeFolderPrefix(candidate.value)
        : candidate.value.replace(/\\/g, "/"),
    label: candidate.label,
    range: {
      from: mentionStart,
      to: mentionStart + candidate.label.length,
    },
  };
  return {
    text: nextText,
    cursor: mention.range.to + separator.length,
    mention,
  };
}

function isValidDisplayMention(text: string, mention: DisplayMention): boolean {
  const { from, to } = mention.range;
  return (
    mention.label.length > 0 &&
    mention.value.trim().length > 0 &&
    Number.isInteger(from) &&
    Number.isInteger(to) &&
    from >= 0 &&
    to > from &&
    to <= text.length &&
    text.slice(from, to) === mention.label
  );
}

export function validDisplayMentions(
  text: string,
  mentions: readonly DisplayMention[],
): DisplayMention[] {
  const sorted = mentions
    .filter((mention) => isValidDisplayMention(text, mention))
    .map((mention) => ({
      ...mention,
      range: { ...mention.range },
    }))
    .sort((left, right) => left.range.from - right.range.from);
  const nonOverlapping: DisplayMention[] = [];
  for (const mention of sorted) {
    const previous = nonOverlapping.at(-1);
    if (previous && previous.range.to > mention.range.from) continue;
    nonOverlapping.push(mention);
  }
  return nonOverlapping;
}

export interface MentionTextEdit {
  /** UTF-16 range in the pre-edit textarea value. */
  from: number;
  to: number;
  /** UTF-16 length inserted in place of the pre-edit range. */
  insertedTextLength: number;
}

function isValidMentionTextEdit(
  previousText: string,
  nextText: string,
  edit: MentionTextEdit,
): boolean {
  return (
    Number.isInteger(edit.from) &&
    Number.isInteger(edit.to) &&
    Number.isInteger(edit.insertedTextLength) &&
    edit.from >= 0 &&
    edit.to >= edit.from &&
    edit.to <= previousText.length &&
    edit.insertedTextLength >= 0 &&
    nextText.length ===
      previousText.length - (edit.to - edit.from) + edit.insertedTextLength &&
    nextText.slice(0, edit.from) === previousText.slice(0, edit.from) &&
    nextText.slice(edit.from + edit.insertedTextLength) ===
      previousText.slice(edit.to)
  );
}

/**
 * Reconcile textarea annotations after one native edit. Edits outside a
 * mention move its range; edits inside or across the visible label unbind it.
 */
export function reconcileDisplayMentions(
  previousText: string,
  nextText: string,
  mentions: readonly DisplayMention[],
  edit?: MentionTextEdit,
): DisplayMention[] {
  const current = validDisplayMentions(previousText, mentions);
  if (previousText === nextText) return validDisplayMentions(nextText, current);
  // Missing or inconsistent edit transactions must not wipe annotations that
  // still match the readable text (e.g. IME / stale beforeinput after @ insert).
  if (!edit || !isValidMentionTextEdit(previousText, nextText, edit)) {
    return validDisplayMentions(nextText, current);
  }

  const delta = edit.insertedTextLength - (edit.to - edit.from);
  const insertion = edit.from === edit.to;
  const adjusted: DisplayMention[] = [];

  for (const mention of current) {
    const { from, to } = mention.range;
    if (insertion && edit.from > from && edit.from < to) continue;
    if (!insertion && edit.from < to && edit.to > from) continue;

    const editIsBefore = insertion ? edit.from <= from : edit.to <= from;
    adjusted.push(
      editIsBefore
        ? {
            ...mention,
            range: { from: from + delta, to: to + delta },
          }
        : mention,
    );
  }

  return validDisplayMentions(nextText, adjusted);
}

export function mentionsToContextScope(
  mentions: readonly DisplayMention[],
): ContextScope {
  const pathPrefixes: string[] = [];
  const requiredTags: string[] = [];
  for (const mention of mentions) {
    if (mention.kind === "folder") {
      const prefix = normalizeFolderPrefix(mention.value);
      if (prefix && !pathPrefixes.includes(prefix)) pathPrefixes.push(prefix);
    } else if (mention.kind === "tag") {
      const tag = mention.value.trim().toLowerCase();
      if (tag && !requiredTags.includes(tag)) requiredTags.push(tag);
    }
  }
  return { paths: [], pathPrefixes, requiredTags };
}

export function trimMentionDraft(
  text: string,
  mentions: readonly DisplayMention[],
): { message: string; displayMentions: DisplayMention[] } {
  const message = text.trim();
  if (!message) return { message: "", displayMentions: [] };
  const offset = text.indexOf(message);
  const trimmedMentions = validDisplayMentions(text, mentions)
    .filter(
      (mention) =>
        mention.range.from >= offset &&
        mention.range.to <= offset + message.length,
    )
    .map((mention) => ({
      ...mention,
      range: {
        from: mention.range.from - offset,
        to: mention.range.to - offset,
      },
    }));
  return {
    message,
    displayMentions: validDisplayMentions(message, trimmedMentions),
  };
}
