import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("assistant conversation height contract", () => {
  it("uses a bounded flex column as the message-list scroll owner", () => {
    const list = read("src/components/ai/AiMessageList.tsx");

    expect(list).toContain('className="relative flex min-h-0 flex-1 flex-col"');
    expect(list).toContain(
      'className="min-h-0 flex-1" viewportRef={viewportRef}',
    );
  });

  it("keeps the panel and composer out of the message list's shrink budget", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
    const composer = read("src/components/ai/AssistantComposerDock.tsx");

    expect(panel).toContain(
      'className="ai-sidecar flex h-full min-h-0 flex-col bg-ai-workspace"',
    );
    expect(composer).toMatch(/className=\{cn\(\s*"flex shrink-0 flex-col",/);
  });
});
