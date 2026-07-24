import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

const markdownProse = readFileSync("src/styles/markdown-prose.css", "utf8");
const globalsCss = readFileSync("src/styles/globals.css", "utf8");
const indexHtml = readFileSync("index.html", "utf8");

describe("prose polish v2 tokens", () => {
  it("uses local fonts instead of external Google Fonts", () => {
    expect(indexHtml).not.toContain("fonts.googleapis.com");
    expect(indexHtml).not.toContain("fonts.gstatic.com");
    expect(indexHtml).toContain("/src/assets/fonts/");
  });

  it("defines editor and conversation prose surfaces", () => {
    expect(markdownProse).toContain('data-prose-surface="editor"');
    expect(markdownProse).toContain('data-prose-surface="conversation"');
    expect(markdownProse).toContain("--prose-size-chat: 15px");
    expect(markdownProse).toContain("--prose-letter-spacing: 0.01em");
    expect(markdownProse).not.toContain("--prose-spacer-ratio");
    expect(markdownProse).not.toContain("letter-spacing: -");
    expect(markdownProse).toContain("text-align: start");
    expect(markdownProse).not.toContain("text-align: justify");
    expect(markdownProse).not.toContain("text-justify: inter-character");
    expect(markdownProse).toContain("line-break: loose");
    expect(markdownProse).not.toContain("text-align-last: justify");
    expect(markdownProse).not.toContain("text-justify: inter-ideograph");
    expect(markdownProse).toContain("--prose-wiki: var(--brand)");
    expect(markdownProse).toContain("--prose-measure: 42rem");
    expect(globalsCss).toContain("max-width: var(--prose-measure)");
    expect(globalsCss).toContain(
      "noto-sans-sc-chinese-simplified-400-normal.woff2",
    );
  });

  it("does not style editable spacer paragraphs in editor", () => {
    expect(markdownProse).not.toContain('data-iris-spacer="true"');
    expect(markdownProse).not.toContain("data-iris-gap-count");
  });

  it("centers document title with stable sans title numerals", () => {
    expect(globalsCss).toContain("text-center font-bold");
    expect(globalsCss).toContain("font-size: calc(2rem * var(--editor-zoom))");
    expect(globalsCss).toContain("font-family: var(--font-title)");
    expect(globalsCss).toContain("--font-title: var(--font-sans)");
    expect(markdownProse).toContain("--font-title: var(--font-sans)");
    expect(globalsCss).toContain("font-variant-numeric: lining-nums");
    expect(globalsCss).toContain('"lnum" 1');
    expect(globalsCss).not.toContain("Noto Serif SC");
    expect(markdownProse).not.toContain("Noto Serif SC");
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
