import type { Editor } from "@tiptap/react";
import type { Node as ProseMirrorNode } from "@tiptap/pm/model";

import { fileSignature } from "@/lib/ipc";
import { classifyMarkdownCapabilities } from "@/lib/markdown-contract/contract";
import type { MarkdownSyntaxFragment } from "@/lib/markdown-contract/types";
import type { ContextReference } from "@/types/ai";
import type { FileSignatureResult } from "@/types/ipc";

const DISPLAY_EXCERPT_LIMIT = 80;
const encoder = new TextEncoder();

export const EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE = "请先保存后再引用";
export const EDITOR_REFERENCE_SELECTION_REQUIRED_MESSAGE =
  "请先在编辑器中选中文本";

interface ProjectionSegment {
  editorFrom: number;
  editorTo: number;
  sourceFrom: number;
  sourceTo: number;
}

interface EditorMarkdownSourceProjection {
  baselineDoc: ProseMirrorNode;
  committedMarkdown: string;
  filePath: string;
  invalid: boolean;
  segments: ProjectionSegment[];
  unmappableEditorRanges: Array<{ from: number; to: number }>;
}

export interface InstallEditorMarkdownSourceProjectionInput {
  filePath: string;
  /** The complete, last-committed note, including frontmatter and title. */
  committedMarkdown: string;
  /** The exact body passed through the Markdown → TipTap ingest pipeline. */
  bodyMarkdown: string;
}

export type EditorContextReferenceFailureReason =
  | "empty_selection"
  | "dirty"
  | "unmappable"
  | "invalid_projection"
  | "source_changed";

export type EditorContextReferenceResult =
  | { ok: true; reference: ContextReference }
  | {
      ok: false;
      reason: EditorContextReferenceFailureReason;
      message: string;
    };

export interface CreateEditorContextReferenceInput {
  editor: Editor;
  kind: "selection" | "paragraph";
  isDirty?: () => boolean;
  getFileSignature?: (path: string) => Promise<FileSignatureResult>;
}

const projections = new WeakMap<Editor, EditorMarkdownSourceProjection>();

const EXPLICITLY_UNMAPPABLE_NODE_TYPES = new Set([
  "footnoteDef",
  "footnoteRef",
  "image",
  "preserveBlock",
  "preserveInline",
  "wikiMediaEmbed",
]);

function failure(
  reason: EditorContextReferenceFailureReason,
  message = EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE,
): EditorContextReferenceResult {
  return { ok: false, reason, message };
}

function trimRange(text: string): {
  start: number;
  end: number;
  value: string;
} {
  const value = text.trim();
  if (!value) return { start: 0, end: 0, value: "" };
  const start = text.indexOf(value);
  return { start, end: start + value.length, value };
}

function sourceBodyStart(
  committedMarkdown: string,
  bodyMarkdown: string,
): { body: string; start: number } | null {
  const trimmed = trimRange(bodyMarkdown);
  if (!trimmed.value) return null;
  const start = committedMarkdown.lastIndexOf(trimmed.value);
  return start < 0 ? null : { body: trimmed.value, start };
}

