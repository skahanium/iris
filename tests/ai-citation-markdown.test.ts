import { describe, expect, it } from "vitest";

import {
  citationHrefForLabel,
  decodeCitationHref,
  findPacketByCitationRef,
  linkifyAiCitations,
  tagCitationLinksInHtml,
} from "@/lib/ai/citation-markdown";
import type { ContextPacket } from "@/types/ai";

describe("linkifyAiCitations", () => {
  it("linkifies bare [citation:3]", () => {
    const out = linkifyAiCitations("见来源 [citation:3] 所述。");
    expect(out).toContain("\\[citation:3\\]");
    expect(out).toContain("#iris-cite-");
  });

  it("does not linkify twice", () => {
    const once = linkifyAiCitations("见 [citation:2]。");
    const twice = linkifyAiCitations(once);
    expect(twice).toBe(once);
  });

  it("fixes citation: protocol markdown links", () => {
    const out = linkifyAiCitations("见 [citation:2](citation:2)。");
    expect(out).toContain("#iris-cite-");
    expect(out).not.toContain("(citation:2)");
  });
});

describe("citation href codec", () => {
  it("round-trips citation ref", () => {
    const href = citationHrefForLabel("citation:3");
    expect(decodeCitationHref(href)).toBe("citation:3");
  });
});

describe("tagCitationLinksInHtml", () => {
  it("adds ai-citation class", () => {
    const html = '<a href="#iris-cite-citation%3A1">citation:1</a>';
    const out = tagCitationLinksInHtml(html);
    expect(out).toContain('class="ai-citation"');
    expect(out).toContain("data-cite-ref=");
  });
});

describe("findPacketByCitationRef", () => {
  const packets = [
    {
      id: "p1",
      citation_label: "[C1]",
    },
  ] as ContextPacket[];

  it("matches citation:N to packet label", () => {
    expect(findPacketByCitationRef("citation:1", packets)?.id).toBe("p1");
  });

  it("matches bracket label", () => {
    expect(findPacketByCitationRef("[C1]", packets)?.id).toBe("p1");
  });
});
