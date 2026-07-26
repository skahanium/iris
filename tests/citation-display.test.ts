import { describe, expect, it } from "vitest";

import {
  filterReferencedWebCitations,
  referencedCitationIndices,
} from "@/lib/ai/citation-display";

const entries = [
  { index: 1, title: "One", url: "https://example.com/one" },
  { index: 2, title: "Two", url: "https://example.com/two" },
  { index: 3, title: "Three", url: "https://example.com/three" },
];

describe("referencedCitationIndices", () => {
  it("detects bare numeric markers", () => {
    expect([...referencedCitationIndices("见 [1] 与 [3]")]).toEqual([1, 3]);
  });

  it("detects linkified https footnotes from persisted answers", () => {
    const content =
      "晋级 [1](https://news.example/a)，教练 [3](https://news.example/c)。";
    expect([...referencedCitationIndices(content)]).toEqual([1, 3]);
    expect(
      filterReferencedWebCitations(entries, content).map((e) => e.index),
    ).toEqual([1, 3]);
  });

  it("detects iris-cite hash links", () => {
    const content = "引用 [2](#iris-cite-2) 结束。";
    expect([...referencedCitationIndices(content)]).toEqual([2]);
  });
});
