import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("llm reasoning routing contract", () => {
  it("adds reasoning mode controls to non-vision capability slots", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const types = read("src/types/llm.ts");

    expect(section).toContain("思考模式");
    expect(section).toContain("reasoningOptionsForModel");
    expect(section).toContain('slot === "vision"');
    expect(section).toContain('value="不支持"');
    expect(types).toContain('"minimal"');
    expect(types).toContain('"xhigh"');
    expect(types).toContain('"level"');
    expect(types).toContain('"tag"');
    expect(section).toContain("极简");
    expect(section).toContain("极高");
  });

  it("persists structured slot reasoning while keeping legacy thinking", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const types = read("src/types/llm.ts");

    expect(types).toContain("ReasoningMode");
    expect(types).toContain("ReasoningSlotConfig");
    expect(types).toContain("ModelCapabilityOverride");
    expect(types).toContain("modelCapabilities?");
    expect(types).toContain("reasoning?: ReasoningSlotConfig");
    expect(section).toContain("normalizeReasoningSlot(route)");
    expect(section).toContain("reasoning: { mode: value as ReasoningMode }");
    expect(section).toContain("updateModelReasoningOverride");
    expect(section).toContain("自动识别");
    expect(section).toContain("标签隔离");
    expect(types).toContain("supportedModes?: ReasoningMode[]");
    expect(types).toContain("defaultMode?: ReasoningMode | null");
    expect(types).toContain("disableSupported?: boolean | null");
    expect(section).toContain("native_effort");
    expect(section).toContain("native_budget");
    expect(section).toContain("tag_template");
  });

  it("uses capability mode lists instead of supportsThinking boolean only", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain("supportedModes");
    expect(section).toContain("catalogReasoningCapability");
    expect(section).toContain("reasoningCapabilityForModel");
    expect(section).toContain("modelLooksOpenAiReasoning");
    expect(section).toContain("modelLooksGlmReasoning");
    expect(section).toContain("modelLooksQwenReasoning");
    expect(section).toContain("tagOnly");
    expect(section).toContain("可能以正文标签形式返回思考");
  });

  it("does not persist null model capability maps", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const types = read("src/types/llm.ts");

    expect(types).not.toContain(
      "modelCapabilities?: Record<string, ModelCapabilityOverride> | null",
    );
    expect(section).not.toContain("modelCapabilities: null");
    expect(section).toContain("sanitizeProviderOverride");
  });

  it("sanitizes null routing maps before saving", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const saveRouting = section.slice(
      section.indexOf("const saveRouting"),
      section.indexOf("const enabledModelIdsForProvider"),
    );

    expect(section).toContain("function isRecord");
    expect(section).toContain(
      "const rawProviders = isRecord(rawRecord.providers)",
    );
    expect(section).toContain("const normalized = normalizeRouting(source)");
    expect(section).toContain("contextStrategy: normalized.contextStrategy");
    expect(saveRouting).not.toContain("llmConfigSet(routing)");
    expect(saveRouting).toContain("sanitizeRoutingForSave(routing)");
  });

  it("clears stale load errors after successful backend actions", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const clearLoadErrorCount =
      section.match(/setLoadError\(null\);/g)?.length ?? 0;

    expect(clearLoadErrorCount).toBeGreaterThanOrEqual(6);
    expect(section).toContain("await load({ preserveRouting: true })");
    expect(section).toContain("llmConfigDeleteProvider(provider.id)");
  });

  it("closes the add-provider draft after creating a custom endpoint", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain("setWizardOpen(false)");
    expect(section).toContain("onCreateCustom={ensureCustomProvider}");
  });
});
