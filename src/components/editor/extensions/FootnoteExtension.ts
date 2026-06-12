import { mergeAttributes, Node } from "@tiptap/core";

function labelFromElement(element: HTMLElement): string {
  return (
    element.getAttribute("data-footnote-ref") ??
    element.getAttribute("data-footnote-def") ??
    ""
  );
}

function suffix(label: string): string {
  const encoded = encodeURIComponent(label.trim()).replace(
    /[!'()*]/g,
    (ch) => `%${ch.charCodeAt(0).toString(16).toUpperCase()}`,
  );
  return encoded || "note";
}

export const FootnoteRefExtension = Node.create({
  name: "footnoteRef",

  group: "inline",

  inline: true,

  atom: true,

  selectable: true,

  addAttributes() {
    return {
      label: {
        default: "",
        parseHTML: (element) => labelFromElement(element as HTMLElement),
        renderHTML: (attributes) => ({
          "data-footnote-ref": attributes.label as string,
        }),
      },
    };
  },

  parseHTML() {
    return [{ tag: "sup[data-footnote-ref]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    const label = String(node.attrs.label ?? "");
    const idSuffix = suffix(label);
    return [
      "sup",
      mergeAttributes(
        {
          id: `footnote-ref-${idSuffix}`,
          contenteditable: "false",
          "aria-label": `Footnote reference ${label}`,
          title: `Footnote reference ${label}`,
        },
        HTMLAttributes,
      ),
      [
        "a",
        {
          href: `#footnote-${idSuffix}`,
          contenteditable: "false",
          "aria-label": `Jump to footnote ${label}`,
        },
        `[^${label}]`,
      ],
    ];
  },
});

export const FootnoteDefExtension = Node.create({
  name: "footnoteDef",

  group: "block",

  content: "block*",

  defining: true,

  addAttributes() {
    return {
      label: {
        default: "",
        parseHTML: (element) => labelFromElement(element as HTMLElement),
        renderHTML: (attributes) => ({
          "data-footnote-def": attributes.label as string,
        }),
      },
      originalRaw: {
        default: "",
        parseHTML: (element) => element.getAttribute("data-original-raw") ?? "",
        renderHTML: (attributes) => ({
          "data-original-raw": attributes.originalRaw as string,
        }),
      },
    };
  },

  parseHTML() {
    return [{ tag: "section[data-footnote-def]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    const label = String(node.attrs.label ?? "");
    const idSuffix = suffix(label);
    return [
      "section",
      mergeAttributes(
        {
          id: `footnote-${idSuffix}`,
          "data-footnote-return": `footnote-ref-${idSuffix}`,
          "aria-label": `Footnote definition ${label}`,
        },
        HTMLAttributes,
      ),
      0,
    ];
  },
});
