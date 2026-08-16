import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

describe("AI message logging privacy", () => {
  it("does not log assistant message content from the render path", () => {
    const source = readFileSync(
      "src/components/ai/AiMessageBubble.tsx",
      "utf8",
    );

    expect(source).not.toContain('content: (renderContent || "").slice');
    expect(source).not.toContain("contentSummary:");
    expect(source).not.toContain("summarizeLogContent");
  });
});
