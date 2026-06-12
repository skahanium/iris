/**
 * contract-serialize-preserved.test.ts — TDD 红灯测试
 *
 * 直接测试 serializePreservedMarkdown() 的行为规范。
 * 当前所有测试必须 FAIL（contract 尚未实现）。
 * 阶段 2.3 实现后，这些测试变为 GREEN。
 *
 * 覆盖 CONTRACT_PLAN.md § Preservation / Fallback：
 * - 含 preserve_only 片段的文档经 editor_export 后原文可恢复
 * - 含 render_only callout 的文档不丢失结构
 * - 含 render_only footnote 的文档完整回吐
 * - 高级语法与普通 GFM 混排时，普通语法不受影响
 */
import { describe, expect, it } from "vitest";

import {
  serializePreservedMarkdown,
  classifyMarkdownCapabilities,
} from "@/lib/markdown-contract/contract";

// ── Helpers ────────────────────────────────────────────────────

function roundTripPreserve(md: string): string {
  const fragments = classifyMarkdownCapabilities(md);
  return serializePreservedMarkdown(md, fragments);
}

// ── 单个 preserve_only 片段回吐 ──────────────────────────────

describe("serialize: single preserve_only fragments", () => {
  it("raw <div> is restored unchanged", () => {
    const original = '<div class="box">content</div>';
    const restored = roundTripPreserve(original);
    expect(restored).toBe(original);
  });

  it("<kbd> elements are preserved unchanged", () => {
    const original = "Press <kbd>Ctrl</kbd> + <kbd>C</kbd>";
    const restored = roundTripPreserve(original);
    expect(restored).toBe(original);
  });

  it("HTML comments are preserved unchanged", () => {
    const original = "<!-- important note -->";
    const restored = roundTripPreserve(original);
    expect(restored).toBe(original);
  });

  it("<details> element is preserved unchanged", () => {
    const original = "<details><summary>Toggle</summary>Content</details>";
    const restored = roundTripPreserve(original);
    expect(restored).toBe(original);
  });

  it("<span> with custom attributes is preserved unchanged", () => {
    const original = '<span class="custom" data-id="42">text</span>';
    const restored = roundTripPreserve(original);
    expect(restored).toContain('data-id="42"');
    expect(restored).toContain("text");
  });
});

// ── preserve_only + native 混合回吐 ──────────────────────────

describe("serialize: mixed preserve_only + native", () => {
  it("raw HTML beside native GFM preserves both", () => {
    const original =
      "# Title\n\n<div class='note'>HTML content</div>\n\n**Bold** paragraph.";
    const restored = roundTripPreserve(original);
    expect(restored).toContain("# Title");
    expect(restored).toContain("<div class='note'>HTML content</div>");
    expect(restored).toContain("**Bold**");
  });

  it("inline raw HTML in native GFM paragraph preserves both", () => {
    const original = "Normal text <mark>highlighted</mark> and **bold**.";
    const restored = roundTripPreserve(original);
    expect(restored).toContain("<mark>highlighted</mark>");
    expect(restored).toContain("**bold**");
  });

  it("multiple preserve_only blocks interleaved with native", () => {
    const original = [
      "## Section 1",
      "",
      "<div class='a'>block A</div>",
      "",
      "- native list item",
      "",
      "<div class='b'>block B</div>",
      "",
      "> native blockquote",
      "",
      "<div class='c'>block C</div>",
    ].join("\n");

    const restored = roundTripPreserve(original);

    // 所有 native 元素保留
    expect(restored).toContain("## Section 1");
    expect(restored).toContain("- native list item");
    expect(restored).toContain("> native blockquote");

    // 所有 preserve_only 元素保留
    expect(restored).toContain("<div class='a'>block A</div>");
    expect(restored).toContain("<div class='b'>block B</div>");
    expect(restored).toContain("<div class='c'>block C</div>");
  });
});

// ── render_only callout 回吐 ─────────────────────────────────

