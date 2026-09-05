import { describe, expect, it } from "vitest";

import { outlineFromDoc } from "@/lib/document-outline";

import {
  createProductionEditorFromHtml,
  pmSerializeBody,
} from "./helpers/tiptap-serialize-harness";

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

  it.each([1, 2, 3])(
    "merges a newly inserted Chinese paragraph back into an empty H%s heading and keeps outline+serializer",
    (level) => {
      const editor = createProductionEditorFromHtml(`<h${level}></h${level}>`);
      try {
        editor.commands.insertContentAt(
          editor.state.doc.content.size,
          "<p>中文</p>",
        );

        expect(editor.state.doc.childCount).toBe(1);
        expect(editor.state.doc.child(0).type.name).toBe("heading");
        expect(editor.state.doc.child(0).textContent).toBe("中文");

        const outline = outlineFromDoc(editor.state.doc);
        expect(outline).toEqual([{ level, text: "中文", pos: 1 }]);

        const md = pmSerializeBody(editor);
        expect(md).not.toContain("\u200b");
        expect(md).toContain(`${"#".repeat(level)} 中文`);
      } finally {
        editor.destroy();
      }
    },
  );

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
