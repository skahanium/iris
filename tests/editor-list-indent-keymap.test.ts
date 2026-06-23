import { Editor } from "@tiptap/core";
import TaskItem from "@tiptap/extension-task-item";
import TaskList from "@tiptap/extension-task-list";
import StarterKit from "@tiptap/starter-kit";
import { afterEach, describe, expect, it } from "vitest";

import { ListIndentKeymapExtension } from "@/components/editor/extensions/ListIndentKeymapExtension";
import {
  createProductionEditorFromIngestedBody,
  pmSerializeBody,
} from "./helpers/tiptap-serialize-harness";

function createListEditor(): Editor {
  return new Editor({
    extensions: [StarterKit, ListIndentKeymapExtension],
    content:
      "<ul><li><p>one</p></li><li><p>two</p></li><li><p>three</p></li></ul>",
  });
}

function createOrderedListEditor(): Editor {
  return new Editor({
    extensions: [StarterKit, ListIndentKeymapExtension],
    content:
      "<ol><li><p>one</p></li><li><p>two</p></li><li><p>three</p></li></ol>",
  });
}

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
    content:
      '<ul data-type="taskList"><li data-type="taskItem" data-checked="false"><label><input type="checkbox"><span></span></label><div><p>one</p></div></li><li data-type="taskItem" data-checked="false"><label><input type="checkbox"><span></span></label><div><p>two</p></div></li></ul>',
  });
}

function createParagraphEditor(): Editor {
  return new Editor({
    extensions: [StarterKit, ListIndentKeymapExtension],
    content: "<p>plain paragraph</p>",
  });
}

function placeCursorInText(editor: Editor, text: string): void {
  let from: number | null = null;
  editor.state.doc.descendants((node, pos) => {
    const offset = node.isText ? node.text?.indexOf(text) : -1;
    if (offset != null && offset >= 0) {
      from = pos + offset;
      return false;
    }
  });
  if (from === null) {
    throw new Error(`text not found: ${text}`);
  }
  editor.commands.setTextSelection(from + text.length);
}

function placeCursorAfterSubstring(editor: Editor, text: string): void {
  let from: number | null = null;
  editor.state.doc.descendants((node, pos) => {
    const offset = node.isText ? node.text?.indexOf(text) : -1;
    if (offset != null && offset >= 0) {
      from = pos + offset + text.length;
      return false;
    }
  });
  if (from === null) {
    throw new Error(`text not found: ${text}`);
  }
  editor.commands.setTextSelection(from);
}

function textRange(editor: Editor, text: string): { from: number; to: number } {
  let range: { from: number; to: number } | null = null;
  editor.state.doc.descendants((node, pos) => {
    const offset = node.isText ? node.text?.indexOf(text) : -1;
    if (offset != null && offset >= 0) {
      const from = pos + offset;
      range = { from, to: from + text.length };
      return false;
    }
  });
  if (range === null) {
    throw new Error(`text not found: ${text}`);
  }
  return range;
}

function expectCursorAfterText(editor: Editor, text: string): void {
  expect(editor.state.selection.from).toBe(textRange(editor, text).to);
}

function pressTab(editor: Editor, shiftKey = false): KeyboardEvent {
  editor.view.focus();
  const event = new KeyboardEvent("keydown", {
    key: "Tab",
    code: "Tab",
    bubbles: true,
    cancelable: true,
    shiftKey,
  });
  editor.view.dom.dispatchEvent(event);
  return event;
}

function pressEnter(editor: Editor): KeyboardEvent {
  editor.view.focus();
  const event = new KeyboardEvent("keydown", {
    key: "Enter",
    code: "Enter",
    keyCode: 13,
    bubbles: true,
    cancelable: true,
  });
  editor.view.dom.dispatchEvent(event);
  return event;
}

function pressBackspace(editor: Editor): KeyboardEvent {
  editor.view.focus();
  const event = new KeyboardEvent("keydown", {
    key: "Backspace",
    code: "Backspace",
    keyCode: 8,
    bubbles: true,
    cancelable: true,
  });
  editor.view.dom.dispatchEvent(event);
  return event;
}

function fireCompositionStart(dom: HTMLElement): void {
  dom.dispatchEvent(
    new CompositionEvent("compositionstart", {
      data: "",
      bubbles: true,
      cancelable: true,
    }),
  );
}

