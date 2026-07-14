import { describe, expect, it } from "vitest";

import { DEFAULT_LLM_ROUTING } from "@/types/llm";

describe("llm routing serialization shape", () => {
  it("uses a model pool config without capability-slot bindings", () => {
    const routing = {
      version: 1,
      providers: {},
      defaultModel: {
        providerId: "deepseek",
        modelId: "deepseek-v4-flash",
      },
    };

    expect(routing.defaultModel.modelId).toBe("deepseek-v4-flash");
    expect(JSON.stringify(routing)).not.toContain("slots");
    expect(JSON.stringify(DEFAULT_LLM_ROUTING)).not.toContain("slots");
  });
});
