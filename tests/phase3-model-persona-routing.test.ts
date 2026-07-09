import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Phase3 model and persona routing contract", () => {
  it("exposes capability slot route and run-plan model/persona metadata types", () => {
    const aiTypes = read("src/types/ai.ts");
    const llmTypes = read("src/types/llm.ts");

    for (const slot of [
      "fast",
      "writer",
      "reasoner",
      "long_context",
      "vision",
      "agent_tools",
      "embedding",
      "reranker",
      "local_private",
    ]) {
      expect(llmTypes).toContain(slot);
    }

    expect(aiTypes).toContain("CapabilityRouteSummary");
    expect(aiTypes).toContain("PersonaLayerSummary");
    expect(aiTypes).toContain("modelRoute");
    expect(aiTypes).toContain("personaLayers");
    expect(llmTypes).toContain("EndpointFamily");
    expect(llmTypes).toContain("ProbeStrategy");
    expect(llmTypes).toContain(
      "slots: Partial<Record<CapabilitySlot, SlotRoute>>",
    );
  });

  it("updates model settings from scene routes to capability slots", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const llmTypes = read("src/types/llm.ts");

    expect(section).toContain("能力槽模型路由");
    expect(section).toContain("USER_CONFIGURABLE_CAPABILITY_SLOTS");
    expect(section).toContain("USER_CONFIGURABLE_CAPABILITY_SLOTS.map");
    expect(section).not.toContain("{CAPABILITY_SLOTS.map");
    expect(llmTypes).toContain("USER_CONFIGURABLE_CAPABILITY_SLOTS");
    expect(llmTypes).toMatch(
      /USER_CONFIGURABLE_CAPABILITY_SLOTS[\s\S]*"fast"[\s\S]*"writer"[\s\S]*"reasoner"[\s\S]*"long_context"[\s\S]*"vision"/,
    );
    expect(section).toContain("connection");
    expect(section).toContain("vision");
    expect(section).toContain("tools");
    expect(section).not.toContain('label: "Agent tools"');
    expect(section).not.toContain('label: "Embedding"');
    expect(section).not.toContain('label: "Reranker"');
    expect(section).not.toContain('label: "Local private"');
    expect(section).not.toContain("场景模型路由");
    expect(section).not.toContain("AI_SCENES.map");
  });

  it("uses provider-level credentials with user-entered activated model ids", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const llmTypes = read("src/types/llm.ts");
    const rustConfig = read("src-tauri/src/llm/config.rs");
    const ipc = read("src/lib/ipc.ts");
    const rust = read("src-tauri/src/commands/llm_config_commands.rs");

    expect(section).toContain("visibleProviders");
    expect(section).toContain('data-testid="llm-provider-card"');
    expect(section).toContain("添加供应商");
    expect(section).toContain("AddModelWizard");
    expect(section).toContain("enabledModelsForProvider");
    expect(section).toContain("addProviderModel");
    expect(section).toContain("removeProviderModel");
    expect(section).toContain("newModelInputs");
    expect(section.indexOf("供应商配置")).toBeLessThan(
      section.indexOf("能力槽模型路由"),
    );
    expect(section).toContain(
      "llmConfigTestProvider(provider.id, apiKeyOverride)",
    );
    expect(section).toContain("llmModelValidate(");
    expect(section).toContain("apiKeyOverride");
    expect(section).not.toContain("llmConfigTest(provider.id, model.id)");
    expect(section).not.toContain("llmConfigTest(provider.id, defaultModel)");
    expect(section).not.toContain("catalogModelsForProvider");
    expect(section).not.toContain("toggleProviderModel");
    expect(section).not.toContain("catalog.length > 0");
    expect(section).not.toContain("currentModel: string");
    expect(section).not.toContain("provider?.default_model ||");
    expect(section).not.toContain("llmConfigApplyDeepseekDefaults");
    expect(section).not.toContain("DeepSeek 推荐");
    expect(section).toContain("未添加模型时不会激活或展示任何模型");
    expect(section).toContain("先在供应商配置中添加模型");
    expect(section).toContain("模型 ID，如 deepseek-v4-flash");
    expect(llmTypes).toContain("enabledModels?: string[] | null");
    expect(rustConfig).toContain("pub enabled_models: Option<Vec<String>>");
    expect(ipc).toContain("model?: string");
    expect(ipc).toContain("llm_config_test");
    expect(rust).toContain("model: Option<String>");
    expect(rust).toContain("api_key_override: Option<String>");
    expect(rust).toContain("resolve_for_provider_without_secret");
  });

  it("uses current MiMo v2.5 catalog labels instead of the old experimental placeholder", () => {
    const providers = read("src-tauri/src/llm/providers.rs");
    const catalog = read("src-tauri/src/llm/model_catalog.rs");
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(providers).toContain('"mimo", "MiMo", "MiMo-V2.5-Pro"');
    expect(catalog).toContain("MiMo-V2.5-Pro");
    expect(catalog).toContain("MiMo-V2.5-Pro-UltraSpeed");
    expect(catalog).toContain("MiMo-V2.5");
    expect(catalog).toContain("MiMo-V2.5-ASR");
    expect(catalog).toContain("MiMo-V2.5-TTS");
    expect(catalog).not.toContain('id: "mimo-vl-7b-experimental"');
    expect(catalog).not.toContain('display_name: "MiMo VL 7B Experimental"');
    expect(providers).not.toContain("MiMo Experimental");
    expect(providers).toContain("endpoint_managed: endpoint_managed(id)");
    expect(section).not.toContain("MiMo 需配置 Base URL");
  });

  it("keeps base URL editing scoped to custom endpoints", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain("providerRequiresBaseUrl");
    expect(section).toContain("isCustomProviderId(provider.id)");
    expect(section).toContain("自定义端点 Base URL");
    expect(section).not.toContain('defaultValue={provider.baseUrl ?? ""}');
    expect(section).not.toContain('provider.id === "mimo"');
  });

  it("removes Ollama from public model provider settings and types", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const providers = read("src-tauri/src/llm/providers.rs");
    const commands = read("src-tauri/src/commands/llm_config_commands.rs");
    const llmTypes = read("src/types/llm.ts");

    expect(section).not.toContain('name: "Ollama"');
    expect(section).not.toContain('provider.id === "ollama"');
    expect(section).not.toContain('providerId === "ollama"');
    expect(providers).toContain("list_external_providers_from_routing");
    expect(commands).toContain(
      "list_external_providers_from_routing(&routing)",
    );
    expect(llmTypes).not.toContain('providerId: "ollama"');
    expect(llmTypes).not.toContain('"ollama_chat"');
    expect(llmTypes).not.toContain('"ollama_tags_then_chat"');
  });

  it("preserves lower-case manual model ids instead of mapping them to catalog labels", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).not.toContain("normalizeModelId(modelId)");
    expect(section).not.toContain('"mimo-v2.5-pro": "MiMo-V2.5-Pro"');
    // Case-insensitive catalog lookup added for MiMo reasoning detection;
    // modelId.toLowerCase() in findModelCatalogForProvider is expected.
    expect(section).toContain("modelId.toLowerCase()");
    expect(section).toContain("{model.id}");
    expect(section).toContain("model.catalog?.displayName");
  });

  it("does not allow fallback routing state to overwrite saved provider settings", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain("disabled={saving || Boolean(loadError)}");
    expect(section).not.toContain("setRouting(DEFAULT_LLM_ROUTING);");
  });

  it("preserves unsaved provider model edits when diagnostics refresh registry data", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain("preserveRouting");
    expect(section).toContain("await load({ preserveRouting: true })");
    expect(section).not.toContain("if (result.ok) await load();");
  });

  it("disables model id autocapitalization and spelling correction", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain('autoCapitalize="none"');
    expect(section).toContain('autoCorrect="off"');
    expect(section).toContain("spellCheck={false}");
  });

  it("keeps model route and persona layers internal without exposing a run-plan panel", () => {
    const aiTypes = read("src/types/ai.ts");
    const hook = read("src/components/ai/hooks/useAssistantRunPlan.tsx");

    expect(aiTypes).toContain("modelRoute");
    expect(aiTypes).toContain("personaLayers");
    expect(hook).toContain("blockedCount");
    expect(hook).toContain("confirmationCount");
    expect(hook).not.toContain("components/ai/RunPlanSummary");
    expect(hook).not.toContain("components/ai/RunPlanDrawer");
    expect(hook).not.toContain("noteContent");
    expect(hook).not.toContain("base64");
    expect(hook).not.toContain("clipboard");
    expect(hook).not.toContain("apiKey");
  });

  it("uses the resolved capability route for unified assistant execution, not just display", () => {
    const assistant = read("src-tauri/src/commands/assistant_commands.rs");
    const harnessTask = read("src-tauri/src/ai_harness/harness_task.rs");
    const aiCommands = read("src-tauri/src/commands/ai_commands.rs");
    const checkpoint = read("src-tauri/src/ai_harness/harness_support.rs");
    const resume = read("src-tauri/src/ai_harness/harness_confirm.rs");

    expect(assistant).toContain("AiSendRoutingOverride");
    expect(assistant).toContain("from_assistant_with_routing");
    expect(harnessTask).toContain("routing_override");
    expect(harnessTask).toContain("execute_ai_send_message_with_routing");
    expect(aiCommands).toContain("to_provider_config_for_slot");
    expect(checkpoint).toContain("capability_slot");
    expect(checkpoint).toContain("endpoint_family");
    expect(resume).toContain("resolve_for_provider");
    expect(resume).toContain("to_provider_config_for_slot");
  });
});
