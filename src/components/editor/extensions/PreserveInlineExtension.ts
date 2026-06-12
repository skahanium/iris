import { mergeAttributes, Node } from "@tiptap/core";

function truncate(text: string, maxLen = 48): string {
  if (text.length <= maxLen) return text;
  return `${text.slice(0, maxLen)}...`;
}

/**
 * PreserveInline — inline atom for safe raw HTML that must round-trip unchanged.
 */
export const PreserveInlineExtension = Node.create({
  name: "preserveInline",

  group: "inline",

  inline: true,

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
    return [{ tag: 'span[data-type="preserve-inline"]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(
        {
          "data-type": "preserve-inline",
          contenteditable: "false",
        },
        HTMLAttributes,
      ),
    ];
  },

  addNodeView() {
    return ({ node: initialNode }) => {
      let currentNode = initialNode;
      const dom = document.createElement("span");
      dom.className =
        "mx-0.5 inline-flex select-none rounded border border-border bg-muted/40 px-1 py-0.5 font-mono text-[0.85em] text-muted-foreground";
      dom.setAttribute("data-type", "preserve-inline");
      dom.setAttribute("contenteditable", "false");
      dom.setAttribute("aria-label", "Preserved inline HTML");

      const paint = (current: typeof initialNode) => {
        const originalRaw = (current.attrs.originalRaw as string) || "";
        const syntaxKind = (current.attrs.syntaxKind as string) || "raw_html";
        dom.textContent = truncate(originalRaw);
        dom.title = originalRaw;
        dom.setAttribute("data-original-raw", originalRaw);
        dom.setAttribute("data-syntax-kind", syntaxKind);
      };

      paint(currentNode);

      return {
        dom,
        update(updatedNode) {
          if (updatedNode.type.name !== "preserveInline") return false;
          currentNode = updatedNode;
          paint(currentNode);
          return true;
        },
      };
    };
  },
});
