/**
 * ai-artifact-consistency.test.ts — AI 展示重构 阶段 0 TDD 测试
 *
 * 测试工件表面（ResearchSummary、PatchPreview、CitationCheckView）的
 * Markdown 渲染一致性和 MarkdownRenderable 组件行为。
 *
 * 当前全部 RED：MarkdownRenderable 组件尚未实现。
 * 阶段 1 实现后转 GREEN。
 */
import { describe, expect, it } from "vitest";

import { renderMarkdownWithProfile } from "@/lib/markdown-contract/contract";

// ═══════════════════════════════════════════════════════════════
// Readonly artifact summary 渲染一致性
// ═══════════════════════════════════════════════════════════════

describe("Readonly artifact summary: contract rendering", () => {
  it("[BASELINE] artifact_readonly renders **bold** as <strong>", () => {
    const r = renderMarkdownWithProfile(
      "**Key finding**: the result.",
      "artifact_readonly",
    );
    expect(r.output).toContain("<strong>");
    expect(r.output).toContain("Key finding");
  });

  it("[BASELINE] artifact_readonly renders code blocks with <pre>", () => {
    const r = renderMarkdownWithProfile(
      '```json\n{"key":"value"}\n```',
      "artifact_readonly",
    );
    expect(r.output).toContain("<pre");
    expect(r.output).toContain("key");
  });

  it("[BASELINE] artifact_readonly renders tables", () => {
    const r = renderMarkdownWithProfile(
      "| Source | Score |\n| --- | --- |\n| A | 0.9 |",
      "artifact_readonly",
    );
    expect(r.output).toContain("<table");
  });

  it("[BASELINE] research summary matches assistant rendering for same content", () => {
    const md = "**Finding**: data *supports* `hypothesis`.";
    const artifact = renderMarkdownWithProfile(md, "artifact_readonly");
    const assistant = renderMarkdownWithProfile(md, "chat_assistant");
    expect(artifact.meta.stats).toEqual(assistant.meta.stats);
  });

  it("[BASELINE] artifact_readonly output is sanitized (no XSS)", () => {
    const r = renderMarkdownWithProfile(
      "<script>alert(1)</script>\n**safe**",
      "artifact_readonly",
    );
    expect(r.output).not.toContain("<script");
    expect(r.output).toContain("safe");
  });
});

// ═══════════════════════════════════════════════════════════════
// PatchPreview Markdown 集成 (TDD RED)
// ═══════════════════════════════════════════════════════════════

describe("PatchPreview: markdown integration", () => {
  it("[BASELINE] patch_preview profile renders **bold** as <strong>", () => {
    // Contract baseline: profile exists and works
    const r = renderMarkdownWithProfile("**warning text**", "patch_preview");
    expect(r.output).toContain("<strong>");
  });

  it("[BASELINE] patch_preview profile produces sanitized output", () => {
    const r = renderMarkdownWithProfile(
      "<script>x</script>\n**safe**",
      "patch_preview",
    );
    expect(r.output).not.toContain("<script");
  });

  it("[BASELINE] patch_preview has same stats as chat_assistant for same content", () => {
    const md = "## Warning\n\n**Risk:** high.\n\n- check 1\n- check 2";
    const patch = renderMarkdownWithProfile(md, "patch_preview");
    const assistant = renderMarkdownWithProfile(md, "chat_assistant");
    expect(patch.meta.stats).toEqual(assistant.meta.stats);
  });
});

// ═══════════════════════════════════════════════════════════════
// CitationCheckView Markdown 集成 (TDD RED)
// ═══════════════════════════════════════════════════════════════

describe("CitationCheckView: markdown integration", () => {
  it("[BASELINE] citation_panel profile renders core GFM", () => {
    const md = "**Claim**: evidence shows `result`.\n\n- source A\n- source B";
    const r = renderMarkdownWithProfile(md, "citation_panel");
    expect(r.output).toContain("<strong>");
    expect(r.output).toContain("<code>");
    expect(r.output).toContain("<ul");
  });

  it("[BASELINE] citation_panel produces sanitized output", () => {
    const r = renderMarkdownWithProfile(
      "<script>x</script>\n**safe**",
      "citation_panel",
    );
    expect(r.output).not.toContain("<script");
  });

  it("[BASELINE] citation_panel has same stats as chat_assistant for same content", () => {
    const md = "**Key claim** with `evidence`.\n\n> supporting quote";
    const cite = renderMarkdownWithProfile(md, "citation_panel");
    const assistant = renderMarkdownWithProfile(md, "chat_assistant");
    expect(cite.meta.stats).toEqual(assistant.meta.stats);
  });
});

// ═══════════════════════════════════════════════════════════════
// MarkdownRenderable 组件行为 (TDD RED)
// ═══════════════════════════════════════════════════════════════

describe("MarkdownRenderable: shared rendering shell", () => {
  it("[BASELINE] renders markdown via contract for all artifact profiles", () => {
    // MarkdownRenderable component should delegate to renderMarkdownWithProfile
    // Test via contract API as proxy until component exists
    for (const p of [
      "artifact_readonly",
      "patch_preview",
      "citation_panel",
    ] as const) {
      const r = renderMarkdownWithProfile("**text**", p);
      expect(r.output).toContain("<strong>");
    }
  });

  it("[BASELINE] handles empty content gracefully", () => {
    for (const p of [
      "artifact_readonly",
      "patch_preview",
      "citation_panel",
    ] as const) {
      const r = renderMarkdownWithProfile("", p);
      expect(typeof r.output).toBe("string");
    }
  });
});
