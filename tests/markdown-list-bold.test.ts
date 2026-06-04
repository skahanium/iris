import { describe, expect, it } from "vitest";

import {
  parseMarkdownToHtml,
  renderAiMarkdownToHtml,
} from "@/lib/markdown-render";
import { renderMarkdownWithProfile } from "@/lib/markdown-contract";

describe("markdown list inline bold", () => {
  it("renders **bold** inside unordered list items", () => {
    const md = "- **解散议会争议**：马克龙在2025年解散国民议会。";
    const html = parseMarkdownToHtml(md);
    expect(html).toContain("<strong>解散议会争议</strong>");
    expect(html).not.toContain("**解散议会争议**");
  });

  it("chat_assistant profile renders bold in lists", () => {
    const md = `**马克龙近期动态**

**1. 政治与国内局势**

- **解散议会争议**：马克龙在2025年解散国民议会。
- **拟用公投破僵局**：马克龙表示2025年可能举行公投。`;
    const { output } = renderMarkdownWithProfile(md, "chat_assistant");
    expect(output).toContain("<strong>解散议会争议</strong>");
    expect(output).toContain("<strong>拟用公投破僵局</strong>");
    expect(output).not.toContain("**解散议会争议**");
  });

  it("renderAiMarkdownToHtml matches parse for list bold", () => {
    const md = "- **Key**: value";
    const html = renderAiMarkdownToHtml(md);
    expect(html).toContain("<strong>Key</strong>");
  });
});
