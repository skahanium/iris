import StarterKit from "@tiptap/starter-kit";
import { Editor } from "@tiptap/core";
import { afterEach, describe, expect, it } from "vitest";

import { IrisDocument } from "@/components/editor/extensions/IrisDocument";
import { resetEditorContentBaseline } from "@/lib/editor-baseline";

describe("resetEditorContentBaseline selection", () => {
  let editor: Editor | null = null;

  afterEach(() => {
    editor?.destroy();
    editor = null;
  });

  it("preserves a nearby valid caret when the prior position lands on a non-text node", () => {
    editor = new Editor({
      extensions: [
        IrisDocument,
        StarterKit.configure({
          document: false,
          codeBlock: false,
          heading: { levels: [1, 2, 3] },
        }),
      ],
      content: "<p>alpha</p><hr><p>beta</p>",
    });

    const beforeSize = editor.state.doc.content.size;
    // Jump the caret near the horizontal rule boundary, then shrink the doc.
    editor.commands.setTextSelection(Math.max(1, beforeSize - 2));

    resetEditorContentBaseline(editor, "<p>only</p>", {
      selection: "preserve",
    });

    const { from, to } = editor.state.selection;
    expect(from).toBeGreaterThanOrEqual(1);
    expect(to).toBeLessThan(editor.state.doc.content.size);
    expect(editor.state.doc.textContent).toContain("only");
  });

  it("falls back to document start when preserve cannot resolve any range", () => {
    editor = new Editor({
      extensions: [
        IrisDocument,
        StarterKit.configure({
          document: false,
          codeBlock: false,
          heading: { levels: [1, 2, 3] },
        }),
      ],
      content: "<p>long starting document with many characters</p>",
    });

    editor.commands.setTextSelection(20);

    resetEditorContentBaseline(editor, "<p>x</p>", {
      selection: { from: 999, to: 999 },
    });

    expect(editor.state.selection.from).toBeGreaterThanOrEqual(1);
    expect(editor.state.doc.textContent).toBe("x");
  });
});
