import type { Editor } from "@tiptap/core";

import { ingestMarkdownForEditor } from "@/lib/editor-ingest";

/**
 * Insert assistant-authored Markdown through the editor ingest pipeline.
 *
 * Passing raw Markdown directly to TipTap creates one enormous text paragraph,
 * which makes later cursor movement and Enter operations increasingly costly.
 */
export function insertAssistantMarkdownAtCursor(
  editor: Editor,
  content: string,
): boolean {
  const bodyMarkdown = content.trim();
  if (!bodyMarkdown) return false;

  const { tipTapHtml } = ingestMarkdownForEditor({ bodyMarkdown });
  if (!tipTapHtml.trim()) return false;

  const { from } = editor.state.selection;
  return editor.chain().focus().insertContentAt(from, tipTapHtml).run();
}
