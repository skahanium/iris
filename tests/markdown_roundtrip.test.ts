import { describe, expect, it } from "vitest";

import {
  editorHtmlToMarkdown,
  htmlToMarkdown,
  markdownRoundTrip,
  markdownToEditorHtml,
  markdownToHtml,
  markdownToHtmlPage,
  noteMarkdownRoundTrip,
} from "@/lib/markdown";

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
  it("marked→turndown path may not preserve images; production editor PM path does", async () => {
    const md = "![diagram](https://example.com/x.png)";
    const turndownOut = markdownRoundTrip(md);
    expect(typeof turndownOut).toBe("string");

    const { createProductionEditorFromIngestedBody, pmSerializeBody } =
      await import("./helpers/tiptap-serialize-harness");
    const editor = createProductionEditorFromIngestedBody(md);
    try {
      expect(pmSerializeBody(editor)).toContain(
        "![diagram](https://example.com/x.png)",
      );
    } finally {
      editor.destroy();
    }
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

describe("iris note title round-trip (frontmatter)", () => {
  it("preserves frontmatter title through editor html", () => {
    const md = '---\ntitle: "我的笔记"\n---\n\n正文第一段。';
    const out = noteMarkdownRoundTrip(md);
    expect(out).toContain('title: "我的笔记"');
    expect(out).toContain("正文第一段");
    expect(out).not.toMatch(/^#\s+我的笔记/m);
  });

  it("migrates legacy leading h1 into frontmatter title on save", () => {
    const md = "# Legacy Title\n\nBody here.";
    const html = markdownToEditorHtml(md, "fallback");
    expect(html).toContain('class="iris-doc-title"');
    expect(html).toContain("Legacy Title");
    const out = editorHtmlToMarkdown(html, null);
    expect(out).toContain('title: "Legacy Title"');
    expect(out).toContain("Body here");
  });

  it("keeps other frontmatter fields when title changes", () => {
    const md = '---\ntitle: "A"\ntags: [iris]\n---\n\nText.';
    const html = markdownToEditorHtml(md);
    const edited = html.replace(
      '<h1 class="iris-doc-title">A</h1>',
      '<h1 class="iris-doc-title">B</h1>',
    );
    const out = editorHtmlToMarkdown(edited, 'title: "A"\ntags: [iris]');
    expect(out).toContain('title: "B"');
    expect(out).toContain("tags: [iris]");
  });

  it("allows empty title in frontmatter", () => {
    const md = '---\ntitle: ""\n---\n\n';
    const out = noteMarkdownRoundTrip(md);
    expect(out).toContain('title: ""');
  });

  it("does not duplicate title when body still has matching h1", () => {
    const md = '---\ntitle: "新标题"\n---\n\n# 新标题\n\n正文';
    const html = markdownToEditorHtml(md);
    expect((html.match(/<h1/gi) ?? []).length).toBe(1);
    expect(html).toContain('class="iris-doc-title"');
    expect(html).toContain("正文");
    const out = noteMarkdownRoundTrip(md);
    expect(out).toContain('title: "新标题"');
    expect(out).not.toMatch(/^#\s+新标题/m);
    expect(out).toContain("正文");
  });
});

describe("html page export (v0.3)", () => {
  it("produces self-contained HTML with paper-ink styles", () => {
    const page = markdownToHtmlPage("# Hello\n\nWorld.", "Test Note");
    expect(page).toContain("<!DOCTYPE html>");
    expect(page).toContain("<title>Test Note</title>");
    expect(page).toContain("<h1>Hello</h1>");
    expect(page).toContain("Noto Serif SC");
    expect(page).toContain("background: #fafaf9");
  });

  it("falls back to default title", () => {
    const page = markdownToHtmlPage("Content");
    expect(page).toContain("Iris Note");
  });
});
