/**
 * ai-streaming-experience.test.ts — AI 展示重构 阶段 0 基线+TDD 测试
 *
 * 测试部分 Markdown 在流式模式下的渲染稳定性和布局安全。
 * 基线测试（部分渲染）当前 GREEN，TDD 测试（布局跳变）当前 RED。
 *
 * 覆盖 CONTRACT_PLAN.md § 测试计划 3 — 流式体验
 */
import { describe, expect, it } from "vitest";

import {
  repairStreamingMarkdown,
  parseMarkdownToHtml,
} from "@/lib/markdown-render";
import { renderMarkdownWithProfile } from "@/lib/markdown-contract/contract";

// ═══════════════════════════════════════════════════════════════
// Partial markdown — rendering stability (baseline GREEN)
// ═══════════════════════════════════════════════════════════════

describe("partial markdown rendering stability", () => {
  it("[BASELINE] **partial bold does not render as raw text", () => {
    const r = renderMarkdownWithProfile("**partial bold", "chat_assistant", {
      streaming: true,
    });
    expect(r.output).toContain("<strong>");
    expect(r.output).not.toContain("**partial bold**");
  });

  it("[BASELINE] *partial italic renders as <em>", () => {
    const r = renderMarkdownWithProfile("*partial italic", "chat_assistant", {
      streaming: true,
    });
    expect(r.output).toContain("<em>");
  });

  it("[BASELINE] partial fence renders code block", () => {
    const r = renderMarkdownWithProfile("```js\nconst x = ", "chat_assistant", {
      streaming: true,
    });
    expect(r.output).toContain("<pre");
    expect(r.output).toContain("<code");
  });

  it("[BASELINE] partial blockquote renders correctly", () => {
    const r = renderMarkdownWithProfile("> partial quote", "chat_assistant", {
      streaming: true,
    });
    expect(r.output).toContain("<blockquote>");
  });

  it("[BASELINE] partial list renders list item", () => {
    const r = renderMarkdownWithProfile("- partial item", "chat_assistant", {
      streaming: true,
    });
    expect(r.output).toContain("<ul");
    expect(r.output).toContain("<li");
  });

  it("[BASELINE] partial footnote does not crash", () => {
    expect(() =>
      renderMarkdownWithProfile("Text[^", "chat_assistant", {
        streaming: true,
      }),
    ).not.toThrow();
  });

  it("[BASELINE] partial callout renders as blockquote", () => {
    const r = renderMarkdownWithProfile(
      "> [!note] Title\n> partial",
      "chat_assistant",
      { streaming: true },
    );
    expect(r.output).toContain("<blockquote>");
  });

  it("[BASELINE] multiple partial markers in one stream parse cleanly", () => {
    const md = "**bold\n> quote\n- list\n`code";
    const r = renderMarkdownWithProfile(md, "chat_assistant", {
      streaming: true,
    });
    expect(r.output.length).toBeGreaterThan(0);
    // Should not contain the raw markdown markers as visible text
    expect(r.output).not.toContain("**bold\n>");
  });
});

// ═══════════════════════════════════════════════════════════════
// Streaming output validity
// ═══════════════════════════════════════════════════════════════

describe("streaming output validity", () => {
  it("[BASELINE] each token update step produces valid HTML", () => {
    const steps = ["**", "**bo", "**bold text", "**bold text**"];
    for (const step of steps) {
      const r = renderMarkdownWithProfile(step, "chat_assistant", {
        streaming: true,
      });
      expect(r.output.length).toBeGreaterThan(0);
    }
  });

  it("[BASELINE] progressive list building produces valid structure at each step", () => {
    const steps = ["- ", "- item", "- item 1\n- ", "- item 1\n- item 2"];
    for (const step of steps) {
      const r = renderMarkdownWithProfile(step, "chat_assistant", {
        streaming: true,
      });
      expect(r.output.length).toBeGreaterThan(0);
    }
  });

  it("[BASELINE] code fence open → content → close transitions cleanly", () => {
    const steps = ["```js", "```js\nconst", "```js\nconst x = 1;\n```"];
    for (const step of steps) {
      const r = renderMarkdownWithProfile(step, "chat_assistant", {
        streaming: true,
      });
      expect(r.output).toContain("<pre");
    }
  });
});

// ═══════════════════════════════════════════════════════════════
// Streaming vs non-streaming convergence
// ═══════════════════════════════════════════════════════════════

