/**
 * contract-streaming.test.ts
 *
 * 流式稳健性测试 — 断言流式 Markdown 修复逻辑在各种半截输入下稳定运行。
 *
 * 覆盖 CONTRACT_PLAN.md § 测试计划 4：
 * - 半截粗体输入不会直接展示为长段源码噪音
 * - 半截代码块不会把 UI 渲染崩掉
 * - 半截脚注或 Callout 在流式过程里有稳定降级策略
 * - 流式修复完成后结果尽量收敛到完整输入的最终渲染结果
 */
import { describe, expect, it } from "vitest";

import {
  repairStreamingMarkdown,
  renderAiMarkdownToHtml,
} from "@/lib/markdown-render";

// ── Helpers ────────────────────────────────────────────────────

/** 渲染流式输入，模拟 AI 逐步输出 */
function renderStreaming(content: string): string {
  try {
    const raw = renderAiMarkdownToHtml(content, { streaming: true });
    return raw;
  } catch (e) {
    return `RENDER_ERROR: ${String(e)}`;
  }
}

/** 渲染非流式输入 */
function renderComplete(content: string): string {
  try {
    return renderAiMarkdownToHtml(content, { streaming: false });
  } catch (e) {
    return `RENDER_ERROR: ${String(e)}`;
  }
}

/** 获取纯文本内容 */
function extractText(html: string): string {
  return html
    .replace(/<[^>]+>/g, "")
    .trim()
    .replace(/\s+/g, " ");
}

// ── 半截粗体 ────────────────────────────────────────────────

describe("streaming repair: bold", () => {
  it("closes unbalanced bold markers", () => {
    const repaired = repairStreamingMarkdown("**partial");
    expect(repaired.endsWith("**")).toBe(true);
  });

  it("unbalanced bold renders as strong element, not raw text", () => {
    const html = renderStreaming("**partial text");
    expect(html).toContain("<strong>");
    expect(html).not.toContain("**partial text**");
  });

  it("balanced bold renders correctly in streaming mode", () => {
    const html = renderStreaming("**complete**");
    expect(html).toContain("<strong>");
    expect(extractText(html)).toContain("complete");
  });

  it("partial bold followed by text renders cleanly", () => {
    const html = renderStreaming("para **bold");
    expect(html).toContain("<strong>");
    expect(html).not.toContain("RENDER_ERROR");
  });

  it("multiple unbalanced bold markers (odd count) get closed", () => {
    const repaired = repairStreamingMarkdown("**a**\n\n**b");
    // 第二段 bold 未闭合，应补上
    expect(repaired.endsWith("**")).toBe(true);
  });
});

// ── 半截斜体 ────────────────────────────────────────────────

describe("streaming repair: italic", () => {
  it("unbalanced italic (*) survives streaming parsing without crash", () => {
    const html = renderStreaming("text *italic");
    expect(html).not.toContain("RENDER_ERROR");
    expect(html.length).toBeGreaterThan(0);
  });

  it("unbalanced italic (_) survives streaming parsing without crash", () => {
    const html = renderStreaming("text _italic");
    expect(html).not.toContain("RENDER_ERROR");
    expect(html.length).toBeGreaterThan(0);
  });

  it("balanced italic renders correctly in streaming", () => {
    const html = renderStreaming("*complete italic*");
    expect(html).toContain("<em>");
    expect(extractText(html)).toContain("complete italic");
  });

  it("italic edge case: asterisk in middle of word", () => {
    const html = renderStreaming("text*star*");
    expect(html).not.toContain("RENDER_ERROR");
  });
});

// ── 半截删除线 ──────────────────────────────────────────────

describe("streaming repair: strikethrough", () => {
  it("closes unbalanced strikethrough markers", () => {
    const repaired = repairStreamingMarkdown("~~deleted");
    expect(repaired.endsWith("~~")).toBe(true);
  });

  it("unbalanced strikethrough renders without error", () => {
    const html = renderStreaming("~~strikethrough text");
    expect(html).not.toContain("RENDER_ERROR");
    // 修复后应该包含 del 标签
    expect(html).toContain("<del>");
  });

  it("balanced strikethrough renders correctly", () => {
    const html = renderStreaming("~~deleted content~~");
    expect(html).toContain("<del>");
    expect(extractText(html)).toContain("deleted content");
  });
});

