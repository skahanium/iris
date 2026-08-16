import { describe, expect, it } from "vitest";

import { splitStreamingMarkdown } from "@/lib/streaming-markdown-splitter";

describe("splitStreamingMarkdown", () => {
  it("promotes a paragraph ending in a blank line to a stable block", () => {
    const split = splitStreamingMarkdown("已完成的段落。\n\n");

    expect(split).toEqual({
      stableMarkdown: "已完成的段落。\n\n",
      tailMarkdown: "",
      stableBlockCount: 1,
    });
  });

  it("keeps an unfinished final paragraph in the streaming tail", () => {
    const split = splitStreamingMarkdown("仍在输出的段落");

    expect(split).toEqual({
      stableMarkdown: "",
      tailMarkdown: "仍在输出的段落",
      stableBlockCount: 0,
    });
  });

  it("keeps an unclosed fenced code block in the tail", () => {
    const split = splitStreamingMarkdown("```ts\nconst answer =");

    expect(split.stableMarkdown).toBe("");
    expect(split.tailMarkdown).toBe("```ts\nconst answer =");
  });

  it.each([
    ["- first\n- second\n\nnext", "- first\n- second\n\n"],
    ["> quoted\n\nnext", "> quoted\n\n"],
    [
      "| A | B |\n| - | - |\n| 1 | 2 |\n\nnext",
      "| A | B |\n| - | - |\n| 1 | 2 |\n\n",
    ],
  ])(
    "stabilizes a terminated Markdown block before the active tail",
    (content, stableMarkdown) => {
      const split = splitStreamingMarkdown(content);

      expect(split.stableMarkdown).toBe(stableMarkdown);
      expect(split.tailMarkdown).toBe("next");
    },
  );
});
