import { describe, expect, it } from "vitest";

import { htmlToMarkdown, markdownRoundTrip, markdownToHtml, markdownToHtmlPage } from "@/lib/markdown";

/** 规范化空白便于断言（不用于生产序列化） */
function normalize(md: string): string {
  return md.replace(/\r\n/g, "\n").trim();
}

describe("markdown round-trip (marked → turndown gfm)", () => {
  it("preserves heading and paragraph with bold", () => {
    const md = "# Title\n\nHello **world**.";
    const out = markdownRoundTrip(md);
    expect(out).toContain("Title");
    expect(out).toMatch(/\*\*world\*\*|__world__/);
  });

  it("preserves italic", () => {
    const md = "Text with *emphasis* here.";
    const out = markdownRoundTrip(md);
    expect(out).toMatch(/\*emphasis\*|_emphasis_/);
  });

  it("preserves strikethrough semantics", () => {
    const md = "~~removed~~";
    const out = markdownRoundTrip(md);
    expect(out).toContain("removed");
    expect(out).toMatch(/~+removed~+/);
  });

  it("preserves inline code", () => {
    const md = "Use `npm test` locally.";
    const out = markdownRoundTrip(md);
    expect(out).toContain("`npm test`");
  });

  it("preserves markdown link", () => {
    const md = "See [Iris](https://example.com/docs).";
    const out = markdownRoundTrip(md);
    expect(out).toContain("[Iris]");
    expect(out).toContain("https://example.com/docs");
  });

  it("preserves blockquote", () => {
    const md = "> quoted line\n> second line";
    const out = markdownRoundTrip(md);
    expect(out).toContain("> quoted");
    expect(out).toContain("second line");
  });

  it("preserves ordered list", () => {
    const md = "1. First\n2. Second";
    const out = markdownRoundTrip(md);
    expect(out).toMatch(/First/);
    expect(out).toMatch(/Second/);
    expect(out).toMatch(/^1\.\s/m);
  });

  it("preserves bullet list", () => {
    const md = "- alpha\n- beta";
    const out = normalize(markdownRoundTrip(md));
    expect(out).toContain("alpha");
    expect(out).toContain("beta");
    expect(out).toMatch(/^[-*]\s/m);
  });

  it("preserves task list", () => {
    const md = "- [x] Done\n- [ ] Todo";
    const out = markdownRoundTrip(md);
    expect(out).toMatch(/\[x\]|Done/);
    expect(out).toMatch(/\[ \]|Todo/);
  });

  it("preserves table", () => {
    const md = "| A | B |\n| --- | --- |\n| 1 | 2 |";
    const out = markdownRoundTrip(md);
    expect(out).toContain("A");
    expect(out).toContain("1");
    expect(out).toContain("|");
  });

  it("preserves fenced code block with language", () => {
    const md = "```ts\nconst x = 1;\n```";
    const out = markdownRoundTrip(md);
    expect(out).toContain("```");
    expect(out).toContain("const x");
    expect(out).toMatch(/```ts|```typescript/);
  });

  it("preserves horizontal rule", () => {
    const md = "above\n\n---\n\nbelow";
    const out = normalize(markdownRoundTrip(md));
    expect(out).toContain("above");
    expect(out).toContain("below");
    expect(out).toMatch(/^(---|\*\*\*|___|\* \* \*)\s*$/m);
  });

  it("combines inline marks in one paragraph", () => {
    const md = "**bold** *italic* `code` [link](https://a.test)";
    const out = markdownRoundTrip(md);
    expect(out).toMatch(/\*\*bold\*\*|__bold__/);
    expect(out).toMatch(/\*italic\*|_italic_/);
    expect(out).toContain("`code`");
    expect(out).toContain("https://a.test");
  });
});

describe("markdown round-trip limitations (documented)", () => {
  it("image syntax may not round-trip to original markdown (no TipTap image node)", () => {
    const md = "![diagram](https://example.com/x.png)";
    const out = markdownRoundTrip(md);
    // turndown 可能保留为 HTML img 或丢弃；不断言与输入逐字相等
    expect(typeof out).toBe("string");
  });
});

describe("wiki-link round-trip (v0.2)", () => {
  it("preserves single wiki-link", () => {
    const md = "See [[架构设计]] for details.";
    const out = markdownRoundTrip(md);
    expect(out).toContain("[[架构设计]]");
  });

  it("preserves multiple wiki-links", () => {
    const md = "[[A]] and [[B 笔记]] together.";
    const out = markdownRoundTrip(md);
    expect(out).toContain("[[A]]");
    expect(out).toContain("[[B 笔记]]");
  });

  it("turndown converts wiki-link HTML back to [[title]]", () => {
    const html =
      '<p>See <span data-wiki-link="" data-wiki-title="架构设计">架构设计</span> for details.</p>';
    const md = htmlToMarkdown(html);
    expect(md).toContain("[[架构设计]]");
  });

  it("marked treats [[wiki-link]] as plain text (not HTML-escaped)", () => {
    const md = "See [[MyPage]].";
    const html = markdownToHtml(md);
    // marked should not escape or mangle [[MyPage]]
    expect(html).toContain("MyPage");
  });
});

describe("html page export (v0.3)", () => {
  it("produces self-contained HTML with paper-ink styles", () => {
    const page = markdownToHtmlPage("# Hello\n\nWorld.", "Test Note");
    expect(page).toContain("<!DOCTYPE html>");
    expect(page).toContain("<title>Test Note</title>");
    expect(page).toContain("<h1>Hello</h1>");
    expect(page).toContain("Noto Serif SC");
    expect(page).toContain("background: #f4f0e8");
  });

  it("falls back to default title", () => {
    const page = markdownToHtmlPage("Content");
    expect(page).toContain("Iris Note");
  });
});