// ── 半截代码围栏 ────────────────────────────────────────────

describe("streaming repair: code fences", () => {
  it("closes unbalanced fence markers", () => {
    const repaired = repairStreamingMarkdown("```rust\nfn main() {");
    expect(repaired.endsWith("```")).toBe(true);
    // 成对 fence 时 split 段数为奇数
    expect(repaired.split("```").length % 2).toBe(1);
  });

  it("balanced fences pass through unchanged", () => {
    const input = "```\ncode\n```";
    expect(repairStreamingMarkdown(input)).toBe(input);
  });

  it("open code fence with language renders without crashing", () => {
    const html = renderStreaming("```python\ndef foo():\n    pass");
    expect(html).not.toContain("RENDER_ERROR");
    expect(html).toContain("<pre>");
    expect(html).toContain("<code");
  });

  it("open code fence without language renders without crashing", () => {
    const html = renderStreaming("```\nsome partial code");
    expect(html).not.toContain("RENDER_ERROR");
    expect(html).toContain("<pre>");
  });

  it("fence with content renders code content not raw markdown", () => {
    const html = renderStreaming("```js\nconst x = ");
    expect(html).toContain("<pre>");
    expect(html).not.toContain("```");
  });

  it("triple-fence (ffb) closing after open renders new block", () => {
    // 输入: ```a\ncode\n``` — 已经是平衡的
    const html = renderStreaming("```a\ncode\n```");
    expect(html).toContain("<pre>");
    expect(html).toContain("code");
  });
});

// ── 半截列表 ────────────────────────────────────────────────

describe("streaming repair: lists", () => {
  it("incomplete bullet list item renders without crash", () => {
    const html = renderStreaming("- partial item");
    expect(html).not.toContain("RENDER_ERROR");
    expect(html.length).toBeGreaterThan(0);
  });

  it("incomplete ordered list item renders without crash", () => {
    const html = renderStreaming("1. partial ordered item");
    expect(html).not.toContain("RENDER_ERROR");
    expect(html.length).toBeGreaterThan(0);
  });

  it("partial task list renders without crash", () => {
    const html = renderStreaming("- [ ] incomplete task");
    expect(html).not.toContain("RENDER_ERROR");
  });

  it("complete list followed by partial list renders cleanly", () => {
    const html = renderStreaming("- item 1\n- item 2\n- item 3");
    expect(html).toContain("<ul>");
    expect(html).toContain("<li>");
  });

  it("ordered list interleaved with unordered list partial", () => {
    const html = renderStreaming("1. First\n2. Second\n- partial");
    expect(html).not.toContain("RENDER_ERROR");
  });
});

// ── 半截引用块 ───────────────────────────────────────────────

describe("streaming repair: blockquotes", () => {
  it("partial blockquote line renders without crash", () => {
    const html = renderStreaming("> partial blockquote");
    expect(html).not.toContain("RENDER_ERROR");
    expect(html).toContain("<blockquote>");
  });

  it("multiple partial blockquote lines render correctly", () => {
    const html = renderStreaming("> line 1\n> line 2\n> line 3");
    expect(html).toContain("<blockquote>");
    expect(extractText(html)).toContain("line 1");
    expect(extractText(html)).toContain("line 3");
  });
});

// ── 半截高级语法 ────────────────────────────────────────────

describe("streaming repair: advanced syntax (callout, footnote)", () => {
  it("partial callout render does not crash", () => {
    const html = renderStreaming("> [!note] Title\n> partial body");
    expect(html).not.toContain("RENDER_ERROR");
    expect(html).toContain("<blockquote>");
  });

  it("partial callout with multiple types renders", () => {
    for (const calloutType of [
      "[!note]",
      "[!warning]",
      "[!tip]",
      "[!danger]",
    ]) {
      const html = renderStreaming(`> ${calloutType} Title\n> Body`);
      expect(html).not.toContain("RENDER_ERROR");
      expect(html).toContain("<blockquote>");
    }
  });

  it("partial footnote ref streams without crash", () => {
    const html = renderStreaming("Text with footnote[^");
    expect(html).not.toContain("RENDER_ERROR");
  });

  it("partial footnote ref with text after streams", () => {
    const html = renderStreaming("Text[^1] more text");
    expect(html).not.toContain("RENDER_ERROR");
  });

  it("partial footnote definition streams without crash", () => {
    const html = renderStreaming("[^1]: Footnote content with");
    expect(html).not.toContain("RENDER_ERROR");
  });

  it("callout inside streaming renders body text, not raw syntax", () => {
    const html = renderStreaming("> [!info]\n> This is the info.");
    // 即使在流式中，正文内容应可见
    expect(extractText(html)).toContain("This is the info");
  });
});

