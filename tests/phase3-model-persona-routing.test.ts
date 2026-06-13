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
    expect(llmTypes).toContain("slots: Record<CapabilitySlot, SlotRoute>");
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
    expect(section).toContain("需配置 Base URL");
  });

  it("keeps model route and persona layers internal without exposing a run-plan panel", () => {
    const aiTypes = read("src/types/ai.ts");
    const hook = read("src/components/ai/hooks/useAssistantRunPlan.tsx");

    expect(aiTypes).toContain("modelRoute");
    expect(aiTypes).toContain("personaLayers");
    expect(hook).toContain("layer: null");
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
