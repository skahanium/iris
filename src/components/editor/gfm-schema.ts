/**
 * Iris editor GFM support contract.
 *
 * Real behavior is covered by `tests/editor-real-roundtrip.test.ts`, which
 * exercises the full `.md -> TipTap document -> HTML -> .md` path.
 */

/** GFM subset that is expected to survive the real editor round-trip. */
export const SUPPORTED_CORE_GFM = [
  "ATX headings (# through ######)",
  "paragraphs and hard breaks",
  "bold, italic, strikethrough",
  "inline code and fenced code blocks with language info",
  "ordered and unordered lists",
  "task lists (- [ ] / - [x])",
  "GFM pipe tables",
  "blockquotes",
  "links ([text](url))",
  "images (![alt](url))",
  "wiki links ([[title]])",
] as const;

/** Syntax that is still best-effort or intentionally rendered as plain text. */
export const UNSUPPORTED_OR_BEST_EFFORT_GFM = [
  "footnotes ([^1])",
  "math ($...$ / $$...$$)",
  "definition lists",
  "raw embedded HTML",
  "complex nested task/table combinations",
  "TOC and emoji shortcodes",
] as const;