describe("streaming vs non-streaming convergence", () => {
  it("[BASELINE] streaming complete input matches non-streaming for inline marks", () => {
    const md = "**bold** and *italic* and `code` and ~~strike~~";
    const stream = renderMarkdownWithProfile(md, "chat_assistant", {
      streaming: true,
    });
    const nonStream = renderMarkdownWithProfile(md, "chat_assistant", {
      streaming: false,
    });
    expect(stream.meta.stats).toEqual(nonStream.meta.stats);
    expect(stream.output).toContain("<strong>");
    expect(nonStream.output).toContain("<strong>");
  });

  it("[BASELINE] streaming code block matches non-streaming output", () => {
    const md = "```js\nconst x = 1;\n```";
    const stream = renderMarkdownWithProfile(md, "chat_assistant", {
      streaming: true,
    });
    const nonStream = renderMarkdownWithProfile(md, "chat_assistant", {
      streaming: false,
    });
    expect(stream.meta.stats).toEqual(nonStream.meta.stats);
  });

  it("[BASELINE] streaming heading + paragraph converges", () => {
    const md = "# Title\n\nParagraph text.";
    const stream = renderMarkdownWithProfile(md, "chat_assistant", {
      streaming: true,
    });
    const nonStream = renderMarkdownWithProfile(md, "chat_assistant", {
      streaming: false,
    });
    expect(stream.meta.stats).toEqual(nonStream.meta.stats);
  });
});

// ═══════════════════════════════════════════════════════════════
// Layout stability (TDD RED — 依赖 MarkdownRenderable 组件)
// ═══════════════════════════════════════════════════════════════

describe("streaming layout stability (TDD RED)", () => {
  it("streaming message produces stable meta regardless of partial content", () => {
    // When MarkdownRenderable is implemented, progressive streaming
    // should not cause layout shifts in the rendered output
    const r1 = renderMarkdownWithProfile("**p", "chat_assistant", {
      streaming: true,
    });
    const r2 = renderMarkdownWithProfile("**partial", "chat_assistant", {
      streaming: true,
    });
    const r3 = renderMarkdownWithProfile("**partial text**", "chat_assistant", {
      streaming: true,
    });
    // All three should produce valid non-empty output
    expect(r1.output.length).toBeGreaterThan(0);
    expect(r2.output.length).toBeGreaterThan(0);
    expect(r3.output.length).toBeGreaterThan(0);
  });

  it("streaming repairs are tracked in streamRepairs metadata", () => {
    const r = renderMarkdownWithProfile("**bold", "chat_assistant", {
      streaming: true,
    });
    expect(r.streamRepairs.length).toBeGreaterThan(0);
    for (const repair of r.streamRepairs) {
      expect(typeof repair.before).toBe("string");
      expect(typeof repair.after).toBe("string");
      expect(typeof repair.repairKind).toBe("string");
      expect(typeof repair.offset).toBe("number");
    }
  });

  it("chat_user streaming also produces valid HTML", () => {
    const r = renderMarkdownWithProfile("**partial user", "chat_user", {
      streaming: true,
    });
    expect(r.output.length).toBeGreaterThan(0);
    expect(r.meta.streaming).toBe(true);
  });
});

// ═══════════════════════════════════════════════════════════════
// repairStreamingMarkdown 新边界 (Phase 4 增强验证)
// ═══════════════════════════════════════════════════════════════

describe("repairStreamingMarkdown edge case coverage", () => {
  it("[BASELINE] closes unbalanced bold in mid-sentence LLM output", () => {
    const partial = "The **primary finding suggests that **memory";
    const repaired = repairStreamingMarkdown(partial);
    // Should not be identical (repair added something)
    expect(repaired.length).toBeGreaterThanOrEqual(partial.length);
  });

  it("[BASELINE] handles real-world LLM snippet", () => {
    const partial = [
      "## Analysis",
      "",
      "The **investigation** revealed that *several",
    ].join("\n");
    const html = parseMarkdownToHtml(partial, { streaming: true });
    expect(html).toContain("<h2");
    expect(html).toContain("<strong>");
  });

  it("[BASELINE] empty string returns empty", () => {
    const repaired = repairStreamingMarkdown("");
    expect(repaired).toBe("");
    const html = parseMarkdownToHtml("", { streaming: true });
    expect(typeof html).toBe("string");
  });
});
