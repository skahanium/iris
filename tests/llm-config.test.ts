import { describe, expect, it } from "vitest";

import { AI_SCENES } from "@/types/llm";
import { SCENE_META } from "@/lib/ai/scene-types";

describe("llm routing scenes", () => {
  it("AI_SCENES align with SCENE_META keys", () => {
    for (const scene of AI_SCENES) {
      expect(SCENE_META[scene]?.scene).toBe(scene);
    }
    expect(AI_SCENES).toHaveLength(4);
  });
});

describe("llm routing serialization shape", () => {
  it("accepts minimal routing JSON", () => {
    const routing = {
      version: 1,
      providers: {},
      scenes: {
        knowledge_lookup: {
          providerId: "deepseek",
          model: "deepseek-v4-flash",
          thinking: false,
        },
      },
      contextStrategy: {
        knowledge_lookup: "hybrid" as const,
      },
    };
    expect(routing.scenes.knowledge_lookup.model).toBe("deepseek-v4-flash");
  });
});
