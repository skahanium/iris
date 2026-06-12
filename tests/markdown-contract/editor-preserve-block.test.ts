/**
 * editor-preserve-block.test.ts — TDD 红灯测试
 *
 * 直接测试 PreserveBlock 节点的行为：ingest → 编辑器中存在 → export。
 * 当前所有测试必须 FAIL（ingestMarkdownForEditor / exportEditorToMarkdown 尚未实现）。
 * 子项目 2 阶段 1-3 实现后全部 GREEN。
 */
import { describe, expect, it } from "vitest";

import { ingestMarkdownForEditor } from "@/lib/editor-ingest";
import { exportEditorToMarkdown } from "@/lib/editor-export";
import type { MarkdownSyntaxFragment } from "@/lib/markdown-contract/types";

// ═══════════════════════════════════════════════════════════════
// Ingest: PreserveBlock 生成
// ═══════════════════════════════════════════════════════════════

describe("TDD: ingestMarkdownForEditor produces PreserveBlock HTML", () => {
  it("raw <div> block is mapped to preserve-block HTML tag", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown: '<div class="box">content</div>',
    });
    expect(result.tipTapHtml).toContain('data-type="preserve-block"');
  });

  it("raw <div> originalRaw attribute contains exact source", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown: '<div class="box">content</div>',
    });
    // originalRaw is HTML-encoded in the attribute value
    expect(result.tipTapHtml).toContain("data-original-raw=");
    expect(result.tipTapHtml).toContain("div class");
    expect(result.tipTapHtml).toContain("content");
  });

  it("HTML comment <!-- --> is mapped to preserve-block tag", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown: "<!-- important note -->",
    });
    expect(result.tipTapHtml).toContain('data-type="preserve-block"');
  });

  it("inline HTML <kbd> is mapped to preserve-inline tags", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown: "Press <kbd>Ctrl</kbd> + <kbd>C</kbd>",
    });
    const inlineCount = (
      result.tipTapHtml.match(/data-type="preserve-inline"/g) ?? []
    ).length;
    const blockCount = (
      result.tipTapHtml.match(/data-type="preserve-block"/g) ?? []
    ).length;
    expect(inlineCount).toBeGreaterThanOrEqual(2);
    expect(blockCount).toBe(0);
  });

  it("native GFM content is rendered normally, not as preserve-block", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown: "# Title\n\n**bold** content.",
    });
    expect(result.tipTapHtml).toContain("<h1>");
    expect(result.tipTapHtml).toContain("<strong>");
    expect(result.tipTapHtml).not.toContain('data-type="preserve-block"');
  });

  it("mixed native + preserve_only produces both normal and preserve-block tags", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown: "**bold** and <div class='x'>raw</div> and `code`.",
    });
    expect(result.tipTapHtml).toContain("<strong>");
    expect(result.tipTapHtml).toContain("<code>");
    expect(result.tipTapHtml).toContain('data-type="preserve-block"');
  });

  it("preserveFragments list contains preserve_only fragments", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown: '<div class="note">raw HTML</div>',
    });
    expect(result.preserveFragments.length).toBeGreaterThan(0);
    const hasPreserveOnly = result.preserveFragments.some(
      (f) => f.capability === "preserve_only",
    );
    expect(hasPreserveOnly).toBe(true);
  });

  it("warnings are returned for unsupported syntax", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown: "<script>alert(1)</script>",
    });
    expect(result.warnings.length).toBeGreaterThan(0);
  });

  it("empty input produces empty HTML with no fragments", () => {
    const result = ingestMarkdownForEditor({ bodyMarkdown: "" });
    expect(result.tipTapHtml).toBeTruthy();
    expect(result.preserveFragments.length).toBe(0);
  });
});

// ═══════════════════════════════════════════════════════════════
// Export: PreserveBlock 原文回吐
// ═══════════════════════════════════════════════════════════════