function fireCompositionEnd(dom: HTMLElement, data: string): void {
  dom.dispatchEvent(
    new CompositionEvent("compositionend", {
      data,
      bubbles: true,
      cancelable: true,
    }),
  );
}

function typeTextThroughInputRules(editor: Editor, text: string): void {
  for (const ch of text) {
    const { from, to } = editor.state.selection;
    let handled = false;
    editor.view.someProp("handleTextInput", (handler) => {
      if (handler(editor.view, from, to, ch, () => editor.state.tr)) {
        handled = true;
        return true;
      }
      return false;
    });
    if (!handled) {
      editor.commands.insertContent(ch);
    }
  }
}

function nestedListItemTexts(editor: Editor): string[] {
  const nested: string[] = [];
  function visit(
    node: ReturnType<Editor["getJSON"]>,
    parentType: string | null,
    grandParentType: string | null,
  ): void {
    if (
      node.type === "listItem" &&
      (parentType === "bulletList" || parentType === "orderedList") &&
      grandParentType === "listItem"
    ) {
      nested.push(textContent(node));
    }
    node.content?.forEach((child) =>
      visit(child, node.type ?? null, parentType),
    );
  }
  visit(editor.getJSON(), null, null);
  return nested;
}

function textContent(node: ReturnType<Editor["getJSON"]>): string {
  return [
    node.text ?? "",
    ...(node.content?.map((child) => textContent(child)) ?? []),
  ].join("");
}

function firstNodeAttrs(
  editor: Editor,
  typeName: string,
): Record<string, unknown> {
  let attrs: Record<string, unknown> | null = null;
  editor.state.doc.descendants((node) => {
    if (node.type.name === typeName) {
      attrs = node.attrs as Record<string, unknown>;
      return false;
    }
  });
  if (attrs === null) {
    throw new Error(`node not found: ${typeName}`);
  }
  return attrs;
}

function topLevelAttrs(
  editor: Editor,
  typeName: string,
): Array<Record<string, unknown>> {
  const attrs: Array<Record<string, unknown>> = [];
  editor.state.doc.forEach((node) => {
    if (node.type.name === typeName) {
      attrs.push(node.attrs as Record<string, unknown>);
    }
  });
  return attrs;
}

function placeCursorInEmptyListItem(editor: Editor): void {
  let target: number | null = null;
  editor.state.doc.descendants((node, pos) => {
    if (node.type.name === "paragraph" && node.textContent === "") {
      target = pos + 1;
      return false;
    }
  });
  if (target == null) {
    throw new Error("empty list item paragraph not found");
  }
  editor.commands.setTextSelection(target);
}

