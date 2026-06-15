import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("model provider registry contract", () => {
  it("splits provider health, model catalog, and capability routing", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain('data-section="llm-providers"');
    expect(section).toContain('data-section="llm-model-catalog"');
    expect(section).toContain('data-section="llm-capability-routing"');
    expect(section).toContain("llmConfigTestProvider");
    expect(section).toContain("llmModelRegistryRefresh");
    expect(section).toContain("llmModelValidate");
    expect(section).toContain("vision");
  });

  it("does not expose Ollama in the external provider settings panel", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const providers = read("src-tauri/src/llm/providers.rs");

    expect(section).not.toContain('name: "Ollama"');
    expect(section).not.toContain('provider.id === "ollama"');
    expect(section).not.toContain('keyless: providerId === "ollama"');
    expect(providers).toContain("list_external_providers_from_routing");
  });

  it("adds typed IPC wrappers for registry operations", () => {
    const ipc = read("src/lib/ipc.ts");
    const types = read("src/types/llm.ts");
    const rust = read("src-tauri/src/commands/llm_config_commands.rs");

    expect(types).toContain("ModelRegistryEntry");
    expect(types).toContain("ModelValidationKind");
    expect(ipc).toContain("llmConfigTestProvider");
    expect(ipc).toContain("llmModelRegistryRefresh");
    expect(ipc).toContain("llmModelValidate");
    expect(ipc).toContain("llmModelConfirmCapability");
    expect(rust).toContain("llm_config_test_provider");
    expect(rust).toContain("llm_model_registry_refresh");
    expect(rust).toContain("llm_model_validate");
    expect(rust).toContain("llm_model_confirm_capability");
  });

  it("filters capability route candidates by verified capability", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain("modelsForSlot");
    expect(section).toContain("supportsModelForSlot");
    expect(section).toContain("userConfirmedCapabilities");
    expect(section).toContain("visionVerifiedAt");
    expect(section).not.toContain("routeModelsForProvider(providerId)");
  });

  it("treats provider model lists as advisory during validation", () => {
    const rust = read("src-tauri/src/commands/llm_config_commands.rs");

    expect(rust).toContain("check_model_list_for_validation");
    expect(rust).toContain("AdvisoryMissing");
    expect(rust).not.toContain('供应商模型列表中没有这个模型 ID".into()');
  });
});
