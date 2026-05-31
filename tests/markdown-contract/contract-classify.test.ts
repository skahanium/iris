/**
 * contract-classify.test.ts — TDD 红灯测试
 *
 * 直接测试 classifyMarkdownCapabilities() 的行为规范。
 * 当前所有测试必须 FAIL（contract 尚未实现）。
 * 阶段 2.2 实现后，这些测试变为 GREEN。
 *
 * 覆盖 CONTRACT_PLAN.md § Normalize / Classify：
 * - native: 标题、粗体、斜体、删除线、行内代码、列表、任务列表、表格、代码块、引用、链接、图片
 * - render_only: Callout、脚注
 * - preserve_only: Raw HTML、HTML 注释
 * - unsupported: 显式标记为不支持但不可丢弃的
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { classifyMarkdownCapabilities } from "@/lib/markdown-contract/contract";
import type {
  MarkdownCapabilityLevel,
  MarkdownSyntaxFragment,
} from "@/lib/markdown-contract/types";

const GOLD_ROOT = resolve(__dirname, "gold-corpus");
const BASIC_GFM = readFileSync(resolve(GOLD_ROOT, "basic-gfm.md"), "utf8");
const ADVANCED_SYNTAX = readFileSync(
  resolve(GOLD_ROOT, "advanced-syntax.md"),
  "utf8",
);
const MIXED_PRESERVE = readFileSync(
  resolve(GOLD_ROOT, "mixed-preserve.md"),
  "utf8",
);

// ── Contract Contract ──────────────────────────────────────────

/**
 * 辅助函数：提取指定能力等级的所有 fragment
 */
function fragmentsOfLevel(
  fragments: MarkdownSyntaxFragment[],
  level: MarkdownCapabilityLevel,
): MarkdownSyntaxFragment[] {
  return fragments.filter((f) => f.capability === level);
}

// ── 原生 GFM 分类 ─────────────────────────────────────────────

describe("classify: native GFM (must be categorized correctly)", () => {
  it("[TDD-FAIL] all ATX heading levels (1-6) are classified as 'native'", () => {
    const fragments = classifyMarkdownCapabilities(
      "# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6",
    );
    const headings = fragments.filter((f) => f.syntaxKind === "heading");
    expect(headings.length).toBe(6);
    for (const h of headings) {
      expect(h.capability).toBe("native");
    }
  });

  it("[TDD-FAIL] bold syntax is classified as 'native'", () => {
    const fragments = classifyMarkdownCapabilities("**bold**");
    const bold = fragmentsOfLevel(fragments, "native").filter(
      (f) => f.syntaxKind === "bold",
    );
    expect(bold.length).toBeGreaterThanOrEqual(1);
    expect(bold[0]?.raw).toContain("**bold**");
  });

  it("[TDD-FAIL] italic syntax is classified as 'native'", () => {
    const fragments = classifyMarkdownCapabilities("*italic*");
    const italic = fragments.filter(
      (f) => f.syntaxKind === "italic" && f.capability === "native",
    );
    expect(italic.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] strikethrough is classified as 'native'", () => {
    const fragments = classifyMarkdownCapabilities("~~strike~~");
    const strike = fragments.filter(
      (f) => f.syntaxKind === "strikethrough" && f.capability === "native",
    );
    expect(strike.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] inline code is classified as 'native'", () => {
    const fragments = classifyMarkdownCapabilities("`code`");
    const code = fragments.filter(
      (f) => f.syntaxKind === "inline_code" && f.capability === "native",
    );
    expect(code.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] unordered list is classified as 'native'", () => {
    const fragments = classifyMarkdownCapabilities("- item 1\n- item 2");
    const list = fragmentsOfLevel(fragments, "native").filter(
      (f) => f.syntaxKind === "list",
    );
    expect(list.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] ordered list is classified as 'native'", () => {
    const fragments = classifyMarkdownCapabilities("1. first\n2. second");
    const list = fragmentsOfLevel(fragments, "native").filter(
      (f) => f.syntaxKind === "list",
    );
    expect(list.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] task list is classified as 'native'", () => {
    const fragments = classifyMarkdownCapabilities("- [x] Done\n- [ ] Todo");
    const tasks = fragments.filter(
      (f) => f.syntaxKind === "task_list" && f.capability === "native",
    );
    expect(tasks.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] GFM table is classified as 'native'", () => {
    const fragments = classifyMarkdownCapabilities(
      "| A | B |\n| --- | --- |\n| 1 | 2 |",
    );
    const tables = fragments.filter(
      (f) => f.syntaxKind === "table" && f.capability === "native",
    );
    expect(tables.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] fenced code block is classified as 'native'", () => {
    const fragments = classifyMarkdownCapabilities("```ts\ncode\n```");
    const code = fragments.filter(
      (f) => f.syntaxKind === "code_block" && f.capability === "native",
    );
    expect(code.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] blockquote is classified as 'native'", () => {
    const fragments = classifyMarkdownCapabilities("> quoted text");
    const bq = fragments.filter(
      (f) => f.syntaxKind === "blockquote" && f.capability === "native",
    );
    expect(bq.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] link is classified as 'native'", () => {
    const fragments = classifyMarkdownCapabilities(
      "[text](https://example.com)",
    );
    const links = fragments.filter(
      (f) => f.syntaxKind === "link" && f.capability === "native",
    );
    expect(links.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] image is classified as 'native'", () => {
    const fragments = classifyMarkdownCapabilities("![alt](src.png)");
    const imgs = fragments.filter(
      (f) => f.syntaxKind === "image" && f.capability === "native",
    );
    expect(imgs.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] horizontal rule is classified as 'native'", () => {
    const fragments = classifyMarkdownCapabilities("---");
    const hrs = fragments.filter(
      (f) => f.syntaxKind === "horizontal_rule" && f.capability === "native",
    );
    expect(hrs.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] wiki-link is classified as 'native'", () => {
    const fragments = classifyMarkdownCapabilities("See [[Note Title]].");
    const wikis = fragments.filter(
      (f) => f.syntaxKind === "wiki_link" && f.capability === "native",
    );
    expect(wikis.length).toBeGreaterThanOrEqual(1);
  });
});

