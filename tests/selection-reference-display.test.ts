import { describe, expect, it } from "vitest";

import { selectionReferenceDisplayFromExplicitReferences } from "@/lib/selection-reference-display";

describe("selection reference history projection", () => {
  it("projects only a safe filename marker from persisted references", () => {
    expect(
      selectionReferenceDisplayFromExplicitReferences([
        {
          kind: "selection",
          filePath: "党纪国法/党的十八大.md",
          utf8Range: { start: 0, end: 12 },
          excerpt: "不应进入历史记录的正文",
        },
      ]),
    ).toEqual({ fileName: "党的十八大.md" });
  });

  it("ignores note, malformed and classified-style references", () => {
    expect(
      selectionReferenceDisplayFromExplicitReferences([
        { kind: "note", filePath: "notes/all.md" },
        { kind: "selection", filePath: "" },
        { kind: "selection", filePath: 42 },
      ]),
    ).toBeNull();
  });

  it("continues past malformed selection records to a valid marker", () => {
    expect(
      selectionReferenceDisplayFromExplicitReferences([
        { kind: "selection", filePath: "   " },
        { kind: "selection", filePath: "notes/valid.md" },
      ]),
    ).toEqual({ fileName: "valid.md" });
  });

  it("treats malformed historical payloads as no selection marker", () => {
    for (const references of [null, {}, "not-an-array", 42]) {
      expect(
        selectionReferenceDisplayFromExplicitReferences(references),
      ).toBeNull();
    }
  });
});
