import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("model pool settings contract", () => {
  it("renders one global model pool and default model selector", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain('data-section="llm-model-pool"');
    expect(section).toContain("模型池与默认模型");
    expect(section).toContain("选择默认模型");
    expect(section).toContain("defaultModel");
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
    expect(section).toContain("模型池与默认模型");
    expect(section).toContain("modelSupportsVision");
    expect(section).not.toContain("modelSupportsSlot");
    expect(section).not.toContain("llmModelConfirmCapability");
    expect(ipc).not.toContain("llmModelConfirmCapability");
    expect(types).not.toContain("userConfirmedCapabilities");
  });

  it("prevents removing the selected default model or its provider", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const rust = read("src-tauri/src/commands/llm_config_commands.rs");

    expect(section).toContain("是当前默认模型，请先选择其他默认模型");
    expect(section).toContain("包含当前默认模型，请先选择其他默认模型");
    expect(rust).toContain("current default model provider");
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
