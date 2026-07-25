import type { FileListItem } from "@/types/ipc";
import { splitFrontmatter } from "@/lib/frontmatter";

/** Default display name for notes without a user title. */
export const UNNAMED_DOCUMENT_PREFIX = "未命名文档";

/** Machine-only paths from an earlier naming scheme; never show to users. */
const INTERNAL_UNTITLED_PATH_RE = /^untitled-\d+\.md$/i;
const INTERNAL_UNTITLED_LABEL_RE = /^untitled-\d+$/i;

const LEGACY_NEW_DOC_STEM_RE = /^新建文档(?:（(\d+)）)?$/;
const LEGACY_WU_BIAOTI_STEM_RE = /^无标题(\d+)$/;

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

/** Map legacy on-disk placeholder stems to user-facing `未命名文档（n）`. */
export function mapLegacyPlaceholderStemToDisplay(stem: string): string | null {
  const newDoc = LEGACY_NEW_DOC_STEM_RE.exec(stem);
  if (newDoc) {
    return newDoc[1]
      ? `${UNNAMED_DOCUMENT_PREFIX}（${newDoc[1]}）`
      : UNNAMED_DOCUMENT_PREFIX;
  }
  const wu = LEGACY_WU_BIAOTI_STEM_RE.exec(stem);
  if (wu) {
    const n = Number(wu[1]);
    return n <= 1
      ? UNNAMED_DOCUMENT_PREFIX
      : `${UNNAMED_DOCUMENT_PREFIX}（${n - 1}）`;
  }
  if (stem === UNNAMED_DOCUMENT_PREFIX || /^未命名文档（\d+）$/.test(stem)) {
    return stem;
  }
  return null;
}

export function isLegacyPlaceholderPath(path: string): boolean {
  return mapLegacyPlaceholderStemToDisplay(pathStem(path)) !== null;
}

/**
 * Resolve a user-visible document title. Never returns `untitled-<digits>`.
 * Empty explicit titles map to `未命名文档` (or legacy path mapping).
 */
export function resolveNoteDisplayTitle(options: {
  path: string;
  title?: string | null;
  markdown?: string | null;
  fallback?: string;
}): string {
  const fallback = options.fallback ?? UNNAMED_DOCUMENT_PREFIX;
  const mappedStem = mapLegacyPlaceholderStemToDisplay(pathStem(options.path));
  if (mappedStem) {
    return mappedStem;
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
  return resolveNoteDisplayTitle({ path: item.path });
}

/** Status bar / tab label while editing (empty field → placeholder semantics). */
export function displayTitleForChrome(
  path: string | null,
  editingTitle: string,
): string {
  if (!path) {
    return "未打开文件";
  }
  if (editingTitle.trim() === "") {
    return UNNAMED_DOCUMENT_PREFIX;
  }
  return editingTitle.trim();
}

/**
 * Optional folder caption under a workspace-empty card title.
 * Returns the parent directory path when present; root-level notes return null
 * so the UI never duplicates the title as a fake preview.
 */
export function noteCardPreviewText(path: string): string | null {
  const normalized = path.replace(/\\/g, "/");
  const slash = normalized.lastIndexOf("/");
  if (slash <= 0) {
    return null;
  }
  return normalized.slice(0, slash);
}

const CARD_EXCERPT_MAX = 120;

/**
 * Plain-text body excerpt for recent-note cards (not a title duplicate).
 * Strips YAML frontmatter and light markdown chrome, then truncates.
 */
export function markdownToCardExcerpt(
  markdown: string,
  maxChars = CARD_EXCERPT_MAX,
): string {
  const { body } = splitFrontmatter(markdown);
  const plain = body
    .replace(/^#{1,6}\s+/gm, "")
    .replace(/!\[[^\]]*]\([^)]*\)/g, "")
    .replace(/\[([^\]]+)]\([^)]*\)/g, "$1")
    .replace(/`{1,3}[^`]*`{1,3}/g, "")
    .replace(/[*_~>]+/g, "")
    .replace(/\s+/g, " ")
    .trim();
  if (!plain) return "";
  if (plain.length <= maxChars) return plain;
  return `${plain.slice(0, maxChars).trimEnd()}…`;
}

/** Subtitle for lists: hide internal machine paths. */
export function noteListSubtitle(path: string): string | undefined {
  if (isInternalUntitledPath(path)) {
    return undefined;
  }
  return path;
}
