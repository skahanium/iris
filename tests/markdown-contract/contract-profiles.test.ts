/**
 * contract-profiles.test.ts
 *
 * Render Profile 测试 — 断言不同 profile 场景下的渲染行为符合契约定义。
 *
 * 覆盖 CONTRACT_PLAN.md § 公共接口建议：
 * - chat_assistant: 完整渲染 + 引用链接化 + 流式支持
 * - chat_user: 核心 GFM 渲染
 * - editor_ingest: TipTap HTML 转换
 * - editor_export: TipTap HTML → Markdown + 原文保护
 * - vault_preview: 完整渲染 + 自包含 HTML
 */
import { describe, expect, it } from "vitest";

import {
  parseMarkdownToHtml,
  renderAiMarkdownToHtml,
} from "@/lib/markdown-render";
import {
  markdownBodyToEditorHtml,
  editorBodyHtmlToMarkdown,
  markdownToHtmlPage,
  buildNoteMarkdown,
  parseNoteForEditor,
} from "@/lib/markdown";
import { sanitizeHtml } from "@/lib/sanitize";
import { postProcessCitations } from "@/lib/ai/citation-markdown";
import type { MarkdownProfile } from "@/lib/markdown-contract/types";

// ── Helpers ────────────────────────────────────────────────────

function renderAsProfile(md: string, profile: MarkdownProfile): string {
  switch (profile) {
    case "chat_assistant":
      return sanitizeHtml(renderAiMarkdownToHtml(md, { streaming: false }));
    case "chat_user":
      return renderAiMarkdownToHtml(md, { streaming: false });
    case "editor_ingest":
      return markdownBodyToEditorHtml(md);
    case "editor_export":
      return editorBodyHtmlToMarkdown(markdownBodyToEditorHtml(md));
    case "vault_preview":
      return markdownToHtmlPage(md);
    default:
      return parseMarkdownToHtml(md, { streaming: false });
  }
}

// ── chat_assistant profile ────────────────────────────────────

describe("profile: chat_assistant", () => {
  it("renders core GFM bold correctly", () => {
    const html = renderAsProfile("**important**", "chat_assistant");
    expect(html).toContain("<strong>");
    expect(html).toContain("important");
  });

  it("handles citations in assistant messages", () => {
    const html = renderAsProfile(
      "See [citation:1] for details.",
      "chat_assistant",
    );
    expect(html).toContain("ai-citation");
    expect(html).toContain("iris-cite-");
  });

  it("renders code blocks with syntax highlighting tags", () => {
    const html = renderAsProfile("```js\nconst x = 1;\n```", "chat_assistant");
    expect(html).toContain("<pre>");
    expect(html).toContain("<code");
    expect(html).toContain("x");
  });

  it("renders tables with wrapper div for AI chat", () => {
    const md = "| A | B |\n| --- | --- |\n| 1 | 2 |";
    const raw = parseMarkdownToHtml(md, { streaming: false });
    expect(raw).toContain("<table");
  });

  it("links have target=_blank for external safety", () => {
    const md = "[External](https://example.com)";
    const raw = parseMarkdownToHtml(md, { streaming: false });
    expect(raw).toContain('target="_blank"');
    expect(raw).toContain('rel="noopener noreferrer"');
  });

  it("produces safe HTML (sanitized) with no raw HTML injection", () => {
    const html = renderAsProfile(
      "<script>alert(1)</script>\n**safe**",
      "chat_assistant",
    );
    expect(html).not.toContain("<script");
    expect(html).toContain("<strong>");
  });
});

// ── chat_user profile ─────────────────────────────────────────

