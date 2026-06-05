import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { afterEach, describe, expect, it } from "vitest";

import { ListIndentKeymapExtension } from "@/components/editor/extensions/ListIndentKeymapExtension";

function createListEditor(): Editor {
  return new Editor({
    extensions: [StarterKit, ListIndentKeymapExtension],
    content:
      "<ul><li><p>one</p></li><li><p>two</p></li><li><p>three</p></li></ul>",
  });
}

function placeCursorInText(editor: Editor, text: string): void {
  let from: number | null = null;
  editor.state.doc.descendants((node, pos) => {
    if (node.isText && node.text === text) {
      from = pos;
      return false;
    }
  });
  if (from === null) {
    throw new Error(`text not found: ${text}`);
  }
  editor.commands.setTextSelection(from + text.length);
}

function nestedListItemTexts(editor: Editor): string[] {
  const nested: string[] = [];
  editor.state.doc.descendants((node, _pos, parent) => {
    if (node.type.name !== "listItem") return;
    if (parent?.type.name === "bulletList") {
      const grandParent = editor.state.doc.resolve(_pos).node(-1);
      if (grandParent.type.name === "listItem") {
        nested.push(node.textContent);
      }
    }
  });
  return nested;
}

describe("ListIndentKeymapExtension", () => {
  let editor: Editor | undefined;

  afterEach(() => {
    editor?.destroy();
    editor = undefined;
  });

  it("sinks the current ordinary list item on Tab", () => {
    editor = createListEditor();
    placeCursorInText(editor, "two");

    expect(editor.commands.keyboardShortcut("Tab")).toBe(true);

    expect(nestedListItemTexts(editor)).toContain("two");
  });

  it("lifts the current ordinary list item on Shift-Tab", () => {
    editor = createListEditor();
    placeCursorInText(editor, "two");
    editor.commands.keyboardShortcut("Tab");
    placeCursorInText(editor, "two");

    expect(editor.commands.keyboardShortcut("Shift-Tab")).toBe(true);

    expect(nestedListItemTexts(editor)).not.toContain("two");
  });
});
