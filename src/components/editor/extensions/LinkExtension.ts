import { Mark, mergeAttributes } from "@tiptap/core";

function isSafeHref(href: string): boolean {
  const trimmed = href.trim();
  if (!trimmed) return false;
  if (/^(https?:|mailto:|tel:|#|\/|\.\/|\.\.\/)/i.test(trimmed)) {
    return true;
  }
  return !/^[a-z][a-z0-9+.-]*:/i.test(trimmed);
}

export const LinkExtension = Mark.create({
  name: "link",

  priority: 1000,

  inclusive: false,

  addAttributes() {
    return {
      href: {
        default: null,
        parseHTML: (element) => {
          const href = element.getAttribute("href") ?? "";
          return isSafeHref(href) ? href : null;
        },
        renderHTML: (attributes) => {
          const href =
            typeof attributes.href === "string" && isSafeHref(attributes.href)
              ? attributes.href
              : null;
          return href ? { href } : {};
        },
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
    return [{ tag: "a[href]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "a",
      mergeAttributes(
        { rel: "noopener noreferrer", target: "_blank" },
        HTMLAttributes,
      ),
      0,
    ];
  },
});
