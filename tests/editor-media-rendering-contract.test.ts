import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("editor media rendering contract", () => {
  it("renders editor images with a stable opaque media surface for animated assets", () => {
    const extension = read(
      "src/components/editor/extensions/ImageExtension.ts",
    );
    const css = read("src/styles/globals.css");

    expect(extension).toContain('"iris-editor-media-image"');
    expect(css).toContain(".iris-editor-media-image");
    expect(css).toContain("object-fit: contain");
    expect(css).toContain("background: hsl(var(--background))");
    expect(css).toContain("contain: paint");
    expect(css).toContain("backface-visibility: hidden");
    expect(css).toContain(
      "aspect-ratio: var(--iris-media-aspect-ratio, 16 / 9)",
    );
    expect(css).toContain("min-height: min(40vh, 22rem)");
  });
});
