import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const proseCss = readFileSync("src/styles/markdown-prose.css", "utf8");

function readRule(pattern: RegExp): string {
  const match = proseCss.match(pattern);
  expect(match).toBeTruthy();
  return match?.[0] ?? "";
}

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
    expect(proseCss).toContain("--callout-tip: var(--brand)");
    expect(proseCss).toContain("--callout-warning: var(--warning)");
    expect(proseCss).toContain("border-left-color: hsl(var(--callout-note))");
    expect(proseCss).toContain("border-left-color: hsl(var(--callout-danger))");
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

  it("left-aligns editor body prose blocks (no forced justify)", () => {
    const editorBodyRule = readRule(
      /\.iris-editor-body\s+\.ProseMirror\.iris-markdown-content\[data-prose-surface="editor"\]\s+> p,[\s\S]+?> blockquote\s*\{[\s\S]+?\}/,
    );
    expect(editorBodyRule).toContain("text-align: start;");
    expect(editorBodyRule).not.toContain("text-align: justify;");
    expect(editorBodyRule).not.toContain("text-justify");
    expect(editorBodyRule).toContain("line-break: loose;");
    expect(editorBodyRule).not.toContain("text-align-last");

    const editorHeadingRule = readRule(
      /\.iris-editor-body\s+\.ProseMirror\.iris-markdown-content\[data-prose-surface="editor"\]\s+> :is\(h1:not\(\.iris-doc-title\), h2, h3, h4, h5, h6\)\s*\{[\s\S]+?\}/,
    );
    expect(editorHeadingRule).toContain("text-align: left;");
    expect(editorHeadingRule).not.toContain("text-align: justify;");

    const conversationBodyRule = readRule(
      /\[data-prose-surface="conversation"\]\.iris-markdown-content\s+:where\(p, li, blockquote, td, th\)\s*\{[\s\S]+?\}/,
    );
    expect(conversationBodyRule).not.toContain("text-align: justify;");
    expect(conversationBodyRule).not.toContain("text-justify");
  });
});
