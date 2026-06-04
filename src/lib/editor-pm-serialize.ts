/**
 * TipTap → Markdown hot path (`prosemirror-markdown`).
 *
 * ## Callout（Obsidian `> [!type]`）
 *
 * - **ingest**：`editor-ingest` 将 callout 片段渲染为带 `data-callout-type` 的 blockquote（`render_only`）。
 * - **schema**：`CalloutBlockquoteExtension` 在 PM 节点上保留 `calloutType`。
 * - **export**：本模块将 callout blockquote 写回 `> [!type] Title` 行前缀；普通 blockquote 仍走 CommonMark `>`。
 *
 * ## preserve_only
 *
 * `preserveBlock` 节点原样写回 `originalRaw`（脚注定义、原始 HTML 等），与 callout 分离。
 *
 * 详见 [docs/markdown-export.md](../../docs/markdown-export.md)。
 */
import type { Editor } from "@tiptap/react";
import type { Node as ProseMirrorNode } from "@tiptap/pm/model";
import {
  defaultMarkdownSerializer,
  MarkdownSerializer,
  type MarkdownSerializerState,
} from "prosemirror-markdown";

import { renderCalloutBlockquote } from "@/lib/callout-pm-serialize";
import { editorBodyHtmlToMarkdown } from "@/lib/markdown";

function cellPlainText(cell: ProseMirrorNode): string {
  let text = "";
  cell.descendants((child) => {
    if (child.isText) {
      let t = child.text ?? "";
      for (const mark of child.marks) {
        if (mark.type.name === "bold") t = `**${t}**`;
        else if (mark.type.name === "italic") t = `*${t}*`;
        else if (mark.type.name === "strike") t = `~~${t}~~`;
        else if (mark.type.name === "code") t = `\`${t}\``;
      }
      text += t;
    }
  });
  return text.trim();
}

function renderTable(state: MarkdownSerializerState, node: ProseMirrorNode) {
  const rows: ProseMirrorNode[] = [];
  node.forEach((row) => rows.push(row));

  rows.forEach((row, rowIndex) => {
    const cells: string[] = [];
    row.forEach((cell) => cells.push(cellPlainText(cell)));
    state.write(`| ${cells.join(" | ")} |\n`);
    if (rowIndex === 0) {
      state.write(`| ${cells.map(() => "---").join(" | ")} |\n`);
    }
  });
  state.closeBlock(node);
}

const baseBlockquoteSerialize = defaultMarkdownSerializer.nodes.blockquote!;
const baseParagraphSerialize = defaultMarkdownSerializer.nodes.paragraph!;
const baseImageSerialize = defaultMarkdownSerializer.nodes.image!;
const baseHardBreakSerialize = defaultMarkdownSerializer.nodes.hard_break!;

function isSpacerParagraph(node: ProseMirrorNode): boolean {
  return node.attrs.irisSpacer === true;
}

const irisMarkdownSerializer = new MarkdownSerializer(
  {
    ...defaultMarkdownSerializer.nodes,
    paragraph(state, node, parent, index) {
      if (isSpacerParagraph(node)) {
        const gapCount =
          typeof node.attrs.irisGapCount === "number" &&
          node.attrs.irisGapCount > 0
            ? node.attrs.irisGapCount
            : 1;
        if (gapCount > 1) {
          state.write("\n".repeat((gapCount - 1) * 2));
        }
        state.closeBlock(node);
        return;
      }
      baseParagraphSerialize(state, node, parent, index);
    },
    image(state, node, parent, index) {
      baseImageSerialize(state, node, parent, index);
    },
    hardBreak(state, node, parent, index) {
      baseHardBreakSerialize(state, node, parent, index);
    },
    blockquote(state, node, parent, index) {
      if (renderCalloutBlockquote(state, node)) {
        return;
      }
      baseBlockquoteSerialize(state, node, parent, index);
    },
    preserveBlock(state, node) {
      const raw =
        typeof node.attrs.originalRaw === "string"
          ? node.attrs.originalRaw
          : "";
      state.write(raw);
      state.closeBlock(node);
    },
    table: renderTable,
    taskList(state, node) {
      node.forEach((item, _, index) => {
        if (index > 0) {
          state.write("\n");
        }
        state.render(item, node, index);
      });
      state.closeBlock(node);
    },
    taskItem(state, node) {
      const checked = node.attrs.checked === true;
      state.write(checked ? "- [x] " : "- [ ] ");
      node.forEach((child) => {
        if (child.type.name === "paragraph") {
          state.renderInline(child);
        }
      });
    },
  },
  {
    ...defaultMarkdownSerializer.marks,
    /** TipTap StarterKit uses `bold` / `italic`; prosemirror-markdown defaults use `strong` / `em`. */
    bold: {
      open: "**",
      close: "**",
      mixable: true,
      expelEnclosingWhitespace: false,
    },
    italic: {
      open: "*",
      close: "*",
      mixable: true,
      expelEnclosingWhitespace: false,
    },
    strike: {
      open: "~~",
      close: "~~",
      mixable: true,
      expelEnclosingWhitespace: false,
    },
    wikiLink: {
      open: "[[",
      close: "]]",
      mixable: true,
      expelEnclosingWhitespace: false,
    },
  },
);

/**
 * Serialize TipTap document tree → markdown (avoids getHTML + Turndown on the hot path).
 * Falls back to HTML turndown when the doc contains unsupported nodes.
 */
export function editorDocToMarkdown(editor: Editor): string {
  try {
    return irisMarkdownSerializer.serialize(editor.state.doc);
  } catch (e) {
    console.error(
      "[editor-pm-serialize] PM serializer failed, falling back to HTML turndown:",
      e,
    );
    return editorBodyHtmlToMarkdown(editor.getHTML());
  }
}
