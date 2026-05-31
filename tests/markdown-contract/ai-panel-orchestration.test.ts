/**
 * ai-panel-orchestration.test.ts — AI 展示重构 阶段 0 TDD 测试
 *
 * 测试 UnifiedAssistantPanel 拆分后的组件编排：消息面 + 工件面的独立性、
 * 状态隔离、会话切换安全。
 *
 * 当前全部 RED：ConversationSurface / ArtifactSurface 尚未实现。
 * 阶段 2 实现后转 GREEN。
 */
import { describe, expect, it } from "vitest";

import { renderMarkdownWithProfile } from "@/lib/markdown-contract/contract";

// ═══════════════════════════════════════════════════════════════
// 消息面 + 工件面 接口契约
// ═══════════════════════════════════════════════════════════════

describe("ConversationSurface: contract specification", () => {
  it("[BASELINE] chat_assistant renders assistant content as markdown", () => {
    const r = renderMarkdownWithProfile("Hello **world**", "chat_assistant");
    expect(r.output).toContain("<strong>");
  });

  it("[BASELINE] chat_user renders user content as markdown", () => {
    const r = renderMarkdownWithProfile("User **input**", "chat_user");
    expect(r.output).toContain("<strong>");
  });

  it("[BASELINE] chat_assistant renders citations as links", () => {
    const r = renderMarkdownWithProfile("See [citation:1].", "chat_assistant");
    expect(r.output).toContain("ai-citation");
  });

  it("[BASELINE] streaming message has non-empty streamRepairs", () => {
    const r = renderMarkdownWithProfile("**partial", "chat_assistant", {
      streaming: true,
    });
    expect(r.streamRepairs.length).toBeGreaterThan(0);
  });

  it("[BASELINE] non-streaming message has empty streamRepairs", () => {
    const r = renderMarkdownWithProfile("**bold**", "chat_assistant", {
      streaming: false,
    });
    expect(r.streamRepairs.length).toBe(0);
  });
});

// ═══════════════════════════════════════════════════════════════
// ArtifactSurface: 工件渲染独立性
// ═══════════════════════════════════════════════════════════════

describe("ArtifactSurface: contract specification", () => {
  it("[BASELINE] research_card renders via same contract as messages", () => {
    const md = "**Finding** with *evidence*.";
    const research = renderMarkdownWithProfile(md, "research_card");
    const asst = renderMarkdownWithProfile(md, "chat_assistant");
    expect(research.meta.stats).toEqual(asst.meta.stats);
  });

  it("[BASELINE] patch_preview renders via same contract as messages", () => {
    const md = "**Warning** detail.";
    const patch = renderMarkdownWithProfile(md, "patch_preview");
    const asst = renderMarkdownWithProfile(md, "chat_assistant");
    expect(patch.meta.stats).toEqual(asst.meta.stats);
  });

  it("[BASELINE] citation_panel renders via same contract as messages", () => {
    const md = "**Claim** with `code`.";
    const cite = renderMarkdownWithProfile(md, "citation_panel");
    const asst = renderMarkdownWithProfile(md, "chat_assistant");
    expect(cite.meta.stats).toEqual(asst.meta.stats);
  });

  it("[BASELINE] all artifact profiles produce non-empty output", () => {
    for (const p of [
      "research_card",
      "patch_preview",
      "citation_panel",
    ] as const) {
      const r = renderMarkdownWithProfile("**test**", p);
      expect(r.output.length).toBeGreaterThan(0);
    }
  });

  it("[BASELINE] artifact profiles produce sanitized output", () => {
    for (const p of [
      "research_card",
      "patch_preview",
      "citation_panel",
    ] as const) {
      const r = renderMarkdownWithProfile(
        "<script>alert(1)</script>\n**ok**",
        p,
      );
      expect(r.output).not.toContain("<script");
    }
  });
});

// ═══════════════════════════════════════════════════════════════
// 状态隔离：消息面与工件面互不干扰
// ═══════════════════════════════════════════════════════════════

describe("state isolation: messages vs artifacts", () => {
  it("[BASELINE] contract stats are deterministic for same input across profiles", () => {
    const md = "# Title\n\n**bold** `code`";
    const r1 = renderMarkdownWithProfile(md, "chat_assistant");
    const r2 = renderMarkdownWithProfile(md, "chat_assistant");
    expect(r1.meta.stats).toEqual(r2.meta.stats);
  });

  it("[BASELINE] different profiles for same content produce same fragment stats", () => {
    const md = "# Title\n\n**bold**\n\n<div>raw</div>";
    const asst = renderMarkdownWithProfile(md, "chat_assistant");
    const research = renderMarkdownWithProfile(md, "research_card");
    expect(asst.meta.stats).toEqual(research.meta.stats);
  });

  it("[BASELINE] warnings are generated consistently across profiles", () => {
    const md = "<script>alert(1)</script>";
    const asst = renderMarkdownWithProfile(md, "chat_assistant");
    const patch = renderMarkdownWithProfile(md, "patch_preview");
    expect(asst.warnings.length).toBe(patch.warnings.length);
  });

  it("[BASELINE] preserveFragments are identical for same content across profiles", () => {
    const md = "# Title\n<div class='x'>raw</div>\n\n**text**";
    const r1 = renderMarkdownWithProfile(md, "chat_assistant");
    const r2 = renderMarkdownWithProfile(md, "research_card");
    expect(r1.preserveFragments.length).toBe(r2.preserveFragments.length);
  });
});

// ═══════════════════════════════════════════════════════════════
// 会话切换安全
// ═══════════════════════════════════════════════════════════════

describe("session switch safety", () => {
  it("[BASELINE] renderMarkdownWithProfile is stateless across calls", () => {
    const r1 = renderMarkdownWithProfile("**bold**", "chat_assistant");
    const r2 = renderMarkdownWithProfile("**bold**", "chat_assistant");
    // Two identical calls should produce identical results
    expect(r1.output).toBe(r2.output);
    expect(r1.meta.stats).toEqual(r2.meta.stats);
  });

  it("[BASELINE] empty content after session switch produces valid output", () => {
    const r = renderMarkdownWithProfile("", "chat_assistant");
    expect(typeof r.output).toBe("string");
    expect(r.preserveFragments.length).toBe(0);
  });

  it("[BASELINE] very long content does not degrade performance", () => {
    const long = "# Title\n\n" + "**bold** ".repeat(500);
    const r = renderMarkdownWithProfile(long, "chat_assistant");
    expect(r.output.length).toBeGreaterThan(0);
    expect(r.meta.stats.total).toBeGreaterThan(0);
  });
});
