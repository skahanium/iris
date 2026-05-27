import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

const globalsCss = readFileSync("src/styles/globals.css", "utf8");
const tipTapSource = readFileSync(
  "src/components/editor/TipTapEditor.tsx",
  "utf8",
);

describe("Notion editor layout", () => {
  it("uses flat canvas classes without paper card or line grid", () => {
    expect(globalsCss).toContain(".iris-editor-canvas");
    expect(globalsCss).toContain(".iris-editor-body");
    expect(globalsCss).not.toContain(".iris-paper {");
    expect(globalsCss).not.toContain("repeating-linear-gradient");
    expect(globalsCss).not.toContain("text-indent: 2em");
  });

  it("TipTapEditor mounts canvas structure", () => {
    expect(tipTapSource).toContain("iris-editor-canvas");
    expect(tipTapSource).toContain("iris-editor-body");
    expect(tipTapSource).not.toContain("iris-paper");
    expect(tipTapSource).not.toContain("measureBodyLinesStart");
  });
});
