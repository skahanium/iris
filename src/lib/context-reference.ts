import { utf8ByteRangeToStringRange } from "@/lib/utf8-range";
import type { ContextReference, SourceSpan } from "@/types/ai";

const DISPLAY_EXCERPT_LIMIT = 80;

function stableContentHash(content: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < content.length; i += 1) {
    hash ^= content.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

function fileName(path: string | null): string {
  if (!path) return "当前上下文";
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
}

function normalizeExcerpt(text: string): string {
  return text.trim();
}

function excerptFromRange(content: string, range: SourceSpan | null): string {
  if (!range) return normalizeExcerpt(content);
  const stringRange = utf8ByteRangeToStringRange(content, range);
  if (!stringRange) return "";
  return normalizeExcerpt(content.slice(stringRange.start, stringRange.end));
}

function truncateChars(text: string, limit: number): string {
  const chars = Array.from(text);
  if (chars.length <= limit) return text;
  return `${chars.slice(0, limit).join("")}...`;
}

export function createContextReference(input: {
  kind: ContextReference["kind"];
  filePath: string | null;
  content: string;
  excerpt?: string;
  utf8Range: SourceSpan | null;
  editorRange: { from: number; to: number } | null;
  headingPath?: string | null;
  anchor?: string | null;
}): ContextReference {
  const contentHash = stableContentHash(input.content);
  const excerpt = truncateChars(
    normalizeExcerpt(
      input.excerpt ?? excerptFromRange(input.content, input.utf8Range),
    ),
    DISPLAY_EXCERPT_LIMIT,
  );
  return {
    id: [
      input.kind,
      input.filePath ?? "context",
      contentHash,
      input.utf8Range?.start ?? "full",
      input.utf8Range?.end ?? "full",
    ].join(":"),
    kind: input.kind,
    filePath: input.filePath,
    contentHash,
    utf8Range: input.utf8Range,
    editorRange: input.editorRange,
    excerpt,
    headingPath: input.headingPath ?? null,
    anchor: input.anchor ?? null,
    stale: false,
  };
}

export function validateContextReference(
  reference: ContextReference,
  currentContent: string | null,
): ContextReference {
  if (currentContent === null) {
    return {
      ...reference,
      stale: true,
      invalidReason: "missing_content",
    };
  }
  if (reference.contentHash !== stableContentHash(currentContent)) {
    return {
      ...reference,
      stale: true,
      invalidReason: "content_changed",
    };
  }
  return {
    ...reference,
    stale: false,
    invalidReason: null,
  };
}

export function contextReferenceDisplayText(
  reference: ContextReference,
): string {
  const suffix = reference.stale ? " · 已失效" : "";
  const excerpt = truncateChars(
    reference.excerpt.replace(/\s+/g, " "),
    DISPLAY_EXCERPT_LIMIT,
  );
  return `${fileName(reference.filePath)} · ${excerpt}${suffix}`;
}
