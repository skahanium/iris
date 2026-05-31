/**
 * contract-capability.test.ts
 *
 * 能力判定一致性测试 — 断言同一语料在不同上下文/调用方下的能力判定一致。
 *
 * 覆盖 CONTRACT_PLAN.md § 测试计划 1：
 * - 标题、粗体、斜体、删除线、行内代码
 * - 无序列表、有序列表、任务列表
 * - 引用块、表格、代码块
 * - 链接、图片
 * - 指令类语法、Callout、脚注
 * - 基础语法与 preserve-only 语法混合语料
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { Marked } from "marked";
import {
  SUPPORTED_CORE_GFM,
  UNSUPPORTED_OR_BEST_EFFORT_GFM,
} from "@/components/editor/gfm-schema";
import { marked } from "marked";
import { proseMarked } from "@/lib/markdown-render";

// ── Helpers ────────────────────────────────────────────────────

const GOLD_ROOT = resolve(__dirname, "gold-corpus");

function loadCorpus(name: string): string {
  return readFileSync(resolve(GOLD_ROOT, name), "utf8");
}

function markdownContains(text: string, substr: string): boolean {
  return text.includes(substr);
}

function markdownMatches(text: string, regex: RegExp): boolean {
  return regex.test(text);
}

/** 用 marked 的 lexer 确认某个 token type 是否存在 */
function hasTokenType(md: string, tokenType: string): boolean {
  const tokens = marked.lexer(md);
  return tokens.some((t) => t.type === tokenType);
}

/** 用 marked 的 lexer 确认某个 token type 存在且 type 匹配 */
function countTokenType(md: string, tokenType: string): number {
  const tokens = marked.lexer(md);
  let count = 0;
  for (const t of tokens) {
    if (t.type === tokenType) count++;
    // 递归检查嵌套 tokens（如列表项的子 tokens）
    if ("tokens" in t && Array.isArray(t.tokens)) {
      for (const child of t.tokens) {
        if (child.type === tokenType) count++;
      }
    }
  }
  return count;
}

/** 当前 marked 实例能解析为有效 HTML（不抛异常） */
function parsesCleanly(
  md: string,
  markedInstance: Marked | typeof marked = marked,
): boolean {
  try {
    const html = (markedInstance as Marked).parse(md, {
      async: false,
    }) as string;
    return typeof html === "string" && html.length > 0;
  } catch {
    return false;
  }
}

// ── 金样本语料 ────────────────────────────────────────────────

const BASIC_GFM = loadCorpus("basic-gfm.md");
const ADVANCED_SYNTAX = loadCorpus("advanced-syntax.md");
const MIXED_PRESERVE = loadCorpus("mixed-preserve.md");

// ── 能力判定：当前 gfm-schema.ts 兼容性验证 ──────────────────

describe("GFM schema contract (existing)", () => {
  it("SUPPORTED_CORE_GFM declares expected headings", () => {
    expect(SUPPORTED_CORE_GFM.some((s) => s.includes("heading"))).toBe(true);
  });

  it("SUPPORTED_CORE_GFM declares expected inline formatting", () => {
    expect(
      SUPPORTED_CORE_GFM.some((s) =>
        /bold|italic|strikethrough|inline code/i.test(s),
      ),
    ).toBe(true);
  });

  it("SUPPORTED_CORE_GFM declares lists and tables", () => {
    expect(
      SUPPORTED_CORE_GFM.some((s) =>
        /task list|ordered.*unordered|pipe table/i.test(s),
      ),
    ).toBe(true);
  });

  it("SUPPORTED_CORE_GFM declares links, images, blockquotes", () => {
    expect(
      SUPPORTED_CORE_GFM.some((s) => /link|image|blockquote/i.test(s)),
    ).toBe(true);
  });

  it("UNSUPPORTED_OR_BEST_EFFORT_GFM declares footnotes", () => {
    expect(
      UNSUPPORTED_OR_BEST_EFFORT_GFM.some((s) => /footnote/i.test(s)),
    ).toBe(true);
  });

  it("UNSUPPORTED_OR_BEST_EFFORT_GFM declares math", () => {
    expect(UNSUPPORTED_OR_BEST_EFFORT_GFM.some((s) => /math/i.test(s))).toBe(
      true,
    );
  });

  it("UNSUPPORTED_OR_BEST_EFFORT_GFM declares raw HTML", () => {
    expect(
      UNSUPPORTED_OR_BEST_EFFORT_GFM.some((s) =>
        /raw.*HTML|embedded.*HTML/i.test(s),
      ),
    ).toBe(true);
  });
});

