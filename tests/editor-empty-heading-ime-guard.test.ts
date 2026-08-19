import { describe, expect, it } from "vitest";

import { outlineFromDoc } from "@/lib/document-outline";

import {
  createProductionEditorFromHtml,
  pmSerializeBody,
} from "./helpers/tiptap-serialize-harness";

describe("EmptyHeadingImeGuardExtension", () => {
  it("keeps a zero-width placeholder in empty headings after a transaction", () => {
    const editor = createProductionEditorFromHtml("<h1></h1>");
    try {
      editor.view.dispatch(editor.state.tr);
      expect(editor.state.doc.child(0).textContent).toBe("\u200b");
    } finally {
      editor.destroy();
    }
  });

  it("removes the zero-width placeholder once real heading text is inserted", () => {
    const editor = createProductionEditorFromHtml("<h1></h1>");
    try {
      editor.view.dispatch(editor.state.tr);
      expect(editor.state.doc.child(0).textContent).toBe("\u200b");

      editor.commands.insertContent("中文");
      expect(editor.state.doc.child(0).type.name).toBe("heading");
      expect(editor.state.doc.child(0).textContent).toBe("中文");
    } finally {
      editor.destroy();
    }
  });

  it.each([1, 2, 3])(
    "keeps an H%s Chinese heading in the outline and serializer after IME-style insertion",
    (level) => {
      const editor = createProductionEditorFromHtml(`<h${level}></h${level}>`);
      try {
        editor.view.dispatch(editor.state.tr);
        editor.commands.insertContent("中文");

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

  it("does not leak the placeholder when serializing an empty heading", () => {
    const editor = createProductionEditorFromHtml("<h1></h1>");
    try {
      editor.view.dispatch(editor.state.tr);
      const md = pmSerializeBody(editor);
      expect(md).not.toContain("\u200b");
      expect(md.trim()).toBe("#");
    } finally {
      editor.destroy();
    }
  });
});