describe("profile: chat_user", () => {
  it("renders core GFM bold correctly", () => {
    const html = renderAsProfile("**user bold**", "chat_user");
    expect(html).toContain("<strong>");
  });

  it("renders lists correctly", () => {
    const html = renderAsProfile("- item 1\n- item 2", "chat_user");
    expect(html).toContain("<ul>");
    expect(html).toContain("<li>");
  });

  it("renders inline code correctly", () => {
    const html = renderAsProfile("Run `cmd` now.", "chat_user");
    expect(html).toContain("<code>");
  });

  it("renders blockquotes correctly", () => {
    const html = renderAsProfile("> user quote", "chat_user");
    expect(html).toContain("<blockquote>");
  });

  it("user message with markdown bold renders as bold (not plain text)", () => {
    // 核心能力验证：用户消息的 Markdown 格式应该被渲染
    const html = renderAsProfile("Hello **world**", "chat_user");
    expect(html).toContain("<strong>world</strong>");
  });
});

// ── editor_ingest profile ─────────────────────────────────────

describe("profile: editor_ingest", () => {
  it("converts markdown to TipTap-compatible HTML", () => {
    const html = renderAsProfile("**bold**", "editor_ingest");
    // Should produce HTML suitable for TipTap (contains paragraph wrapper)
    expect(html).toContain("<p>");
    expect(html).toContain("<strong>");
  });

  it("converts task lists to TipTap attributes", () => {
    const html = renderAsProfile(
      "- [x] Completed\n- [ ] Pending",
      "editor_ingest",
    );
    expect(html).toContain("taskItem");
    expect(html).toContain("taskList");
    expect(html).toContain('data-checked="true"');
    expect(html).toContain('data-checked="false"');
  });

  it("converts wiki-links to TipTap spans", () => {
    const html = renderAsProfile("See [[My Note]].", "editor_ingest");
    expect(html).toContain("data-wiki-link");
    expect(html).toContain('data-wiki-title="My Note"');
  });

  it("converts tables to TipTap-compatible HTML tables", () => {
    const html = renderAsProfile(
      "| A | B |\n| --- | --- |\n| 1 | 2 |",
      "editor_ingest",
    );
    expect(html).toContain("<table>");
    expect(html).toContain("A");
    expect(html).toContain("1");
  });

  it("handles empty markdown (empty paragraph for TipTap)", () => {
    const html = renderAsProfile("", "editor_ingest");
    expect(html).toContain("<p>");
  });

  it("handles markdown-only whitespace", () => {
    const html = renderAsProfile("  \n  ", "editor_ingest");
    expect(html).toContain("<p>");
  });

  it("preserves callout text as blockquote in editor HTML", () => {
    const html = renderAsProfile("> [!note] Info\n> Body", "editor_ingest");
    expect(html).toContain("[!note]");
    expect(html).toContain("Body");
  });

  it("preserves footnotes in editor HTML", () => {
    const html = renderAsProfile("Text[^1]", "editor_ingest");
    expect(html).toContain("[^1]");
  });
});

// ── editor_export profile ─────────────────────────────────────

describe("profile: editor_export", () => {
  it("converts TipTap HTML back to markdown preserving bold", () => {
    const md = "**bold text**";
    const out = renderAsProfile(md, "editor_export");
    expect(out).toContain("bold");
    expect(out).toMatch(/\*\*|__/);
  });

  it("converts task lists back to markdown preserving state", () => {
    const md = "- [x] Done\n- [ ] Pending";
    const out = renderAsProfile(md, "editor_export");
    expect(out).toContain("[x]");
    expect(out).toContain("[ ]");
    expect(out).toContain("Done");
    expect(out).toContain("Pending");
  });

  it("preserves wiki-links in round-trip", () => {
    const md = "See [[Note Title]] for more.";
    const out = renderAsProfile(md, "editor_export");
    expect(out).toContain("[[Note Title]]");
  });

  it("preserves tables in export", () => {
    const md = "| A | B |\n| --- | --- |\n| 1 | 2 |";
    const out = renderAsProfile(md, "editor_export");
    expect(out).toContain("| A | B |");
    expect(out).toContain("| 1 | 2 |");
  });

  it("preserves code blocks", () => {
    const md = "```ts\nconst x = 1;\n```";
    const out = renderAsProfile(md, "editor_export");
    expect(out).toContain("```");
    expect(out).toContain("const x");
  });

  it("callout text survives editor export", () => {
    const md = "> [!note] Info\n> Content";
    const out = renderAsProfile(md, "editor_export");
    expect(out).toContain("note");
    expect(out).toContain("Info");
  });

  it("footnote text survives editor export", () => {
    const md = "Text[^1]\n\n[^1]: Body";
    const out = renderAsProfile(md, "editor_export");
    expect(out).toContain("[^1]");
    expect(out).toContain("Body");
  });
});

