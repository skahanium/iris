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
      filterReferencedWebCitations(entries, content).map(
        (entry) => entry.index,
      ),
    ).toEqual([1, 3]);
  });

  it("detects iris-cite hash links", () => {
    const content = "引用 [2](#iris-cite-2) 结束。";
    expect([...referencedCitationIndices(content)]).toEqual([2]);
  });
});

describe("filterReferencedWebCitations", () => {
  it("shows every verified source for an answer-level source group", () => {
    expect(
      filterReferencedWebCitations(entries, "没有行内格式", false),
    ).toEqual(entries);
  });

  it("shows only Run-local sources referenced by a follow-up answer", () => {
    const entries = [
      { index: 1, title: "follow-up source 1", url: "https://example.test/1" },
      { index: 2, title: "follow-up source 2", url: "https://example.test/2" },
      { index: 3, title: "follow-up source 3", url: "https://example.test/3" },
    ];

    expect(
      filterReferencedWebCitations(entries, "第二轮结论。[W2] [W3]"),
    ).toEqual([entries[1], entries[2]]);
  });

  it("does not suppress sources after a historical Run-local projection rebuild", () => {
    const rebuiltEntries = [
      { index: 1, title: "历史 Run 来源", url: "https://example.test/history" },
    ];

    expect(
      filterReferencedWebCitations(rebuiltEntries, "历史回答。[W1]"),
    ).toEqual(rebuiltEntries);
  });
});
