import { describe, expect, it, vi } from "vitest";

import { proseMarked } from "@/lib/markdown-render";

import {
  splitStreamingMarkdown,
  updateStreamingMarkdown,
} from "@/lib/streaming-markdown-splitter";

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

describe("source boundary regressions", () => {
  it.each(["# 标", "标题\n---", "- 第一项\n\n", "| A |\n| - |\n| B |\n"])(
    "keeps absorbable final syntax active: %s",
    (source) => {
      expect(splitStreamingMarkdown(source).stableMarkdown).toBe("");
    },
  );
  it.each(["~~~ts\na\n~~~\n", "````ts\n```\na\n````\n"])(
    "recognizes exact tilde/backtick fence closure",
    (source) => {
      expect(splitStreamingMarkdown(source).stableMarkdown).toBe(source);
    },
  );
  it("does not close a four-backtick fence with three backticks", () => {
    expect(splitStreamingMarkdown("````ts\na\n```\n").stableMarkdown).toBe("");
  });
});

it("does not freeze a closing fence before its line terminates", async () => {
  const { updateStreamingMarkdown } =
    await import("@/lib/streaming-markdown-splitter");
  const first = "```txt\na\n```";
  const full = first + "x\nb\n```";
  expect(
    updateStreamingMarkdown(full, updateStreamingMarkdown(first)).blocks,
  ).toEqual(updateStreamingMarkdown(full).blocks);
});

it("does not locate a paragraph inside the preceding reference URL", async () => {
  const { updateStreamingMarkdown } =
    await import("@/lib/streaming-markdown-splitter");
  const first = "intro\n\n[ref]: x\n\nx";
  const full = first + "y";
  expect(
    updateStreamingMarkdown(full, updateStreamingMarkdown(first)).blocks,
  ).toEqual(updateStreamingMarkdown(full).blocks);
});

describe("incremental reference definitions", () => {
  it("keeps an unfinished definition outside the frozen source offset", () => {
    const first = "intro\n\n[ref]: https://ex";
    const final = first + "ample.com\n\n[link][ref]";
    const initial = updateStreamingMarkdown(first);
    const incremental = updateStreamingMarkdown(final, initial);
    const fresh = updateStreamingMarkdown(final);
    expect(incremental.blocks).toEqual(fresh.blocks);
    expect(incremental.definitions).toBe(fresh.definitions);
    expect(initial.blocks[0]?.end).toBe("intro\n\n".length);
  });

  it("accumulates confirmed definitions while replacing only the active partial definition", () => {
    const parts = [
      "[first]: https://one.example\n\nintro\n\n",
      "[second]: https://tw",
      "o.example\n\nnext\n\n",
      "[third]: https://th",
      "ree.example\n\n[first] [second] [third]",
    ];
    let state = updateStreamingMarkdown("");
    let source = "";
    for (const part of parts) {
      source += part;
      state = updateStreamingMarkdown(source, state);
    }
    const fresh = updateStreamingMarkdown(source);
    expect(state.blocks).toEqual(fresh.blocks);
    expect(state.definitions).toBe(fresh.definitions);
    expect(state.definitions).toContain("https://two.example");
    expect(state.definitions).toContain("https://three.example");
  });
  it("retains first-definition precedence across confirmed blocks", () => {
    const first = "[ref]: https://first.example\n\nintro\n\n";
    const final = first + "[ref]: https://second.example\n\n[ref]";
    expect(
      updateStreamingMarkdown(final, updateStreamingMarkdown(first))
        .definitions,
    ).toBe(updateStreamingMarkdown(final).definitions);
    expect(
      updateStreamingMarkdown(final, updateStreamingMarkdown(first))
        .definitions,
    ).toContain("https://first.example");
  });

  it("does not re-lex a confirmed prefix while completing a definition", () => {
    const prefix = "confirmed paragraph\n\n".repeat(2000);
    const first = prefix + "[ref]: https://ex";
    const initial = updateStreamingMarkdown(first);
    const lexer = vi.spyOn(proseMarked, "lexer");
    try {
      const final = updateStreamingMarkdown(
        first + "ample.com\n\n[ref]",
        initial,
      );
      expect(final.blocks[0]).toBe(initial.blocks[0]);
      expect(lexer.mock.calls.length).toBeGreaterThan(0);
      expect(lexer.mock.calls.every(([source]) => source.length < 100)).toBe(
        true,
      );
      expect(final.definitions).toContain("https://example.com");
    } finally {
      lexer.mockRestore();
    }
  });
});
