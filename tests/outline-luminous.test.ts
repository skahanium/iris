import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import {
  captionCoordsFromTrack,
  getTickTop,
  nearestIndexFromPointer,
  stepScrubIndex,
  tickTopPx,
  wheelScrubIndex,
} from "@/lib/outline-luminous";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("outline luminous", () => {
  it("maps indices to percentage positions along the rail", () => {
    expect(getTickTop(0, 5)).toBe(0);
    expect(getTickTop(4, 5)).toBe(100);
    expect(getTickTop(0, 1)).toBe(50);
  });

  it("hit-tests pointer position to nearest heading index", () => {
    expect(nearestIndexFromPointer(0, 400, 10)).toBe(0);
    expect(nearestIndexFromPointer(400, 400, 10)).toBe(9);
    expect(nearestIndexFromPointer(200, 400, 10)).toBe(5);
  });

  it("maps rail indices to pixel offsets and viewport caption coords", () => {
    expect(tickTopPx(0, 10, 400)).toBe(0);
    expect(tickTopPx(9, 10, 400)).toBe(400);
    expect(tickTopPx(5, 10, 400)).toBeCloseTo(222.22, 1);
    expect(
      captionCoordsFromTrack({ top: 100, right: 24, height: 400 }, 50),
    ).toEqual({ top: 300, left: 30 });
  });

  it("scrubs indices with wheel and keyboard", () => {
    expect(wheelScrubIndex(120, 3, 10)).toBe(4);
    expect(wheelScrubIndex(-120, 3, 10)).toBe(2);
    expect(wheelScrubIndex(120, 9, 10)).toBe(9);
    expect(stepScrubIndex(2, 10, 1)).toBe(3);
    expect(stepScrubIndex(0, 10, -1)).toBe(0);
  });

  it("defines luminous rail tokens and no scroll list", () => {
    const css = read("src/styles/globals.css");
    expect(css).toContain("--editor-outline-rail-width: 1.75rem");
    expect(css).not.toContain(".outline-spine-list");
    expect(css).not.toContain(".outline-spine");
    expect(css).toContain(".outline-luminous-track");
    expect(css).toContain(".outline-luminous-tick--level-1");
  });

  it("implements luminous rail component contracts", () => {
    const outline = read("src/components/editor/EditorOutline.tsx");
    const caption = read("src/components/editor/OutlineLuminousCaption.tsx");
    const css = read("src/styles/globals.css");
    expect(outline).toContain("outline-luminous--active");
    expect(outline).toContain("outline-luminous-tick");
    expect(outline).toContain("OutlineLuminousCaption");
    expect(outline).toContain("--outline-tick-top");
    expect(outline).toContain("captionIndex === index");
    expect(caption).toContain("Anchored to parent tick");
    expect(caption).not.toContain("createPortal");
    expect(caption).not.toContain("topPx");
    expect(css).toContain(".outline-luminous-caption");
    expect(css).toContain("position: absolute");
    expect(css).not.toContain("bottom: 1rem");
    expect(outline).not.toContain("OutlineSpineList");
    expect(outline).not.toContain("overflow-y");
    expect(outline).not.toContain("outline-spine-list");
  });
});
