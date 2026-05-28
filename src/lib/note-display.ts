import { displayTitleFromMarkdown } from "@/lib/note-title";
import type { FileListItem } from "@/types/ipc";

/** Machine-only paths from an earlier naming scheme; never show to users. */
const INTERNAL_UNTITLED_PATH_RE = /^untitled-\d+\.md$/i;
const INTERNAL_UNTITLED_LABEL_RE = /^untitled-\d+$/i;

export function pathStem(path: string): string {
  return path.replace(/\.md$/i, "").split("/").pop() ?? path;
}

export function isInternalUntitledPath(path: string): boolean {
  const base = path.split("/").pop() ?? path;
  return INTERNAL_UNTITLED_PATH_RE.test(base);
}

export function isInternalUntitledLabel(label: string): boolean {
  return INTERNAL_UNTITLED_LABEL_RE.test(label.trim());
}

/**
 * Resolve a user-visible document title. Never returns `untitled-<digits>`.
 */
export function resolveNoteDisplayTitle(options: {
  path: string;
  title?: string | null;
  markdown?: string | null;
  fallback?: string;
}): string {
  const fallback = options.fallback ?? "无标题1";
  const fromMarkdown = options.markdown
    ? displayTitleFromMarkdown(options.markdown, "")
    : "";
  const candidates = [fromMarkdown, options.title?.trim() ?? ""].filter(
    (value) => value.length > 0,
  );

  for (const candidate of candidates) {
    if (!isInternalUntitledLabel(candidate)) {
      return candidate;
    }
  }

  if (!isInternalUntitledPath(options.path)) {
    const stem = pathStem(options.path);
    if (!isInternalUntitledLabel(stem)) {
      return stem;
    }
  }

  return fallback;
}

export function displayTitleForFileListItem(item: FileListItem): string {
  return resolveNoteDisplayTitle({
    path: item.path,
    title: item.title,
  });
}

/** Subtitle for lists: hide internal machine paths. */
export function noteListSubtitle(path: string): string | undefined {
  if (isInternalUntitledPath(path)) {
    return undefined;
  }
  return path;
}
