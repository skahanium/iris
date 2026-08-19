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

const ZWSP = "\u200b";

function stripZeroWidthSpacesFromHeading(
  node: ProseMirrorNode,
): ProseMirrorNode {
  if (!node.textContent.includes(ZWSP)) return node;
  const children: ProseMirrorNode[] = [];
  node.forEach((child) => {
    if (child.isText) {
      const text = (child.text ?? "").replaceAll(ZWSP, "");
      if (text) {
        children.push(node.type.schema.text(text, child.marks));
      }
    } else {
      children.push(child);
    }
  });
  return node.type.create(
    node.attrs,
    children.length > 0 ? Fragment.fromArray(children) : undefined,
  );
}

function escapeTableCellText(text: string): string {
  return text.replace(/\|/g, "\\|").replace(/\r?\n/g, "<br>");
}

function applyInlineMarks(text: string, marks: readonly unknown[]): string {
  let value = text;
  for (const mark of marks) {
    const typeName = (mark as { type: { name: string } }).type.name;
    if (typeName === "bold") value = `**${value}**`;
    else if (typeName === "italic") value = `*${value}*`;
    else if (typeName === "strike") value = `~~${value}~~`;
    else if (typeName === "code") value = `\`${value}\``;
    else if (typeName === "link") {
      const href = (mark as { attrs: { href?: string } }).attrs.href ?? "";
      value = `[${value}](${href})`;
    } else if (typeName === "wikiLink") {
      value = `[[${value}]]`;
    }
  }
  return value;
}

function inlineNodeToMarkdown(node: ProseMirrorNode): string {
  if (node.isText) {
    return applyInlineMarks(escapeTableCellText(node.text ?? ""), node.marks);
  }
  if (node.type.name === "image") {
    const attrs = node.attrs as {
      src?: string;
      alt?: string;
      title?: string | null;
    };
    const alt = attrs.alt ?? "";
    const title = attrs.title ? ` "${attrs.title}"` : "";
    return `![${escapeTableCellText(alt)}](${attrs.src ?? ""}${title})`;
  }
  if (node.type.name === "hardBreak") {
    return "<br>";
  }
  // Fallback: preserve text content for unknown inline nodes.
  return escapeTableCellText(node.textContent);
}

function cellToMarkdown(cell: ProseMirrorNode): string {
  const parts: string[] = [];
  cell.forEach((child) => {
    if (child.type.name === "paragraph") {
      const inline: string[] = [];
      child.forEach((inner) => inline.push(inlineNodeToMarkdown(inner)));
      parts.push(inline.join(""));
    } else {
      parts.push(escapeTableCellText(child.textContent));
    }
  });
  return parts.join("<br>").trim();
}

function renderTable(state: MarkdownSerializerState, node: ProseMirrorNode) {
  const rows: ProseMirrorNode[] = [];
  node.forEach((row) => rows.push(row));

  rows.forEach((row, rowIndex) => {
    const cells: string[] = [];
    row.forEach((cell) => cells.push(cellToMarkdown(cell)));
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
const baseHorizontalRuleSerialize =
  defaultMarkdownSerializer.nodes.horizontal_rule!;

function longestBacktickRun(text: string): number {
  let longest = 0;
  for (const match of text.matchAll(/`+/g)) {
    longest = Math.max(longest, match[0].length);
  }
  return longest;
}

function renderCodeBlock(
  state: MarkdownSerializerState,
  node: ProseMirrorNode,
) {
  const language =
    typeof node.attrs.language === "string"
      ? node.attrs.language.trim()
      : typeof node.attrs.params === "string"
        ? node.attrs.params.trim()
        : "";
  const text = node.textContent.replace(/\n+$/g, "");
  const fence = "`".repeat(Math.max(3, longestBacktickRun(text) + 1));
  state.write(`${fence}${language ? language : ""}\n`);
  if (text) state.text(text, false);
  state.ensureNewLine();
  state.write(fence);
  state.closeBlock(node);
}

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
      baseHeadingSerialize(
        state,
        stripZeroWidthSpacesFromHeading(node),
        parent,
        index,
      );
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
    codeBlock(state, node, _parent, _index) {
      renderCodeBlock(state, node);
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
      let first = true;
      node.forEach((child) => {
        if (child.type.name === "paragraph") {
          if (!first) state.write("\n");
          state.renderInline(child);
          first = false;
        } else {
          if (!first) state.write("\n");
          state.render(child, node, 0);
          first = false;
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
  return irisMarkdownSerializer.serialize(editor.state.doc);
}
