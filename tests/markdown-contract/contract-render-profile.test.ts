/**
 * contract-render-profile.test.ts — TDD 红灯测试
 *
 * 直接测试 renderMarkdownWithProfile() 的行为规范。
 * 当前所有测试必须 FAIL（contract 尚未实现）。
 * 阶段 2.4 实现后，这些测试变为 GREEN。
 *
 * 覆盖 CONTRACT_PLAN.md § Render Profiles：
 * - chat_assistant: 完整渲染 + 引用链接化 + 代码高亮
 * - chat_user: 核心 GFM 渲染
 * - editor_ingest: TipTap 兼容 HTML + 占位标记
 * - editor_export: Markdown 输出 + 原文保护
 * - vault_preview: 自包含 HTML + 安全清洗
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { renderMarkdownWithProfile } from "@/lib/markdown-contract/contract";
import type {
  MarkdownContractResult,
  MarkdownProfile,
} from "@/lib/markdown-contract/types";

const GOLD_ROOT = resolve(__dirname, "gold-corpus");
const BASIC_GFM = readFileSync(resolve(GOLD_ROOT, "basic-gfm.md"), "utf8");

function render(p: MarkdownProfile, md: string): MarkdownContractResult {
  return renderMarkdownWithProfile(md, p);
}

// ── 公共契约 ─────────────────────────────────────────────────

describe("contract: result structure", () => {
  it("result.output is a non-empty string", () => {
    const result = render("chat_assistant", "**Hello**");
    expect(typeof result.output).toBe("string");
    expect(result.output.length).toBeGreaterThan(0);
  });

  it("result.meta.profile matches the requested profile", () => {
    const result = render("chat_user", "**text**");
    expect(result.meta.profile).toBe("chat_user");
  });

  it("result.meta.streaming is false for non-streaming render", () => {
    const result = renderMarkdownWithProfile("text", "chat_assistant", {
      streaming: false,
    });
    expect(result.meta.streaming).toBe(false);
  });

  it("result.meta.streaming is true for streaming render", () => {
    const result = renderMarkdownWithProfile("**partial", "chat_assistant", {
      streaming: true,
    });
    expect(result.meta.streaming).toBe(true);
  });

  it("result.warnings is an array", () => {
    const result = render("chat_assistant", "**text**");
    expect(Array.isArray(result.warnings)).toBe(true);
  });

  it("result.preserveFragments is an array", () => {
    const result = render("chat_assistant", "**text**");
    expect(Array.isArray(result.preserveFragments)).toBe(true);
  });

  it("result.streamRepairs is an array", () => {
    const result = render("chat_assistant", "**text**");
    expect(Array.isArray(result.streamRepairs)).toBe(true);
  });

  it("result.meta.stats has counts for all 4 capability levels", () => {
    const result = render("chat_assistant", BASIC_GFM);
    expect(result.meta.stats.native).toBeGreaterThan(0);
    expect(typeof result.meta.stats.render_only).toBe("number");
    expect(typeof result.meta.stats.preserve_only).toBe("number");
    expect(typeof result.meta.stats.unsupported).toBe("number");
    expect(result.meta.stats.total).toBe(
      result.meta.stats.native +
        result.meta.stats.render_only +
        result.meta.stats.preserve_only +
        result.meta.stats.unsupported,
    );
  });
});

// ── chat_assistant profile ────────────────────────────────────

describe("profile: chat_assistant", () => {
  it("renders bold as <strong>", () => {
    const result = render("chat_assistant", "**bold**");
    expect(result.output).toContain("<strong>");
    expect(result.output).not.toContain("**bold**");
  });

  it("renders code block with syntax highlighting", () => {
    const result = render("chat_assistant", "```js\nconst x = 1;\n```");
    expect(result.output).toContain("<pre>");
    expect(result.output).toContain("<code");
    expect(result.output).toContain("language-js");
  });

  it("citation [citation:1] is linkified", () => {
    const result = render("chat_assistant", "See [citation:1].");
    expect(result.output).toContain("ai-citation");
    expect(result.output).toContain("iris-cite-");
  });

  it("external links have target=_blank", () => {
    const result = render("chat_assistant", "[link](https://a.test)");
    expect(result.output).toContain('target="_blank"');
    expect(result.output).toContain('rel="noopener noreferrer"');
  });

  it("preserve_only raw HTML is rendered as escaped raw text", () => {
    const result = render(
      "chat_assistant",
      "<div class='x'>raw</div>\n\n**safe**",
    );
    expect(result.output).toContain("<strong>");
    // raw HTML should be visible but escaped
    expect(result.output).toContain("raw");
  });

  it("streaming mode adds streamRepairs to result", () => {
    const result = renderMarkdownWithProfile("**unclosed", "chat_assistant", {
      streaming: true,
    });
    expect(result.streamRepairs.length).toBeGreaterThan(0);
  });

  it("streaming mode produces streamRepairs with required fields", () => {
    const result = renderMarkdownWithProfile(
      "*unclosed italic",
      "chat_assistant",
      { streaming: true },
    );
    for (const repair of result.streamRepairs) {
      expect(typeof repair.before).toBe("string");
      expect(typeof repair.after).toBe("string");
      expect(typeof repair.repairKind).toBe("string");
      expect(typeof repair.offset).toBe("number");
    }
  });

  it("warnings have required sub-fields when unsupported syntax present", () => {
    const result = renderMarkdownWithProfile(
      "# Title\n\n<object data='x'></object>",
      "chat_assistant",
    );
    for (const warning of result.warnings) {
      expect(typeof warning.fragment).toBe("object");
      expect(typeof warning.message).toBe("string");
      expect(warning.message.length).toBeGreaterThan(0);
      expect(["info", "warn"]).toContain(warning.severity);
    }
  });
});

// ── chat_user profile ─────────────────────────────────────────

describe("profile: chat_user", () => {
  it("renders bold as <strong>", () => {
    const result = render("chat_user", "Hello **world**");
    expect(result.output).toContain("<strong>world</strong>");
  });

  it("renders lists", () => {
    const result = render("chat_user", "- item 1\n- item 2");
    expect(result.output).toContain("<ul>");
    expect(result.output).toContain("<li>");
  });

  it("does NOT linkify citations", () => {
    const result = render("chat_user", "[citation:1]");
    // user messages typically don't have citation linkification
    expect(result.output).not.toContain("ai-citation");
  });
});

// ── editor_ingest profile ─────────────────────────────────────

describe("profile: editor_ingest", () => {
  it("produces TipTap-compatible HTML with data attributes", () => {
    const result = render("editor_ingest", "- [x] Done");
    expect(result.output).toContain("taskList");
    expect(result.output).toContain("taskItem");
  });

  it("wiki-links are converted to TipTap spans", () => {
    const result = render("editor_ingest", "[[My Note]]");
    expect(result.output).toContain("data-wiki-link");
    expect(result.output).toContain('data-wiki-title="My Note"');
  });

  it("callout is wrapped in placeholder for read-only display", () => {
    const result = render("editor_ingest", "> [!note] Info\n> Body");
    // callouts are render_only for editor → should be in placeholder
    expect(result.output).toContain("[!note]");
    expect(result.output).toContain("Body");
  });

  it("preserve_only raw HTML is wrapped in iris-preserve-readonly", () => {
    const result = render("editor_ingest", "<div>block</div>");
    expect(result.preserveFragments.length).toBeGreaterThan(0);
    const hasPreserve = result.preserveFragments.some((f) =>
      f.raw.includes("<div>"),
    );
    expect(hasPreserve).toBe(true);
  });
});

// ── editor_export profile ─────────────────────────────────────

describe("profile: editor_export", () => {
  it("produces markdown string as output", () => {
    const result = render("editor_export", "**bold**");
    expect(result.output).toContain("bold");
  });

  it("preserve_only fragments are included in preserveFragments with original raw", () => {
    const result = render("editor_export", "<div class='x'>raw</div>");
    const hasDiv = result.preserveFragments.some(
      (f) => f.raw === "<div class='x'>raw</div>",
    );
    expect(hasDiv).toBe(true);
  });

  it("callout text is present in exported markdown", () => {
    const result = render("editor_export", "> [!note] Info\n> Content");
    expect(result.output).toContain("Info");
    expect(result.output).toContain("Content");
  });
});

// ── vault_preview profile ─────────────────────────────────────

describe("profile: vault_preview", () => {
  it("produces self-contained HTML with <!DOCTYPE html>", () => {
    const result = render("vault_preview", "# Title");
    expect(result.output).toContain("<!DOCTYPE html>");
    expect(result.output).toContain("<html");
    expect(result.output).toContain("<title>");
  });

  it("includes styling (Paper Ink)", () => {
    const result = render("vault_preview", "# Title");
    expect(result.output).toContain("<style>");
  });

  it("sanitizes dangerous HTML", () => {
    const result = render("vault_preview", "<script>alert(1)</script>\nSafe.");
    expect(result.output).not.toContain("<script");
    expect(result.output).toContain("Safe");
  });
});

// ── 跨 Profile 一致性 ────────────────────────────────────────

describe("cross-profile: semantic consistency", () => {
  it("all display profiles produce output containing text content", () => {
    const profiles: MarkdownProfile[] = [
      "chat_assistant",
      "chat_user",
      "vault_preview",
    ];
    for (const p of profiles) {
      const result = render(p, "**important**");
      expect(result.output).toContain("important");
    }
  });

  it("same markdown across display profiles has same meta stats", () => {
    const md = "# Title\n\n**Bold** `code` - list";
    const assistant = render("chat_assistant", md);
    const user = render("chat_user", md);
    const vault = render("vault_preview", md);

    // stats should be identical since capability classification is source-level
    expect(assistant.meta.stats).toEqual(user.meta.stats);
    expect(user.meta.stats).toEqual(vault.meta.stats);
  });

  it("warnings are generated for unsupported syntax", () => {
    const result = render("chat_assistant", "<object data='x'></object>");
    expect(result.warnings.length).toBeGreaterThan(0);
  });
});
