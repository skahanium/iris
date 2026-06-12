import { describe, expect, it } from "vitest";

import { ingestMarkdownForEditor } from "@/lib/editor-ingest";

describe("editor ingest footnotes", () => {
  it("emits stable anchor relationship for footnote refs and definitions", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown: "Text[^a]\n\n[^a]: Body",
    });

    expect(result.tipTapHtml).toContain('data-footnote-ref="a"');
    expect(result.tipTapHtml).toContain('id="footnote-ref-a"');
    expect(result.tipTapHtml).toContain('href="#footnote-a"');
    expect(result.tipTapHtml).toContain('data-footnote-def="a"');
    expect(result.tipTapHtml).toContain('id="footnote-a"');
    expect(result.tipTapHtml).toContain(
      'data-footnote-return="footnote-ref-a"',
    );
  });

  it("does not create nested paragraph footnote definitions", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown: "Text[^a]\n\n[^a]: Body",
    });

    expect(result.tipTapHtml).not.toMatch(
      /<p[^>]*data-footnote-def[^>]*>\s*<p>/,
    );
  });

  it("escapes malicious footnote labels before writing attributes", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown:
        'Text[^x" onmouseover="alert(1)]\n\n[^x" onmouseover="alert(1)]: Body',
    });

    expect(result.tipTapHtml).not.toContain('onmouseover="alert(1)"');
    expect(result.tipTapHtml).toContain("&quot;");
  });
});
