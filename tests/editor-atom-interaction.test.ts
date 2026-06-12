import { describe, expect, it } from "vitest";

import {
  createProductionEditorFromIngestedBody,
  pmSerializeBody,
} from "./helpers/tiptap-serialize-harness";

function firstNodePosition(
  editor: ReturnType<typeof createProductionEditorFromIngestedBody>,
  typeName: string,
): { from: number; to: number } {
  let range: { from: number; to: number } | null = null;
  editor.state.doc.descendants((node, pos) => {
    if (!range && node.type.name === typeName) {
      range = { from: pos, to: pos + node.nodeSize };
      return false;
    }
    return true;
  });
  if (!range) throw new Error(`Missing ${typeName} node`);
  return range;
}

describe("editor atom node interactions", () => {
  it("renders preserveInline as a labelled, non-editable atom", () => {
    const editor = createProductionEditorFromIngestedBody(
      "Press <kbd>Ctrl</kbd> + <kbd>C</kbd>.",
    );
    try {
      const atom = editor.view.dom.querySelector(
        '[data-type="preserve-inline"]',
      );

      expect(atom).toBeInstanceOf(HTMLElement);
      expect(atom?.getAttribute("contenteditable")).toBe("false");
      expect(atom?.getAttribute("aria-label")).toBe("Preserved inline HTML");
      expect(atom?.getAttribute("title")).toBe("<kbd>Ctrl</kbd>");
    } finally {
      editor.destroy();
    }
  });

  it("deletes and restores preserveInline as a whole atom", () => {
    const editor = createProductionEditorFromIngestedBody(
      "Press <kbd>Ctrl</kbd> + <kbd>C</kbd>.",
    );
    try {
      const range = firstNodePosition(editor, "preserveInline");
      expect(editor.commands.deleteRange(range)).toBe(true);
      expect(pmSerializeBody(editor)).not.toContain("<kbd>Ctrl</kbd>");
      expect(pmSerializeBody(editor)).toContain("<kbd>C</kbd>");

      expect(editor.commands.undo()).toBe(true);
      expect(pmSerializeBody(editor)).toContain("<kbd>Ctrl</kbd>");
      expect(pmSerializeBody(editor)).toContain("<kbd>C</kbd>");
    } finally {
      editor.destroy();
    }
  });

  it("renders footnotes as labelled atoms with native anchor targets", () => {
    const editor = createProductionEditorFromIngestedBody(
      "Text[^a]\n\n[^a]: Body",
    );
    try {
      const ref = editor.view.dom.querySelector("[data-footnote-ref]");
      const def = editor.view.dom.querySelector("[data-footnote-def]");
      const link = ref?.querySelector("a");

      expect(ref).toBeInstanceOf(HTMLElement);
      expect(ref?.getAttribute("contenteditable")).toBe("false");
      expect(ref?.getAttribute("aria-label")).toBe("Footnote reference a");
      expect(ref?.getAttribute("id")).toBe("footnote-ref-a");
      expect(link?.getAttribute("href")).toBe("#footnote-a");
      expect(def).toBeInstanceOf(HTMLElement);
      expect(def?.getAttribute("aria-label")).toBe("Footnote definition a");
      expect(def?.getAttribute("data-footnote-return")).toBe("footnote-ref-a");
    } finally {
      editor.destroy();
    }
  });
});
