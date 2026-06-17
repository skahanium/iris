import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

const markdownProse = readFileSync("src/styles/markdown-prose.css", "utf8");
const globalsCss = readFileSync("src/styles/globals.css", "utf8");
const indexHtml = readFileSync("index.html", "utf8");

describe("prose polish v2 tokens", () => {
  it("loads Noto fonts in index.html", () => {
    expect(indexHtml).toContain("Noto+Sans+SC");
    expect(indexHtml).toContain("Noto+Serif+SC");
  });

  it("defines editor and conversation prose surfaces", () => {
    expect(markdownProse).toContain('data-prose-surface="editor"');
    expect(markdownProse).toContain('data-prose-surface="conversation"');
    expect(markdownProse).toContain("--prose-size-chat: 15px");
    expect(markdownProse).toContain("--prose-letter-spacing: 0");
    expect(markdownProse).not.toContain("--prose-spacer-ratio");
    expect(markdownProse).not.toContain("letter-spacing: -");
    expect(markdownProse).not.toContain("text-align: justify");
    expect(markdownProse).not.toContain("text-justify: inter-ideograph");
  });

  it("does not style editable spacer paragraphs in editor", () => {
    expect(markdownProse).not.toContain('data-iris-spacer="true"');
    expect(markdownProse).not.toContain("data-iris-gap-count");
  });

  it("centers document title with serif font", () => {
    expect(globalsCss).toContain("text-center font-bold");
    expect(globalsCss).toContain(
      "font-size: calc(2.25rem * var(--editor-zoom))",
    );
    expect(globalsCss).toContain("font-family: var(--font-title)");
    expect(globalsCss).toContain("margin-bottom: var(--prose-title-gap)");
  });

  it("removes streaming inset left bar on AI bubbles", () => {
    expect(globalsCss).not.toContain("inset 3px 0 0");
    expect(globalsCss).toContain(".ai-thinking-row");
    expect(globalsCss).not.toMatch(/\.ai-msg h1\s*\{/);
  });

  it("wraps long commands and urls inside AI conversation markdown", () => {
    expect(markdownProse).toContain("overflow-wrap: anywhere");
    expect(markdownProse).toContain("word-break: break-word");
    expect(markdownProse).toMatch(
      /\[data-prose-surface="conversation"\]\.iris-markdown-content\s+:where\(p, li, blockquote, td, th\)\s*\{[\s\S]*overflow-wrap: anywhere;/,
    );
    expect(markdownProse).toMatch(
      /\[data-prose-surface="conversation"\]\.iris-markdown-content\s+code\s*\{[\s\S]*overflow-wrap: anywhere;/,
    );
    expect(markdownProse).toMatch(
      /\[data-prose-surface="conversation"\]\.iris-markdown-content\s+pre code\s*\{[\s\S]*white-space: pre-wrap;/,
    );
  });

  it("styles assistant code block copy controls", () => {
    expect(markdownProse).toContain(".ai-code-block");
    expect(markdownProse).toContain(".ai-code-copy-button");
    expect(markdownProse).toContain("position: absolute");
    expect(markdownProse).toContain("padding-top: 2.75rem");
  });
});