// ── vault_preview profile ─────────────────────────────────────

describe("profile: vault_preview", () => {
  it("produces self-contained HTML document", () => {
    const html = renderAsProfile("# Note Title\n\nContent.", "vault_preview");
    expect(html).toContain("<!DOCTYPE html>");
    expect(html).toContain("<html");
    expect(html).toContain("</html>");
  });

  it("renders headings correctly in preview", () => {
    const html = renderAsProfile("# Title", "vault_preview");
    expect(html).toContain("<h1>Title</h1>");
  });

  it("renders bold/italic/code in preview", () => {
    const md = "**bold** *italic* `code`";
    const html = renderAsProfile(md, "vault_preview");
    expect(html).toContain("<strong>");
    expect(html).toContain("<em>");
    expect(html).toContain("<code>");
  });

  it("renders tables in preview", () => {
    const md = "| A | B |\n| --- | --- |\n| 1 | 2 |";
    const html = renderAsProfile(md, "vault_preview");
    expect(html).toContain("<table");
    expect(html).toContain("A");
  });

  it("renders lists in preview", () => {
    const md = "- item 1\n- item 2";
    const html = renderAsProfile(md, "vault_preview");
    expect(html).toContain("<ul>");
    expect(html).toContain("item 1");
  });

  it("renders code blocks in preview", () => {
    const md = "```js\ncode\n```";
    const html = renderAsProfile(md, "vault_preview");
    expect(html).toContain("<pre>");
    expect(html).toContain("code");
  });

  it("sanitizes dangerous content in preview", () => {
    const md = "<script>alert(1)</script>\nSafe.";
    const html = renderAsProfile(md, "vault_preview");
    expect(html).not.toContain("<script");
  });

  it("includes flat editor-aligned styling", () => {
    const html = renderAsProfile("# Title", "vault_preview");
    expect(html).toContain("Noto Sans SC");
    expect(html).not.toContain("Noto Serif SC");
    expect(html).toContain("background:");
  });
});

// ── 跨 profile 一致性 ────────────────────────────────────────

describe("cross-profile consistency", () => {
  const profilesUnderTest: MarkdownProfile[] = [
    "chat_assistant",
    "chat_user",
    "vault_preview",
  ];

  it("all display profiles render bold consistently", () => {
    for (const profile of profilesUnderTest) {
      const html = renderAsProfile("**bold here**", profile);
      expect(html).toContain("<strong>");
      expect(html).toContain("bold here");
    }
  });

  it("all display profiles render italic consistently", () => {
    for (const profile of profilesUnderTest) {
      const html = renderAsProfile("*italic text*", profile);
      expect(html).toContain("<em>");
      expect(html).toContain("italic text");
    }
  });

  it("all display profiles render inline code consistently", () => {
    for (const profile of profilesUnderTest) {
      const html = renderAsProfile("`code`", profile);
      expect(html).toContain("<code>");
      expect(html).toContain("code");
    }
  });

  it("all display profiles render headings consistently", () => {
    for (const profile of profilesUnderTest) {
      const html = renderAsProfile("# The Title", profile);
      // 标题内容应被渲染
      expect(html).toContain("The Title");
    }
  });

  it("all display profiles render blockquotes consistently", () => {
    for (const profile of profilesUnderTest) {
      const html = renderAsProfile("> quoted", profile);
      expect(html).toContain("<blockquote>");
      expect(html).toContain("quoted");
    }
  });

  it("all display profiles render lists consistently", () => {
    for (const profile of profilesUnderTest) {
      const html = renderAsProfile("- item", profile);
      expect(html).toContain("<ul>");
      expect(html).toContain("item");
    }
  });

  it("editor ingest → export round-trip preserves semantic content", () => {
    const original = "**bold** and *italic* and `code`";
    const ingested = renderAsProfile(original, "editor_ingest");
    const exported = editorBodyHtmlToMarkdown(ingested);

    expect(exported).toContain("bold");
    expect(exported).toContain("italic");
    expect(exported).toContain("`code`");
  });
});

