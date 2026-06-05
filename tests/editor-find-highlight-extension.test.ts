import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { afterEach, describe, expect, it } from "vitest";

import {
  FindHighlightExtension,
  findHighlightPluginKey,
  setFindHighlightState,
} from "@/components/editor/extensions/FindHighlightExtension";

describe("FindHighlightExtension", () => {
  let editor: Editor | undefined;

  afterEach(() => {
    editor?.destroy();
    editor = undefined;
  });

  it("stores matches and decorations for the current query", () => {
    editor = new Editor({
      extensions: [StarterKit, FindHighlightExtension],
      content: "<p>Alpha beta alpha</p>",
    });

    setFindHighlightState(editor, {
      query: "alpha",
      caseSensitive: false,
      currentIndex: 1,
    });

    const state = findHighlightPluginKey.getState(editor.state);
    expect(state?.ranges).toEqual([
      { from: 1, to: 6 },
      { from: 12, to: 17 },
    ]);
    expect(state?.currentIndex).toBe(1);
    expect(state?.decorations.find()).toHaveLength(2);
  });
});
