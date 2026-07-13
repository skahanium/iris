import { describe, expect, it } from "vitest";

import {
  citationHrefForLabel,
  decodeCitationHref,
  linkifyAiCitations,
  postProcessCitations,
  repairOverEscapedCitationLinks,
  tagCitationLinksInHtml,
} from "@/lib/ai/citation-markdown";
import {
  parseMarkdownToHtml,
  renderAiMarkdownToHtml,
} from "@/lib/markdown-render";

describe("citation markdown rendering", () => {
  it("linkifies a bare citation label", () => {
    const output = linkifyAiCitations("source [citation:3]");
    expect(output).toContain("#iris-cite-");
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
});
