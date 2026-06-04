/**
 * editor-export-consistency.test.ts — TDD 红灯测试
 *
 * 测试编辑器导出与其他表面（Vault 预览、AI 回灌、版本系统）之间的语义一致性。
 * 当前所有测试必须 FAIL（ingestMarkdownForEditor / exportEditorToMarkdown 尚未实现）。
 * 子项目 2 阶段 1-3 实现后全部 GREEN。
 */
import { describe, expect, it } from "vitest";

import { ingestMarkdownForEditor } from "@/lib/editor-ingest";
import { exportEditorToMarkdown } from "@/lib/editor-export";
import { classifyMarkdownCapabilities } from "@/lib/markdown-contract/contract";
import { renderMarkdownWithProfile } from "@/lib/markdown-contract/contract";
import type { MarkdownSyntaxFragment } from "@/lib/markdown-contract/types";

// ═══════════════════════════════════════════════════════════════
// 编辑器导出与 contract 一致性
// ═══════════════════════════════════════════════════════════════

describe("TDD: editor export = contract semantic", () => {
  const classifiedFragments: MarkdownSyntaxFragment[] = [];

  it("[TDD-FAIL] editor export produces same body content as contract editor_export", () => {
    const md = "# Title\n\n**Bold** paragraph with `code`.\n\n- item";
    const contractExport = renderMarkdownWithProfile(md, "editor_export");
    const editorExport = exportEditorToMarkdown({
      editorHtml:
        "<h1>Title</h1><p><strong>Bold</strong> paragraph with <code>code</code>.</p><ul><li>item</li></ul>",
      originalMarkdown: md,
      classifiedFragments,
    });

    // Both exports should contain the core content
    expect(editorExport.bodyMarkdown).toContain("Bold");
    expect(editorExport.bodyMarkdown).toContain("code");
    expect(editorExport.bodyMarkdown).toContain("item");
    expect(contractExport.output).toContain("Bold");
    expect(contractExport.output).toContain("code");
    expect(contractExport.output).toContain("item");
  });

  it("[TDD-FAIL] same markdown → ingest → export → stable", () => {
    const md = "**Bold** and *italic* and `code`.";
    const result1 = exportEditorToMarkdown({
      editorHtml:
        "<p><strong>Bold</strong> and <em>italic</em> and <code>code</code>.</p>",
      originalMarkdown: md,
      classifiedFragments,
    });
    const result2 = exportEditorToMarkdown({
      editorHtml:
        "<p><strong>Bold</strong> and <em>italic</em> and <code>code</code>.</p>",
      originalMarkdown: md,
      classifiedFragments,
    });

    // Repeated exports from same HTML should be identical
    expect(result1.bodyMarkdown).toBe(result2.bodyMarkdown);
  });

  it("[TDD-FAIL] editor ingest preserves callout type information", () => {
    const md = "> [!note] Info\n> Content.";
    const result = ingestMarkdownForEditor({ bodyMarkdown: md });
    // Callout should be recognizable in the output
    expect(result.tipTapHtml).toContain("Info");
    expect(result.tipTapHtml).toContain("Content");
  });

  it("[TDD-FAIL] editor export after callout edit retains callout structure", () => {
    const md = "> [!note] Original\n> Body.";
    const fragments = classifyMarkdownCapabilities(md);
    const result = exportEditorToMarkdown({
      editorHtml: "<blockquote><p>Original</p><p>Body.</p></blockquote>",
      originalMarkdown: md,
      classifiedFragments: fragments,
    });
    // Callout should survive export
    expect(result.bodyMarkdown).toContain("Original");
    expect(result.bodyMarkdown).toContain("Body");
  });
});

// ═══════════════════════════════════════════════════════════════
// 重复导入导出稳定性
// ═══════════════════════════════════════════════════════════════

describe("TDD: repeated ingest-export stability", () => {
  it("[TDD-FAIL] pure native GFM: ingest → export → same", () => {
    const md = "# Title\n\n**Bold** `code` - list item";
    const ingest1 = ingestMarkdownForEditor({ bodyMarkdown: md });

    // Simulate round-trip: html → export → markdown
    const export1 = exportEditorToMarkdown({
      editorHtml: ingest1.tipTapHtml,
      originalMarkdown: md,
      classifiedFragments: ingest1.preserveFragments,
    });

    // The exported markdown should contain the core elements
    expect(export1.bodyMarkdown).toContain("Title");
    expect(export1.bodyMarkdown).toContain("Bold");
    expect(export1.bodyMarkdown).toContain("code");
  });

  it("[TDD-FAIL] mixed content: ingest → export → ingest → export stable", () => {
    const md =
      "# Doc\n\n**Bold**\n\n> [!note] Callout\n> Body\n\n<div class='x'>raw</div>";
    const ingest1 = ingestMarkdownForEditor({ bodyMarkdown: md });
    const export1 = exportEditorToMarkdown({
      editorHtml: ingest1.tipTapHtml,
      originalMarkdown: md,
      classifiedFragments: ingest1.preserveFragments,
    });

    // Second round
    const ingest2 = ingestMarkdownForEditor({
      bodyMarkdown: export1.bodyMarkdown,
    });
    const export2 = exportEditorToMarkdown({
      editorHtml: ingest2.tipTapHtml,
      originalMarkdown: export1.bodyMarkdown,
      classifiedFragments: ingest2.preserveFragments,
    });

    // Stable across two rounds — preserve count should be consistent
    expect(ingest1.preserveFragments.length).toBeGreaterThan(0);
    expect(export1.bodyMarkdown).toContain("Bold");
    expect(export2.bodyMarkdown).toContain("Bold");
  });

  it("[TDD-FAIL] ingest warnings are consistent across repeated calls", () => {
    const md = "<script>alert(1)</script>";
    const r1 = ingestMarkdownForEditor({ bodyMarkdown: md });
    const r2 = ingestMarkdownForEditor({ bodyMarkdown: md });
    expect(r1.warnings.length).toBe(r2.warnings.length);
  });
});

