import { describe, expect, it } from "vitest";

import { isNoteSubstantivelyEmpty } from "@/lib/note-substance";

describe("isNoteSubstantivelyEmpty", () => {
  it("treats a blank body as empty even when a legacy title is present", () => {
    expect(isNoteSubstantivelyEmpty('---\ntitle: "Legacy"\n---\n\n')).toBe(
      true,
    );
  });

  it("keeps body text and headings substantive", () => {
    expect(isNoteSubstantivelyEmpty("# Section\n\nParagraph.")).toBe(false);
  });

  it("keeps a code-only note substantive", () => {
    expect(isNoteSubstantivelyEmpty("```ts\nconst a = 1;\n```\n")).toBe(false);
  });
});
