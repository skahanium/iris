import { mergeAttributes, Node } from "@tiptap/core";

declare module "@tiptap/core" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface Commands<ReturnType> {
    preserveBlock: Record<string, never>;
  }
}

const SYNTAX_KIND_LABELS: Record<string, string> = {
  raw_html: "Raw HTML",
  html_comment: "HTML 注释",
  unknown: "不可编辑",
};

function truncate(text: string, maxLen = 120): string {
  if (text.length <= maxLen) return text;
  return `${text.slice(0, maxLen)}…`;
}

/**
 * PreserveBlock — 只读保护块节点。
 *
 * 使用原生 DOM NodeView（非 React），避免大文档每键重渲染 React NodeView。
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
    return ({ node: initialNode }) => {
      let currentNode = initialNode;
      const dom = document.createElement("div");
      dom.className =
        "my-2 select-none rounded border border-dashed border-muted-foreground/30 bg-muted/30 px-3 py-2";
      dom.setAttribute("contenteditable", "false");

      const paint = (current: typeof initialNode) => {
        const originalRaw = (current.attrs.originalRaw as string) || "";
        const syntaxKind = (current.attrs.syntaxKind as string) || "unknown";
        const label = SYNTAX_KIND_LABELS[syntaxKind] ?? "不可编辑";
        const truncated = truncate(originalRaw);

        dom.replaceChildren();

        const header = document.createElement("div");
        header.className =
          "flex items-center gap-2 text-[11px] text-muted-foreground";

        const badge = document.createElement("span");
        badge.className = "font-medium";
        badge.textContent = label;

        const hint = document.createElement("span");
        hint.className = "text-muted-foreground/50";
        hint.textContent = "· 只读 · 原文保留";

        header.append(badge, hint);

        const body = document.createElement("div");
        body.className =
          "mt-1 whitespace-pre-wrap break-all font-mono text-[11px] text-muted-foreground/70";
        if (originalRaw.length > truncated.length) {
          body.title = originalRaw;
        }
        body.textContent = truncated;

        dom.append(header, body);
      };

      paint(currentNode);

      return {
        dom,
        update(updatedNode) {
          if (updatedNode.type.name !== "preserveBlock") return false;
          if (
            updatedNode.attrs.originalRaw === currentNode.attrs.originalRaw &&
            updatedNode.attrs.syntaxKind === currentNode.attrs.syntaxKind
          ) {
            return true;
          }
          currentNode = updatedNode;
          paint(currentNode);
          return true;
        },
      };
    };
  },
});
