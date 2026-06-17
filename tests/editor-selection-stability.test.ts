import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { createProductionEditorFromIngestedBody } from "./helpers/tiptap-serialize-harness";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("editor selection stability around headings", () => {
  it("does not hide editable empty paragraphs after headings", () => {
    const css = read("src/styles/globals.css");
    const hiddenEmptyParagraphRule =
      />\s*:is\([^)]*h1[\s\S]*?\)\s*\+\s*p\.is-empty:has\(\+\s*\*\)\s*\{[\s\S]*?(?:display:\s*none|height:\s*0|min-height:\s*0)/u;

    expect(css).not.toMatch(hiddenEmptyParagraphRule);
  });

  it("does not create contract spacer paragraphs between adjacent headings", () => {
    const editor = createProductionEditorFromIngestedBody("# One\n\n# Two");

    try {
      expect(editor.state.doc.childCount).toBe(2);
      expect(editor.state.doc.child(0).type.name).toBe("heading");
      expect(editor.state.doc.child(1).type.name).toBe("heading");
    } finally {
      editor.destroy();
    }
  });

  it("does not wire the same disk-load tick to a second imperative reload", () => {
    const workspace = read("src/components/layout/AppEditorWorkspace.tsx");

    expect(workspace).not.toContain("reloadContentTick={editorContentTick}");
  });
});
