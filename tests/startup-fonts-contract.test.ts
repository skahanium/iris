import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("startup font loading contract", () => {
  it("does not load Google Fonts or gstatic during startup", () => {
    const html = read("index.html");

    expect(html).not.toContain("fonts.googleapis.com");
    expect(html).not.toContain("fonts.gstatic.com");
    expect(html).not.toContain("display=swap");
  });

  it("preloads only local first-viewport fonts", () => {
    const html = read("index.html");

    expect(html).toContain('rel="preload"');
    expect(html).toContain('as="font"');
    expect(html).toContain("/src/assets/fonts/inter-latin-400-normal.woff2");
    expect(html).toContain("/src/assets/fonts/inter-latin-600-normal.woff2");
    expect(html).toContain("/src/assets/fonts/inter-latin-700-normal.woff2");
    expect(html).toContain(
      "/src/assets/fonts/jetbrains-mono-latin-400-normal.woff2",
    );
    expect(html).toContain(
      "/src/assets/fonts/noto-sans-sc-chinese-simplified-400-normal.woff2",
    );
    expect(html).not.toContain("Noto+Sans+SC");
    expect(html).not.toContain("Noto+Serif+SC");
  });

  it("declares local Inter, JetBrains Mono, and Noto Sans SC faces", () => {
    const css = read("src/styles/globals.css");

    expect(css).toContain("@font-face");
    expect(css).toContain('font-family: "Inter"');
    expect(css).toContain('font-family: "JetBrains Mono"');
    expect(css).toContain('font-family: "Noto Sans SC"');
    expect(css).toContain("font-display: swap");
    expect(css).toContain("--font-sans");
    expect(css).toContain("--font-prose");
    expect(css).toContain("PingFang SC");
    expect(css).toContain("Microsoft YaHei");
  });

  it("keeps static brand preview free of external font CDNs", () => {
    const preview = read("public/brand-preview.html");

    expect(preview).not.toContain("fonts.googleapis.com");
    expect(preview).not.toContain("fonts.gstatic.com");
  });
  it("keeps font licenses with the bundled font assets", () => {
    expect(existsSync("src/assets/fonts/OFL.txt")).toBe(true);
  });
});
