import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

function functionSlice(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("Agent Task Runtime Phase C capability affinity contract", () => {
  it("tool catalog exposes capability affinity and keeps scene affinity legacy-only", () => {
    const types = read("src-tauri/src/ai_types/mod.rs");
    const catalog = read("src-tauri/src/ai_runtime/tool_catalog_impl.rs");
    const capability = read(
      "src-tauri/src/ai_runtime/tool_catalog/capability.rs",
    );
    const executor = read("src-tauri/src/ai_runtime/tool_executor.rs");
    const writeTools = read("src-tauri/src/ai_runtime/tool_catalog/write.rs");

    for (const capability of [
      "ReadNotes",
      "SearchNotes",
      "WriteNotes",
      "PatchDocument",
      "WebFetch",
      "ResearchSynthesis",
      "SkillManagement",
      "VaultOrganize",
    ]) {
      expect(types).toContain(capability);
    }

    expect(catalog).toContain("tool_catalog/capability.rs");
    expect(capability).toContain("pub fn capability_affinity");
    expect(capability).toContain("ToolCapabilityAffinity");
    expect(catalog).toContain("Legacy scenes");
    expect(executor).not.toContain("scene_affinity.contains");
    expect(catalog).not.toContain("scene_allowlist");
    expect(writeTools).not.toContain(
      "scene_affinity: &[AiScene::ExemplarLearning]",
    );
  });

  it("tool policy uses task policy and capability affinity instead of scene mismatch", () => {
    const policy = read("src-tauri/src/ai_runtime/tool_policy.rs");
    const evaluate = functionSlice(
      policy,
      "fn evaluate_entry",
      "/// Minimum autonomy level required",
    );

    expect(policy).toContain("pub task_policy: Option<AgentTaskPolicy>");
    expect(evaluate).toContain("capability_affinity()");
    expect(evaluate).toContain("task_policy");
    expect(evaluate).not.toContain("scene_affinity.contains");
    expect(policy).not.toContain("SceneMismatch");
  });

  it("skill activation ranks by task intent and capability, with legacy scene only as compatibility", () => {
    const parent = read("src-tauri/src/ai_runtime/skills_impl.rs");
    const activation = read("src-tauri/src/ai_runtime/skills/activation.rs");

    expect(parent).toContain("rank_skills_for_task");
    expect(parent).toContain("build_skill_activation_plan_for_task");
    expect(activation).toContain("pub fn rank_skills_for_task");
    expect(activation).toContain("capability_terms_for_skill");
    expect(activation).toContain("legacy_scene_or_vector_match");
    expect(activation).not.toContain("exemplar_learning");
  });

  it("task-facing tool and skill surfaces do not expose legacy scene naming", () => {
    const executor = read("src-tauri/src/ai_runtime/tool_executor.rs");
    const skillModel = read("src-tauri/src/ai_runtime/skills/model.rs");
    const skillActivation = read(
      "src-tauri/src/ai_runtime/skills/activation.rs",
    );
    const ipc = read("src/lib/ipc.ts");
    const skillsPanel = read("src/components/ai/SkillsPanel.tsx");
    const statusBadge = read("src/components/ai/AgentStatusBadge.tsx");

    expect(executor).not.toMatch(/pub fn for_scene\(/);
    expect(executor).not.toContain("auto_tools_for_scene");
    expect(skillModel).toContain("task_active");
    expect(skillModel).toContain("task_score");
    expect(skillModel).not.toContain("scene_active");
    expect(skillModel).not.toContain("scene_score");
    expect(skillActivation).toContain("entry.task_active");
    expect(skillActivation).toContain("entry.task_score");
    expect(skillActivation).not.toContain("entry.scene_active");
    expect(skillActivation).not.toContain("entry.scene_score");
    expect(ipc).toContain("task_active?: boolean");
    expect(ipc).toContain("task_score?: number");
    expect(ipc).not.toContain("scene_active?: boolean");
    expect(ipc).not.toContain("scene_score?: number");
    expect(skillsPanel).not.toContain("scene_active");
    expect(statusBadge).not.toContain("scene_active");
  });

  it("MCP capability resolver exists and ignores unapproved MCP annotations", () => {
    const runtimeMod = read("src-tauri/src/ai_runtime/mod.rs");
    const resolver = read("src-tauri/src/ai_runtime/capability_resolver.rs");

    expect(runtimeMod).toContain("pub mod capability_resolver;");
    expect(resolver).toContain("pub fn resolve_required_capability");
    expect(resolver).toContain("unsupported_capability");
    expect(resolver).toContain("missing_mcp_profile");
    expect(resolver).toContain("profile_disabled");
    expect(resolver).toContain("profile_unhealthy");
    expect(resolver).toContain("explicit_mapping_contains_capability");
    expect(resolver).toContain('get("capability")');
    expect(resolver).not.toContain('get("annotations")');
  });
});
