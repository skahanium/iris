import { describe, expect, it } from "vitest";

import {
  createProductionEditorFromHtml,
  pmSerializeBody,
} from "./helpers/tiptap-serialize-harness";

describe("EmptyHeadingImeGuardExtension", () => {
  it("adds a zero-width space before IME composition and removes it after", () => {
    const editor = createProductionEditorFromHtml("<h1></h1>");
    try {
      editor.commands.setTextSelection(1);

      editor.view.dom.dispatchEvent(
        new CompositionEvent("compositionstart", { bubbles: true }),
      );
      expect(editor.state.doc.child(0).textContent).toBe("\u200b");

      editor.commands.insertContent("中文");

      editor.view.dom.dispatchEvent(
        new CompositionEvent("compositionend", { bubbles: true }),
      );
      expect(editor.state.doc.child(0).type.name).toBe("heading");
      expect(editor.state.doc.child(0).textContent).toBe("中文");
    } finally {
      editor.destroy();
    }
  });

  it("strips zero-width spaces from headings during serialization", () => {
    const editor = createProductionEditorFromHtml("<h1>\u200b中文</h1>");
    try {
      const md = pmSerializeBody(editor);
      expect(md).not.toContain("\u200b");
      expect(md).toContain("# 中文");
    } finally {
      editor.destroy();
    }
  });
});
