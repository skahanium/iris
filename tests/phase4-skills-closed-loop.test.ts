import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Phase4 skills closed-loop contract", () => {
  it("keeps skill activation summaries but removes runtime capability DTOs", () => {
    const aiTypes = read("src/types/ai.ts");
    const rustTypes = read("src-tauri/src/ai_types/mod.rs");
    const ipc = read("src/lib/ipc.ts");

    for (const token of [
      "SkillActivationPlanSummary",
      "BlockedCapabilitySummary",
      "SkillCapabilitySupportStatus",
      "manage_skills",
    ]) {
      expect(aiTypes).toContain(token);
      expect(rustTypes).toContain(token);
    }

    for (const token of [
      "SkillRuntimeCapability",
      "SkillCompatibilitySource",
      "requestedCapabilities",
      "workspaceRoot",
      "workspaceReady",
      "workspaceMissingItems",
      "mcp_dependencies",
      "source_url",
    ]) {
      expect(aiTypes + rustTypes + ipc).not.toContain(token);
    }

    expect(aiTypes).toContain("SkillConfirmationStatus");
    expect(aiTypes).toContain("SkillScopeRule");
    expect(ipc).toContain("confirmation_status");
    expect(ipc).toContain("scope_rules");
    expect(aiTypes).toContain("skillActivationPlan");
    expect(aiTypes).toContain("blockedCapabilities");
    expect(aiTypes).toContain("auditSummary");
    expect(rustTypes).toContain("skill_activation_plan");
    expect(rustTypes).toContain("blocked_capabilities");
    expect(rustTypes).toContain("audit_summary");
  });

  it("builds a confirmed prompt-only skill activation plan before assistant harness execution", () => {
    const assistant = read("src-tauri/src/commands/assistant_commands.rs");
    const harnessContext = read("src-tauri/src/ai_harness/harness/context.rs");
    const harnessTask = read("src-tauri/src/ai_harness/harness_task.rs");
    const skills = read("src-tauri/src/ai_runtime/skills_impl.rs");
    const activation = read("src-tauri/src/ai_runtime/skills/activation.rs");

    expect(skills).toContain("build_skill_activation_plan");
    expect(activation).toContain("SkillConfirmationStatus::Confirmed");
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

  it("keeps prompt-only skill diagnostics in typed IPC and internal run plan data", () => {
    const ipc = read("src/lib/ipc.ts");
    const aiTypes = read("src/types/ai.ts");

    expect(ipc).toContain("lastMatchedAt");
    expect(ipc).toContain("lastUsedAt");
    expect(ipc).toContain("blockedCapabilities");
    expect(ipc).toContain("compatibilityWarnings");

    for (const token of [
      "requestedCapabilities",
      "workspaceRoot",
      "workspaceReady",
      "workspaceMissingItems",
      "mcpDependencies",
    ]) {
      expect(ipc).not.toContain(token);
      expect(aiTypes).not.toContain(token);
    }

    expect(aiTypes).toContain("skillActivationPlan");
    expect(aiTypes).toContain("blockedCapabilities");
    expect(aiTypes).toContain("fallbackGuidance");
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

  it("uses structured permission preflight without skill resource diagnostics", () => {
    const aiTypes = read("src/types/ai.ts");
    const rustTypes = read("src-tauri/src/ai_types/mod.rs");
    const activation = read("src-tauri/src/ai_runtime/skills/activation.rs");

    expect(aiTypes).toContain("PermissionPreflightSummary");
    expect(rustTypes).toContain("PermissionPreflightSummary");
    expect(aiTypes).toContain("requiredConfirmations");
    expect(rustTypes).toContain("required_confirmations");
    expect(activation).not.toContain("build_resource_summaries");
    expect(activation).not.toContain("SkillRuntimeCapability");
  });
});
