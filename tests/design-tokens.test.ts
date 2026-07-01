import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

const globalsCss = readFileSync("src/styles/globals.css", "utf8");
const tailwindConfigSource = readFileSync("tailwind.config.js", "utf8");

function cssVariable(name: string): string | undefined {
  const match = globalsCss.match(new RegExp(`${name}:\\s*([^;]+);`));
  return match?.[1]?.trim();
}

function tailwindMapsToken(key: string, value: string): boolean {
  const escapedValue = value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const pattern = new RegExp(`${key}:\\s*["']${escapedValue}["']`);
  return pattern.test(tailwindConfigSource);
}

describe("design tokens", () => {
  it("clips transparent Tauri shell to window radius on non-Windows", () => {
    expect(globalsCss).toContain("html[data-iris-desktop-transparent]");
    expect(globalsCss).toContain("background: transparent");
    expect(globalsCss).toContain(".iris-desktop-frame");
  });

  it("defines Notion-style overlay, radius, shadow, and motion variables", () => {
    expect(cssVariable("--overlay-scrim")).toBe("0 0% 5% / 0.55");
    expect(cssVariable("--radius-sm")).toBe("6px");
    expect(cssVariable("--radius-md")).toBe("8px");
    expect(cssVariable("--radius-lg")).toBe("12px");
    expect(cssVariable("--radius-xl")).toBe("16px");
    expect(cssVariable("--window-radius")).toBe("12px");
    expect(cssVariable("--shadow-overlay")).toBeDefined();
    expect(globalsCss).not.toContain("--shadow-paper");
    expect(cssVariable("--motion-fast")).toBe("150ms");
    expect(cssVariable("--motion-base")).toBe("200ms");
    expect(cssVariable("--motion-exit")).toBe("140ms");
    expect(cssVariable("--motion-ease-out")).toBe(
      "cubic-bezier(0.16, 1, 0.3, 1)",
    );
  });

  it("uses neutral dark theme and blue-gray primary", () => {
    expect(cssVariable("--background")).toBe("0 0% 10%");
    expect(cssVariable("--primary")).toBe("210 18% 62%");
    expect(cssVariable("--editor-paper")).toBe("var(--background)");
  });

  it("defines Chrome surface, command, and AI tokens", () => {
    expect(cssVariable("--surface-chrome")).toBe("0 0% 12%");
    expect(cssVariable("--surface-elevated")).toBe("0 0% 14%");
    expect(cssVariable("--command-highlight-bg")).toBeDefined();
    expect(cssVariable("--ai-user-bg")).toBe("0 0% 18%");
    expect(cssVariable("--ai-composer-bg")).toBe("0 0% 14%");
    expect(tailwindConfigSource).toContain(
      'chrome: "hsl(var(--surface-chrome))"',
    );
    expect(tailwindConfigSource).toContain(
      'highlight: "hsl(var(--command-highlight-bg))"',
    );
    expect(tailwindConfigSource).toContain('user: "hsl(var(--ai-user-bg))"');
  });

  it("defines Iris Rail semantic surface tokens and Tailwind mappings", () => {
    expect(cssVariable("--knowledge-accent")).toBe("150 12% 54%");
    expect(cssVariable("--iris-rail-bg")).toBe("var(--surface-chrome)");
    expect(cssVariable("--iris-rail-active")).toBe("150 12% 54%");
    expect(cssVariable("--outline-rail-bg")).toBe("0 0% 12% / 0.88");
    expect(cssVariable("--ai-workspace-bg")).toBe("var(--panel)");
    expect(cssVariable("--overlay-task-header")).toBe(
      "var(--surface-elevated)",
    );
    expect(tailwindConfigSource).toContain(
      'accent: "hsl(var(--knowledge-accent))"',
    );
    expect(tailwindConfigSource).toContain('bg: "hsl(var(--iris-rail-bg))"');
    expect(tailwindConfigSource).toContain('bg: "hsl(var(--outline-rail-bg))"');
    expect(tailwindConfigSource).toContain(
      'header: "hsl(var(--overlay-task-header))"',
    );
  });

  it("exposes design tokens through Tailwind theme extensions", () => {
    expect(tailwindConfigSource).toContain(
      'scrim: "hsl(var(--overlay-scrim))"',
    );
    expect(tailwindMapsToken("sm", "var(--radius-sm)")).toBe(true);
    expect(tailwindMapsToken("md", "var(--radius-md)")).toBe(true);
    expect(tailwindMapsToken("lg", "var(--radius-lg)")).toBe(true);
    expect(tailwindMapsToken("xl", "var(--radius-xl)")).toBe(true);
    expect(tailwindMapsToken("overlay", "var(--shadow-overlay)")).toBe(true);
    expect(tailwindMapsToken("fast", "var(--motion-fast)")).toBe(true);
    expect(tailwindMapsToken("base", "var(--motion-base)")).toBe(true);
    expect(tailwindConfigSource).toContain('"Inter"');
    expect(tailwindConfigSource).toContain('"Noto Sans SC"');
    expect(tailwindConfigSource).toContain("title: [");
    expect(tailwindConfigSource).not.toContain('"Noto Serif SC"');
    expect(tailwindConfigSource).not.toContain("shadow-paper");
  });
});
