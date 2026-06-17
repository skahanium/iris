import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Phase4 skills closed-loop contract", () => {
  it("exposes skill activation and blocked capability summaries across the wire", () => {
    const aiTypes = read("src/types/ai.ts");
    const rustTypes = read("src-tauri/src/ai_types/mod.rs");

    for (const token of [
      "SkillActivationPlanSummary",
      "BlockedCapabilitySummary",
      "SkillCapabilitySupportStatus",
      "SkillRuntimeCapability",
      "SkillCompatibilitySource",
      "manage_skills",
    ]) {
      expect(aiTypes).toContain(token);
      expect(rustTypes).toContain(token);
    }

    for (const token of [
      "workspaceRoot",
      "workspaceReady",
      "workspaceMissingItems",
    ]) {
      expect(aiTypes).toContain(token);
    }
    expect(rustTypes).toContain("workspace_root");
    expect(rustTypes).toContain("workspace_ready");
    expect(rustTypes).toContain("workspace_missing_items");
    expect(aiTypes).toContain("skillActivationPlan");
    expect(aiTypes).toContain("blockedCapabilities");
    expect(aiTypes).toContain("auditSummary");
    expect(rustTypes).toContain("skill_activation_plan");
    expect(rustTypes).toContain("blocked_capabilities");
    expect(rustTypes).toContain("audit_summary");
  });

  it("builds a skill activation plan before assistant harness execution", () => {
    const assistant = read("src-tauri/src/commands/assistant_commands.rs");
    const harnessContext = read("src-tauri/src/ai_harness/harness/context.rs");
    const harnessTask = read("src-tauri/src/ai_harness/harness_task.rs");
    const skills = read("src-tauri/src/ai_runtime/skills_impl.rs");

    expect(skills).toContain("build_skill_activation_plan");
    expect(assistant).toContain("build_skill_activation_plan");
    expect(assistant).toContain("with_skill_activation_plan");
    expect(assistant).toContain("build_permission_preflight_summary");
    expect(assistant).toContain("record_skill_activation_matched");
    expect(assistant).toContain("record_skill_activation_used");
    expect(harnessContext).toContain(
      "prepare_environment_and_skills_with_plan",
    );
    expect(harnessContext).toContain(
      "resolve_active_skill_allowed_tools_with_plan",
    );
    expect(harnessTask).toContain("legacy_skill_overlay_from_plan");
    expect(harnessTask).toContain("apply_skill_overlay_to_goal");
  });

  it("keeps Phase4 skill diagnostics in typed IPC and internal run plan data", () => {
    const ipc = read("src/lib/ipc.ts");
    const aiTypes = read("src/types/ai.ts");

    expect(ipc).toContain("lastMatchedAt");
    expect(ipc).toContain("lastUsedAt");
    expect(ipc).toContain("requestedCapabilities");
    expect(ipc).toContain("blockedCapabilities");
    expect(ipc).toContain("compatibilityWarnings");
    expect(ipc).toContain("workspaceRoot");
    expect(ipc).toContain("workspaceReady");
    expect(ipc).toContain("workspaceMissingItems");

    expect(aiTypes).toContain("skillActivationPlan");
    expect(aiTypes).toContain("blockedCapabilities");
    expect(aiTypes).toContain("fallbackGuidance");
    expect(aiTypes).toContain("workspaceRoot");
    expect(aiTypes).toContain("workspaceMissingItems");
  });

  it("keeps sensitive skill/runtime content out of user-facing skill UI", () => {
    const combined = [read("src/components/ai/SkillsPanel.tsx")].join("\n");

    expect(combined).not.toContain("resourceContent");
    expect(combined).not.toContain("noteContent");
    expect(combined).not.toContain("clipboard");
    expect(combined).not.toContain("apiKey");
    expect(combined).not.toContain("base64");
    expect(combined).not.toContain("shellOutput");
  });

  it("uses structured permission preflight and resource diagnostics", () => {
    const aiTypes = read("src/types/ai.ts");
    const rustTypes = read("src-tauri/src/ai_types/mod.rs");
    const activation = read("src-tauri/src/ai_runtime/skills/activation.rs");
    const resources = read("src-tauri/src/ai_runtime/skills/resources.rs");

    expect(aiTypes).toContain("PermissionPreflightSummary");
    expect(rustTypes).toContain("PermissionPreflightSummary");
    expect(aiTypes).toContain("requiredConfirmations");
    expect(rustTypes).toContain("required_confirmations");
    expect(activation).toContain("build_resource_summaries");
    expect(activation).toContain("size_bytes");
    expect(resources).toContain('"resources"');
    expect(resources).not.toContain('"scripts"');
  });
});
