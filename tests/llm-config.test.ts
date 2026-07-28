import { describe, expect, it } from "vitest";

import { DEFAULT_LLM_ROUTING } from "@/types/llm";

describe("llm routing serialization shape", () => {
  it("uses an ordered model pool without capability-slot bindings", () => {
    const routing = {
      version: 1,
      schemaVersion: 6,
      providers: {},
      candidateOrder: [
        { providerId: "deepseek", modelId: "deepseek-v4-flash" },
      ],
    };

    expect(routing.candidateOrder[0]?.modelId).toBe("deepseek-v4-flash");
    expect(JSON.stringify(routing)).not.toContain("slots");
    expect(JSON.stringify(DEFAULT_LLM_ROUTING)).not.toContain("slots");
    expect(DEFAULT_LLM_ROUTING.schemaVersion).toBe(6);
  });
});
