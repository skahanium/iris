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

export function isAutoSyncableNotePath(path: string): boolean {
  return (
    isInternalUntitledPath(path) ||
    mapLegacyPlaceholderStemToDisplay(pathStem(path)) !== null
  );
}

export function isPlaceholderDocumentTitle(title: string): boolean {
  const trimmed = title.trim();
  return (
    !trimmed ||
    isInternalUntitledLabel(trimmed) ||
    mapLegacyPlaceholderStemToDisplay(trimmed) !== null
  );
}

interface AllocateAvailableNotePathOptions {
  files: FileListItem[];
  folderPrefix: string;
  preferredFileName: string;
  excludePaths?: Iterable<string>;
  reservedPaths?: Iterable<string>;
}

function normalizeVaultPathForCompare(path: string): string {
  return path.replace(/\\/g, "/");
}

function normalizePreferredNoteFileName(fileName: string): string {
  const leaf = fileName.replace(/\\/g, "/").split("/").pop()?.trim() ?? "";
  const stem = leaf.replace(/\.md$/i, "");
  return titleToNotePath(stem || DEFAULT_NEW_DOCUMENT_TITLE);
}

export function allocateAvailableNotePath({
  files,
  folderPrefix,
  preferredFileName,
  excludePaths,
  reservedPaths,
}: AllocateAvailableNotePathOptions): string {
  const excluded = new Set(
    Array.from(excludePaths ?? [], normalizeVaultPathForCompare),
  );
  const taken = new Set(
    files
      .map((file) => normalizeVaultPathForCompare(file.path))
      .filter((filePath) => !excluded.has(filePath)),
  );
  for (const reserved of reservedPaths ?? []) {
    taken.add(normalizeVaultPathForCompare(reserved));
  }

  const normalizedFileName = normalizePreferredNoteFileName(preferredFileName);
  const baseTitle = normalizedFileName.replace(/\.md$/i, "");
  let candidate = notePathInFolder(folderPrefix, normalizedFileName);
  if (!taken.has(candidate)) return candidate;

  for (let n = 1; n <= 500; n += 1) {
    candidate = notePathInFolder(
      folderPrefix,
      `${baseTitle}\uff08${n}\uff09.md`,
    );
    if (!taken.has(candidate)) return candidate;
  }
  throw new Error("Unable to allocate a non-conflicting note path");
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