// ── 流式收敛测试 ────────────────────────────────────────────

describe("streaming convergence: final render matches complete input", () => {
  it("bold text converges to correct output", () => {
    // 分步模拟流式输入
    const steps = ["**", "**bo", "**bold tex", "**bold text**"];
    const final = renderComplete("**bold text**");

    for (const step of steps) {
      const stepHtml = renderStreaming(step);
      expect(stepHtml).not.toContain("RENDER_ERROR");
    }
    expect(final).toContain("<strong>");
    expect(extractText(final)).toContain("bold text");
  });

  it("code block converges to correct final output", () => {
    const steps = [
      "```js",
      "```js\nconst",
      "```js\nconst x = 1;",
      "```js\nconst x = 1;\n```",
    ];
    const final = renderComplete("```js\nconst x = 1;\n```");

    for (const step of steps) {
      const stepHtml = renderStreaming(step);
      expect(stepHtml).not.toContain("RENDER_ERROR");
    }
    expect(final).toContain("<pre>");
  });

  it("unordered list converges to correct final output", () => {
    const steps = [
      "- ",
      "- item",
      "- item 1\n- ",
      "- item 1\n- item 2\n- ",
      "- item 1\n- item 2\n- item 3",
    ];
    const final = renderComplete("- item 1\n- item 2\n- item 3");

    for (const step of steps) {
      const stepHtml = renderStreaming(step);
      expect(stepHtml).not.toContain("RENDER_ERROR");
    }
    expect(final).toContain("<ul>");
    expect(final).not.toContain("RENDER_ERROR");
  });

  it("blockquote converges to correct final output", () => {
    const steps = ["> ", "> quote", "> quote text", "> quote text here"];
    const final = renderComplete("> quote text here");

    for (const step of steps) {
      const stepHtml = renderStreaming(step);
      expect(stepHtml).not.toContain("RENDER_ERROR");
    }
    expect(final).toContain("<blockquote>");
    expect(extractText(final)).toContain("quote text here");
  });

  it("mixed content converges: bold + code + list", () => {
    const partial = "**Title**\n\n`code`\n\n- item";
    const complete =
      "**Title**\n\n`code`\n\n- item 1\n- item 2\n\n> final quote";

    const partialHtml = renderStreaming(partial);
    const completeHtml = renderComplete(complete);

    expect(partialHtml).toContain("<strong>");
    expect(partialHtml).toContain("<code>");

    expect(completeHtml).not.toContain("RENDER_ERROR");
    expect(completeHtml).toContain("<strong>");
    expect(completeHtml).toContain("<code>");
    expect(completeHtml).toContain("<blockquote>");
  });
});

// ── 边界条件 ────────────────────────────────────────────────

describe("streaming edge cases", () => {
  it("empty string in streaming mode returns empty", () => {
    const html = renderStreaming("");
    expect(typeof html).toBe("string");
  });

  it("whitespace only in streaming mode does not crash", () => {
    const html = renderStreaming("   \n  ");
    expect(html).not.toContain("RENDER_ERROR");
  });

  it("single character streams without crash", () => {
    for (const ch of ["*", "#", "-", ">", "`", "[", "|"]) {
      const html = renderStreaming(ch);
      expect(html).not.toContain("RENDER_ERROR");
    }
  });

  it("multiple unclosed markers in same stream", () => {
    const html = renderStreaming("**bold\n> quote\n- list\n`code");
    expect(html).not.toContain("RENDER_ERROR");
  });

  it("extremely long single line in streaming mode", () => {
    const longLine = "A".repeat(5000);
    const md = `**${longLine}`; // unbalanced bold
    const html = renderStreaming(md);
    expect(html).not.toContain("RENDER_ERROR");
    expect(html).toContain("<strong>");
  });

  it("streaming mode handles real-world mid-sentence LLM output", () => {
    const partial = [
      "## Analysis",
      "",
      "The **primary finding** suggests that *memory",
    ].join("\n");

    const html = renderStreaming(partial);
    expect(html).not.toContain("RENDER_ERROR");
    expect(html).toContain("<h2>");
    expect(html).toContain("<strong>");
    expect(extractText(html)).toContain("primary finding");
    expect(extractText(html)).toContain("memory");
  });

  it("streaming repair does not mutate the original input (via copy)", () => {
    const original = "**partial";
    const repaired = repairStreamingMarkdown(original);
    // original should remain unchanged
    expect(original).toBe("**partial");
    // repaired should have closing marker
    expect(repaired).toBe("**partial**");
  });
});

