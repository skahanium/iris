import type { Editor } from "@tiptap/react";

import { editorDocToMarkdown } from "@/lib/editor-pm-serialize";
import { splitFrontmatter } from "@/lib/frontmatter";
import { buildNoteMarkdown } from "@/lib/markdown";

export interface SerializeOpenNoteOptions {
  yaml: string | null;
  editor: Editor | null;
  /** True only after the editor body has been hydrated with the current note. */
  editorReady?: boolean;
  /** Used when `editor` is unavailable; typically `splitFrontmatter(ref).body`. */
  bodyFallbackMd: string;
}

/** Single persistence pipeline: title state + TipTap body → full note markdown. */
export function serializeOpenNote(options: SerializeOpenNoteOptions): string {
  const { yaml, editor, editorReady = true, bodyFallbackMd } = options;
  const hasEditor = editorReady && editor != null && !editor.isDestroyed;
  const bodyMd = hasEditor ? editorDocToMarkdown(editor) : bodyFallbackMd;
  return buildNoteMarkdown(yaml, bodyMd);
}

/** Body markdown from a persisted note ref (for fallbacks). */
export function bodyMarkdownFromNoteRef(noteMarkdown: string): string {
  return splitFrontmatter(noteMarkdown).body;
}
