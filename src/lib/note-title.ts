import type { Editor } from "@tiptap/react";
import type { Node as ProseMirrorNode } from "@tiptap/pm/model";

import { splitFrontmatter, titleFromFields } from "@/lib/frontmatter";

/** Read display title from the `noteTitle` block (not body section headings). */
export function noteTitleFromDoc(doc: ProseMirrorNode): string {
  let text = "";
  doc.descendants((node) => {
    if (node.type.name === "noteTitle") {
      text = node.textContent.trim();
      return false;
    }
  });
  return text;
}

/** Read display title from a TipTap editor instance. */
export function noteTitleFromEditor(editor: Editor): string {
  return noteTitleFromDoc(editor.state.doc);
}

/**
 * Tab / status-bar title from persisted markdown.
 * Only `frontmatter.title` counts — body ATX `#` headings are section titles, not the doc title.
 */
export function displayTitleFromMarkdown(
  md: string,
  fallback = "无标题",
): string {
  const title = titleFromFields(splitFrontmatter(md).fields).trim();
  return title || fallback;
}
