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
import {
  DOMSerializer,
  Fragment,
  type Node as ProseMirrorNode,
} from "@tiptap/pm/model";
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
const baseHeadingSerialize = defaultMarkdownSerializer.nodes.heading!;
const baseImageSerialize = defaultMarkdownSerializer.nodes.image!;
const baseHardBreakSerialize = defaultMarkdownSerializer.nodes.hard_break!;
const baseCodeBlockSerialize = defaultMarkdownSerializer.nodes.code_block!;
const baseHorizontalRuleSerialize =
  defaultMarkdownSerializer.nodes.horizontal_rule!;

function irisIndent(node: ProseMirrorNode): number {
  const value = node.attrs.irisIndent;
  const raw =
    typeof value === "number"
      ? value
      : typeof value === "string"
        ? Number(value)
        : 0;
  if (!Number.isFinite(raw)) return 0;
  return Math.max(0, Math.trunc(raw));
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function inlineHtml(node: ProseMirrorNode): string {
  if (globalThis.document === undefined) {
    return escapeHtml(node.textContent);
  }

  const serializer = DOMSerializer.fromSchema(node.type.schema);
  const fragment = serializer.serializeFragment(node.content, {
    document: globalThis.document,
  });
  const container = globalThis.document.createElement("div");
  container.appendChild(fragment);
  return container.innerHTML;
}

function renderIrisIndentedHtmlBlock(
  state: MarkdownSerializerState,
  node: ProseMirrorNode,
  tag: string,
): boolean {
  const indent = irisIndent(node);
  if (indent <= 0) return false;

  state.write(
    `<${tag} data-iris-indent="${indent}">${inlineHtml(node)}</${tag}>`,
  );
  state.closeBlock(node);
  return true;
}

function isTransientEmptyListLikeItem(node: ProseMirrorNode): boolean {
  if (node.type.name !== "listItem" && node.type.name !== "taskItem") {
    return false;
  }
  if (node.textContent.trim() !== "") return false;

  let hasStructuralContent = false;
  node.forEach((child) => {
    if (child.type.name !== "paragraph" || child.childCount > 0) {
      hasStructuralContent = true;
    }
  });
  return !hasStructuralContent;
}

function withoutTrailingEmptyListItems(
  node: ProseMirrorNode,
): ProseMirrorNode | null {
  const children: ProseMirrorNode[] = [];
  node.forEach((child) => children.push(child));

  while (
    children.length > 0 &&
    isTransientEmptyListLikeItem(children[children.length - 1]!)
  ) {
    children.pop();
  }

  return children.length > 0 ? node.copy(Fragment.fromArray(children)) : null;
}

function shouldLogSerializerFallback(): boolean {
  return import.meta.env.MODE !== "test";
}

const irisMarkdownSerializer = new MarkdownSerializer(
  {
    ...defaultMarkdownSerializer.nodes,
    paragraph(state, node, parent, index) {
      if (node.childCount === 0) {
        return;
      }
      if (renderIrisIndentedHtmlBlock(state, node, "p")) {
        return;
      }
      baseParagraphSerialize(state, node, parent, index);
    },
    heading(state, node, parent, index) {
      const rawLevel = node.attrs.level;
      const level =
        typeof rawLevel === "number"
          ? Math.min(6, Math.max(1, Math.trunc(rawLevel)))
          : 1;
      if (renderIrisIndentedHtmlBlock(state, node, `h${level}`)) {
        return;
      }
      baseHeadingSerialize(state, node, parent, index);
    },
    image(state, node, parent, index) {
      baseImageSerialize(state, node, parent, index);
    },
    wikiMediaEmbed(state, node) {
      const target =
        typeof node.attrs.target === "string" ? node.attrs.target.trim() : "";
      const alias =
        typeof node.attrs.alias === "string" ? node.attrs.alias.trim() : "";
      if (!target) return;
      state.write(alias ? `![[${target}|${alias}]]` : `![[${target}]]`);
      state.closeBlock(node);
    },
    hardBreak(state, node, parent, index) {
      baseHardBreakSerialize(state, node, parent, index);
    },
    codeBlock(state, node, parent, index) {
      baseCodeBlockSerialize(state, node, parent, index);
    },
    horizontalRule(state, node, parent, index) {
      baseHorizontalRuleSerialize(state, node, parent, index);
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
    preserveInline(state, node) {
      const raw =
        typeof node.attrs.originalRaw === "string"
          ? node.attrs.originalRaw
          : "";
      state.write(raw);
    },
    footnoteRef(state, node) {
      const label =
        typeof node.attrs.label === "string" ? node.attrs.label : "";
      state.write(`[^${label}]`);
    },
    footnoteDef(state, node) {
      const raw =
        typeof node.attrs.originalRaw === "string"
          ? node.attrs.originalRaw
          : "";
      state.write(raw);
      state.closeBlock(node);
    },
    table: renderTable,
    taskList(state, node) {
      const persistedNode = withoutTrailingEmptyListItems(node);
      if (!persistedNode) return;
      persistedNode.forEach((item, _, index) => {
        if (index > 0) {
          state.write("\n");
        }
        state.render(item, persistedNode, index);
      });
      state.closeBlock(persistedNode);
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
    bulletList(state, node) {
      const persistedNode = withoutTrailingEmptyListItems(node);
      if (!persistedNode) return;
      state.renderList(persistedNode, "  ", () => "- ");
    },
    orderedList(state, node) {
      const persistedNode = withoutTrailingEmptyListItems(node);
      if (!persistedNode) return;
      const start = typeof node.attrs.start === "number" ? node.attrs.start : 1;
      const maxWidth = String(start + persistedNode.childCount - 1).length;
      const space = state.repeat(" ", maxWidth + 2);
      state.renderList(persistedNode, space, (index) => {
        const number = String(start + index);
        return `${state.repeat(" ", maxWidth - number.length)}${number}. `;
      });
    },
    listItem(state, node) {
      state.renderContent(node);
    },
    aiStream() {
      // Inline AI suggestions are transient UI state. Persist the surrounding
      // document only; generated suggestion text is accepted explicitly.
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
    if (shouldLogSerializerFallback()) {
      console.error(
        "[editor-pm-serialize] PM serializer failed, falling back to HTML turndown:",
        e,
      );
    }
    return editorBodyHtmlToMarkdown(editor.getHTML());
  }
}
