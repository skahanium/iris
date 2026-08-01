import { existsSync, readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("assistant header localization contract", () => {
  it("uses Chinese labels and tooltips for conversation actions", () => {
    const header = read("src/components/ai/AssistantPanelHeader.tsx");
    const history = read("src/components/ai/SessionHistoryDropdown.tsx");

    expect(header).toContain('title="新建对话"');
    expect(header).toContain("新对话");
    expect(header).not.toContain("New chat");
    expect(header).not.toContain("New conversation");

    expect(history).toContain('title="对话历史"');
    expect(history).toContain("历史记录");
    expect(history).not.toContain(">History<");
    expect(history).not.toContain("Conversation history");
  });

  it("uses Chinese labels and tooltips for the focus surface toggle", () => {
    const header = read("src/components/ai/AssistantPanelHeader.tsx");

    expect(header).toContain(
      'title={assistantFocus ? "返回文档" : "展开阅读"}',
    );
    expect(header).toContain(
      'aria-label={assistantFocus ? "返回文档" : "展开阅读"}',
    );
    expect(header).toContain("Maximize2");
    expect(header).toContain("Minimize2");
    expect(header).not.toContain("Expand to read");
    expect(header).not.toContain("Back to document");
    expect(header).not.toContain('title="Maximize"');
  });

  it("removes the non-interactive header status badge and its stale bundle contracts", () => {
    const header = read("src/components/ai/AssistantPanelHeader.tsx");
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
    const bundleContract = read("tests/vite-bundle-contract.test.ts");
    const hygieneContract = read("tests/repo-hygiene.test.ts");

    expect(existsSync("src/components/ai/AgentStatusBadge.tsx")).toBe(false);
    expect(header).not.toContain("AgentStatusBadge");
    expect(panel).not.toContain("webSearchProviderName");
    expect(bundleContract).not.toContain("AgentStatusBadge.tsx");
    expect(hygieneContract).not.toContain("AgentStatusBadge.tsx");
  });
});
