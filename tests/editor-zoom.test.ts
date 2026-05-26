import { describe, expect, it } from "vitest";

import {
  clampEditorZoom,
  EDITOR_ZOOM_DEFAULT,
  EDITOR_ZOOM_MAX,
  EDITOR_ZOOM_MIN,
  formatEditorZoomPercent,
  stepEditorZoom,
} from "@/lib/editor-zoom";

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
});
