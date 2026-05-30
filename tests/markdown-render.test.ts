import { describe, expect, it } from "vitest";

import {
  parseMarkdownToHtml,
  repairStreamingMarkdown,
  renderAiMarkdownToHtml,
} from "@/lib/markdown-render";

describe("repairStreamingMarkdown", () => {
  it("closes unbalanced code fence", () => {
    const input = "text\n```rust\nfn main() {";
    const repaired = repairStreamingMarkdown(input);
    expect(repaired.endsWith("```")).toBe(true);
    // 成对 fence 时 split 段数为奇数
    expect(repaired.split("```").length % 2).toBe(1);
  });

  it("leaves balanced fences unchanged", () => {
    const input = "```\ncode\n```";
    expect(repairStreamingMarkdown(input)).toBe(input);
  });

  it("closes unbalanced bold markers", () => {
    const repaired = repairStreamingMarkdown("**partial");
    expect(repaired.endsWith("**")).toBe(true);
    const html = parseMarkdownToHtml(repaired);
    expect(html).toContain("<strong>");
  });
});

describe("parseMarkdownToHtml", () => {
  it("renders bold without throwing", () => {
    const html = parseMarkdownToHtml("**hello**");
    expect(html).toContain("<strong>");
  });

  it("streaming mode tolerates open fence", () => {
    const html = parseMarkdownToHtml("```\nline", { streaming: true });
    expect(html.length).toBeGreaterThan(0);
    expect(html).not.toContain('<pre class="text-muted');
  });

  it("renderAiMarkdownToHtml preserves bold with citations", () => {
    const html = renderAiMarkdownToHtml("**标题** 与 [citation:1]");
    expect(html).toContain("<strong>标题</strong>");
    expect(html).toContain("ai-citation");
  });
});
