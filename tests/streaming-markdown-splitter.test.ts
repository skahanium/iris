import { describe, expect, it } from "vitest";

import { splitStreamingMarkdown } from "@/lib/streaming-markdown-splitter";

describe("splitStreamingMarkdown", () => {
  it("keeps the current paragraph in the tail", () => {
    const split = splitStreamingMarkdown("still writing");

    expect(split.stableMarkdown).toBe("");
    expect(split.tailMarkdown).toBe("still writing");
    expect(split.stableBlockCount).toBe(0);
  });

  it("stabilizes a paragraph after a blank separator", () => {
    const split = splitStreamingMarkdown("first paragraph\n\n");

    expect(split.stableMarkdown).toBe("first paragraph\n\n");
    expect(split.tailMarkdown).toBe("");
  });

  it("stabilizes headings immediately", () => {
    const split = splitStreamingMarkdown("## Title\nnext");

    expect(split.stableMarkdown).toBe("## Title\n");
    expect(split.tailMarkdown).toBe("next");
  });

  it("keeps an unclosed code fence in the tail", () => {
    const split = splitStreamingMarkdown("```ts\nconst x = 1");

    expect(split.stableMarkdown).toBe("");
    expect(split.tailMarkdown).toBe("```ts\nconst x = 1");
  });

  it("stabilizes a closed code fence", () => {
    const split = splitStreamingMarkdown("```ts\nconst x = 1\n```");

    expect(split.stableMarkdown).toBe("```ts\nconst x = 1\n```");
    expect(split.tailMarkdown).toBe("");
  });
});
