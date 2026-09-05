import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

const tipTapEditor = readFileSync(
  "src/components/editor/TipTapEditor.tsx",
  "utf8",
);
const editorRoundtrip = readFileSync("src/lib/editor-roundtrip.ts", "utf8");

const GUARD_EXTENSIONS = [
  "ImeCompositionGuardExtension",
  "HeadingDomGuardExtension",
  "EmptyHeadingImeGuardExtension",
] as const;

describe("editor heading IME guard wiring contract", () => {
  it("keeps all heading IME guards in the real TipTapEditor", () => {
    for (const extension of GUARD_EXTENSIONS) {
      expect(tipTapEditor).toContain(`import { ${extension} }`);
      expect(tipTapEditor).toContain(extension);
    }
  });

  it("keeps all heading IME guards in the production round-trip extension factory", () => {
    for (const extension of GUARD_EXTENSIONS) {
      expect(editorRoundtrip).toContain(`import { ${extension} }`);
      expect(editorRoundtrip).toContain(extension);
    }
  });

  it("lists the three guards together in TipTapEditor's extension array", () => {
    const arraySlice = tipTapEditor.slice(
      tipTapEditor.lastIndexOf("ImeCompositionGuardExtension"),
      tipTapEditor.lastIndexOf("IrisParagraphExtension"),
    );
    expect(arraySlice).toContain("HeadingDomGuardExtension");
    expect(arraySlice).toContain("EmptyHeadingImeGuardExtension");
  });
});