function replaceSelectedParagraphDomText(editor: Editor, text: string): void {
  const { $from } = editor.state.selection;
  let paragraphPos: number | null = null;
  for (let depth = $from.depth; depth > 0; depth--) {
    if ($from.node(depth).type.name === "paragraph") {
      paragraphPos = $from.before(depth);
      break;
    }
  }
  if (paragraphPos == null) {
    throw new Error("selected paragraph not found");
  }

  const paragraphDom = editor.view.nodeDOM(paragraphPos) as HTMLElement | null;
  if (!paragraphDom) {
    throw new Error("selected paragraph DOM not found");
  }
  paragraphDom.textContent = text;
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

  it("handles a real Tab keydown by indenting the current list item", () => {
    editor = createListEditor();
    placeCursorInText(editor, "two");

    const event = pressTab(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(nestedListItemTexts(editor)).toContain("two");
  });

  it("handles a real Tab keydown in the middle of an ordered list item", () => {
    editor = createOrderedListEditor();
    placeCursorAfterSubstring(editor, "tw");

    const event = pressTab(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.getText()).toContain("two");
    expect(nestedListItemTexts(editor)).toContain("two");
  });

  it("keeps unordered list item text when indenting the second completed item", () => {
    editor = createListEditor();
    placeCursorInText(editor, "two");

    expect(pressTab(editor).defaultPrevented).toBe(true);

    expect(editor.getText()).toContain("one");
    expect(editor.getText()).toContain("two");
    expect(editor.getText()).toContain("three");
    expect(nestedListItemTexts(editor)).toContain("two");
  });

  it("keeps ordered list item text when indenting the second completed item", () => {
    editor = createOrderedListEditor();
    placeCursorInText(editor, "two");

    expect(pressTab(editor).defaultPrevented).toBe(true);

    expect(editor.getText()).toContain("one");
    expect(editor.getText()).toContain("two");
    expect(editor.getText()).toContain("three");
    expect(nestedListItemTexts(editor)).toContain("two");
  });

  it("keeps unordered first item text when pressing Enter at the end", () => {
    editor = createProductionEditorFromIngestedBody("- one");
    placeCursorInText(editor, "one");

    const event = pressEnter(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.getText()).toContain("one");
    expect(editor.getJSON()).toMatchObject({
      content: [
        {
          type: "bulletList",
          content: [
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "one" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [{ type: "paragraph" }],
            },
          ],
        },
      ],
    });
  });

  it("keeps ordered first item text when pressing Enter at the end", () => {
    editor = createProductionEditorFromIngestedBody("1. one");
    placeCursorInText(editor, "one");

    const event = pressEnter(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.getText()).toContain("one");
    expect(editor.getJSON()).toMatchObject({
      content: [
        {
          type: "orderedList",
          content: [
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "one" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [{ type: "paragraph" }],
            },
          ],
        },
      ],
    });
  });

  it("keeps unordered first item text after input-rule creation then Enter", () => {
    editor = createProductionEditorFromIngestedBody("");
    typeTextThroughInputRules(editor, "- one");

    const event = pressEnter(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.getText()).toContain("one");
    expect(editor.getJSON()).toMatchObject({
      content: [
        {
          type: "bulletList",
          content: [
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "one" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [{ type: "paragraph" }],
            },
          ],
        },
      ],
    });
  });

  it("keeps ordered first item text after input-rule creation then Enter", () => {
    editor = createProductionEditorFromIngestedBody("");
    typeTextThroughInputRules(editor, "1. one");

    const event = pressEnter(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.getText()).toContain("one");
    expect(editor.getJSON()).toMatchObject({
      content: [
        {
          type: "orderedList",
          content: [
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "one" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [{ type: "paragraph" }],
            },
          ],
        },
      ],
    });
  });

  it("keeps ordered second item text after typing then pressing Enter", () => {
    editor = createProductionEditorFromIngestedBody("");
    typeTextThroughInputRules(editor, "1. one");
    expect(pressEnter(editor).defaultPrevented).toBe(true);
    typeTextThroughInputRules(editor, "two");

    const event = pressEnter(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.getText()).toContain("one");
    expect(editor.getText()).toContain("two");
    expect(editor.getJSON()).toMatchObject({
      content: [
        {
          type: "orderedList",
          content: [
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "one" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "two" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [{ type: "paragraph" }],
            },
          ],
        },
      ],
    });
  });

  it("keeps unordered second item text after typing then pressing Enter", () => {
    editor = createProductionEditorFromIngestedBody("");
    typeTextThroughInputRules(editor, "- one");
    expect(pressEnter(editor).defaultPrevented).toBe(true);
    typeTextThroughInputRules(editor, "two");

    const event = pressEnter(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.getText()).toContain("one");
    expect(editor.getText()).toContain("two");
    expect(editor.getJSON()).toMatchObject({
      content: [
        {
          type: "bulletList",
          content: [
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "one" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "two" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [{ type: "paragraph" }],
            },
          ],
        },
      ],
    });
  });

  it("keeps DOM-composed unordered second item text when Enter follows compositionend", () => {
    editor = createProductionEditorFromIngestedBody("");
    typeTextThroughInputRules(editor, "- one");
    expect(pressEnter(editor).defaultPrevented).toBe(true);
    placeCursorInEmptyListItem(editor);

    fireCompositionStart(editor.view.dom);
    replaceSelectedParagraphDomText(editor, "two");
    fireCompositionEnd(editor.view.dom, "two");
    const event = pressEnter(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.getText()).toContain("one");
    expect(editor.getText()).toContain("two");
    expect(editor.getJSON()).toMatchObject({
      content: [
        {
          type: "bulletList",
          content: [
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "one" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "two" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [{ type: "paragraph" }],
            },
          ],
        },
      ],
    });
  });

  it("keeps DOM-composed ordered second item text when Enter follows compositionend", () => {
    editor = createProductionEditorFromIngestedBody("");
    typeTextThroughInputRules(editor, "1. one");
    expect(pressEnter(editor).defaultPrevented).toBe(true);
    placeCursorInEmptyListItem(editor);

    fireCompositionStart(editor.view.dom);
    replaceSelectedParagraphDomText(editor, "two");
    fireCompositionEnd(editor.view.dom, "two");
    const event = pressEnter(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.getText()).toContain("one");
    expect(editor.getText()).toContain("two");
    expect(editor.getJSON()).toMatchObject({
      content: [
        {
          type: "orderedList",
          content: [
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "one" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "two" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [{ type: "paragraph" }],
            },
          ],
        },
      ],
    });
  });

  it("keeps nested list content when pressing Enter at the end of a non-empty child item", () => {
    editor = createOrderedListEditor();
    placeCursorInText(editor, "two");
    pressTab(editor);
    placeCursorInText(editor, "two");

    const event = pressEnter(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.getText()).toContain("two");
    expect(nestedListItemTexts(editor)).toContain("two");
  });

  it("keeps unordered nested item content after real Tab then real Enter", () => {
    editor = createListEditor();
    placeCursorInText(editor, "two");

    expect(pressTab(editor).defaultPrevented).toBe(true);
    placeCursorInText(editor, "two");
    expect(pressEnter(editor).defaultPrevented).toBe(true);

    expect(editor.getText()).toContain("one");
    expect(editor.getText()).toContain("two");
    expect(editor.getText()).toContain("three");
    expect(nestedListItemTexts(editor)).toContain("two");
  });

  it("keeps production-ingested nested list content after real Tab then real Enter", () => {
    editor = createProductionEditorFromIngestedBody("- one\n- two\n- three");
    placeCursorInText(editor, "two");

    expect(pressTab(editor).defaultPrevented).toBe(true);
    placeCursorInText(editor, "two");
    expect(pressEnter(editor).defaultPrevented).toBe(true);

    expect(editor.getText()).toContain("one");
    expect(editor.getText()).toContain("two");
    expect(editor.getText()).toContain("three");
    expect(nestedListItemTexts(editor)).toContain("two");
  });

  it("handles a real Tab keydown after production Markdown ingest", () => {
    editor = createProductionEditorFromIngestedBody("- one\n- two\n- three");
    placeCursorInText(editor, "two");

    expect(editor.isActive("listItem")).toBe(true);

    const event = pressTab(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(nestedListItemTexts(editor)).toContain("two");
  });

  it("can sink a production-ingested list item with the listItem command", () => {
    editor = createProductionEditorFromIngestedBody("- one\n- two\n- three");
    placeCursorInText(editor, "two");

    expect(editor.isActive("listItem")).toBe(true);
    expect(editor.commands.sinkListItem("listItem")).toBe(true);
    expect(nestedListItemTexts(editor)).toContain("two");
  });

  it("handles task list indentation without falling through to browser Tab", () => {
    editor = createTaskListEditor();
    placeCursorInText(editor, "two");

    const beforeSelection = editor.state.selection.from;

    expect(editor.commands.keyboardShortcut("Tab")).toBe(true);
    expect(editor.state.selection.from).toBe(beforeSelection);
    expect(editor.getJSON()).toMatchObject({
      content: [
        {
          type: "taskList",
          content: [
            {
              type: "taskItem",
              content: expect.arrayContaining([
                expect.objectContaining({
                  type: "taskList",
                  content: expect.arrayContaining([
                    expect.objectContaining({ type: "taskItem" }),
                  ]),
                }),
              ]),
            },
          ],
        },
      ],
    });
  });

  it("exits an empty trailing unordered list item on Backspace", () => {
    editor = createProductionEditorFromIngestedBody("- AutoClaw\n- ZCode\n- ");
    placeCursorInEmptyListItem(editor);

    const event = pressBackspace(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.getJSON()).toMatchObject({
      content: [
        {
          type: "bulletList",
          content: [
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "AutoClaw" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "ZCode" }],
                },
              ],
            },
          ],
        },
        {
          type: "paragraph",
        },
      ],
    });
  });

  it("does not outdent the previous unordered item after empty-item Backspace", () => {
    editor = createProductionEditorFromIngestedBody("- AutoClaw\n- ZCode\n- ");
    placeCursorInEmptyListItem(editor);
    pressBackspace(editor);

    expect(editor.commands.keyboardShortcut("Shift-Tab")).toBe(true);

    expect(editor.getJSON()).toMatchObject({
      content: [
        {
          type: "bulletList",
          content: [
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "AutoClaw" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "ZCode" }],
                },
              ],
            },
          ],
        },
        {
          type: "paragraph",
        },
      ],
    });
  });

  it("does not recreate an empty unordered list item on a second Backspace", () => {
    editor = createProductionEditorFromIngestedBody("- AutoClaw\n- ZCode\n- ");
    placeCursorInEmptyListItem(editor);

    pressBackspace(editor);
    const event = pressBackspace(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.getJSON()).toMatchObject({
      content: [
        {
          type: "bulletList",
          content: [
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "AutoClaw" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "ZCode" }],
                },
              ],
            },
          ],
        },
      ],
    });
    expect(editor.getJSON().content).toHaveLength(1);
    expectCursorAfterText(editor, "ZCode");
  });

  it("exits an empty trailing ordered list item on Backspace", () => {
    editor = createProductionEditorFromIngestedBody(
      "1. AutoClaw\n2. ZCode\n3. ",
    );
    placeCursorInEmptyListItem(editor);

    const event = pressBackspace(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.getJSON()).toMatchObject({
      content: [
        {
          type: "orderedList",
          content: [
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "AutoClaw" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "ZCode" }],
                },
              ],
            },
          ],
        },
        {
          type: "paragraph",
        },
      ],
    });
  });

  it("does not recreate an empty ordered list item on a second Backspace", () => {
    editor = createProductionEditorFromIngestedBody(
      "1. AutoClaw\n2. ZCode\n3. ",
    );
    placeCursorInEmptyListItem(editor);

    pressBackspace(editor);
    const event = pressBackspace(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.getJSON()).toMatchObject({
      content: [
        {
          type: "orderedList",
          content: [
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "AutoClaw" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "ZCode" }],
                },
              ],
            },
          ],
        },
      ],
    });
    expect(editor.getJSON().content).toHaveLength(1);
    expectCursorAfterText(editor, "ZCode");
  });

  it("keeps default Backspace behavior for empty paragraphs inside non-empty list items", () => {
    editor = new Editor({
      extensions: [StarterKit, ListIndentKeymapExtension],
      content: "<ul><li><p>AutoClaw</p></li><li><p>ZCode</p><p></p></li></ul>",
    });
    placeCursorInEmptyListItem(editor);

    const event = pressBackspace(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.getJSON()).toMatchObject({
      content: [
        {
          type: "bulletList",
          content: [
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "AutoClaw" }],
                },
              ],
            },
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "ZCode" }],
                },
              ],
            },
          ],
        },
      ],
    });
  });

  it("indents a non-list paragraph on Tab without losing caret position", () => {
    editor = createParagraphEditor();
    placeCursorInText(editor, "plain paragraph");

    const beforeSelection = editor.state.selection.from;

    expect(editor.commands.keyboardShortcut("Tab")).toBe(true);
    expect(editor.state.selection.from).toBe(beforeSelection);
    expect(editor.getText()).toBe("plain paragraph");
    expect(firstNodeAttrs(editor, "paragraph")).toMatchObject({
      irisIndent: 1,
    });
    expect(pmSerializeBody(editor)).toContain(
      '<p data-iris-indent="1">plain paragraph</p>',
    );
  });

  it("handles a real Tab keydown by indenting a non-list paragraph", () => {
    editor = createParagraphEditor();
    placeCursorInText(editor, "plain paragraph");

    const event = pressTab(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.getText()).toBe("plain paragraph");
    expect(firstNodeAttrs(editor, "paragraph")).toMatchObject({
      irisIndent: 1,
    });
  });

  it("indents a paragraph from the middle without deleting completed text", () => {
    editor = createProductionEditorFromIngestedBody(
      "\u5df2\u7ecf\u5199\u5b8c\u7684\u6bb5\u843d\u4e0d\u5e94\u8be5\u4e22\u5931",
    );
    placeCursorAfterSubstring(editor, "\u5df2\u7ecf\u5199\u5b8c");

    const beforeText = editor.getText();
    const beforeSelection = editor.state.selection.from;

    const event = pressTab(editor);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.getText()).toBe(beforeText);
    expect(editor.state.selection.from).toBe(beforeSelection);
    expect(firstNodeAttrs(editor, "paragraph")).toMatchObject({
      irisIndent: 1,
    });
  });

  it("preserves non-list paragraph indentation after Markdown reingest", () => {
    editor = createParagraphEditor();
    placeCursorInText(editor, "plain paragraph");
    editor.commands.keyboardShortcut("Tab");
    const md = pmSerializeBody(editor);
    editor.destroy();

    editor = createProductionEditorFromIngestedBody(md);

    expect(editor.getText()).toBe("plain paragraph");
    expect(firstNodeAttrs(editor, "paragraph")).toMatchObject({
      irisIndent: 1,
    });
  });

  it("outdents a non-list paragraph on Shift-Tab", () => {
    editor = createParagraphEditor();
    placeCursorInText(editor, "plain paragraph");
    editor.commands.keyboardShortcut("Tab");
    placeCursorInText(editor, "plain paragraph");

    expect(editor.commands.keyboardShortcut("Shift-Tab")).toBe(true);
    expect(editor.getText()).toBe("plain paragraph");
    expect(firstNodeAttrs(editor, "paragraph")).toMatchObject({
      irisIndent: 0,
    });
  });

  it("indents and outdents headings as blocks", () => {
    editor = createProductionEditorFromIngestedBody("## Heading");
    placeCursorInText(editor, "Heading");

    expect(editor.commands.keyboardShortcut("Tab")).toBe(true);
    expect(firstNodeAttrs(editor, "heading")).toMatchObject({
      irisIndent: 1,
    });
    expect(pmSerializeBody(editor)).toContain(
      '<h2 data-iris-indent="1">Heading</h2>',
    );

    expect(editor.commands.keyboardShortcut("Shift-Tab")).toBe(true);
    expect(firstNodeAttrs(editor, "heading")).toMatchObject({
      irisIndent: 0,
    });
  });

  it("indents multiple selected text blocks together", () => {
    editor = createProductionEditorFromIngestedBody("Alpha\n\nBeta");
    const alpha = textRange(editor, "Alpha");
    const beta = textRange(editor, "Beta");
    editor.commands.setTextSelection({ from: alpha.from, to: beta.to });

    expect(editor.commands.keyboardShortcut("Tab")).toBe(true);

    expect(topLevelAttrs(editor, "paragraph")).toEqual([
      expect.objectContaining({ irisIndent: 1 }),
      expect.objectContaining({ irisIndent: 1 }),
    ]);
    expect(editor.getText()).toContain("Alpha");
    expect(editor.getText()).toContain("Beta");
    expect(editor.getText()).not.toContain("\u3000");
  });

  it("converts a numbered paragraph to a real ordered list on Tab", () => {
    editor = createProductionEditorFromIngestedBody("1.\u7b2c\u4e00\u6bb5");
    placeCursorInText(editor, "\u7b2c\u4e00\u6bb5");

    expect(editor.commands.keyboardShortcut("Tab")).toBe(true);

    expect(editor.getJSON()).toMatchObject({
      content: [
        {
          type: "orderedList",
          content: [
            {
              type: "listItem",
              content: [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "\u7b2c\u4e00\u6bb5" }],
                },
              ],
            },
          ],
        },
      ],
    });
    expect(pmSerializeBody(editor).trim()).toBe("1. \u7b2c\u4e00\u6bb5");
  });

  it("preserves inline content when converting a completed numbered paragraph", () => {
    editor = createProductionEditorFromIngestedBody(
      "1.\u5f00\u5934 **\u91cd\u70b9** [\u94fe\u63a5](https://example.com) \u7ed3\u5c3e",
    );
    placeCursorAfterSubstring(editor, "\u91cd\u70b9");

    expect(editor.commands.keyboardShortcut("Tab")).toBe(true);

    const md = pmSerializeBody(editor);
    expect(md).toContain("1. \u5f00\u5934");
    expect(md).toContain("**\u91cd\u70b9**");
    expect(md).toContain("[\u94fe\u63a5](https://example.com)");
    expect(md).toContain("\u7ed3\u5c3e");
  });

  it("converts adjacent numbered paragraphs before nesting the current item", () => {
    editor = createProductionEditorFromIngestedBody(
      "1\u3001\u7b2c\u4e00\u6bb5\n\n2\u3001\u7b2c\u4e8c\u6bb5",
    );
    placeCursorInText(editor, "\u7b2c\u4e8c\u6bb5");

    expect(editor.commands.keyboardShortcut("Tab")).toBe(true);

    expect(nestedListItemTexts(editor)).toContain("\u7b2c\u4e8c\u6bb5");
    expect(pmSerializeBody(editor)).toContain("1. \u7b2c\u4e00\u6bb5");
    expect(pmSerializeBody(editor)).toContain("1. \u7b2c\u4e8c\u6bb5");
  });
});
