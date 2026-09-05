import { Editor } from "@tiptap/core";

import {
  createProductionEditorExtensions,
  createProductionEditorFromMarkdown,
} from "@/lib/editor-roundtrip";
import { editorDocToMarkdown } from "@/lib/editor-pm-serialize";
import { EDITOR_PARSE_OPTIONS } from "@/lib/editor-parse-options";
import { markdownBodyToEditorHtml, parseNoteForEditor } from "@/lib/markdown";
import { serializeOpenNote } from "@/lib/serialize-open-note";

/** Ingest via the same production contract pipeline used by the editor. */
export function createProductionEditorFromIngestedBody(
  bodyMd: string,
  vaultPath: string | null = null,
): Editor {
  return createProductionEditorFromMarkdown(bodyMd, vaultPath);
}

/** Legacy HTML-based ingest path; kept for contract tests only. */
export function createProductionEditorFromBody(
  bodyMd: string,
  vaultPath: string | null = null,
): Editor {
  return new Editor({
    extensions: createProductionEditorExtensions(vaultPath),
    content: markdownBodyToEditorHtml(bodyMd),
    parseOptions: EDITOR_PARSE_OPTIONS,
  });
}

export function createProductionEditorFromHtml(
  html: string,
  vaultPath: string | null = null,
): Editor {
  return new Editor({
    extensions: createProductionEditorExtensions(vaultPath),
    content: html,
    parseOptions: EDITOR_PARSE_OPTIONS,
  });
}

export function createProductionEditorFromNote(
  md: string,
  vaultPath: string | null = null,
): Editor {
  const { bodyMd } = parseNoteForEditor(md, "Fallback");
  return createProductionEditorFromIngestedBody(bodyMd, vaultPath);
}

export function pmSerializeBody(editor: Editor): string {
  return editorDocToMarkdown(editor);
}

export function fullNoteRoundTrip(md: string): string {
  const { yaml, bodyMd } = parseNoteForEditor(md, "Fallback");
  const editor = createProductionEditorFromNote(md);
  try {
    return serializeOpenNote({ yaml, editor, bodyFallbackMd: bodyMd });
  } finally {
    editor.destroy();
  }
}

export function normalizeMd(md: string): string {
  return md.replace(/\r\n/g, "\n").trim();
}
