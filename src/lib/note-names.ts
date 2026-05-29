import {
  isInternalUntitledLabel,
  isInternalUntitledPath,
  pathStem,
} from "@/lib/note-display";
import type { FileListItem } from "@/types/ipc";

export const DEFAULT_NEW_DOCUMENT_TITLE = "新建文档";
/** Prefix for auto-allocated names: `无标题1`, `无标题2`, … */
export const UNTITLED_TITLE_PREFIX = "无标题";

const INVALID_FILE_CHARS = /[\\/:*?"<>|]/g;

/** Strip characters illegal in vault file names. */
export function sanitizeNoteFileName(title: string): string {
  const trimmed = title.replace(INVALID_FILE_CHARS, "_").trim();
  return trimmed.length > 0 ? trimmed : DEFAULT_NEW_DOCUMENT_TITLE;
}

export function titleToNotePath(title: string): string {
  return `${sanitizeNoteFileName(title)}.md`;
}

/** Collect display titles and path stems already used in the vault index. */
export function collectTakenDocumentNames(files: FileListItem[]): Set<string> {
  const taken = new Set<string>();
  for (const f of files) {
    const title = f.title.trim();
    if (title && !isInternalUntitledLabel(title)) {
      taken.add(title);
    }
    if (!isInternalUntitledPath(f.path)) {
      taken.add(pathStem(f.path));
    }
  }
  return taken;
}

/**
 * Next available display name: `新建文档`, `新建文档（1）`, `新建文档（2）`, …
 */
export function allocateNewDocumentName(
  files: FileListItem[],
  extraTaken?: Iterable<string>,
): { title: string; path: string } {
  const taken = collectTakenDocumentNames(files);
  for (const name of extraTaken ?? []) {
    const trimmed = name.trim();
    if (trimmed && !isInternalUntitledLabel(trimmed)) {
      taken.add(trimmed);
    }
  }

  if (!taken.has(DEFAULT_NEW_DOCUMENT_TITLE)) {
    const title = DEFAULT_NEW_DOCUMENT_TITLE;
    return { title, path: titleToNotePath(title) };
  }

  let n = 1;
  while (taken.has(`${DEFAULT_NEW_DOCUMENT_TITLE}（${n}）`)) {
    n += 1;
  }
  const title = `${DEFAULT_NEW_DOCUMENT_TITLE}（${n}）`;
  return { title, path: titleToNotePath(title) };
}

/**
 * Next auto name: `无标题1`, `无标题2`, `无标题3`, …
 * Filename matches display title (`无标题1.md`).
 * @deprecated 使用 {@link allocateNewDocumentName} 代替，新文档默认使用 `新建文档` 命名。
 */
export function allocateUntitledDocumentName(
  files: FileListItem[],
  extraTaken?: Iterable<string>,
): { title: string; path: string } {
  const taken = collectTakenDocumentNames(files);
  for (const name of extraTaken ?? []) {
    const trimmed = name.trim();
    if (trimmed && !isInternalUntitledLabel(trimmed)) {
      taken.add(trimmed);
    }
  }

  let n = 1;
  while (taken.has(`${UNTITLED_TITLE_PREFIX}${n}`)) {
    n += 1;
  }
  const title = `${UNTITLED_TITLE_PREFIX}${n}`;
  return { title, path: titleToNotePath(title) };
}
