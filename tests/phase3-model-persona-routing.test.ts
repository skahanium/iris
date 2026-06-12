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

    expect(section).toContain("能力槽模型路由");
    expect(section).toContain("CAPABILITY_SLOTS");
    expect(section).toContain("connection");
    expect(section).toContain("vision");
    expect(section).toContain("tools");
    expect(section).not.toContain("场景模型路由");
    expect(section).not.toContain("AI_SCENES.map");
  });

  it("shows model route and persona layers in run plan UI without sensitive fields", () => {
    const summary = read("src/components/ai/RunPlanSummary.tsx");
    const drawer = read("src/components/ai/RunPlanDrawer.tsx");
    const combined = `${summary}\n${drawer}`;

    expect(drawer).toContain("Model");
    expect(drawer).toContain("Persona");
    expect(drawer).toContain("modelRoute");
    expect(drawer).toContain("personaLayers");
    expect(summary).toContain("modelRoute");
    expect(combined).not.toContain("noteContent");
    expect(combined).not.toContain("base64");
    expect(combined).not.toContain("clipboard");
    expect(combined).not.toContain("apiKey");
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
