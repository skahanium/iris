import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return "";
  }
}

describe("status bar assistant chrome", () => {
  it("StatusBar accepts assistantChrome and renders token usage", () => {
    const bar = read("src/components/layout/StatusBar.tsx");
    expect(bar).toContain("assistantChrome");
    expect(bar).toContain("StatusBarTokenUsage");
    expect(bar).toContain("toolActivityLabel");
  });

  it("StatusBar never renders classified vault lock state in the global status line", () => {
    const bar = read("src/components/layout/StatusBar.tsx");
    const app = read("src/App.impl.tsx");

    expect(bar).toContain("isClassifiedStatusLine");
    expect(bar).not.toContain("{statusLine}");
    expect(app).not.toContain("涉密保险库已锁定");
  });

  it("StatusBarTokenUsage shows cumulative summary only", () => {
    const token = read("src/components/layout/StatusBarTokenUsage.tsx");
    expect(token).toContain("累计");
    expect(token).not.toContain("本轮");
    expect(token).toContain('data-testid="status-bar-token-usage"');
  });

  it("AiMessageList does not render tool call bubbles", () => {
    const list = read("src/components/ai/AiMessageList.tsx");
    expect(list).not.toContain("ToolCallList");
    expect(list).not.toContain("ToolCallBubble");
  });

  it("UnifiedAssistantPanel does not mount panel token or context status bars", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(panel).not.toContain("TokenUsageBar");
    expect(panel).not.toContain("ContextStatusBar");
    expect(panel).not.toContain("HarnessActivityStrip");
  });
});