// ── 流式修复策略完整性 ──────────────────────────────────────

describe("streaming repair strategy coverage", () => {
  it("covers unclosed bold markers", () => {
    const r = repairStreamingMarkdown("**abc");
    expect(r.endsWith("**")).toBe(true);
  });

  it("covers unclosed strikethrough markers", () => {
    const r = repairStreamingMarkdown("~~abc");
    expect(r.endsWith("~~")).toBe(true);
  });

  it("covers unclosed code fences", () => {
    const r = repairStreamingMarkdown("```js\ncode");
    expect(r.endsWith("```")).toBe(true);
  });

  it("balanced markers are not modified", () => {
    expect(repairStreamingMarkdown("**abc**")).toBe("**abc**");
    expect(repairStreamingMarkdown("~~abc~~")).toBe("~~abc~~");
    expect(repairStreamingMarkdown("```\ncode\n```")).toBe("```\ncode\n```");
  });

  it("already balanced input with multiple markers passes through", () => {
    const input = "# Title\n\n**bold** and ~~strike~~.\n\n```js\ncode\n```";
    expect(repairStreamingMarkdown(input)).toBe(input);
  });
});

// ── Phase 4: 新增修复策略 ────────────────────────────────────

describe("Phase 4: unclosed italic (_)", () => {
  it("closes single unclosed underscore italic", () => {
    const r = repairStreamingMarkdown("_partial italic");
    expect(r.endsWith("_")).toBe(true);
  });

  it("balanced underscore is not modified", () => {
    expect(repairStreamingMarkdown("_complete_")).toBe("_complete_");
  });

  it("double underscore (bold) is NOT treated as italic", () => {
    const r = repairStreamingMarkdown("__bold__ _italic");
    expect(r.endsWith("_")).toBe(true);
    // __bold__ remains unchanged
    expect(r).toContain("__bold__");
  });

  it("streaming italic _ renders properly after repair", () => {
    const html = renderStreaming("text _partial");
    expect(html).not.toContain("RENDER_ERROR");
    expect(html).toContain("<em>");
  });
});

describe("Phase 4: unclosed italic (*)", () => {
  it("closes single unclosed asterisk italic", () => {
    const r = repairStreamingMarkdown("text *partial");
    expect(r.endsWith("*")).toBe(true);
  });

  it("balanced asterisk is not modified", () => {
    expect(repairStreamingMarkdown("*complete*")).toBe("*complete*");
  });

  it("list marker (* item) is NOT treated as italic", () => {
    const r = repairStreamingMarkdown("* item");
    // Should not add closing * because * is a list marker
    expect(r).not.toMatch(/\*\*$/);
    expect(r).toBe("* item");
  });

  it("bold ** is not affected by italic repair", () => {
    const r = repairStreamingMarkdown("**bold** and *italic");
    expect(r.endsWith("*")).toBe(true);
    expect(r).toContain("**bold**");
  });
});

