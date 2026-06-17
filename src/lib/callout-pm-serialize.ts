import type { Node as ProseMirrorNode } from "@tiptap/pm/model";
import type { MarkdownSerializerState } from "prosemirror-markdown";

import { calloutMarkdownFromLines } from "@/lib/callout-markdown";

function paragraphInlineMarkdown(paragraph: ProseMirrorNode): string {
  let text = "";
  paragraph.descendants((child) => {
    if (child.isText) {
      let t = child.text ?? "";
      for (const mark of child.marks) {
        const name = mark.type.name;
        if (name === "bold") t = `**${t}**`;
        else if (name === "italic") t = `*${t}*`;
        else if (name === "strike") t = `~~${t}~~`;
        else if (name === "code") t = `\`${t}\``;
        else if (name === "link") t = `[${t}](${mark.attrs.href ?? ""})`;
        else if (name === "wikiLink") t = `[[${t}]]`;
      }
      text += t;
    }
  });
  return text.trim();
}

/** Extract plain text from a paragraph (ignoring all marks). */
function paragraphPlainText(paragraph: ProseMirrorNode): string {
  let text = "";
  paragraph.descendants((child) => {
    if (child.isText) {
      text += child.text ?? "";
    }
  });
  return text.trim();
}

/** Collect display lines from a callout blockquote (title + body paragraphs).
 *  The title paragraph is always plain text — its <strong> wrapping is an
 *  ingest presentation convention, not part of the original markdown.
 *  Body paragraphs preserve inline marks (bold, italic, code, links, etc.). */
export function calloutLinesFromBlockquote(node: ProseMirrorNode): string[] {
  const lines: string[] = [];
  let isTitle = true;
  node.forEach((child) => {
    if (child.type.name === "paragraph") {
      const line = isTitle
        ? paragraphPlainText(child)
        : paragraphInlineMarkdown(child);
      lines.push(line);
      isTitle = false;
    }
  });
  return lines;
}

/**
 * Serialize a blockquote node that carries `calloutType` (Obsidian callout).
 * Returns true when handled; false for plain blockquotes.
 */
export function renderCalloutBlockquote(
  state: MarkdownSerializerState,
  node: ProseMirrorNode,
): boolean {
  const calloutType = node.attrs.calloutType as string | null | undefined;
  if (!calloutType?.trim()) {
    return false;
  }

  const originalRaw = node.attrs.calloutOriginalRaw as
    | string
    | null
    | undefined;
  if (originalRaw?.trim()) {
    // Compare current content against original; if user edited, serialize current content
    const currentMd = calloutMarkdownFromLines(
      calloutType.trim(),
      calloutLinesFromBlockquote(node),
    );
    if (currentMd.trim() === originalRaw.trim()) {
      state.write(originalRaw);
    } else {
      state.write(currentMd);
    }
    state.closeBlock(node);
    return true;
  }

  const md = calloutMarkdownFromLines(
    calloutType.trim(),
    calloutLinesFromBlockquote(node),
  );
  state.write(md);
  state.closeBlock(node);
  return true;
}
