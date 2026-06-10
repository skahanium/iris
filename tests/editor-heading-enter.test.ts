import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";

import { HeadingFoldExtension } from "@/components/editor/extensions/HeadingFoldExtension";
import { IrisDocument } from "@/components/editor/extensions/IrisDocument";

function createIrisEditor(content: object) {
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
      HeadingFoldExtension,
    ],
    content,
  });
}

function blockPos(editor: Editor, index: number): number {
  let pos = 0;
  for (let i = 0; i < index; i++) {
    pos += editor.state.doc.child(i).nodeSize;
  }
  return pos + 1;
}

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("editor heading Enter behavior", () => {
  let editor: Editor | undefined;

  afterEach(() => {
    editor?.destroy();
    editor = undefined;
  });

  it("splits section h1 in the middle (HeadingFold does not hijack Enter)", () => {
    editor = createIrisEditor({
      type: "doc",
      content: [
        {
          type: "heading",
          attrs: { level: 1 },
          content: [{ type: "text", text: "章节标题" }],
        },
        { type: "paragraph" },
      ],
    });

    const headingStart = blockPos(editor, 0);
    editor.commands.setTextSelection(headingStart + 2);
    expect(editor.commands.splitBlock()).toBe(true);

    const heading = editor.state.doc.child(0);
    expect(heading.type.name).toBe("heading");
    expect(heading.textContent.length).toBeLessThan("章节标题".length);
    expect(editor.state.doc.childCount).toBeGreaterThanOrEqual(2);
  });

  it("allows splitBlock at end of section h1", () => {
    editor = createIrisEditor({
      type: "doc",
      content: [
        {
          type: "heading",
          attrs: { level: 1 },
          content: [{ type: "text", text: "一级" }],
        },
      ],
    });

    const heading = editor.state.doc.child(0);
    const endInside = blockPos(editor, 0) + heading.textContent.length;
    editor.commands.setTextSelection(endInside);
    expect(editor.commands.splitBlock()).toBe(true);
    expect(editor.state.doc.childCount).toBeGreaterThanOrEqual(2);
  });

  it("keeps heading fold controls outside editable heading DOM for IME composition", () => {
    editor = createIrisEditor({
      type: "doc",
      content: [
        {
          type: "heading",
          attrs: { level: 1 },
          content: [{ type: "text", text: "拼音输入标题" }],
        },
        { type: "paragraph" },
      ],
    });

    const heading = editor.view.dom.querySelector("h1");
    expect(heading).not.toBeNull();
    expect(heading?.querySelector(".iris-heading-fold-gutter")).toBeNull();
    expect(heading?.querySelector(".ProseMirror-widget")).toBeNull();
  });

  it("renders heading fold controls through a React overlay gutter", () => {
    const editorSource = read("src/components/editor/TipTapEditor.tsx");
    const css = read("src/styles/globals.css");

    expect(editorSource).toContain("HeadingFoldOverlay");
    expect(css).toContain(".iris-heading-fold-overlay");
    expect(css).toContain(".iris-heading-fold-row");
  });
});
