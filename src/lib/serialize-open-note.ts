import type { Editor } from "@tiptap/react";

import { editorDocToMarkdown } from "@/lib/editor-pm-serialize";
import { splitFrontmatter } from "@/lib/frontmatter";
import { buildNoteMarkdown } from "@/lib/markdown";

export interface SerializeOpenNoteOptions {
  yaml: string | null;
  title: string;
  editor: Editor | null;
  /** Used when `editor` is unavailable; typically `splitFrontmatter(ref).body`. */
  bodyFallbackMd: string;
}

/** Single persistence pipeline: title state + TipTap body → full note markdown. */
export function serializeOpenNote(options: SerializeOpenNoteOptions): string {
  const { yaml, title, editor, bodyFallbackMd } = options;
  const bodyMd = editor ? editorDocToMarkdown(editor) : bodyFallbackMd;
  return buildNoteMarkdown(yaml, title.trim(), bodyMd);
}

/** Body markdown from a persisted note ref (for fallbacks). */
export function bodyMarkdownFromNoteRef(noteMarkdown: string): string {
  return splitFrontmatter(noteMarkdown).body;
}