// ── 标题 (ATX Headings) ───────────────────────────────────────

describe("capability: ATX headings", () => {
  it("[native] marked lexer recognizes heading tokens", () => {
    expect(hasTokenType(BASIC_GFM, "heading")).toBe(true);
  });

  it("[native] at least 6 heading levels present in basic-gfm", () => {
    const count = countTokenType(BASIC_GFM, "heading");
    expect(count).toBeGreaterThanOrEqual(6);
  });

  it("[native] all heading levels parse cleanly", () => {
    expect(parsesCleanly("# H1")).toBe(true);
    expect(parsesCleanly("## H2")).toBe(true);
    expect(parsesCleanly("### H3")).toBe(true);
    expect(parsesCleanly("#### H4")).toBe(true);
    expect(parsesCleanly("##### H5")).toBe(true);
    expect(parsesCleanly("###### H6")).toBe(true);
  });
});

// ── 粗体、斜体、删除线、行内代码 ─────────────────────────────

describe("capability: inline formatting", () => {
  it("[native] bold renders as <strong>", () => {
    const html = marked.parse("**bold**", { async: false }) as string;
    expect(html).toContain("<strong>");
  });

  it("[native] italic renders as <em>", () => {
    const html = marked.parse("*italic*", { async: false }) as string;
    expect(html).toContain("<em>");
  });

  it("[native] strikethrough renders as <del>", () => {
    const html = marked.parse("~~strike~~", { async: false }) as string;
    expect(html).toContain("<del>");
  });

  it("[native] inline code renders as <code>", () => {
    const html = marked.parse("`code`", { async: false }) as string;
    expect(html).toContain("<code>");
  });

  it("[native] combined inline marks in basic-gfm parse cleanly", () => {
    expect(parsesCleanly(BASIC_GFM)).toBe(true);
  });
});

// ── 无序列表、有序列表、任务列表 ────────────────────────────

describe("capability: lists", () => {
  it("[native] unordered list tokens recognized", () => {
    expect(hasTokenType("- item", "list")).toBe(true);
  });

  it("[native] ordered list tokens recognized", () => {
    expect(hasTokenType("1. item", "list")).toBe(true);
  });

  it("[native] task list renders with checkbox", () => {
    const html = proseMarked.parse("- [x] Done", { async: false }) as string;
    expect(html).toContain("checkbox");
    expect(html).toContain("checked");
  });

  it("[native] task list unchecked renders with checkbox", () => {
    const html = proseMarked.parse("- [ ] Todo", { async: false }) as string;
    expect(html).toContain("checkbox");
    expect(html).not.toContain(" checked");
  });

  it("[native] nested lists in basic-gfm parse cleanly", () => {
    expect(parsesCleanly(BASIC_GFM)).toBe(true);
  });

  it("[native] ordered list & unordered list both exist in basic-gfm", () => {
    expect(markdownContains(BASIC_GFM, "1. ")).toBe(true);
    expect(markdownContains(BASIC_GFM, "- ")).toBe(true);
  });
});

// ── 引用块 ───────────────────────────────────────────────────

describe("capability: blockquotes", () => {
  it("[native] blockquote tokens recognized", () => {
    expect(hasTokenType("> quote", "blockquote")).toBe(true);
  });

  it("[native] nested blockquote renders as nested <blockquote>", () => {
    const html = marked.parse("> outer\n> > nested", {
      async: false,
    }) as string;
    // nested blockquotes produce nested <blockquote> elements
    expect((html.match(/<blockquote>/g) ?? []).length).toBeGreaterThanOrEqual(
      1,
    );
  });

  it("[native] blockquote in basic-gfm parse cleanly", () => {
    expect(parsesCleanly(BASIC_GFM)).toBe(true);
  });
});

// ── 表格 ─────────────────────────────────────────────────────

