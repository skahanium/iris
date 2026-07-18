/**
 * editor-preserve-block.test.ts — PreserveBlock round-trip
 *
 * Ingest → production TipTap editor → PM serialize (`pmSerializeBody`).
 */
import { describe, expect, it } from "vitest";

import { ingestMarkdownForEditor } from "@/lib/editor-ingest";

import {
  createProductionEditorFromIngestedBody,
  pmSerializeBody,
} from "../helpers/tiptap-serialize-harness";

function serializeBody(md: string): string {
  const editor = createProductionEditorFromIngestedBody(md);
  try {
    return pmSerializeBody(editor);
  } finally {
    editor.destroy();
  }
}

// ═══════════════════════════════════════════════════════════════
// Ingest: PreserveBlock 生成
// ═══════════════════════════════════════════════════════════════

describe("ingestMarkdownForEditor produces PreserveBlock HTML", () => {
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
// Export: PreserveBlock 原文回吐（生产 PM 路径）
// ═══════════════════════════════════════════════════════════════

describe("PM serialize restores preserve original text", () => {
  it("preserveBlock originalRaw is restored in export", () => {
    const md = "before\n\n<div class='x'>raw</div>\n\nafter";
    const bodyMarkdown = serializeBody(md);
    expect(bodyMarkdown).toContain("before");
    expect(bodyMarkdown).toContain("<div class='x'>raw</div>");
    expect(bodyMarkdown).toContain("after");
  });

  it("preserveBlock content after native GFM is restored", () => {
    const md = "# Title\n\n**Bold**\n\n<!-- note -->\n\nAfter";
    const bodyMarkdown = serializeBody(md);
    expect(bodyMarkdown).toContain("Title");
    expect(bodyMarkdown).toContain("Bold");
    expect(bodyMarkdown).toContain("<!-- note -->");
    expect(bodyMarkdown).toContain("After");
  });

  it("multiple preserveBlocks are all restored in correct order", () => {
    const md = "start\n\n<div class='a'>A</div>\n\nmiddle\n\n<!-- B -->\n\nend";
    const bodyMarkdown = serializeBody(md);
    expect(bodyMarkdown).toContain("<div class='a'>A</div>");
    expect(bodyMarkdown).toContain("<!-- B -->");
    expect(bodyMarkdown).toContain("start");
    expect(bodyMarkdown).toContain("end");
    const idxA = bodyMarkdown.indexOf("<div class='a'>");
    const idxB = bodyMarkdown.indexOf("<!-- B -->");
    expect(idxA).toBeLessThan(idxB);
  });

  it("editor with no preserveBlocks works normally", () => {
    const md = "# Title\n\nContent.";
    const bodyMarkdown = serializeBody(md);
    expect(bodyMarkdown).toContain("Title");
    expect(bodyMarkdown).toContain("Content");
  });

  it("multiple inline preserve atoms are all restored", () => {
    const md = "<kbd>A</kbd> <kbd>B</kbd>";
    const bodyMarkdown = serializeBody(md);
    expect(bodyMarkdown).toContain("<kbd>A</kbd>");
    expect(bodyMarkdown).toContain("<kbd>B</kbd>");
  });
});

// ═══════════════════════════════════════════════════════════════
// 编辑安全性：相邻编辑不破坏 PreserveBlock
// ═══════════════════════════════════════════════════════════════

describe("PreserveBlock editing safety via production editor", () => {
  it("PreserveBlock HTML tag has data-original-raw attribute", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown: '<div class="x">raw</div>',
    });
    expect(result.tipTapHtml).toContain("data-original-raw");
  });

  it("PreserveBlock HTML tag has data-syntax-kind attribute", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown: '<div class="x">raw</div>',
    });
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
    expect(result.preserveFragments.length).toBeGreaterThanOrEqual(1);
    const nativeInResult = result.tipTapHtml.includes("<strong>");
    expect(nativeInResult).toBe(true);
  });

  it("export handles PreserveBlock interleaved with native elements", () => {
    const md =
      "before\n\n<div class='x'>raw</div>\n\nbetween\n\n<!-- comment -->\n\nafter";
    const bodyMarkdown = serializeBody(md);
    const posBefore = bodyMarkdown.indexOf("before");
    const posRaw = bodyMarkdown.indexOf("raw");
    const posBetween = bodyMarkdown.indexOf("between");
    const posComment = bodyMarkdown.indexOf("<!-- comment -->");
    const posAfter = bodyMarkdown.indexOf("after");
    expect(posBefore).toBeLessThan(posRaw);
    expect(posRaw).toBeLessThan(posBetween);
    expect(posBetween).toBeLessThan(posComment);
    expect(posComment).toBeLessThan(posAfter);
  });
});