function fencedCodeContentRange(raw: string): { from: number; to: number } {
  const firstLineEnd = raw.indexOf("\n");
  if (firstLineEnd < 0 || !/^\s*(`{3,}|~{3,})/u.test(raw)) {
    return { from: 0, to: raw.length };
  }
  const lastLineStart = raw.lastIndexOf("\n", raw.length - 2);
  return {
    from: firstLineEnd + 1,
    to: lastLineStart > firstLineEnd ? lastLineStart : raw.length,
  };
}

function candidateAllowedByFragment(
  fragments: readonly MarkdownSyntaxFragment[],
  from: number,
  to: number,
): boolean {
  let low = 0;
  let high = fragments.length - 1;
  let fragment: MarkdownSyntaxFragment | undefined;
  while (low <= high) {
    const middle = low + Math.floor((high - low) / 2);
    const candidate = fragments[middle];
    if (!candidate) break;
    if (from < candidate.offset) {
      high = middle - 1;
    } else if (from >= candidate.endOffset) {
      low = middle + 1;
    } else {
      fragment = to <= candidate.endOffset ? candidate : undefined;
      break;
    }
  }
  if (!fragment) return false;
  const relativeFrom = from - fragment.offset;
  const relativeTo = to - fragment.offset;
  if (fragment.syntaxKind === "code_block") {
    const content = fencedCodeContentRange(fragment.raw);
    return relativeFrom >= content.from && relativeTo <= content.to;
  }
  if (fragment.syntaxKind === "link") {
    if (fragment.raw.startsWith("[")) {
      const labelEnd = fragment.raw.indexOf("]");
      return labelEnd > 0 && relativeFrom >= 1 && relativeTo <= labelEnd;
    }
    if (fragment.raw.startsWith("<") && fragment.raw.endsWith(">")) {
      return relativeFrom >= 1 && relativeTo <= fragment.raw.length - 1;
    }
  }
  return ![
    "footnote_def",
    "footnote_ref",
    "html_comment",
    "image",
    "raw_html",
  ].includes(fragment.syntaxKind);
}

function findVisibleText(
  body: string,
  text: string,
  from: number,
  fragments: readonly MarkdownSyntaxFragment[],
): number {
  let candidate = body.indexOf(text, from);
  while (candidate >= 0) {
    if (
      candidateAllowedByFragment(fragments, candidate, candidate + text.length)
    ) {
      return candidate;
    }
    candidate = body.indexOf(text, candidate + 1);
  }
  return -1;
}

/**
 * Bind one hydrated ProseMirror document to the exact committed Markdown it
 * represents. Text nodes are projected monotonically; source-only markup is
 * allowed between mapped nodes, while lossy/opaque nodes are marked unmappable.
 */
export function installEditorMarkdownSourceProjection(
  editor: Editor,
  input: InstallEditorMarkdownSourceProjectionInput,
): void {
  const sourceBody = sourceBodyStart(
    input.committedMarkdown,
    input.bodyMarkdown,
  );
  const segments: ProjectionSegment[] = [];
  const unmappableEditorRanges: Array<{ from: number; to: number }> = [];
  let sourceCursor = 0;

  if (sourceBody) {
    const fragments = classifyMarkdownCapabilities(sourceBody.body);
    editor.state.doc.descendants((node, pos) => {
      if (EXPLICITLY_UNMAPPABLE_NODE_TYPES.has(node.type.name)) {
        unmappableEditorRanges.push({ from: pos, to: pos + node.nodeSize });
        return false;
      }
      if (!node.isText) return;
      const text = node.text ?? "";
      if (!text) return;
      const sourceFromInBody = findVisibleText(
        sourceBody.body,
        text,
        sourceCursor,
        fragments,
      );
      if (sourceFromInBody < 0) {
        unmappableEditorRanges.push({ from: pos, to: pos + node.nodeSize });
        return;
      }
      const sourceToInBody = sourceFromInBody + text.length;
      segments.push({
        editorFrom: pos,
        editorTo: pos + node.nodeSize,
        sourceFrom: sourceBody.start + sourceFromInBody,
        sourceTo: sourceBody.start + sourceToInBody,
      });
      sourceCursor = sourceToInBody;
    });
  }

  projections.set(editor, {
    baselineDoc: editor.state.doc,
    committedMarkdown: input.committedMarkdown,
    filePath: input.filePath.trim(),
    invalid: !sourceBody || !input.filePath.trim() || segments.length === 0,
    segments,
    unmappableEditorRanges,
  });
}

function editorRangeFor(
  editor: Editor,
  kind: CreateEditorContextReferenceInput["kind"],
): { from: number; to: number } | null {
  const selection = editor.state.selection;
  if (kind === "selection") {
    return selection.empty ? null : { from: selection.from, to: selection.to };
  }
  if (!selection.$from.parent.isTextblock) return null;
  return { from: selection.$from.start(), to: selection.$from.end() };
}

function segmentAt(
  segments: readonly ProjectionSegment[],
  position: number,
  edge: "start" | "end",
): ProjectionSegment | null {
  const candidates = segments.filter(
    (segment) => position >= segment.editorFrom && position <= segment.editorTo,
  );
  if (candidates.length === 0) return null;
  return edge === "start" ? candidates.at(-1)! : candidates[0]!;
}

function overlaps(
  left: { from: number; to: number },
  right: { from: number; to: number },
): boolean {
  return left.from < right.to && right.from < left.to;
}

function sourceRangeFor(
  projection: EditorMarkdownSourceProjection,
  editorRange: { from: number; to: number },
): { from: number; to: number } | null {
  const start = segmentAt(projection.segments, editorRange.from, "start");
  const end = segmentAt(projection.segments, editorRange.to, "end");
  if (!start || !end) return null;
  if (
    projection.unmappableEditorRanges.some((range) =>
      overlaps(editorRange, range),
    )
  ) {
    return null;
  }
  const from = start.sourceFrom + (editorRange.from - start.editorFrom);
  const to = end.sourceFrom + (editorRange.to - end.editorFrom);
  if (
    from < start.sourceFrom ||
    from > start.sourceTo ||
    to < end.sourceFrom ||
    to > end.sourceTo ||
    from >= to
  ) {
    return null;
  }
  return { from, to };
}

function utf8OffsetAt(text: string, stringOffset: number): number | null {
  if (stringOffset < 0 || stringOffset > text.length) return null;
  if (
    stringOffset > 0 &&
    stringOffset < text.length &&
    /[\uD800-\uDBFF]/u.test(text[stringOffset - 1]!) &&
    /[\uDC00-\uDFFF]/u.test(text[stringOffset]!)
  ) {
    return null;
  }
  return encoder.encode(text.slice(0, stringOffset)).length;
}

async function sha256(content: string): Promise<string | null> {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) return null;
  const digest = await subtle.digest("SHA-256", encoder.encode(content));
  return Array.from(new Uint8Array(digest), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

function truncateChars(text: string, limit: number): string {
  const chars = Array.from(text);
  return chars.length <= limit ? text : `${chars.slice(0, limit).join("")}...`;
}

function referenceId(
  kind: CreateEditorContextReferenceInput["kind"],
  filePath: string,
  contentHash: string,
  start: number,
  end: number,
): string {
  return [kind, filePath, contentHash, start, end].join(":");
}

/**
 * Create a range-only local reference shared by inline and sidecar AI.
 * The current editor text is never returned or sent: the backend rereads the
 * path after verifying the disk signature and applies the UTF-8 range itself.
 */
export async function createEditorContextReference(
  input: CreateEditorContextReferenceInput,
): Promise<EditorContextReferenceResult> {
  const projection = projections.get(input.editor);
  if (!projection || projection.invalid) return failure("invalid_projection");
  if (
    input.isDirty?.() === true ||
    !projection.baselineDoc.eq(input.editor.state.doc)
  ) {
    return failure("dirty");
  }
  const editorRange = editorRangeFor(input.editor, input.kind);
  if (!editorRange) {
    return failure(
      "empty_selection",
      EDITOR_REFERENCE_SELECTION_REQUIRED_MESSAGE,
    );
  }
  const sourceRange = sourceRangeFor(projection, editorRange);
  if (!sourceRange) return failure("unmappable");
  const utf8Start = utf8OffsetAt(
    projection.committedMarkdown,
    sourceRange.from,
  );
  const utf8End = utf8OffsetAt(projection.committedMarkdown, sourceRange.to);
  if (utf8Start === null || utf8End === null || utf8Start >= utf8End) {
    return failure("unmappable");
  }

  let signature: FileSignatureResult;
  try {
    signature = await (input.getFileSignature ?? fileSignature)(
      projection.filePath,
    );
  } catch {
    return failure("source_changed");
  }
  const projectedHash = await sha256(projection.committedMarkdown);
  if (
    !projectedHash ||
    signature.contentHash !== projectedHash ||
    signature.byteLength !== encoder.encode(projection.committedMarkdown).length
  ) {
    return failure("source_changed");
  }
  if (
    input.isDirty?.() === true ||
    !projection.baselineDoc.eq(input.editor.state.doc)
  ) {
    return failure("dirty");
  }

  return {
    ok: true,
    reference: {
      id: referenceId(
        input.kind,
        projection.filePath,
        signature.contentHash,
        utf8Start,
        utf8End,
      ),
      kind: input.kind,
      filePath: projection.filePath,
      contentHash: signature.contentHash,
      utf8Range: { start: utf8Start, end: utf8End },
      editorRange,
      excerpt: "",
      headingPath: null,
      anchor: null,
      stale: false,
      invalidReason: null,
    },
  };
}

function fileName(path: string | null): string {
  if (!path) return "当前上下文";
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
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
