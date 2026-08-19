import { describe, expect, it } from "vitest";

import { createProductionEditorFromHtml } from "./helpers/tiptap-serialize-harness";

describe("HeadingDomGuardExtension", () => {
  it("merges a newly inserted paragraph back into an empty heading", () => {
    const editor = createProductionEditorFromHtml("<h1></h1>");
    try {
      editor.commands.insertContentAt(
        editor.state.doc.content.size,
        "<p>Hello</p>",
      );
      expect(editor.state.doc.childCount).toBe(1);
      expect(editor.state.doc.child(0).type.name).toBe("heading");
      expect(editor.state.doc.child(0).textContent).toBe("Hello");
      expect(editor.getHTML()).toContain("<h1");
      expect(editor.getHTML()).toContain("Hello");
    } finally {
      editor.destroy();
    }
  });

  it("does not merge a pre-existing empty heading followed by a paragraph", () => {
    const editor = createProductionEditorFromHtml("<h1></h1><p>Existing</p>");
    try {
      editor.view.dispatch(editor.state.tr);
      expect(editor.state.doc.childCount).toBe(2);
      expect(editor.state.doc.child(0).type.name).toBe("heading");
      expect(editor.state.doc.child(1).type.name).toBe("paragraph");
      expect(editor.state.doc.child(1).textContent).toBe("Existing");
    } finally {
      editor.destroy();
    }
  });
});
