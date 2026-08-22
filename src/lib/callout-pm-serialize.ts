import type { Node as ProseMirrorNode } from "@tiptap/pm/model";
import type { MarkdownSerializerState } from "prosemirror-markdown";

function renderBlockToMarkdown(
  state: MarkdownSerializerState,
  node: ProseMirrorNode,
): string {
  const internal = state as unknown as { out: string };
  const start = internal.out.length;
  state.render(node, node, 0);
  const captured = internal.out.slice(start);
  internal.out = internal.out.slice(0, start);
  return captured.trim();
}

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
 * Serialize a callout node with support for nested block content.
 *
 * Simple paragraph-only callouts reuse the compact line builder. When the
 * callout contains nested blocks (lists, blockquotes, code blocks, tables),
 * the inner blocks are rendered through the active PM serializer and prefixed
 * with `> ` so the structure is preserved.
 */
function calloutMarkdownFromNode(
  node: ProseMirrorNode,
  state: MarkdownSerializerState,
): string {
  const calloutType = node.attrs.calloutType as string | null | undefined;
  const type = calloutType?.trim() || "note";
  const lines: string[] = [];
  let isTitle = true;

  node.forEach((child) => {
    if (child.type.name === "paragraph") {
      const line = isTitle
        ? paragraphPlainText(child)
        : paragraphInlineMarkdown(child);
      lines.push(line);
      isTitle = false;
      return;
    }
    const blockMd = renderBlockToMarkdown(state, child);
    if (blockMd) {
      lines.push(...blockMd.split("\n"));
    }
    isTitle = false;
  });

  const title = lines.shift() ?? type;
  const body = lines.map((line) => `> ${line}`).join("\n");
  return body ? `> [!${type}] ${title}\n${body}` : `> [!${type}] ${title}`;
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
    const currentMd = calloutMarkdownFromNode(node, state);
    if (currentMd.trim() === originalRaw.trim()) {
      state.write(originalRaw);
    } else {
      state.write(currentMd);
    }
    state.closeBlock(node);
    return true;
  }

  const md = calloutMarkdownFromNode(node, state);
  state.write(md);
  state.closeBlock(node);
  return true;
}
