import { existsSync, readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Agent Task Runtime Phase F legacy scene cleanup", () => {
  it("removes scene_router and SceneProfile from production strategy modules", () => {
    const runtimeMod = read("src-tauri/src/ai_runtime/mod.rs");
    const aiTypes = read("src-tauri/src/ai_types/mod.rs");

    expect(existsSync("src-tauri/src/ai_runtime/scene_router.rs")).toBe(false);
    expect(runtimeMod).not.toContain("pub mod scene_router");
    expect(aiTypes).toContain("pub enum LegacyAiScene");
    expect(aiTypes).not.toContain("pub enum AiScene");
    expect(aiTypes).not.toContain("pub struct SceneProfile");
    expect(aiTypes).not.toContain("pub fn resolve_scene");
    expect(aiTypes).not.toContain("pub fn slot_for_scene");
  });

  it("keeps exemplar learning out of new task policy and context planning paths", () => {
    const policy = read("src-tauri/src/ai_runtime/agent_task_policy.rs");
    const planner = read("src-tauri/src/ai_runtime/context_planner.rs");
    const assistantFacade = read(
      "src-tauri/src/ai_workflows/assistant_facade.rs",
    );

    expect(policy).not.toContain("ExemplarLearning");
    expect(planner).not.toContain("ExemplarLearning");
    expect(assistantFacade).not.toContain("ExemplarLearning");
  });

  it("removes frontend four-scene routing config as a public settings dependency", () => {
    const llmTypes = read("src/types/llm.ts");
    const section = read("src/components/settings/LlmRoutingSection.tsx");
    const providerHook = read("src/hooks/useLlmProvider.ts");

    expect(llmTypes).not.toContain("export const AI_SCENES");
    expect(llmTypes).not.toContain("scenes: Record<string, SceneRoute>");
    expect(llmTypes).not.toContain("exemplar_learning");
    expect(section).not.toContain("DEFAULT_LLM_ROUTING.scenes");
    expect(providerHook).not.toContain("routing.scenes");
  });

  it("updates roadmap and architecture to Agent Task Runtime as the main AI architecture", () => {
    const roadmap = read("ROADMAP.md");
    const architecture = read("ARCHITECTURE.md");

    expect(roadmap).toContain("Agent Task Runtime");
    expect(roadmap).not.toContain("scene_router");
    expect(architecture).toContain("Agent Task Runtime");
    expect(architecture).not.toContain("四场景");
    expect(architecture).not.toContain("scene_router");
  });
});
