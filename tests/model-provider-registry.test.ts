import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("model provider registry contract", () => {
  it("merges model catalog validation into provider cards while keeping routing separate", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain('data-section="llm-providers"');
    expect(section).not.toContain('data-section="llm-model-catalog"');
    expect(section).toContain('data-section="llm-capability-routing"');
    expect(section).toContain("llmConfigTestProvider");
    expect(section).toContain("llmModelRegistryRefresh");
    expect(section).toContain("llmModelValidate");
    expect(section).toContain("检查端点");
    expect(section).toContain("验证模型");
    expect(section).not.toContain("验证视觉");
    expect(section).not.toContain("测试连接");
    expect(section).not.toContain("诊断");
    expect(section.indexOf("验证模型")).toBeLessThan(
      section.indexOf("能力槽模型路由"),
    );
    expect(section).toContain('"text"');
    expect(section).toContain('"vision"');
  });

  it("shows user-facing model capability summary instead of internal validation fields", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain("modelCapabilitySummary");
    expect(section).toContain("文本可用");
    expect(section).toContain("视觉可用");
    expect(section).toContain("视觉不支持");
    expect(section).toContain("未验证");
    expect(section).not.toContain("文本实测通过");
    expect(section).not.toContain("视觉实测通过");
    expect(section).not.toContain("文本已验证");
    expect(section).not.toContain("视觉已验证");
  });

  it("does not show provider discovery refresh warnings as persistent UI copy", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).not.toContain("providerDiscoveryNeedsRefresh");
    expect(section).not.toContain("providerNeedsRefresh(entries");
    expect(section).not.toContain("建议刷新模型目录");
  });

  it("removes model-row capability slot chips and hides catalog metadata by default", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).not.toContain("modelCapabilityLabels");
    expect(section).not.toContain("capabilityLabels.map");
    expect(section).not.toContain("<CapabilityTags model={model.catalog}");
    expect(section).not.toContain("<CapabilityTags model={catalogModel}");
    expect(section).not.toContain("目录元数据");
    expect(section).not.toContain("confirmProviderModelCapability");
    expect(section).not.toContain("llmModelConfirmCapability");
    expect(section).not.toContain("void confirmProviderModelCapability");
  });

  it("keeps Vision candidates limited to actual vision-capable models", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain("modelSupportsSlot");
    expect(section).toContain('slot === "vision"');
    expect(section).toContain("return model.catalog.supportsVision");
    expect(section).toContain("model.registry?.visionVerifiedAt");
    expect(section).toContain("findModelCatalogForProvider");
    expect(section).not.toContain("userConfirmedCapabilities.includes(slot)");
    expect(section).toContain("无可用视觉模型");
  });

  it("limits capability routing providers to configured providers with slot candidates", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain("providersForSlot");
    expect(section).toContain("isProviderConfiguredForRouting");
    expect(section).toContain("routeProviderOptions.map");
    expect(section).toContain("configuredProviderIds");
    expect(section).not.toContain("data.providers.map((p) => (");
    expect(section).not.toContain("addProvider(route.providerId");
  });

  it("keeps built-in providers in the add flow and reserves base URLs for custom endpoints", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const types = read("src/types/llm.ts");

    expect(section).toContain("endpointManaged");
    expect(section).toContain("providerRequiresBaseUrl");
    expect(section).toContain("custom ? (");
    expect(section).not.toContain("Base URL（可选）");
    expect(section).not.toContain('provider.id === "mimo"');
    expect(types).toContain('endpointManaged: "builtin" | "custom"');
  });

  it("starts with no default slot bindings so empty capability slots freeze", () => {
    const types = read("src/types/llm.ts");
    const config = read("src-tauri/src/llm/config.rs");

    expect(types).toContain("slots: {}");
    expect(types).not.toContain('providerId: "deepseek"');
    expect(types).not.toContain('providerId: "mimo"');
    expect(config).toContain("empty_slot_defaults");
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
    expect(section).toContain("modelSupportsSlot");
    expect(section).toContain("visionVerifiedAt");
    expect(section).toContain("textVerifiedAt");
    expect(section).not.toContain("userConfirmedCapabilities.includes(slot)");
    expect(section).not.toContain("routeModelsForProvider(providerId)");
  });

  it("treats provider model lists as advisory during validation", () => {
    const rust = read("src-tauri/src/commands/llm_config_commands.rs");

    expect(rust).toContain("check_model_list_for_validation");
    expect(rust).toContain("AdvisoryMissing");
    expect(rust).not.toContain('供应商模型列表中没有这个模型 ID".into()');
  });
});
