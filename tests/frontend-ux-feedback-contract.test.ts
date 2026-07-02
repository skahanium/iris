import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("frontend UX feedback contract", () => {
  it("shows explicit empty search results and AI copy feedback", () => {
    const search = read("src/components/file/SearchPanel.tsx");
    const messages = read("src/components/ai/AiMessageList.tsx");

    expect(search).toContain("未找到匹配结果");
    expect(search).toContain("试试更具体的关键词，或切换语义搜索。");
    expect(messages).toContain("copyStatus");
    expect(messages).toContain('aria-live="polite"');
    expect(messages).toContain("已复制回答");
    expect(messages).toContain("复制失败");
  });
});
