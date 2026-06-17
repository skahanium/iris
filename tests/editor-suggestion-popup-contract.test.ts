import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

function layerDepthBefore(
  css: string,
  layerName: string,
  index: number,
): number {
  const layerStart = css.indexOf(`@layer ${layerName}`);
  expect(layerStart).toBeGreaterThanOrEqual(0);
  const bodyStart = css.indexOf("{", layerStart);
  expect(bodyStart).toBeGreaterThanOrEqual(0);

  let depth = 1;
  for (const char of css.slice(bodyStart + 1, index)) {
    if (char === "{") depth += 1;
    if (char === "}") depth -= 1;
  }
  return depth;
}

describe("editor suggestion popup chrome", () => {
  it("uses a shell-less tippy theme for slash and wiki-link suggestions", () => {
    const slash = read(
      "src/components/editor/extensions/SlashCommandExtension.ts",
    );
    const wiki = read("src/components/editor/extensions/WikiLinkExtension.ts");
    const css = read("src/styles/globals.css");

    for (const source of [slash, wiki]) {
      expect(source).toContain('theme: "iris-suggestion"');
      expect(source).toContain("arrow: false");
      expect(source).toContain('maxWidth: "none"');
    }

    expect(css).toContain('.tippy-box[data-theme~="iris-suggestion"]');
    expect(css).toContain("background: transparent");
    expect(css).toContain("padding: 0");
    expect(css).toContain("intentionally stay outside Tailwind layers");

    const tippyRuleIndex = css.indexOf(
      '.tippy-box[data-theme~="iris-suggestion"]',
    );
    expect(layerDepthBefore(css, "components", tippyRuleIndex)).toBe(0);
  });
});
