import { mergeAttributes, Node } from "@tiptap/core";
import { ReactNodeViewRenderer } from "@tiptap/react";

import { PreserveNodeView } from "../PreserveNodeView";

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    preserveBlock: Record<string, never>;
  }
}

/**
 * PreserveBlock — 只读保护块节点。
 *
 * 用于承载当前不可完整编辑的 Markdown 语法（Raw HTML、HTML 注释等）。
 *
 * 属性：
 * - `originalRaw`: 原始 Markdown 原文，导出时无操作即可回吐
 * - `syntaxKind`: 语法族标识，用于渲染时展示类型标签
 *
 * 行为：
 * - `atom: true` — 不可进入内部编辑
 * - `selectable: true` — 可选中（复制/删除整块）
 * - NodeView 渲染为只读卡片
 */
export const PreserveBlockExtension = Node.create({
  name: "preserveBlock",

  group: "block",

  atom: true,

  selectable: true,

  addAttributes() {
    return {
      originalRaw: {
        default: "",
        parseHTML: (element) => element.getAttribute("data-original-raw") ?? "",
        renderHTML: (attributes) => ({
          "data-original-raw": attributes.originalRaw as string,
        }),
      },
      syntaxKind: {
        default: "raw_html",
        parseHTML: (element) =>
          element.getAttribute("data-syntax-kind") ?? "raw_html",
        renderHTML: (attributes) => ({
          "data-syntax-kind": attributes.syntaxKind as string,
        }),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'div[data-type="preserve-block"]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "div",
      mergeAttributes({ "data-type": "preserve-block" }, HTMLAttributes),
    ];
  },

  addNodeView() {
    return ReactNodeViewRenderer(PreserveNodeView);
  },
});
