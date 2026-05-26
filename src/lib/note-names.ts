import type { FileListItem } from "@/types/ipc";

export const DEFAULT_NEW_DOCUMENT_TITLE = "新建文档";

const INVALID_FILE_CHARS = /[\\/:*?"<>|]/g;

/** Strip characters illegal in vault file names. */
export function sanitizeNoteFileName(title: string): string {
  const trimmed = title.replace(INVALID_FILE_CHARS, "_").trim();
  return trimmed.length > 0 ? trimmed : DEFAULT_NEW_DOCUMENT_TITLE;
}

export function titleToNotePath(title: string): string {
  return `${sanitizeNoteFileName(title)}.md`;
}

function pathStem(path: string): string {
  return path.replace(/\.md$/i, "").split("/").pop() ?? path;
}

/** Collect display titles and path stems already used in the vault index. */
export function collectTakenDocumentNames(files: FileListItem[]): Set<string> {
  const taken = new Set<string>();
  for (const f of files) {
    if (f.title.trim()) {
      taken.add(f.title.trim());
    }
    taken.add(pathStem(f.path));
  }
  return taken;
}

/**
 * Next available display name: `新建文档`, `新建文档（1）`, `新建文档（2）`, …
 */
export function allocateNewDocumentName(files: FileListItem[]): {
  title: string;
  path: string;
} {
  const taken = collectTakenDocumentNames(files);
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