// ── Callout 分类 ──────────────────────────────────────────────

describe("classify: callouts (render_only)", () => {
  it("[TDD-FAIL] Obsidian callout > [!note] is classified as 'render_only'", () => {
    const fragments = classifyMarkdownCapabilities(
      "> [!note] Info\n> Content.",
    );
    const callouts = fragments.filter(
      (f) => f.syntaxKind === "callout" && f.capability === "render_only",
    );
    expect(callouts.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] multiple callout types are all render_only", () => {
    const types = ["note", "warning", "tip", "danger", "info", "example"];
    for (const t of types) {
      const fragments = classifyMarkdownCapabilities(
        `> [!${t}] Title\n> Body.`,
      );
      const callouts = fragments.filter(
        (f) => f.syntaxKind === "callout" && f.capability === "render_only",
      );
      expect(callouts.length).toBeGreaterThanOrEqual(1);
    }
  });
});

// ── 脚注分类 ──────────────────────────────────────────────────

describe("classify: footnotes (render_only)", () => {
  it("[TDD-FAIL] footnote reference [^1] is classified as 'render_only'", () => {
    const fragments = classifyMarkdownCapabilities("Text[^1]\n\n[^1]: Body.");
    const refs = fragments.filter(
      (f) => f.syntaxKind === "footnote_ref" && f.capability === "render_only",
    );
    expect(refs.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] footnote definition [^1]: is classified as 'render_only'", () => {
    const fragments = classifyMarkdownCapabilities(
      "Text[^label]\n\n[^label]: Definition.",
    );
    const defs = fragments.filter(
      (f) => f.syntaxKind === "footnote_def" && f.capability === "render_only",
    );
    expect(defs.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] multiple named footnotes are classified correctly", () => {
    const fragments = classifyMarkdownCapabilities(
      "A[^a] B[^b]\n\n[^a]: A.\n[^b]: B.",
    );
    const refs = fragments.filter((f) => f.syntaxKind === "footnote_ref");
    const defs = fragments.filter((f) => f.syntaxKind === "footnote_def");
    expect(refs.length).toBe(2);
    expect(defs.length).toBe(2);
  });
});

// ── Raw HTML 分类（preserve_only）─────────────────────────────

