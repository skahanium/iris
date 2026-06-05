import { describe, expect, it } from "vitest";

import {
  findTextRanges,
  findTextRangesInDoc,
  replaceAllTextRanges,
  replaceTextRange,
} from "@/lib/editor-find-replace";
import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";

describe("editor find/replace helpers", () => {
  it("finds plain text ranges case-insensitively by default", () => {
    expect(findTextRanges("Alpha beta alpha", "alpha")).toEqual([
      { from: 0, to: 5 },
      { from: 11, to: 16 },
    ]);
  });

  it("can find plain text ranges case-sensitively", () => {
    expect(
      findTextRanges("Alpha beta alpha", "alpha", { caseSensitive: true }),
    ).toEqual([{ from: 11, to: 16 }]);
  });

  it("replaces the selected range only", () => {
    expect(
      replaceTextRange("Alpha beta alpha", { from: 6, to: 10 }, "gamma"),
    ).toBe("Alpha gamma alpha");
  });

  it("replaces all ranges from back to front", () => {
    const text = "one two one";
    const ranges = findTextRanges(text, "one");
    expect(replaceAllTextRanges(text, ranges, "three")).toBe(
      "three two three",
    );
  });

  it("maps text matches to ProseMirror document positions", () => {
    const editor = new Editor({
      extensions: [StarterKit],
      content: "<p>Alpha beta alpha</p>",
    });
    try {
      expect(findTextRangesInDoc(editor.state.doc, "alpha")).toEqual([
        { from: 1, to: 6 },
        { from: 12, to: 17 },
      ]);
    } finally {
      editor.destroy();
    }
  });
});
