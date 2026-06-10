import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import {
  clampEditorZoom,
  EDITOR_ZOOM_DEFAULT,
  EDITOR_ZOOM_MAX,
  EDITOR_ZOOM_MIN,
  formatEditorZoomPercent,
  stepEditorZoom,
} from "@/lib/editor-zoom";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("editor-zoom", () => {
  it("clamps zoom to allowed range", () => {
    expect(clampEditorZoom(0.5)).toBe(EDITOR_ZOOM_MIN);
    expect(clampEditorZoom(2)).toBe(EDITOR_ZOOM_MAX);
    expect(clampEditorZoom(1.1)).toBe(1.1);
  });

  it("steps zoom in and out", () => {
    expect(stepEditorZoom(1, 1)).toBe(1.1);
    expect(stepEditorZoom(1.1, -1)).toBe(1);
  });

  it("formats percent label", () => {
    expect(formatEditorZoomPercent(EDITOR_ZOOM_DEFAULT)).toBe("100%");
    expect(formatEditorZoomPercent(1.25)).toBe("125%");
  });

  it("drives editor typography through a CSS zoom variable", () => {
    const editor = read("src/components/editor/TipTapEditor.tsx");
    const css = read("src/styles/globals.css");
    const prose = read("src/styles/markdown-prose.css");

    expect(editor).toContain("--editor-zoom");
    expect(editor).not.toContain("fontSize:");
    expect(css).toContain("font-size: calc(2.25rem * var(--editor-zoom))");
    expect(prose).toContain(
      "font-size: calc(var(--prose-size-editor) * var(--editor-zoom))",
    );
    expect(prose).toContain("font-size: calc(1.875rem * var(--editor-zoom))");
  });
});
