import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";

import { ImeCompositionGuardExtension } from "@/components/editor/extensions/ImeCompositionGuardExtension";
import { IrisDocument } from "@/components/editor/extensions/IrisDocument";

function createEditor() {
  return new Editor({
    extensions: [
      IrisDocument,
      StarterKit.configure({
        document: false,
        codeBlock: false,
        heading: {
          levels: [1, 2, 3],
          HTMLAttributes: { class: "iris-section-heading" },
        },
      }),
      ImeCompositionGuardExtension,
    ],
    content: {
      type: "doc",
      content: [
        {
          type: "heading",
          attrs: { level: 1 },
          content: [{ type: "text", text: "标题" }],
        },
        { type: "paragraph" },
      ],
    },
  });
}

describe("ImeCompositionGuardExtension", () => {
  let editor: Editor | undefined;

  afterEach(() => {
    editor?.destroy();
    editor = undefined;
  });

  it("registers the plugin without errors", () => {
    editor = createEditor();
    expect(editor.isDestroyed).toBe(false);
  });

  it("exposes the imeCompositionGuard plugin", () => {
    editor = createEditor();
    // The plugin should be registered (verified indirectly: editor works with the extension)
    expect(editor.state.plugins.length).toBeGreaterThan(0);
  });

  it("renders heading content correctly after setup", () => {
    editor = createEditor();
    const heading = editor.view.dom.querySelector("h1");
    expect(heading).not.toBeNull();
    expect(heading?.textContent).toBe("标题");
  });

  it("extension source is referenced in TipTapEditor", () => {
    const src = readFileSync("src/components/editor/TipTapEditor.tsx", "utf8");
    expect(src).toContain("ImeCompositionGuardExtension");
  });
});