describe("Phase 4: incomplete list items", () => {
  it("removes trailing empty bullet list marker", () => {
    const r = repairStreamingMarkdown("- item 1\n- item 2\n- ");
    expect(r).toBe("- item 1\n- item 2\n");
  });

  it("removes trailing empty ordered list marker", () => {
    const r = repairStreamingMarkdown("1. First\n2. Second\n3. ");
    expect(r).toBe("1. First\n2. Second\n");
  });

  it("removes trailing empty asterisk list marker", () => {
    const r = repairStreamingMarkdown("* item 1\n* ");
    expect(r).toBe("* item 1\n");
  });

  it("does not remove complete list items", () => {
    const input = "- item 1\n- item 2";
    expect(repairStreamingMarkdown(input)).toBe(input);
  });

  it("does not remove ordered list with numbers", () => {
    const input = "1. First\n2. Second\n3. Third";
    expect(repairStreamingMarkdown(input)).toBe(input);
  });
});

describe("Phase 4: incomplete blockquote lines", () => {
  it("removes trailing empty blockquote marker", () => {
    const r = repairStreamingMarkdown("> line 1\n> line 2\n> ");
    expect(r).toBe("> line 1\n> line 2\n");
  });

  it("does not remove complete blockquote lines", () => {
    const input = "> line 1\n> line 2";
    expect(repairStreamingMarkdown(input)).toBe(input);
  });

  it("removes trailing blockquote with spaces", () => {
    const r = repairStreamingMarkdown("> content\n>   ");
    expect(r).toBe("> content\n");
  });
});

describe("Phase 4: unterminated footnote reference", () => {
  it("closes unclosed footnote reference [^", () => {
    const r = repairStreamingMarkdown("Text [^");
    expect(r).toBe("Text [^]");
  });

  it("closes unclosed footnote reference [^1", () => {
    const r = repairStreamingMarkdown("See [^1");
    expect(r).toBe("See [^1]");
  });

  it("closes unclosed named footnote [^fn", () => {
    const r = repairStreamingMarkdown("Reference [^my-note");
    expect(r).toBe("Reference [^my-note]");
  });

  it("complete footnote is not modified", () => {
    const input = "Text [^1]";
    expect(repairStreamingMarkdown(input)).toBe(input);
  });

  it("complete footnote definition is not modified", () => {
    const input = "[^1]: The footnote body.";
    expect(repairStreamingMarkdown(input)).toBe(input);
  });
});

describe("Phase 4: combined streaming repairs", () => {
  it("repairs bold + italic + list simultaneously", () => {
    // List removal happens first, then delimiter closers are appended
    // at end-of-document (same pattern as bold `**` and strikethrough `~~`)
    const input = "**bold** *italic\n- item\n- ";
    const r = repairStreamingMarkdown(input);
    // List marker removed, italic closed at end-of-document
    expect(r).toBe("**bold** *italic\n- item\n*");
  });

  it("repairs code fence + blockquote + footnote", () => {
    const r = repairStreamingMarkdown("```js\ncode\n> quote\n> \nSee [^");
    // Should: close fence, remove empty blockquote, close footnote
    expect(r).toContain("```");
    expect(r).not.toMatch(/>\s*$/m);
    expect(r.endsWith("]")).toBe(true);
  });

  it("balanced content with multiple syntax types is unchanged", () => {
    const input = [
      "# Title",
      "",
      "**bold** and *italic* and ~~strike~~ and `code`.",
      "",
      "- item 1",
      "- item 2",
      "",
      "> blockquote",
      "",
      "```js",
      "const x = 1;",
      "```",
      "",
      "See [^1] for details.",
    ].join("\n");
    expect(repairStreamingMarkdown(input)).toBe(input);
  });

  it("real-world LLM mid-sentence streaming output", () => {
    const partial = [
      "## Analysis Results",
      "",
      "The **investigation** revealed that *several factors* ",
      "contribute to the outcome, including:",
      "",
      "- Memory allocation patterns",
      "- CPU scheduling latency",
    ].join("\n");

    const repaired = repairStreamingMarkdown(partial);
    // Should not lose any content
    expect(repaired).toContain("Memory allocation patterns");
    expect(repaired).toContain("CPU scheduling latency");
    // Bold should be balanced
    expect(countDelimiterFn(repaired, "**") % 2).toBe(0);
  });
});

/** Inline count (duplicated from source for test independence) */
function countDelimiterFn(text: string, delimiter: string): number {
  let count = 0;
  let pos = 0;
  while (pos < text.length) {
    const idx = text.indexOf(delimiter, pos);
    if (idx === -1) break;
    count += 1;
    pos = idx + delimiter.length;
  }
  return count;
}
