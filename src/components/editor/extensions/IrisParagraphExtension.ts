import Paragraph from "@tiptap/extension-paragraph";

/**
 * Paragraph with `irisSpacer` — round-trips block spacing (`\n\n`) from contract `space` fragments.
 */
export const IrisParagraphExtension = Paragraph.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      irisSpacer: {
        default: false,
        parseHTML: (element) =>
          element.getAttribute("data-iris-spacer") === "true",
        renderHTML: (attributes) =>
          attributes.irisSpacer ? { "data-iris-spacer": "true" } : {},
      },
      irisGapCount: {
        default: 1,
        parseHTML: (element) => {
          const raw = element.getAttribute("data-iris-gap-count");
          const n = raw ? Number.parseInt(raw, 10) : 1;
          return Number.isFinite(n) && n > 0 ? n : 1;
        },
        renderHTML: (attributes) => {
          const count =
            typeof attributes.irisGapCount === "number" &&
            attributes.irisGapCount > 1
              ? attributes.irisGapCount
              : null;
          return count ? { "data-iris-gap-count": String(count) } : {};
        },
      },
    };
  },
});
