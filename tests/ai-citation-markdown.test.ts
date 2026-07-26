import { describe, expect, it } from "vitest";

import {
  citationHrefForLabel,
  decodeCitationHref,
  formatCitationDisplayLabel,
  isExternalHttpsHref,
  linkifyAiCitations,
  normalizeCitationLabel,
  postProcessCitations,
  repairOverEscapedCitationLinks,
} from "@/lib/ai/citation-markdown";
import { renderAiMarkdownToHtml } from "@/lib/markdown-render";

describe("citation markdown rendering", () => {
  it("linkifies a bare citation label with a superscript badge display", () => {
    const output = linkifyAiCitations("source [citation:3]");
    expect(output).toContain("#iris-cite-");
    expect(output).toContain("[3](#iris-cite-");
    expect(output).not.toContain("\\[");
  });

  it("normalizes Unicode superscript citation markers", () => {
    expect(normalizeCitationLabel("¹")).toBe("1");
    const output = linkifyAiCitations("见 [¹] 与 [²]");
    expect(output).toContain("[1](#iris-cite-1)");
    expect(output).toContain("[2](#iris-cite-2)");
  });

  it("formats display labels without brackets", () => {
    expect(formatCitationDisplayLabel("citation:3")).toBe("3");
    expect(formatCitationDisplayLabel("W2")).toBe("2");
  });

  it("does not linkify the same citation twice", () => {
    const once = linkifyAiCitations("[citation:2]");
    expect(linkifyAiCitations(once)).toBe(once);
  });

  it("repairs escaped citation links before markdown rendering", () => {
    const escaped = "[\\\\[citation:2\\\\]](#iris-cite-citation%3A2)";
    const output = renderAiMarkdownToHtml(repairOverEscapedCitationLinks(escaped));
    expect(output).toContain("ai-citation-wrap");
  });

  it("post-processes citation anchors without breaking markdown", () => {
    const html = renderAiMarkdownToHtml("**important** [citation:1]");
    expect(html).toContain("<strong>important</strong>");
    expect(postProcessCitations(html)).toContain("ai-citation");
    expect(postProcessCitations(html)).toContain("ai-citation-wrap");
  });

  it("round-trips a safe citation hash", () => {
    const href = citationHrefForLabel("citation:3");
    expect(decodeCitationHref(href)).toBe("citation:3");
  });

  it("detects external https hrefs for system-browser open", () => {
    expect(isExternalHttpsHref("https://example.com/a")).toBe(true);
    expect(isExternalHttpsHref("http://example.com/a")).toBe(false);
    expect(isExternalHttpsHref("#iris-cite-1")).toBe(false);
  });

  it("renders numeric https markdown citations as badge links", () => {
    const html = renderAiMarkdownToHtml("[3](https://www.euronews.com/a)");
    expect(html).toContain('href="https://www.euronews.com/a"');
    expect(html).toContain('class="ai-citation"');
    expect(html).toContain("ai-citation-wrap");
    expect(html).not.toContain("underline");
  });

  it("renders descriptive https links as prose links not citations", () => {
    const html = renderAiMarkdownToHtml(
      "[Euronews article](https://www.euronews.com/a)",
    );
    expect(html).toContain('href="https://www.euronews.com/a"');
    expect(html).not.toContain("ai-citation");
  });
});
