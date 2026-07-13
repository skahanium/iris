import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("model routing and Run execution contracts", () => {
  it("keeps capability-slot routing types for configured providers", () => {
    const llmTypes = read("src/types/llm.ts");

    for (const slot of [
      "fast",
      "writer",
      "reasoner",
      "long_context",
      "vision",
    ]) {
      expect(llmTypes).toContain(slot);
    }
    expect(llmTypes).toContain("USER_CONFIGURABLE_CAPABILITY_SLOTS");
    expect(llmTypes).toContain("EndpointFamily");
  });

  it("renders provider configuration before capability slot routing", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain("USER_CONFIGURABLE_CAPABILITY_SLOTS.map");
    expect(section).toContain("AddModelWizard");
    expect(section).toContain("addProviderModel");
    expect(section).toContain("removeProviderModel");
    expect(section).not.toContain("AI_SCENES.map");
  });

  it("keeps credentials provider-scoped and model identifiers user-entered", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const rust = read("src-tauri/src/commands/llm_config_commands.rs");

    expect(section).toContain("apiKeyOverride");
    expect(section).toContain('autoCapitalize="none"');
    expect(section).toContain("spellCheck={false}");
    expect(rust).toContain("resolve_for_provider_without_secret");
  });

  it("keeps base URL editing scoped to custom providers", () => {
    const section = read("src/components/settings/LlmRoutingSection.tsx");

    expect(section).toContain("providerRequiresBaseUrl");
    expect(section).toContain("isCustomProviderId(provider.id)");
    expect(section).not.toContain('provider.id === "mimo"');
  });

  it("starts the unified assistant through a scene-free Run request", () => {
    const sender = read("src/components/ai/hooks/useUnifiedAssistantSend.ts");
    const run = read("src/hooks/useAssistantRun.ts");

    expect(sender).toContain("start({");
    expect(sender).toContain("explicitReferences");
    expect(sender).toContain("securityDomain: aiDomain");
    expect(run).toContain("assistantRunStart");
    expect(run).toContain("assistantRunControl");
    expect(run).toContain("listenAssistantRunEvent");
    expect(sender).not.toContain("scene");
  });
});