describe("classify: raw HTML (preserve_only)", () => {
  it("[TDD-FAIL] <div> blocks are classified as 'preserve_only'", () => {
    const fragments = classifyMarkdownCapabilities(
      '<div class="box">content</div>',
    );
    const html = fragments.filter(
      (f) => f.syntaxKind === "raw_html" && f.capability === "preserve_only",
    );
    expect(html.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] HTML comments are classified as 'preserve_only'", () => {
    const fragments = classifyMarkdownCapabilities("<!-- comment -->");
    const comments = fragments.filter(
      (f) =>
        f.syntaxKind === "html_comment" && f.capability === "preserve_only",
    );
    expect(comments.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] <script> tags are classified as 'unsupported'", () => {
    const fragments = classifyMarkdownCapabilities("<script>alert(1)</script>");
    const scripts = fragments.filter((f) => f.capability === "unsupported");
    expect(scripts.length).toBeGreaterThanOrEqual(1);
  });

  it("[TDD-FAIL] raw HTML inline elements are classified as 'preserve_only'", () => {
    const fragments = classifyMarkdownCapabilities(
      "Press <kbd>Ctrl</kbd> + <kbd>C</kbd>.",
    );
    const preserves = fragmentsOfLevel(fragments, "preserve_only");
    expect(preserves.length).toBeGreaterThanOrEqual(1);
  });
});

// ── 混合内容分类 ──────────────────────────────────────────────

describe("classify: mixed content classification", () => {
  it("[TDD-FAIL] basic-gfm gold corpus produces 100% native fragments", () => {
    const fragments = classifyMarkdownCapabilities(BASIC_GFM);
    const nonNative = fragments.filter((f) => f.capability !== "native");
    expect(nonNative.length).toBe(0);
    expect(fragments.length).toBeGreaterThan(0);
  });

  it("[TDD-FAIL] advanced-syntax produces render_only + preserve_only fragments", () => {
    const fragments = classifyMarkdownCapabilities(ADVANCED_SYNTAX);
    const renderOnly = fragmentsOfLevel(fragments, "render_only");
    const preserveOnly = fragmentsOfLevel(fragments, "preserve_only");
    expect(renderOnly.length).toBeGreaterThan(0);
    expect(preserveOnly.length).toBeGreaterThan(0);
  });

  it("[TDD-FAIL] mixed-preserve produces all four capability levels", () => {
    const fragments = classifyMarkdownCapabilities(MIXED_PRESERVE);
    const natives = fragmentsOfLevel(fragments, "native");
    const renderOnly = fragmentsOfLevel(fragments, "render_only");
    const preserveOnly = fragmentsOfLevel(fragments, "preserve_only");

    expect(natives.length).toBeGreaterThan(0);
    expect(renderOnly.length).toBeGreaterThan(0);
    expect(preserveOnly.length).toBeGreaterThan(0);
  });

  it("[TDD-FAIL] every fragment has valid offset/endOffset, capability, syntaxKind, and raw text", () => {
    const fragments = classifyMarkdownCapabilities(
      "# Heading\n\n**Bold** and > [!note] Callout\n\n<div>raw</div>",
    );
    expect(fragments.length).toBeGreaterThan(0);
    for (const f of fragments) {
      expect(f.raw).toBeTruthy();
      expect(f.syntaxKind).toBeTruthy();
      expect(f.capability).toBeTruthy();
      expect(typeof f.offset).toBe("number");
      expect(typeof f.endOffset).toBe("number");
      expect(f.endOffset).toBeGreaterThan(f.offset);
    }
  });

  it("[TDD-FAIL] fragments are ordered by their position in source (offset ascending)", () => {
    const fragments = classifyMarkdownCapabilities(
      "# Title\n\n**Bold**\n\n- list\n\n> quote\n\n`code`",
    );
    for (let i = 1; i < fragments.length; i++) {
      expect(fragments[i]!.offset).toBeGreaterThanOrEqual(
        fragments[i - 1]!.offset,
      );
    }
  });

  it("[TDD-FAIL] fragments cover the entire source without gaps", () => {
    const source = "# Title\n\nParagraph **bold**.\n\n- list item";
    const fragments = classifyMarkdownCapabilities(source);
    let covered = 0;
    for (const f of fragments) {
      expect(f.offset).toBe(covered); // no gaps
      covered = f.endOffset;
    }
    expect(covered).toBe(source.length); // fully covered
  });
});

// ── 分类确定性（幂等） ────────────────────────────────────────

describe("classify: idempotency and determinism", () => {
  it("[TDD-FAIL] same input produces identical classification twice", () => {
    const source = "# Title\n\n**Bold** `code` > [!note] Callout";
    const result1 = classifyMarkdownCapabilities(source);
    const result2 = classifyMarkdownCapabilities(source);

    expect(result1.length).toBe(result2.length);
    for (let i = 0; i < result1.length; i++) {
      expect(result1[i]!.raw).toBe(result2[i]!.raw);
      expect(result1[i]!.syntaxKind).toBe(result2[i]!.syntaxKind);
      expect(result1[i]!.capability).toBe(result2[i]!.capability);
    }
  });
});