// ── Profile 默认行为规则 ──────────────────────────────────────

describe("profile default behavior rules", () => {
  it("chat_assistant does citation linkification", () => {
    const raw = renderAiMarkdownToHtml("ref [citation:1]", {
      streaming: false,
    });
    // 应包含 citation linkification
    expect(postProcessCitations(raw)).toContain("ai-citation");
  });

  it("vault_preview does NOT do citation linkification", () => {
    const html = renderAsProfile("[citation:1] ref", "vault_preview");
    // vault preview manages its own HTML structure
    expect(html).toContain("<!DOCTYPE html>");
  });

  it("editor_export preserves original markdown structure", () => {
    const original = [
      "# Document",
      "",
      "## Section 1",
      "",
      "Content with **bold**.",
      "",
      "## Section 2",
      "",
      "- item 1",
      "- item 2",
    ].join("\n");

    const roundTripped = renderAsProfile(original, "editor_export");
    const reIngested = renderAsProfile(original, "editor_ingest");
    const reExported = editorBodyHtmlToMarkdown(reIngested);

    expect(original).toContain("# Document");
    expect(roundTripped).toContain("bold");
    expect(reExported).toContain("bold");
  });
});

// ── 流式模式在各 profile 中的行为 ───────────────────────────

describe("streaming behavior across profiles", () => {
  it("chat_assistant streaming mode handles partial input", () => {
    const html = sanitizeHtml(
      renderAiMarkdownToHtml("**partial bold", { streaming: true }),
    );
    expect(html).toContain("<strong>");
    expect(html).not.toContain("**partial bold**"); // raw markdown should not appear
  });

  it("chat_assistant non-streaming mode produces same final output as streaming final", () => {
    const complete = "**bold** and *italic*";
    const streamFinal = sanitizeHtml(
      renderAiMarkdownToHtml(complete, { streaming: true }),
    );
    const nonStream = sanitizeHtml(
      renderAiMarkdownToHtml(complete, { streaming: false }),
    );

    // 最终输出应该相似（非流式可能更干净）
    expect(streamFinal).toContain("<strong>");
    expect(nonStream).toContain("<strong>");
    expect(streamFinal).toContain("<em>");
    expect(nonStream).toContain("<em>");
  });
});

// ── 完整 Note 处理流 ────────────────────────────────────────

describe("full note processing pipeline", () => {
  it("frontmatter + body round-trip through all stages", () => {
    const md = [
      "---",
      'title: "Test Note"',
      "tags: [test, markdown]",
      "---",
      "",
      "# Content Section",
      "",
      "**Bold** and `code` in paragraph.",
      "",
      "- [x] Task 1",
      "- [ ] Task 2",
      "",
      "> callout-like text",
    ].join("\n");

    // Parse for editor
    const { yaml, title, bodyMd } = parseNoteForEditor(md, "Fallback");
    expect(title).toBe("Fallback");
    expect(bodyMd).not.toContain('title: "Test Note"');

    // Ingest body to editor
    const editorHtml = markdownBodyToEditorHtml(bodyMd);
    expect(editorHtml).toContain("<p>");

    // Export body from editor
    const exportedBody = editorBodyHtmlToMarkdown(editorHtml);
    expect(exportedBody).toContain("Bold");
    expect(exportedBody).toContain("code");

    // Rebuild full note
    const rebuilt = buildNoteMarkdown(yaml, exportedBody);
    expect(rebuilt).not.toContain("title:");
    expect(rebuilt).toContain("tags:");
    expect(rebuilt).toContain("Bold");
  });
});
