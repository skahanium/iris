import type { Node as ProseMirrorNode } from "@tiptap/pm/model";
import type { MarkdownSerializerState } from "prosemirror-markdown";

import { calloutMarkdownFromLines } from "@/lib/callout-markdown";

function paragraphPlainText(paragraph: ProseMirrorNode): string {
  let text = "";
  paragraph.descendants((child) => {
    if (child.isText) {
      text += child.text;
    }
  });
  return text.trim();
}

/** Collect display lines from a callout blockquote (title + body paragraphs). */
export function calloutLinesFromBlockquote(node: ProseMirrorNode): string[] {
  const lines: string[] = [];
  node.descendants((child) => {
    if (child.type.name === "paragraph") {
      const line = paragraphPlainText(child);
      if (line) {
        lines.push(line);
      }
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
