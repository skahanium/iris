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
    expect(markdownProse).toContain("--prose-spacer-ratio: 0.55");
    expect(markdownProse).toContain("text-justify: inter-ideograph");
  });

  it("styles compact spacer paragraphs in editor", () => {
    expect(markdownProse).toContain('p[data-iris-spacer="true"]');
    expect(markdownProse).toContain('data-iris-gap-count="2"');
  });

  it("centers document title with serif font", () => {
    expect(globalsCss).toContain("text-center text-4xl");
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
