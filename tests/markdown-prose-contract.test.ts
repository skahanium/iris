import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const proseCss = readFileSync("src/styles/markdown-prose.css", "utf8");

describe("markdown prose CSS contract", () => {
  it("styles all supported callout types without affecting plain blockquotes", () => {
    for (const type of [
      "note",
      "info",
      "tip",
      "warning",
      "danger",
      "example",
    ]) {
      expect(proseCss).toContain(`blockquote[data-callout-type="${type}"]`);
    }

    expect(proseCss).toContain(
      ".iris-markdown-content blockquote:not([data-callout-type])",
    );
    expect(proseCss).toContain(
      ".iris-markdown-content blockquote[data-callout-type] {",
    );
    expect(proseCss).toContain("border-left-width: 4px;");
    expect(proseCss).toContain(
      ".iris-markdown-content blockquote[data-callout-type] > p:first-child",
    );
    expect(proseCss).toContain("@apply text-sm font-semibold;");
    expect(proseCss).toContain("@apply border-l-primary bg-primary/5;");
    expect(proseCss).toContain("background: hsl(var(--primary) / 0.08);");
    expect(proseCss).toContain("border-left-color: hsl(var(--destructive)");
    expect(proseCss).toContain("@apply border-l-destructive bg-destructive/5;");
    expect(proseCss).toContain("@apply border-l-muted-foreground bg-muted/30;");
  });

  it("styles footnote anchors and definitions", () => {
    expect(proseCss).toContain("[data-footnote-ref]");
    expect(proseCss).toContain("[data-footnote-def]");
    expect(proseCss).toContain("@apply cursor-pointer text-primary;");
    expect(proseCss).toContain("hover:bg-primary/10");
    expect(proseCss).toContain(
      "@apply mt-3 rounded-md border border-border bg-muted/20 px-3 py-2 text-sm text-editor-muted;",
    );
  });
});