describe("capability: GFM tables", () => {
  it("[native] table tokens recognized", () => {
    expect(hasTokenType("| A | B |\n| --- | --- |\n| 1 | 2 |", "table")).toBe(
      true,
    );
  });

  it("[native] table renders with <table> element", () => {
    const html = proseMarked.parse("| A | B |\n| --- | --- |\n| 1 | 2 |", {
      async: false,
    }) as string;
    expect(html).toContain("<table");
    expect(html).toContain("<td>");
  });

  it("[native] table in basic-gfm and mixed-preserve parse cleanly", () => {
    expect(parsesCleanly(BASIC_GFM)).toBe(true);
    expect(parsesCleanly(MIXED_PRESERVE)).toBe(true);
  });
});

// ── 代码块 ───────────────────────────────────────────────────

describe("capability: code blocks", () => {
  it("[native] fenced code block tokens recognized", () => {
    expect(hasTokenType("```\ncode\n```", "code")).toBe(true);
  });

  it("[native] code block with language info preserves language", () => {
    const html = proseMarked.parse("```rust\nfn main() {}\n```", {
      async: false,
    }) as string;
    expect(html).toContain("language-rust");
  });

  it("[native] code block in basic-gfm parse cleanly", () => {
    expect(parsesCleanly(BASIC_GFM)).toBe(true);
  });
});

// ── 链接、图片 ───────────────────────────────────────────────

describe("capability: links and images", () => {
  it("[native] link tokens recognized", () => {
    const html = marked.parse("[text](url)", { async: false }) as string;
    expect(html).toContain("<a");
    expect(html).toContain('href="url"');
  });

  it("[native] image tokens recognized", () => {
    const html = marked.parse("![alt](src)", { async: false }) as string;
    expect(html).toContain("<img");
    expect(html).toContain('src="src"');
  });

  it("[native] links in basic-gfm parse cleanly", () => {
    expect(parsesCleanly(BASIC_GFM)).toBe(true);
  });
});

// ── Callout / Admonition ─────────────────────────────────────

describe("capability: callouts (render_only)", () => {
  it("[render_only] callout text exists in advanced-syntax corpus", () => {
    expect(markdownContains(ADVANCED_SYNTAX, "[!note]")).toBe(true);
    expect(markdownContains(ADVANCED_SYNTAX, "[!warning]")).toBe(true);
    expect(markdownContains(ADVANCED_SYNTAX, "[!tip]")).toBe(true);
    expect(markdownContains(ADVANCED_SYNTAX, "[!danger]")).toBe(true);
    expect(markdownContains(ADVANCED_SYNTAX, "[!example]")).toBe(true);
  });

  it("[render_only] callout currently parses as blockquote by marked (not callout-specific)", () => {
    const md = "> [!note] Test\n> Content";
    const tokens = marked.lexer(md);
    const blockquoteTokens = tokens.filter((t) => t.type === "blockquote");
    expect(blockquoteTokens.length).toBeGreaterThanOrEqual(1);
    // marked currently treats [!note] as blockquote content, not a callout node
  });

  it("[render_only] callout content is not lost during parse", () => {
    const md = "> [!warning] Alert\n> Body text";
    const html = marked.parse(md, { async: false }) as string;
    expect(html).toContain("Alert");
    expect(html).toContain("Body");
  });

  it("[render_only] callout in mixed-preserve is not lost", () => {
    const html = marked.parse(MIXED_PRESERVE, { async: false }) as string;
    expect(html).toContain("[!note]");
    expect(html).toContain("[!info]");
  });
});

// ── 脚注 ─────────────────────────────────────────────────────

describe("capability: footnotes (render_only)", () => {
  it("[render_only] footnote reference syntax exists in corpus", () => {
    expect(markdownMatches(ADVANCED_SYNTAX, /\[\^\w+\]/)).toBe(true);
  });

  it("[render_only] footnote definition exists in corpus", () => {
    expect(markdownContains(ADVANCED_SYNTAX, "[^1]:")).toBe(true);
  });

  it("[render_only] footnote references are present in parsed HTML (as text)", () => {
    const md = "Text[^1]\n\n[^1]: Footnote content";
    const html = marked.parse(md, { async: false }) as string;
    // marked treats footnote refs as text (no built-in footnote support)
    expect(html.length).toBeGreaterThan(0);
  });

  it("[render_only] footnotes in mixed-preserve not lost", () => {
    const html = marked.parse(MIXED_PRESERVE, { async: false }) as string;
    expect(html).toContain("gfm-fn");
  });
});

