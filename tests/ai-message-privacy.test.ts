import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

describe("AI message logging privacy", () => {
  it("does not log raw message body snippets when markdown rendering fails", () => {
    const source = readFileSync(
      "src/components/ai/AiMessageBubble.tsx",
      "utf8",
    );

    expect(source).not.toContain('content: (renderContent || "").slice');
    expect(source).toContain("summarizeLogContent");
  });
});
