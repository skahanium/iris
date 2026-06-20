import { describe, expect, it } from "vitest";

import { SCENE_META, SCENE_OPTIONS } from "@/lib/ai/scene-types";
import { CAPABILITY_SLOTS } from "@/types/llm";

describe("llm routing scenes", () => {
  it("active scene metadata excludes legacy-only scenes", () => {
    const scenes = SCENE_OPTIONS.map(
      (scene) => scene.scene,
    ) as (keyof typeof SCENE_META)[];
    for (const scene of scenes) {
      expect(SCENE_META[scene]?.scene).toBe(scene);
    }
    expect(scenes).toEqual([
      "knowledge_lookup",
      "drafting_assist",
      "research_synthesis",
    ]);
  });
});

describe("llm routing serialization shape", () => {
  it("accepts minimal capability-slot routing JSON", () => {
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
      contextStrategy: {
        knowledge_lookup: "hybrid" as const,
      },
    };
    expect(CAPABILITY_SLOTS).toContain("fast");
    expect(routing.slots.fast.model).toBe("deepseek-v4-flash");
  });
});
