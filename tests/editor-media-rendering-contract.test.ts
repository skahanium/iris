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
    const wikiExtension = read(
      "src/components/editor/extensions/WikiMediaEmbedExtension.ts",
    );
    const css = read("src/styles/globals.css");
    const proseCss = read("src/styles/markdown-prose.css");

    expect(extension).toContain('"iris-editor-media-frame"');
    expect(extension).toContain('"iris-editor-media-image"');
    expect(wikiExtension).toContain("iris-editor-media-frame");
    expect(css).toContain(".iris-editor-media-frame");
    expect(css).toContain(".iris-editor-media-image");
    expect(css).toContain("background: hsl(var(--background))");
    expect(css).toContain(".iris-editor-media-frame[data-media-error");
    expect(css).toContain("object-fit: contain");

    // Loaded images follow natural aspect ratio; only placeholder states lock 16:9.
    expect(css).toContain("aspect-ratio: unset");
    expect(css).toContain("overflow: visible");
    expect(css).toMatch(
      /\[data-media-state="pending"\][\s\S]*?aspect-ratio: var\(--iris-media-aspect-ratio, 16 \/ 9\)/,
    );
    expect(css).toContain("min-height: min(40vh, 22rem)");

    // Avoid double borders from prose + frame.
    expect(proseCss).toContain("img:not(.iris-editor-media-image)");

    const imageRule = css.match(
      /\.iris-editor-body \.ProseMirror \.iris-editor-media-image \{[^}]+\}/,
    )?.[0];
    expect(imageRule).toBeTruthy();
    expect(imageRule).not.toContain("background:");
    expect(imageRule).not.toContain("contain: paint");
    expect(imageRule).not.toContain("transform: translateZ");
  });
});
