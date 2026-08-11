import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { catalogReasoningCapability } from "@/components/settings/llmRoutingModelHelpers";
import type { ModelCatalogEntry } from "@/types/llm";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("model-level reasoning contract", () => {
  it("shows reasoning as validated model capability, not a slot setting", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const helpers = read("src/components/settings/llmRoutingModelHelpers.ts");
    const types = read("src/types/llm.ts");

    expect(section).toContain("reasoningCapabilitySummary");
    expect(helpers).toContain("推理可用");
    expect(helpers).toContain("推理未知");
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

  it("models MiniMax M3 as switchable and M2 as always-on only on the native provider", () => {
    const m3Catalog = {
      providerId: "minimax",
      supportsThinking: true,
    } as ModelCatalogEntry;

    expect(
      catalogReasoningCapability("minimax", "MiniMax-M3", m3Catalog),
    ).toMatchObject({
      supported: true,
      control: "switch",
      supportedModes: ["off", "auto"],
      defaultMode: "auto",
      disableSupported: true,
    });
    expect(
      catalogReasoningCapability("minimax", "MiniMax-M2.7", undefined),
    ).toMatchObject({
      supported: true,
      control: "switch",
      supportedModes: ["on"],
      defaultMode: "on",
      disableSupported: false,
    });
    expect(
      catalogReasoningCapability("custom", "MiniMax-M3", undefined),
    ).toBeNull();

    const section = read("src/components/settings/LlmRoutingSection.tsx");
    expect(section).not.toContain("modelLooksTagReasoningRisk");
  });
});
