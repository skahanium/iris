import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("model-level reasoning contract", () => {
  it("shows reasoning as validated model capability, not a slot setting", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const types = read("src/types/llm.ts");

    expect(section).toContain("reasoningCapabilitySummary");
    expect(section).toContain("推理可用");
    expect(section).toContain("推理未知");
    expect(section).not.toContain("推理开关");
    expect(section).not.toContain("推理强度");
    expect(types).toContain("ModelCapabilityOverride");
    expect(types).not.toContain("ReasoningSlotConfig");
  });

  it("persists providers and their ordered primary-to-fallback candidates", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const types = read("src/types/llm.ts");

    expect(section).toContain("candidateOrder: normalizeCandidateOrder(");
    expect(section).toContain("defaultModel: null");
    expect(section).not.toContain("contextStrategy");
    expect(section).not.toContain("routing.slots");
    expect(types).toContain("candidateOrder: ModelReference[]");
    expect(types).not.toContain("SlotRoute");
  });
});
