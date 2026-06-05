import { readFileSync } from "node:fs";

import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import TaskItem from "@tiptap/extension-task-item";
import TaskList from "@tiptap/extension-task-list";
import { afterEach, describe, expect, it } from "vitest";

import { ListIndentKeymapExtension } from "@/components/editor/extensions/ListIndentKeymapExtension";

function createTaskListEditor(): Editor {
  return new Editor({
    extensions: [
      StarterKit.configure({
        bulletList: false,
        orderedList: false,
      }),
      ListIndentKeymapExtension,
      TaskList,
      TaskItem.configure({ nested: true }),
    ],
    content: `<ul data-type="taskList">
      <li data-checked="false" data-type="taskItem"><label><input type="checkbox" /></label><div><p>alpha</p></div></li>
      <li data-checked="false" data-type="taskItem"><label><input type="checkbox" /></label><div><p>beta</p></div></li>
    </ul>`,
  });
}

function placeCursorInText(editor: Editor, text: string): void {
  let from: number | null = null;
  editor.state.doc.descendants((node, pos) => {
    if (node.isText && node.text === text) {
      from = pos;
    }
  });
  if (from === null) {
    throw new Error(`text not found: ${text}`);
  }
  editor.commands.setTextSelection(from + text.length);
}

function countListItems(editor: Editor): number {
  let count = 0;
  editor.state.doc.descendants((node) => {
    if (node.type.name === "listItem") {
      count += 1;
    }
  });
  return count;
}

describe("ListIndentKeymapExtension task list", () => {
  let editor: Editor | undefined;

  afterEach(() => {
    editor?.destroy();
    editor = undefined;
  });

  it("guards task lists in extension source", () => {
    const source = readFileSync(
      "src/components/editor/extensions/ListIndentKeymapExtension.ts",
      "utf8",
    );
    expect(source).toContain('isActive("taskList")');
    expect(source).toContain('sinkListItem("listItem")');
  });

  it("does not introduce ordinary listItem nodes when Tab is pressed in a task list", () => {
    editor = createTaskListEditor();
    placeCursorInText(editor, "beta");
    expect(editor.isActive("taskList")).toBe(true);
    expect(countListItems(editor)).toBe(0);

    editor.commands.keyboardShortcut("Tab");

    expect(countListItems(editor)).toBe(0);
  });
});