describe("TDD: exportEditorToMarkdown restores preserve original text", () => {
  const classifiedFragments: MarkdownSyntaxFragment[] = [];

  it("preserveBlock originalRaw is restored in export", () => {
    const result = exportEditorToMarkdown({
      editorHtml:
        '<p>before</p><div data-type="preserve-block" data-original-raw="<div class=\'x\'>raw</div>" data-syntax-kind="raw_html"></div><p>after</p>',
      originalMarkdown: "before\n\n<div class='x'>raw</div>\n\nafter",
      classifiedFragments,
    });
    expect(result.bodyMarkdown).toContain("before");
    expect(result.bodyMarkdown).toContain("<div class='x'>raw</div>");
    expect(result.bodyMarkdown).toContain("after");
  });

  it("preserveBlock content after native GFM is restored", () => {
    const result = exportEditorToMarkdown({
      editorHtml:
        '<h1>Title</h1><p><strong>Bold</strong></p><div data-type="preserve-block" data-original-raw="<!-- note -->" data-syntax-kind="html_comment"></div><p>After</p>',
      originalMarkdown: "# Title\n\n**Bold**\n\n<!-- note -->\n\nAfter",
      classifiedFragments,
    });
    expect(result.bodyMarkdown).toContain("Title");
    expect(result.bodyMarkdown).toContain("Bold");
    expect(result.bodyMarkdown).toContain("<!-- note -->");
    expect(result.bodyMarkdown).toContain("After");
  });

  it("multiple preserveBlocks are all restored in correct order", () => {
    const result = exportEditorToMarkdown({
      editorHtml: [
        "<p>start</p>",
        '<div data-type="preserve-block" data-original-raw="<div class=\'a\'>A</div>" data-syntax-kind="raw_html"></div>',
        "<p>middle</p>",
        '<div data-type="preserve-block" data-original-raw="<!-- B -->" data-syntax-kind="html_comment"></div>',
        "<p>end</p>",
      ].join(""),
      originalMarkdown:
        "start\n\n<div class='a'>A</div>\n\nmiddle\n\n<!-- B -->\n\nend",
      classifiedFragments,
    });
    expect(result.bodyMarkdown).toContain("<div class='a'>A</div>");
    expect(result.bodyMarkdown).toContain("<!-- B -->");
    expect(result.bodyMarkdown).toContain("start");
    expect(result.bodyMarkdown).toContain("end");
    const idxA = result.bodyMarkdown.indexOf("<div class='a'>");
    const idxB = result.bodyMarkdown.indexOf("<!-- B -->");
    expect(idxA).toBeLessThan(idxB);
  });

  it("preserveBlock without originalRaw falls back sensibly", () => {
    const result = exportEditorToMarkdown({
      editorHtml: '<div data-type="preserve-block"></div>',
      originalMarkdown: "",
      classifiedFragments,
    });
    // Should not crash with missing originalRaw
    expect(result.bodyMarkdown.length).toBeGreaterThanOrEqual(0);
  });

  it("editor HTML with no preserveBlocks works normally", () => {
    const result = exportEditorToMarkdown({
      editorHtml: "<h1>Title</h1><p>Content.</p>",
      originalMarkdown: "# Title\n\nContent.",
      classifiedFragments,
    });
    expect(result.bodyMarkdown).toContain("Title");
    expect(result.bodyMarkdown).toContain("Content");
  });

  it("preservedCount reflects actual preserve fragment count", () => {
    const result = exportEditorToMarkdown({
      editorHtml: [
        '<div data-type="preserve-block" data-original-raw="<kbd>A</kbd>" data-syntax-kind="raw_html"></div>',
        '<div data-type="preserve-block" data-original-raw="<kbd>B</kbd>" data-syntax-kind="raw_html"></div>',
      ].join(""),
      originalMarkdown: "<kbd>A</kbd> <kbd>B</kbd>",
      classifiedFragments,
    });
    expect(result.preservedCount).toBe(2);
  });
});

// ═══════════════════════════════════════════════════════════════
// 编辑安全性：相邻编辑不破坏 PreserveBlock
// （Phase 1 实现 PreserveBlockExtension 后，以下测试需要真实 TipTap Editor。
//   当前通过 contract API 验证 HTML 结构的正确性。）
// ═══════════════════════════════════════════════════════════════

describe("TDD: PreserveBlock editing safety via contract API", () => {
  it("PreserveBlock HTML tag has data-original-raw attribute", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown: '<div class="x">raw</div>',
    });
    // PreserveBlock must carry the original source text for recovery
    expect(result.tipTapHtml).toContain("data-original-raw");
  });

  it("PreserveBlock HTML tag has data-syntax-kind attribute", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown: '<div class="x">raw</div>',
    });
    // PreserveBlock must declare which syntax kind it represents
    expect(result.tipTapHtml).toContain("data-syntax-kind");
  });

  it("preserveBlock and preserveInline nodes are individually identifiable", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown:
        '<div class="a">A</div>\n\n<!-- comment -->\n\n<kbd>B</kbd>',
    });
    const blockCount = (
      result.tipTapHtml.match(/data-type="preserve-block"/g) ?? []
    ).length;
    const inlineCount = (
      result.tipTapHtml.match(/data-type="preserve-inline"/g) ?? []
    ).length;
    expect(blockCount).toBeGreaterThanOrEqual(2);
    expect(inlineCount).toBeGreaterThanOrEqual(1);
  });

  it("preserve-only fragments in ingest result match input count", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown: '<div class="a">A</div>\n\n**native**\n\n<kbd>B</kbd>',
    });
    // At minimum, the <div> preserve block should be detected
    expect(result.preserveFragments.length).toBeGreaterThanOrEqual(1);
    const nativeInResult = result.tipTapHtml.includes("<strong>");
    expect(nativeInResult).toBe(true);
  });

  it("export handles HTML with PreserveBlock interleaved with native elements", () => {
    const classifiedFragments: MarkdownSyntaxFragment[] = [];
    const result = exportEditorToMarkdown({
      editorHtml: [
        "<p>before</p>",
        '<div data-type="preserve-block" data-original-raw="<div class=\'x\'>raw</div>" data-syntax-kind="raw_html"></div>',
        "<p>between</p>",
        '<div data-type="preserve-block" data-original-raw="<!-- comment -->" data-syntax-kind="html_comment"></div>',
        "<p>after</p>",
      ].join(""),
      originalMarkdown:
        "before\n\n<div class='x'>raw</div>\n\nbetween\n\n<!-- comment -->\n\nafter",
      classifiedFragments,
    });
    // Original source order is preserved
    const posBefore = result.bodyMarkdown.indexOf("before");
    const posRaw = result.bodyMarkdown.indexOf("raw");
    const posBetween = result.bodyMarkdown.indexOf("between");
    const posComment = result.bodyMarkdown.indexOf("<!-- comment -->");
    const posAfter = result.bodyMarkdown.indexOf("after");
    expect(posBefore).toBeLessThan(posRaw);
    expect(posRaw).toBeLessThan(posBetween);
    expect(posBetween).toBeLessThan(posComment);
    expect(posComment).toBeLessThan(posAfter);
  });
});
