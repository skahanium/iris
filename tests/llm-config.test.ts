import { describe, expect, it } from "vitest";

import {
  CAPABILITY_SLOTS,
  USER_CONFIGURABLE_CAPABILITY_SLOTS,
} from "@/types/llm";

describe("llm routing serialization shape", () => {
  it("accepts minimal capability-slot routing JSON without a scene selector", () => {
    const routing = {
      version: 1,
      providers: {},
      slots: {
        fast: {
          providerId: "deepseek",
          model: "deepseek-v4-flash",
          thinking: false,
        },
      },
      contextStrategy: {},
    };
    expect(CAPABILITY_SLOTS).toContain("fast");
    expect(routing.slots.fast.model).toBe("deepseek-v4-flash");
    expect(JSON.stringify(routing)).not.toContain("scene");
  });

  it("exposes the dedicated agent-tools route as a configurable capability", () => {
    expect(CAPABILITY_SLOTS).toContain("agent_tools");
    expect(USER_CONFIGURABLE_CAPABILITY_SLOTS).toContain("agent_tools");
  });
});
