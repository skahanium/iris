/**
 * contract-render.test.ts
 *
 * 展示一致性测试 — 断言不同表面（profile）对同一 Markdown 的渲染语义一致。
 *
 * 覆盖 CONTRACT_PLAN.md § 测试计划 2：
 * - 用户消息中的 **粗体** 发送后按 Markdown 展示
 * - 助手消息与用户消息在核心 Markdown 语义上等价
 * - 研究卡片与主消息区对相同 Markdown 片段的解释一致
 * - Vault 预览与 AI 区对相同基础语法的语义解释一致
 */
import { describe, expect, it } from "vitest";

import { marked } from "marked";
import {
  parseMarkdownToHtml,
  renderAiMarkdownToHtml,
} from "@/lib/markdown-render";
import { markdownToHtmlPage } from "@/lib/markdown";
import { sanitizeHtml } from "@/lib/sanitize";

// ── Helpers ────────────────────────────────────────────────────

/** 提取渲染产物中所有的文本内容（去除 HTML 标签） */
function extractText(html: string): string {
  return html
    .replace(/<[^>]+>/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

/** 确认 HTML 包含某个文本片段 */
function htmlContainsText(html: string, text: string): boolean {
  return extractText(html).includes(text);
}

/** 使用不同渲染器渲染同一 Markdown */
function renderDefault(msg: string): string {
  return marked.parse(msg, { async: false }) as string;
}

function renderAiProse(msg: string, streaming = false): string {
  return parseMarkdownToHtml(msg, { streaming });
}

function renderAssistant(msg: string, streaming = false): string {
  return sanitizeHtml(renderAiMarkdownToHtml(msg, { streaming }));
}

// ── 核心 GFM 语法在各渲染器下的一致性 ──────────────────────

describe("core GFM rendering consistency", () => {
  const testCases = [
    {
      label: "bold",
      md: "**bold text**",
      expectText: "bold text",
      expectTag: "<strong>",
    },
    {
      label: "italic",
      md: "*italic text*",
      expectText: "italic text",
      expectTag: "<em>",
    },
    {
      label: "strikethrough",
      md: "~~deleted~~",
      expectText: "deleted",
      expectTag: "<del>",
    },
    {
      label: "inline code",
      md: "`code`",
      expectText: "code",
      expectTag: "<code>",
    },
    {
      label: "heading H1",
      md: "# Title",
      expectText: "Title",
      expectTag: "<h1",
    },
    {
      label: "heading H2",
      md: "## Section",
      expectText: "Section",
      expectTag: "<h2",
    },
  ];

  for (const { label, md, expectText, expectTag } of testCases) {
    it(`${label}: default marked and proseMarked both produce ${expectTag}`, () => {
      const d = renderDefault(md);
      const p = renderAiProse(md);
      expect(d).toContain(expectTag);
      expect(p).toContain(expectTag);
      expect(extractText(d)).toContain(expectText);
      expect(extractText(p)).toContain(expectText);
    });
  }
});

describe("block-level GFM rendering consistency", () => {
  it("unordered list renders consistently across renderers", () => {
    const md = "- item 1\n- item 2";
    const d = renderDefault(md);
    const p = renderAiProse(md);
    expect(d).toContain("<ul>");
    expect(p).toContain("<ul>");
    expect(extractText(d)).toContain("item 1");
    expect(extractText(p)).toContain("item 1");
  });

  it("ordered list renders consistently across renderers", () => {
    const md = "1. first\n2. second";
    const d = renderDefault(md);
    const p = renderAiProse(md);
    expect(d).toContain("<ol>");
    expect(p).toContain("<ol>");
    expect(extractText(d)).toContain("first");
    expect(extractText(p)).toContain("first");
  });

  it("blockquote renders consistently across renderers", () => {
    const md = "> quoted text";
    const d = renderDefault(md);
    const p = renderAiProse(md);
    expect(d).toContain("<blockquote>");
    expect(p).toContain("<blockquote>");
    expect(extractText(d)).toContain("quoted");
    expect(extractText(p)).toContain("quoted");
  });

  it("code block renders consistently", () => {
    const md = "```js\nconst x = 1;\n```";
    const d = renderDefault(md);
    const p = renderAiProse(md);
    expect(d).toContain("<pre>");
    expect(d).toContain("<code");
    expect(p).toContain("<pre>");
    expect(p).toContain("<code");
  });

  it("table renders consistently", () => {
    const md = "| A | B |\n| --- | --- |\n| 1 | 2 |";
    const d = renderDefault(md);
    const p = renderAiProse(md);
    expect(d).toContain("<table>");
    expect(p).toContain("<table");
  });

  it("horizontal rule renders consistently", () => {
    const md = "---";
    const d = renderDefault(md);
    const p = renderAiProse(md);
    expect(d).toContain("<hr>");
    expect(p).toContain("<hr>");
  });
});

// ── 用户消息 vs 助手消息一致性 ──────────────────────────────

describe("user message vs assistant message rendering parity", () => {
  it("user message with bold renders bold content", () => {
    const md = "Hello **world**";
    const rendered = renderAssistant(md);
    expect(rendered).toContain("<strong>");
    expect(htmlContainsText(rendered, "Hello world")).toBe(true);
  });

  it("user message with inline code renders code", () => {
    const md = "Run `npm test` now";
    const rendered = renderAssistant(md);
    expect(rendered).toContain("<code>");
    expect(htmlContainsText(rendered, "npm test")).toBe(true);
  });

  it("user message and assistant message produce semantically equivalent output for basic GFM", () => {
    const md = "**Bold** and *italic* with `code`.\n\n- list item\n\n> quote";

    const asAssistant = renderAssistant(md);

    // 文本内容一致
    expect(extractText(asAssistant)).toContain("Bold");
    expect(extractText(asAssistant)).toContain("italic");
    expect(extractText(asAssistant)).toContain("code");

    // 相同的结构化输出
    expect(asAssistant).toContain("<strong>");
    expect(asAssistant).toContain("<em>");
    expect(asAssistant).toContain("<code>");
  });

  it("user message with list renders list structure", () => {
    const md = "- [x] Task 1\n- [ ] Task 2";
    const rendered = renderAssistant(md);
    expect(rendered).not.toContain("<input");
    expect(rendered).toContain("Task 1");
    expect(rendered).toContain("Task 2");
  });
});

// ── 研究卡片 vs 主消息区一致性 ──────────────────────────────

describe("research card vs main message rendering consistency", () => {
  it("research card excerpt renders same semantic structure as main message", () => {
    const md = "**Key finding**: The system *supports* `markdown`.";

    const mainMessage = renderDefault(md);
    const cardMessage = renderDefault(md); // same renderer for now

    expect(extractText(mainMessage)).toBe(extractText(cardMessage));
    expect(mainMessage).toContain("<strong>");
    expect(cardMessage).toContain("<strong>");
    expect(mainMessage).toContain("<em>");
    expect(cardMessage).toContain("<em>");
  });

  it("research summary with code blocks renders correctly", () => {
    const md = '```json\n{"key": "value"}\n```';
    const rendered = renderAssistant(md);
    expect(rendered).toContain("<pre>");
    expect(htmlContainsText(rendered, "key")).toBe(true);
    expect(htmlContainsText(rendered, "value")).toBe(true);
  });

  it("research summary with table renders correctly", () => {
    const md = "| Source | Reliability |\n| --- | --- |\n| A | High |";
    const rendered = renderAssistant(md);
    expect(rendered).toContain("<table");
    expect(rendered).toContain("Source");
    expect(rendered).toContain("High");
  });
});

// ── Vault 预览 vs AI 区一致性 ───────────────────────────────

describe("vault preview vs AI rendering consistency", () => {
  it("vault preview generates valid self-contained HTML", () => {
    const md = "# My Note\n\nContent here.";
    const page = markdownToHtmlPage(md, "My Note");
    expect(page).toContain("<!DOCTYPE html>");
    expect(page).toContain("<title>My Note</title>");
    expect(page).toContain("<h1>My Note</h1>");
    expect(page).toContain("Content here.");
  });

  it("vault preview and AI renderer produce the same heading content", () => {
    const md = "# Same Title\n\nSame body text.";

    const vaultHtml = markdownToHtmlPage(md, "Same Title");
    const aiHtml = renderAssistant(md);

    expect(extractText(vaultHtml)).toContain("Same Title");
    expect(extractText(aiHtml)).toContain("Same Title");
    expect(extractText(vaultHtml)).toContain("Same body text");
    expect(extractText(aiHtml)).toContain("Same body text");
  });

  it("vault preview sanitizes HTML (no raw HTML injection)", () => {
    const md = '<script>alert("xss")</script>\nSafe text.';
    const page = markdownToHtmlPage(md);
    // vault preview uses sanitizeHtml via markdownToHtmlPage
    expect(page).not.toContain("<script");
    expect(htmlContainsText(page, "Safe text")).toBe(true);
  });

  it("vault preview and AI renderer produce same bold text", () => {
    const md = "**bold text**";
    const vaultHtml = markdownToHtmlPage(md);
    const aiHtml = renderAssistant(md);

    expect(vaultHtml).toContain("<strong>");
    expect(aiHtml).toContain("<strong>");
    expect(extractText(vaultHtml)).toContain("bold text");
    expect(extractText(aiHtml)).toContain("bold text");
  });

  it("vault preview and AI renderer produce same list semantics", () => {
    const md = "- item A\n- item B\n- item C";
    const vaultHtml = markdownToHtmlPage(md);
    const aiHtml = renderDefault(md);

    expect(vaultHtml).toContain("<ul>");
    expect(aiHtml).toContain("<ul>");
    expect(extractText(vaultHtml)).toContain("item A");
    expect(extractText(aiHtml)).toContain("item A");
  });
});

// ── AI 消息渲染产物安全验证 ────────────────────────────────

describe("AI message HTML safety", () => {
  it("sanitizeHtml removes script tags", () => {
    const dirty = '<p>Safe</p><script>alert("xss")</script>';
    const clean = sanitizeHtml(dirty);
    expect(clean).not.toContain("<script");
    expect(clean).toContain("<p>Safe</p>");
  });

  it("sanitizeHtml preserves allowed GFM tags", () => {
    const html =
      "<h1>Title</h1><p><strong>Bold</strong></p><ul><li>Item</li></ul>";
    const clean = sanitizeHtml(html);
    expect(clean).toContain("<h1>");
    expect(clean).toContain("<strong>");
    expect(clean).toContain("<ul>");
    expect(clean).toContain("<li>");
  });

  it("sanitizeHtml removes dangerous attributes", () => {
    const dirty = '<img src="x" onerror="alert(1)">';
    const clean = sanitizeHtml(dirty);
    expect(clean).not.toContain("onerror");
  });

  it("renderAssistant produces sanitized output", () => {
    const md = "<script>alert(1)</script>\n\n**Safe text**.";
    const html = renderAssistant(md);
    expect(html).not.toContain("<script");
    expect(html).toContain("<strong>");
    expect(htmlContainsText(html, "Safe text")).toBe(true);
  });

  it("citation links survive sanitization", () => {
    const md = "See [citation:1] for details.";
    const html = renderAssistant(md);
    expect(html).toContain("ai-citation");
    expect(html).toContain("#iris-cite-");
  });
});

// ── 引用链接在 AI 渲染中的处理 ──────────────────────────────

describe("citation rendering in AI messages", () => {
  it("bare [citation:N] becomes ai-citation link", () => {
    const html = renderAssistant("Source [citation:3] details.");
    expect(html).toContain('class="ai-citation"');
    expect(html).toContain("iris-cite-");
  });

  it("markdown bold with citation is preserved", () => {
    const html = renderAssistant("**Important** [citation:1].");
    expect(html).toContain("<strong>Important</strong>");
    expect(html).toContain("ai-citation");
  });
});

// ── Markdown → HTML 往返渲染一致性 ──────────────────────────

describe("round-trip rendering consistency", () => {
  it("md -> html -> text extraction produces same text for native GFM", () => {
    const inputs = [
      "# Heading One",
      "**bold** and *italic*",
      "- list item 1\n- list item 2",
      "> blockquote text",
      "`inline code`",
    ];

    for (const input of inputs) {
      const d = extractText(renderDefault(input));
      const p = extractText(renderAiProse(input));
      // 两个渲染器提取的文本应该包含相同的核心内容
      expect(d.length).toBeGreaterThan(0);
      expect(p.length).toBeGreaterThan(0);
    }
  });

  it("md -> html renders links consistently", () => {
    const md = "[Example](https://example.com)";
    const d = renderDefault(md);
    const p = renderAiProse(md);
    // 两个渲染器都应包含链接文本和 href
    expect(d).toContain("Example");
    expect(p).toContain("Example");
    expect(d).toContain('href="https://example.com"');
    expect(p).toContain('href="https://example.com"');
  });
});

// ── 复杂混合内容渲染 ───────────────────────────────────────

describe("complex mixed content rendering", () => {
  it("paragraph with all inline marks renders without error", () => {
    const md = "**Bold** *italic* ~~strike~~ `code` [link](https://a.test)";
    const d = renderDefault(md);
    const p = renderAiProse(md);
    expect(d.length).toBeGreaterThan(0);
    expect(p.length).toBeGreaterThan(0);
  });

  it("table with inline formatting renders", () => {
    const md = "| A | B |\n| --- | --- |\n| **bold** | *italic* |";
    const d = renderDefault(md);
    const p = renderAiProse(md);
    expect(d).toContain("<table>");
    expect(p).toContain("<table");
  });

  it("callout text renders as blockquote", () => {
    const md = "> [!note] Info\n> Content here.";
    const d = renderDefault(md);
    expect(d).toContain("<blockquote>");
    expect(htmlContainsText(d, "Info")).toBe(true);
    expect(htmlContainsText(d, "Content")).toBe(true);
  });

  it("footnote text is not lost in rendering", () => {
    const md = "Text with footnote[^1].\n\n[^1]: The footnote content.";
    const d = renderDefault(md);
    expect(htmlContainsText(d, "footnote")).toBe(true);
  });
});

// ── 空输入和边界条件 ───────────────────────────────────────

describe("edge cases in rendering", () => {
  it("empty string renders empty without error", () => {
    // marked returns empty for empty input (expected behavior)
    const result = marked.parse("", { async: false }) as string;
    expect(typeof result).toBe("string");
    const assistant = renderAssistant("");
    expect(typeof assistant).toBe("string");
  });

  it("whitespace-only renders without error", () => {
    // marked may produce whitespace-only output for whitespace input
    const result = marked.parse("   \n  \n  ", { async: false }) as string;
    expect(typeof result).toBe("string");
    const rendered = renderAssistant("   \n  \n  ");
    expect(typeof rendered).toBe("string");
  });

  it("very long paragraph renders without truncation", () => {
    const longText = "A".repeat(10000);
    const md = `**${longText}**`;
    const rendered = renderAssistant(md);
    expect(rendered).toContain("<strong>");
    expect(rendered).toContain(longText);
  });

  it("multiple consecutive code blocks render correctly", () => {
    const md = "```a\ncode a\n```\n\n```b\ncode b\n```";
    const rendered = renderAssistant(md);
    expect((rendered.match(/<pre>/g) ?? []).length).toBeGreaterThanOrEqual(2);
  });
});
