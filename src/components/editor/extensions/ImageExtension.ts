import { mergeAttributes, Node } from "@tiptap/core";

function isSafeImageSrc(src: string): boolean {
  const trimmed = src.trim();
  if (!trimmed) return false;
  if (/^(https?:|file:|\/|\.\/|\.\.\/)/i.test(trimmed)) return true;
  return !/^[a-z][a-z0-9+.-]*:/i.test(trimmed);
}

export const ImageExtension = Node.create({
  name: "image",

  group: "block",

  atom: true,

  draggable: true,

  addAttributes() {
    return {
      src: {
        default: null,
        parseHTML: (element) => {
          const src = element.getAttribute("src") ?? "";
          return isSafeImageSrc(src) ? src : null;
        },
        renderHTML: (attributes) => {
          const src =
            typeof attributes.src === "string" && isSafeImageSrc(attributes.src)
              ? attributes.src
              : null;
          return src ? { src } : {};
        },
      },
      alt: {
        default: "",
        parseHTML: (element) => element.getAttribute("alt") ?? "",
        renderHTML: (attributes) =>
          typeof attributes.alt === "string" ? { alt: attributes.alt } : {},
      },
      title: {
        default: null,
        parseHTML: (element) => element.getAttribute("title"),
        renderHTML: (attributes) =>
          typeof attributes.title === "string" && attributes.title.trim()
            ? { title: attributes.title }
            : {},
      },
    };
  },

  parseHTML() {
    return [{ tag: "img[src]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return ["img", mergeAttributes(HTMLAttributes)];
  },
});