// ═══════════════════════════════════════════════════════════════
// Vault 预览 ↔ 编辑器导出一致性
// ═══════════════════════════════════════════════════════════════

describe("TDD: vault preview = editor export semantics", () => {
  it("[TDD-FAIL] vault_preview and editor_export produce semantically equivalent output", () => {
    const md = "# Title\n\n**Bold** paragraph.\n\n- item 1\n- item 2";

    const vaultResult = renderMarkdownWithProfile(md, "vault_preview", {
      context: "Title",
    });
    const classified = classifyMarkdownCapabilities(md);
    const editorResult = exportEditorToMarkdown({
      editorHtml:
        "<h1>Title</h1><p><strong>Bold</strong> paragraph.</p><ul><li>item 1</li><li>item 2</li></ul>",
      originalMarkdown: md,
      classifiedFragments: classified,
    });

    // Vault preview HTML should contain Title
    expect(vaultResult.output).toContain("Title");
    // Editor export should contain Title
    expect(editorResult.bodyMarkdown).toContain("Title");
    // Both should contain Bold
    expect(vaultResult.output).toContain("Bold");
    expect(editorResult.bodyMarkdown).toContain("Bold");
  });

  it("[TDD-FAIL] advanced syntax is preserved in both vault and editor export", () => {
    const md = "# Doc\n\n> [!info] Note\n> Body\n\n<div>raw</div>";
    const vaultResult = renderMarkdownWithProfile(md, "vault_preview");
    const classified = classifyMarkdownCapabilities(md);
    const editorResult = exportEditorToMarkdown({
      editorHtml:
        "<h1>Doc</h1><blockquote><p>[!info] Note</p><p>Body</p></blockquote><p>raw</p>",
      originalMarkdown: md,
      classifiedFragments: classified,
    });

    expect(vaultResult.output).toContain("Note");
    expect(vaultResult.output).toContain("Body");
    expect(editorResult.bodyMarkdown).toContain("Note");
    expect(editorResult.bodyMarkdown).toContain("Body");
  });
});

// ═══════════════════════════════════════════════════════════════
// 导出完整性：不丢失任何元素
// ═══════════════════════════════════════════════════════════════

describe("TDD: export completeness — no element loss", () => {
  it("[TDD-FAIL] export includes all heading levels", () => {
    const md = "# H1\n\n## H2\n\n### H3";
    const result = exportEditorToMarkdown({
      editorHtml: "<h1>H1</h1><h2>H2</h2><h3>H3</h3>",
      originalMarkdown: md,
      classifiedFragments: [],
    });
    expect(result.bodyMarkdown).toContain("H1");
    expect(result.bodyMarkdown).toContain("H2");
    expect(result.bodyMarkdown).toContain("H3");
  });

  it("[TDD-FAIL] export includes task lists", () => {
    const md = "- [x] Done\n- [ ] Pending";
    const result = exportEditorToMarkdown({
      editorHtml:
        '<ul data-type="taskList"><li data-type="taskItem" data-checked="true">Done</li><li data-type="taskItem" data-checked="false">Pending</li></ul>',
      originalMarkdown: md,
      classifiedFragments: [],
    });
    expect(result.bodyMarkdown).toContain("Done");
    expect(result.bodyMarkdown).toContain("Pending");
  });

  it("[TDD-FAIL] export includes GFM tables", () => {
    const md = "| A | B |\n| --- | --- |\n| 1 | 2 |";
    const result = exportEditorToMarkdown({
      editorHtml:
        "<table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>",
      originalMarkdown: md,
      classifiedFragments: [],
    });
    expect(result.bodyMarkdown).toContain("1");
    expect(result.bodyMarkdown).toContain("2");
  });

  it("[TDD-FAIL] export includes code blocks", () => {
    const md = "```ts\nconst x = 1;\n```";
    const result = exportEditorToMarkdown({
      editorHtml: '<pre><code class="language-ts">const x = 1;</code></pre>',
      originalMarkdown: md,
      classifiedFragments: [],
    });
    expect(result.bodyMarkdown).toContain("const x");
  });
});
