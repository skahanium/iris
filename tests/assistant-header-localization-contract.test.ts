import { readFileSync } from "node:fs";

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
});