describe("serialize: render_only callout preservation", () => {
  it("[!note] callout is preserved exactly", () => {
    const original = "> [!note] Important\n> Body text.";
    const restored = roundTripPreserve(original);
    expect(restored).toContain("[!note]");
    expect(restored).toContain("Important");
    expect(restored).toContain("Body text");
  });

  it("callout with GFM inside is preserved", () => {
    const original = "> [!warning] Alert\n> - item 1\n> - item 2\n> `code`";
    const restored = roundTripPreserve(original);
    expect(restored).toContain("[!warning]");
    expect(restored).toContain("item 1");
    expect(restored).toContain("`code`");
  });

  it("callout with nested code block preserves structure", () => {
    const original = "> [!example] Code\n> ```js\n> const x = 1;\n> ```";
    const restored = roundTripPreserve(original);
    expect(restored).toContain("[!example]");
    expect(restored).toContain("```");
    expect(restored).toContain("const x");
  });

  it("multiple callouts preserve order and content", () => {
    const original = [
      "> [!note] First",
      "> First body.",
      "",
      "Normal paragraph.",
      "",
      "> [!warning] Second",
      "> Second body.",
    ].join("\n");

    const restored = roundTripPreserve(original);

    const firstIdx = restored.indexOf("[!note]");
    const secondIdx = restored.indexOf("[!warning]");
    expect(firstIdx).toBeGreaterThan(-1);
    expect(secondIdx).toBeGreaterThan(firstIdx);
    expect(restored).toContain("First body");
    expect(restored).toContain("Normal paragraph");
    expect(restored).toContain("Second body");
  });
});

// ── render_only footnote 回吐 ────────────────────────────────

describe("serialize: render_only footnote preservation", () => {
  it("single footnote preserves ref + definition", () => {
    const original = "Text[^1]\n\n[^1]: The footnote.";
    const restored = roundTripPreserve(original);
    expect(restored).toContain("[^1]");
    expect(restored).toContain("[^1]:");
    expect(restored).toContain("The footnote");
  });

  it("multiple named footnotes preserve all", () => {
    const original = [
      "See [^alpha] and [^beta].",
      "",
      "[^alpha]: First note.",
      "[^beta]: Second note.",
    ].join("\n");

    const restored = roundTripPreserve(original);

    expect(restored).toContain("[^alpha]");
    expect(restored).toContain("[^beta]");
    expect(restored).toContain("First note");
    expect(restored).toContain("Second note");
  });

  it("footnote inside callout is preserved", () => {
    const original = "> [!info] Note\n> Content[^fn]\n\n[^fn]: The footnote.";
    const restored = roundTripPreserve(original);
    expect(restored).toContain("Content[^fn]");
    expect(restored).toContain("[^fn]:");
  });
});

// ── 全 Mix：Callout + Footnote + Raw HTML + GFM ──────────────

describe("serialize: full mixed content round-trip", () => {
  it("all syntax types survive round-trip without loss", () => {
    const original = [
      "# Document Title",
      "",
      "**Bold** paragraph with `code`.",
      "",
      "> [!note] Callout",
      "> Body with footnote[^1].",
      "",
      "- [x] Native task",
      "- [ ] Another task",
      "",
      "| A | B |",
      "| --- | --- |",
      "| 1 | 2 |",
      "",
      "```ts",
      "const x = 1;",
      "```",
      "",
      "<div class='preserved'>Raw HTML</div>",
      "",
      "[^1]: The footnote content.",
    ].join("\n");

    const restored = roundTripPreserve(original);

    // Native
    expect(restored).toContain("# Document Title");
    expect(restored).toContain("**Bold**");
    expect(restored).toContain("`code`");
    expect(restored).toContain("- [x]");
    expect(restored).toContain("| A | B |");
    expect(restored).toContain("```");
    expect(restored).toContain("const x");

    // Render_only: callout
    expect(restored).toContain("[!note]");
    expect(restored).toContain("Body with footnote");

    // Render_only: footnote
    expect(restored).toContain("[^1]");
    expect(restored).toContain("The footnote content");

    // Preserve_only: raw HTML
    expect(restored).toContain("<div class='preserved'>Raw HTML</div>");
  });
});

// ── 边界条件 ──────────────────────────────────────────────────

describe("serialize: edge cases", () => {
  it("empty source produces empty output", () => {
    const restored = roundTripPreserve("");
    expect(restored).toBe("");
  });

  it("whitespace-only source is preserved", () => {
    const original = "\n  \n";
    const restored = roundTripPreserve(original);
    expect(restored).toBe(original);
  });

  it("only native content without preserves produces identical output", () => {
    const original = "# Title\n\n**Bold** only.";
    const restored = roundTripPreserve(original);
    expect(restored).toBe(original);
  });

  it("only preserve_only content produces identical output", () => {
    const original = "<div class='x'>only raw</div>";
    const restored = roundTripPreserve(original);
    expect(restored).toBe(original);
  });

  it("consecutive preserve_only blocks are separated correctly", () => {
    const original = "<div class='a'>A</div>\n\n<div class='b'>B</div>";
    const restored = roundTripPreserve(original);
    expect(restored).toBe(original);
  });
});
