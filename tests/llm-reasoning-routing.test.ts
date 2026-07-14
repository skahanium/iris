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

  it("persists only providers and a global default model", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const types = read("src/types/llm.ts");

    expect(section).toContain("defaultModel: normalized.defaultModel ?? null");
    expect(section).not.toContain("contextStrategy");
    expect(section).not.toContain("routing.slots");
    expect(types).toContain("defaultModel?: ModelReference | null");
    expect(types).not.toContain("SlotRoute");
  });
});