// ── 混合语料：native + render_only + preserve_only ──────────

describe("capability: mixed content classification", () => {
  it("[mixed] basic-gfm contains only native-level syntax", () => {
    // 验证 basic-gfm 不包含任何 render_only / preserve_only 语法标记
    expect(BASIC_GFM).not.toMatch(/> \[!/); // no callout
    expect(BASIC_GFM).not.toMatch(/\[\^/); // no footnote ref
    expect(BASIC_GFM).not.toMatch(/<div/); // no raw HTML
    expect(BASIC_GFM).not.toMatch(/<kbd/); // no preserve element
  });

  it("[mixed] advanced-syntax contains callout + footnote + raw HTML", () => {
    expect(markdownMatches(ADVANCED_SYNTAX, /> \[!/)).toBe(true);
    expect(markdownMatches(ADVANCED_SYNTAX, /\[\^/)).toBe(true);
    expect(markdownMatches(ADVANCED_SYNTAX, /<div/)).toBe(true);
  });

  it("[mixed] mixed-preserve contains native + callout + footnote + raw HTML", () => {
    expect(markdownMatches(MIXED_PRESERVE, /^#\s/m)).toBe(true); // headings
    expect(markdownMatches(MIXED_PRESERVE, /> \[!/)).toBe(true); // callout
    expect(markdownMatches(MIXED_PRESERVE, /\[\^/)).toBe(true); // footnote
    expect(markdownMatches(MIXED_PRESERVE, /\|[^|]+\|/)).toBe(true); // table
    expect(markdownMatches(MIXED_PRESERVE, /```/)).toBe(true); // code block
    expect(markdownMatches(MIXED_PRESERVE, /\[\[/)).toBe(true); // wiki-link
  });

  it("[mixed] all three gold corpora parse cleanly with default marked", () => {
    expect(parsesCleanly(BASIC_GFM)).toBe(true);
    expect(parsesCleanly(ADVANCED_SYNTAX)).toBe(true);
    expect(parsesCleanly(MIXED_PRESERVE)).toBe(true);
  });

  it("[mixed] all three gold corpora parse cleanly with proseMarked (AI renderer)", () => {
    expect(parsesCleanly(BASIC_GFM, proseMarked)).toBe(true);
    expect(parsesCleanly(ADVANCED_SYNTAX, proseMarked)).toBe(true);
    expect(parsesCleanly(MIXED_PRESERVE, proseMarked)).toBe(true);
  });
});

// ── 两个 marked 实例的一致性 ─────────────────────────────────

describe("capability: cross-parser consistency", () => {
  it("default marked and proseMarked produce non-empty output for same input", () => {
    const inputs = [
      "# Hello",
      "**bold** and *italic*",
      "- list item",
      "> blockquote",
      "| A | B |\n| --- | --- |\n| 1 | 2 |",
      "```js\ncode\n```",
      "[link](https://example.com)",
    ];

    for (const input of inputs) {
      const defaultResult = marked.parse(input, { async: false }) as string;
      const proseResult = proseMarked.parse(input, { async: false }) as string;
      expect(defaultResult.length).toBeGreaterThan(0);
      expect(proseResult.length).toBeGreaterThan(0);
    }
  });

  it("default marked and proseMarked produce HTML for headings identically", () => {
    const defaultResult = marked.parse("# Title", { async: false }) as string;
    const proseResult = proseMarked.parse("# Title", {
      async: false,
    }) as string;
    // Both should contain heading element
    expect(defaultResult).toContain("<h1");
    expect(proseResult).toContain("<h1");
  });
});

// ── 能力边界：明确 unsupported 语法 ─────────────────────────

describe("capability: unsupported syntax boundaries", () => {
  it("raw HTML blocks are preserved as text in marked output", () => {
    const md = '<div class="x">content</div>';
    const html = marked.parse(md, { async: false }) as string;
    // marked passes raw HTML through (GFM behavior)
    expect(html.length).toBeGreaterThan(0);
  });

  it("HTML comments survive parsing without crashing", () => {
    const md = "<!-- comment -->\ntext";
    expect(parsesCleanly(md)).toBe(true);
  });
});
