import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { afterEach, describe, expect, it } from "vitest";

import { ListIndentKeymapExtension } from "@/components/editor/extensions/ListIndentKeymapExtension";
import {
  hasNestedListItem,
  hasTopLevelListItem,
} from "./helpers/nested-list-doc";
import {
  createProductionEditorFromIngestedBody,
  normalizeMd,
  pmSerializeBody,
} from "./helpers/tiptap-serialize-harness";

function createBulletListEditor(): Editor {
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

function assertStableMarkdownRoundTrip(sourceMd: string): void {
  const first = createProductionEditorFromIngestedBody(sourceMd);
  try {
    const serialized = normalizeMd(pmSerializeBody(first));
    const second = createProductionEditorFromIngestedBody(serialized);
    try {
      expect(normalizeMd(pmSerializeBody(second))).toBe(serialized);
    } finally {
      second.destroy();
    }
  } finally {
    first.destroy();
  }
}

describe("nested list markdown round-trip", () => {
  let editor: Editor | undefined;

  afterEach(() => {
    editor?.destroy();
    editor = undefined;
  });

  it("Tab-indented bullet list round-trips with nested structure preserved", () => {
    editor = createBulletListEditor();
    placeCursorInText(editor, "two");
    expect(editor.commands.keyboardShortcut("Tab")).toBe(true);
    expect(hasNestedListItem(editor, "two")).toBe(true);

    const serialized = normalizeMd(pmSerializeBody(editor));
    const roundTrip = createProductionEditorFromIngestedBody(serialized);
    try {
      expect(hasNestedListItem(roundTrip, "two")).toBe(true);
      expect(hasTopLevelListItem(roundTrip, "one")).toBe(true);
      expect(hasTopLevelListItem(roundTrip, "three")).toBe(true);
      assertStableMarkdownRoundTrip(serialized);
    } finally {
      roundTrip.destroy();
    }
  });

  it("round-trips ingested nested bullet markdown through production extensions", () => {
    assertStableMarkdownRoundTrip("- one\n  - two\n- three");
  });

  it("round-trips ingested nested ordered markdown through production extensions", () => {
    assertStableMarkdownRoundTrip("1. one\n   2. two\n3. three");
  });

  it("Tab-indented ordered list round-trips with nested structure preserved", () => {
    editor = createOrderedListEditor();
    placeCursorInText(editor, "two");
    expect(editor.commands.keyboardShortcut("Tab")).toBe(true);
    expect(hasNestedListItem(editor, "two")).toBe(true);

    const serialized = normalizeMd(pmSerializeBody(editor));
    const roundTrip = createProductionEditorFromIngestedBody(serialized);
    try {
      expect(hasNestedListItem(roundTrip, "two")).toBe(true);
      assertStableMarkdownRoundTrip(serialized);
    } finally {
      roundTrip.destroy();
    }
  });
});
