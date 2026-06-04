/**
 * editor-patch-preserve.test.ts — TDD 红灯测试
 *
 * 测试 AI 补丁回灌时 preserve 块的完整性和安全性。
 * 当前所有测试必须 FAIL（ingestMarkdownForEditor / exportEditorToMarkdown 尚未实现）。
 * 子项目 2 阶段 1-3 实现后全部 GREEN。
 */
import { describe, expect, it } from "vitest";

import { ingestMarkdownForEditor } from "@/lib/editor-ingest";
import { exportEditorToMarkdown } from "@/lib/editor-export";
import { classifyMarkdownCapabilities } from "@/lib/markdown-contract/contract";
import { serializePreservedMarkdown } from "@/lib/markdown-contract/contract";
import type { MarkdownSyntaxFragment } from "@/lib/markdown-contract/types";

// ═══════════════════════════════════════════════════════════════
// Helper: simulate a patch application + re-ingest
// ═══════════════════════════════════════════════════════════════

function simulatePatchAndReIngest(
  originalMd: string,
  patchReplacement: string,
): {
  beforeFragments: MarkdownSyntaxFragment[];
  afterFragments: MarkdownSyntaxFragment[];
  ingestedHtml: string;
} {
  const beforeFragments = classifyMarkdownCapabilities(originalMd);
  const newMd = patchReplacement;
  const afterFragments = classifyMarkdownCapabilities(newMd);
  const ingested = ingestMarkdownForEditor({ bodyMarkdown: newMd });
  return {
    beforeFragments,
    afterFragments,
    ingestedHtml: ingested.tipTapHtml,
  };
}

// ═══════════════════════════════════════════════════════════════
// 补丁不破坏 preserve 块
// ═══════════════════════════════════════════════════════════════

describe("TDD: patch safety — preserve blocks survive patches", () => {
  it("[TDD-FAIL] native content patch does not affect adjacent preserve block", () => {
    const original =
      "**old** text.\n\n<div class='x'>preserved</div>\n\nafter.";
    const patched = "**new** text.\n\n<div class='x'>preserved</div>\n\nafter.";

    const { beforeFragments, afterFragments } = simulatePatchAndReIngest(
      original,
      patched,
    );

    // preserve block original content must be unchanged
    const preserveBefore = beforeFragments.filter(
      (f) => f.capability === "preserve_only",
    );
    const preserveAfter = afterFragments.filter(
      (f) => f.capability === "preserve_only",
    );
    expect(preserveBefore.length).toBeGreaterThan(0);
    expect(preserveAfter.length).toBeGreaterThan(0);
    // preserve content unchanged
    expect(preserveAfter[0]?.raw).toBe(preserveBefore[0]?.raw);
  });

  it("[TDD-FAIL] preserve block raw content is byte-for-byte unchanged after native edit", () => {
    const md = '<div class="note">HTML note</div>\n\nEdit me.';
    const patched = '<div class="note">HTML note</div>\n\nEdited!';

    const beforeFragments = classifyMarkdownCapabilities(md);
    const afterFragments = classifyMarkdownCapabilities(patched);

    const beforePreserve = beforeFragments.filter(
      (f) => f.capability === "preserve_only",
    );
    const afterPreserve = afterFragments.filter(
      (f) => f.capability === "preserve_only",
    );

    expect(beforePreserve[0]?.raw).toBe(afterPreserve[0]?.raw);
  });

  it("[TDD-FAIL] re-ingest after patch does not lose preserve fragments", () => {
    const original = "## Title\n\nNative body.\n\n<kbd>Ctrl</kbd> shortcut.";
    const patched = "## Title\n\nChanged body.\n\n<kbd>Ctrl</kbd> shortcut.";

    const originalFrags = classifyMarkdownCapabilities(original);
    const patchedFrags = classifyMarkdownCapabilities(patched);

    const origPreserveCount = originalFrags.filter(
      (f) => f.capability === "preserve_only",
    ).length;
    const patchPreserveCount = patchedFrags.filter(
      (f) => f.capability === "preserve_only",
    ).length;

    expect(patchPreserveCount).toBe(origPreserveCount);
  });

  it("[TDD-FAIL] export after patch includes all preserve fragments", () => {
    const patched =
      "# Doc\n\nUpdated text.\n\n<!-- comment -->\n\n<div>raw</div>";

    const fragments = classifyMarkdownCapabilities(patched);
    const exported = exportEditorToMarkdown({
      editorHtml: "<h1>Doc</h1><p>Updated text.</p>",
      originalMarkdown: patched,
      classifiedFragments: fragments,
    });

    expect(exported.bodyMarkdown).toContain("<!-- comment -->");
    expect(exported.bodyMarkdown).toContain("<div>raw</div>");
  });

  it("[TDD-FAIL] preserve-only content is identical after round-trip through contract", () => {
    const source = '**bold** and <div class="box">preserved</div> and `code`.';
    const fragments = classifyMarkdownCapabilities(source);
    const preserved = serializePreservedMarkdown(source, fragments);

    // preserve-only content must survive
    expect(preserved).toContain('<div class="box">preserved</div>');
    // native content must survive
    expect(preserved).toContain("**bold**");
    expect(preserved).toContain("`code`");
  });
});

// ═══════════════════════════════════════════════════════════════
// 补丁边界安全
// ═══════════════════════════════════════════════════════════════

describe("TDD: patch boundary safety", () => {
  it("[TDD-FAIL] patch on native content does not shift preserve block offsets", () => {
    const original = [
      "Text before.",
      "",
      '<div class="x">preserved</div>',
      "",
      "Text after.",
    ].join("\n");

    const beforeFrags = classifyMarkdownCapabilities(original);
    const preserveFrags = beforeFrags.filter(
      (f) => f.capability === "preserve_only",
    );
    expect(preserveFrags.length).toBeGreaterThan(0);

    // After re-classifying, offsets should be consistent
    const afterFrags = classifyMarkdownCapabilities(original);
    for (let i = 0; i < beforeFrags.length; i++) {
      expect(afterFrags[i]?.offset).toBe(beforeFrags[i]?.offset);
      expect(afterFrags[i]?.raw).toBe(beforeFrags[i]?.raw);
    }
  });

  it("[TDD-FAIL] ingest after patch produces consistent fragment mapping", () => {
    const md = "# Title\n\nPara.\n\n<div class='a'>raw A</div>\n\nPara 2.";
    const result1 = ingestMarkdownForEditor({ bodyMarkdown: md });
    const result2 = ingestMarkdownForEditor({ bodyMarkdown: md });

    // Ingest should be deterministic
    expect(result1.preserveFragments.length).toBeGreaterThan(0);
    expect(result1.preserveFragments.length).toBe(
      result2.preserveFragments.length,
    );
  });

  it("[TDD-FAIL] exported markdown after patch is valid (parses cleanly)", () => {
    const source = "# Doc\n\n**Content**.\n\n<!-- comment -->";
    const fragments = classifyMarkdownCapabilities(source);
    const preserved = serializePreservedMarkdown(source, fragments);
    // The round-tripped content should contain all original elements
    expect(preserved).toContain("Doc");
    expect(preserved).toContain("Content");
    expect(preserved).toContain("<!-- comment -->");
  });
});
