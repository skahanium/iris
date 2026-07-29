import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("model pool settings contract", () => {
  it("renders one global model pool with primary-to-fallback ordering", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain('data-section="llm-model-pool"');
    expect(section).toContain("模型池与主备顺序");
    expect(section).toContain("第一项是主模型，后两项是备用模型");
    expect(section).toContain("candidateOrder");
    expect(section).toContain("moveCandidate");
    expect(section).not.toContain("能力槽模型路由");
    expect(section).not.toContain("Agent tools");
    expect(section).not.toContain("llm-capability-routing");
  });

  it("keeps model validation as capability facts without slot confirmation", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const ipc = read("src/lib/ipc.ts");
    const types = read("src/types/llm.ts");

    expect(section).toContain("llmConfigTestProvider");
    expect(section).toContain("llmModelRegistryRefresh");
    expect(section).toContain("llmModelValidate");
    expect(section).toContain("模型池与主备顺序");
    expect(section).toContain("modelSupportsVision");
    expect(section).not.toContain("modelSupportsSlot");
    expect(section).not.toContain("llmModelConfirmCapability");
    expect(ipc).not.toContain("llmModelConfirmCapability");
    expect(types).not.toContain("userConfirmedCapabilities");
  });

  it("prevents deleting a provider while it remains in the ordered model pool", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const rust = read("src-tauri/src/commands/llm_config_commands.rs");

    expect(section).toContain("仍在主备模型池中，请先移除其模型");
    expect(rust).toContain("ordered model pool");
    expect(section).not.toContain("usedSlots");
  });

  it("keeps built-in providers in the add flow and reserves base URLs for custom endpoints", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const types = read("src/types/llm.ts");

    expect(section).toContain("endpointManaged");
    expect(section).toContain("providerRequiresBaseUrl");
    expect(section).not.toContain("Base URL（可选）");
    expect(types).toContain('endpointManaged: "builtin" | "custom"');
  });
});
