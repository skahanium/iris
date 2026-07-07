import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("llm reasoning routing contract", () => {
  it("adds reasoning mode controls to non-vision capability slots", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const types = read("src/types/llm.ts");

    expect(section).toContain("推理开关");
    expect(section).toContain("推理强度");
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
    expect(section).not.toContain("updateModelReasoningOverride");
    expect(section).not.toContain("reasoning_content");
    expect(section).not.toContain("tag 模板");
    expect(section).not.toContain("标签隔离");
    expect(types).toContain("supportedModes?: ReasoningMode[]");
    expect(types).toContain("defaultMode?: ReasoningMode | null");
    expect(types).toContain("disableSupported?: boolean | null");
    expect(section).not.toContain("native_effort");
    expect(section).not.toContain("native_budget");
    expect(section).not.toContain("tag_template");
  });

  it("uses capability mode lists instead of supportsThinking boolean only", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const catalog = read("src-tauri/src/llm/model_catalog.rs");

    expect(section).toContain("supportedModes");
    expect(section).toContain("catalogReasoningCapability");
    expect(section).toContain("reasoningCapabilityForModel");
    expect(section).toContain("modelLooksOpenAiReasoning");
    expect(section).toContain("modelLooksGlmReasoning");
    expect(section).toContain("modelLooksQwenReasoning");
    expect(section).toContain("tagOnly");
    expect(section).toContain("无强度控制");
    expect(section).not.toContain("已启用内部思考隔离");
    expect(catalog).toContain("DEEPSEEK_REASONING_MODES");
    expect(catalog).toContain("ReasoningMode::Xhigh");
  });

  it("renders separate reasoning switch and strength controls for non-vision slots", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain("推理开关");
    expect(section).toContain("推理强度");
    expect(section).toContain("reasoningSwitchOptionsForModel");
    expect(section).toContain("reasoningStrengthOptionsForModel");
    expect(section).toContain("reasoningLabelForModel");
    expect(section).toContain("不支持");
    expect(section).toContain("不可配置");
    expect(section).toContain("Max");
    expect(section).toContain('slot === "vision"');
  });

  it("keeps model card reasoning status user-facing", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain("reasoningCapabilitySummary");
    expect(section).toContain("推理可用");
    expect(section).toContain("推理未知");
    expect(section).toContain("推理不支持");
    expect(section).toContain("支持强度");
    expect(section).toContain("无强度控制");
    expect(section).toContain("来源：内置目录");
    expect(section).toContain("来源：验证探测");
    expect(section).toContain("来源：用户确认");
    expect(section).not.toContain("仅隔离");
    expect(section).not.toContain("思考能力覆盖");
    expect(section).toContain("if (catalog)");
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
