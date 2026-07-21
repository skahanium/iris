import { describe, expect, it } from "vitest";

import {
  citationHrefForLabel,
  decodeCitationHref,
  isExternalHttpsHref,
  linkifyAiCitations,
  normalizeCitationLabel,
  postProcessCitations,
  repairOverEscapedCitationLinks,
  tagCitationLinksInHtml,
} from "@/lib/ai/citation-markdown";
import {
  parseMarkdownToHtml,
  renderAiMarkdownToHtml,
} from "@/lib/markdown-render";

describe("citation markdown rendering", () => {
  it("linkifies a bare citation label with a clean bracket display", () => {
    const output = linkifyAiCitations("source [citation:3]");
    expect(output).toContain("#iris-cite-");
    expect(output).toContain("[citation:3](");
    expect(output).not.toContain("\\[");
  });

  it("normalizes Unicode superscript citation markers", () => {
    expect(normalizeCitationLabel("¹")).toBe("1");
    const output = linkifyAiCitations("见 [¹] 与 [²]");
    expect(output).toContain("[1](#iris-cite-1)");
    expect(output).toContain("[2](#iris-cite-2)");
  });

  it("does not linkify the same citation twice", () => {
    const once = linkifyAiCitations("[citation:2]");
    expect(linkifyAiCitations(once)).toBe(once);
  });

  it("repairs escaped citation links before markdown rendering", () => {
    const escaped = "[\\\\[citation:2\\\\]](#iris-cite-citation%3A2)";
    const output = repairOverEscapedCitationLinks(escaped);
    expect(tagCitationLinksInHtml(parseMarkdownToHtml(output))).toContain(
      "ai-citation",
    );
  });

  it("post-processes citation anchors without breaking markdown", () => {
    const html = renderAiMarkdownToHtml("**important** [citation:1]");
    expect(html).toContain("<strong>important</strong>");
    expect(postProcessCitations(html)).toContain("ai-citation");
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

  it("renders https markdown citations as styled clickable anchors", () => {
    const html = renderAiMarkdownToHtml(
      "[1. Euronews, 2026-07-20](https://www.euronews.com/a)",
    );
    expect(html).toContain('href="https://www.euronews.com/a"');
    expect(html).toContain('class="ai-citation"');
    expect(html).toContain("1. Euronews, 2026-07-20");
  });
});
