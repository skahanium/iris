import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Skills settings prompt-only contract", () => {
  it("does not expose external skill installation entry points", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
    const ipc = read("src/lib/ipc.ts");
    const lib = read("src-tauri/src/lib.rs");
    const catalog = read("src-tauri/src/ai_runtime/tool_catalog/skills.rs");

    expect(panel).not.toContain("skillsInstall");
    expect(panel).not.toContain("Git");
    expect(panel).not.toContain("Registry");
    expect(ipc).not.toContain("export async function skillsInstall");
    expect(ipc).not.toContain('"skills_install"');
    expect(lib).not.toContain("commands::ai_commands::skills_install");
    expect(catalog).toContain("skills_list");
    expect(catalog).not.toContain("skills_install");
  });

  it("keeps Skills prompt-only without platform capability vocabulary", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
    const card = read("src/components/ai/skills/SkillCard.tsx");
    const ipc = read("src/lib/ipc.ts");
    const model = read("src-tauri/src/ai_runtime/skills/model.rs");
    const scan = read("src-tauri/src/ai_runtime/skills/scan.rs");
    const activation = read("src-tauri/src/ai_runtime/skills/activation.rs");
    const prompt = read("src-tauri/src/ai_runtime/skills/prompt.rs");
    const ui = `${panel}\n${card}`;

    for (const token of [
      "allowed_tools",
      "confirmation_required_tools",
      "capability_preview",
      "execute_script_sandboxed",
      "install_dependency",
      "mcp_bridge",
      "Requested Iris tools",
      "运行命令",
      "凭据访问",
    ]) {
      expect(ui).not.toContain(token);
      expect(ipc).not.toContain(token);
      expect(prompt).not.toContain(token);
    }

    expect(model).not.toContain("pub allowed_tools");
    expect(model).not.toContain("capability_preview");
    expect(scan).not.toContain('meta.get("allowed-tools")');
    expect(activation).not.toContain("active_skill_allowed_tools");
  });

  it("does not expose toggle uninstall or arbitrary SKILL.md editor controls", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
    const card = read("src/components/ai/skills/SkillCard.tsx");
    const ipc = read("src/lib/ipc.ts");
    const aiCommands = read("src-tauri/src/commands/ai_commands.rs");
    const lib = read("src-tauri/src/lib.rs");

    for (const token of [
      "skillsRead",
      "skillsWrite",
      "skillsToggle",
      "skillsUninstall",
      "editingSkill",
      "editContent",
      "<Textarea",
      "onToggle",
      "onUninstall",
    ]) {
      expect(`${panel}\n${card}`).not.toContain(token);
    }

    for (const token of [
      "export async function skillsRead",
      "export async function skillsWrite",
      "export async function skillsToggle",
      "export async function skillsUninstall",
      '"skills_read"',
      '"skills_write"',
      '"skills_toggle"',
      '"skills_uninstall"',
    ]) {
      expect(ipc).not.toContain(token);
    }

    expect(aiCommands).not.toContain("pub async fn skills_toggle");
    expect(aiCommands).not.toContain("pub async fn skills_uninstall");
    expect(aiCommands).not.toContain("pub async fn skills_read");
    expect(aiCommands).not.toContain("pub async fn skills_write");
    expect(lib).not.toContain("commands::ai_commands::skills_toggle");
    expect(lib).not.toContain("commands::ai_commands::skills_uninstall");
  });

  it("keeps prompt-only skill state visible and supports draft confirmation", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
    const card = read("src/components/ai/skills/SkillCard.tsx");
    const ui = `${panel}\n${card}`;
    const ipc = read("src/lib/ipc.ts");

    expect(ipc).not.toContain("export async function skillsPrepareWorkspace");
    expect(ipc).not.toContain('"skills_prepare_workspace"');
    expect(ipc).toContain("confirmation_status");
    expect(ipc).toContain("scope_rules");
    expect(ipc).toContain("export async function skillsCreateDraft");
    expect(ipc).toContain("export async function skillsConfirm");
    expect(ui).toContain("skillsCreateDraft");
    expect(ui).toContain("skillsConfirm");
    expect(ui).toContain('data-testid="skill-create-draft"');
    expect(ui).toContain('data-testid="skill-confirm-draft"');
  });

  it("keeps prompt-only manifest summary minimal in the Skills panel contract", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
    const card = read("src/components/ai/skills/SkillCard.tsx");
    const badges = read("src/components/ai/skills/SkillStatusBadges.tsx");
    const statusState = read("src/components/ai/skills/skill-status-state.ts");
    const ui = `${panel}\n${card}\n${badges}\n${statusState}`;
    const ipc = read("src/lib/ipc.ts");

    expect(ipc).toContain("export type SkillManifestKind");
    expect(ipc).toContain("kind: SkillManifestKind");
    expect(ipc).toContain("confirmation_status:");
    expect(ipc).toContain("scope_rules:");
    expect(ipc).toContain("activation_ready: boolean");
    expect(ipc).not.toContain("activated_sections: string[]");
    expect(ipc).not.toContain("blocked_sections: string[]");
    expect(ipc).not.toContain("degraded_reasons: string[]");
    expect(ipc).not.toContain("mcp_dependencies: string[]");

    expect(panel).not.toContain("sectionState");
    expect(panel).not.toContain("skill.activated_sections");
    expect(panel).not.toContain("skill.blocked_sections");
    expect(ui).not.toContain("可用片段");
    expect(ui).not.toContain("阻塞片段");
    expect(ui).toContain("confirmationState");
    expect(ui).toContain("skill.confirmation_status");
  });

  it("splits Skills cards from web evidence provider IPC management", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
    const skillCard = read("src/components/ai/skills/SkillCard.tsx");
    const statusBadges = read("src/components/ai/skills/SkillStatusBadges.tsx");
    const statusState = read("src/components/ai/skills/skill-status-state.ts");
    const ipc = read("src/lib/ipc.ts");

    expect(panel).toContain("<SkillCard");
    expect(skillCard).toContain("SkillStatusBadges");
    expect(`${statusBadges}\n${statusState}`).toContain("needs_confirmation");
    expect(panel).not.toContain("MCP / Providers");
    expect(ipc).toContain("export async function webEvidenceProvidersList");
    expect(ipc).toContain(
      "export async function webEvidenceProviderDiagnostics",
    );
    expect(ipc).not.toContain("mcpRuntimeProfilesList");
  });
});
