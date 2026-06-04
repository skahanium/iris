import {
  isInternalUntitledLabel,
  isInternalUntitledPath,
  mapLegacyPlaceholderStemToDisplay,
  pathStem,
  UNNAMED_DOCUMENT_PREFIX,
} from "@/lib/note-display";
import { notePathInFolder } from "@/lib/vault-tree";
import type { FileListItem } from "@/types/ipc";

export const DEFAULT_NEW_DOCUMENT_TITLE = UNNAMED_DOCUMENT_PREFIX;
/** @deprecated Use {@link UNNAMED_DOCUMENT_PREFIX} */
export const UNTITLED_TITLE_PREFIX = "无标题";

const INVALID_FILE_CHARS = /[\\/:*?"<>|]/g;

const LEGACY_PLACEHOLDER_STEM_RE =
  /^(?:新建文档|无标题\d+|untitled-\d+)(?:（\d+）)?$/i;

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
    const stem = pathStem(f.path);
    if (!isInternalUntitledPath(f.path)) {
      taken.add(stem);
      const mapped = mapLegacyPlaceholderStemToDisplay(stem);
      if (mapped) {
        taken.add(mapped);
      }
    }
    if (LEGACY_PLACEHOLDER_STEM_RE.test(stem)) {
      taken.add(stem);
    }
  }
  return taken;
}

/**
 * Next available display name: `未命名文档`, `未命名文档（1）`, `未命名文档（2）`, …
 */
export function allocateNewDocumentName(
  files: FileListItem[],
  extraTaken?: Iterable<string>,
  folderPrefix = "",
  titleHint?: string,
): { title: string; path: string } {
  const taken = collectTakenDocumentNames(files);
  for (const name of extraTaken ?? []) {
    const trimmed = name.trim();
    if (trimmed && !isInternalUntitledLabel(trimmed)) {
      taken.add(trimmed);
    }
  }

  const baseTitle = sanitizeNoteFileName(
    titleHint?.replace(/\.md$/i, "") || DEFAULT_NEW_DOCUMENT_TITLE,
  );

  if (!taken.has(baseTitle)) {
    const title = baseTitle;
    return {
      title,
      path: notePathInFolder(folderPrefix, titleToNotePath(title)),
    };
  }

  let n = 1;
  while (taken.has(`${baseTitle}（${n}）`)) {
    n += 1;
  }
  const title = `${baseTitle}（${n}）`;
  return {
    title,
    path: notePathInFolder(folderPrefix, titleToNotePath(title)),
  };
}

/** @deprecated Use {@link allocateNewDocumentName} */
export function allocateUntitledDocumentName(
  files: FileListItem[],
  extraTaken?: Iterable<string>,
): { title: string; path: string } {
  return allocateNewDocumentName(files, extraTaken);
}

export const allocateUnnamedDocumentName = allocateNewDocumentName;
