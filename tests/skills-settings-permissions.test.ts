import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Skills settings permission UX contract", () => {
  it("defaults installs to the current vault and shows resolved target paths", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
    const ipc = read("src/lib/ipc.ts");
    const commands = read("src-tauri/src/commands/ai_commands.rs");
    const catalog = read("src-tauri/src/ai_runtime/tool_catalog/skills.rs");

    expect(panel).toContain('useState<"global" | "vault">("vault")');
    expect(panel).toContain("skillsPaths");
    expect(panel).toContain("installTargetPath");
    expect(panel).toContain("当前库");
    expect(panel).toContain("目标路径");

    expect(ipc).toContain("SkillsPathsDto");
    expect(ipc).toContain('"skills_paths"');
    expect(commands).toContain("skills_paths");
    expect(catalog).toContain('"default": "vault"');
  });

  it("shows user-language capability groups instead of raw capability debug labels", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
    const card = read("src/components/ai/skills/SkillCard.tsx");
    const ui = `${panel}\n${card}`;

    for (const label of [
      "只读笔记",
      "联网读取",
      "写入笔记",
      "管理 Skills",
      "运行命令",
      "凭据访问",
      "权限摘要",
    ]) {
      expect(ui).toContain(label);
    }

    expect(ui).not.toContain("Requested capabilities:");
    expect(ui).not.toContain("Blocked capabilities:");
    expect(ui).not.toContain("Compatibility warnings:");
  });

  it("keeps scope explicit when toggling or uninstalling skills", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");

    expect(panel).toContain("skillsToggle(skill.name, sc");
    expect(panel).toContain("skillsUninstall(skill.name, sc)");
  });

  it("shows workspace archive status and prepare path in the Skills panel", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
    const card = read("src/components/ai/skills/SkillCard.tsx");
    const ui = `${panel}\n${card}`;
    const ipc = read("src/lib/ipc.ts");

    expect(ui).toContain("工作区");
    expect(panel).toContain("需要准备");
    expect(ui).toContain("准备工作区");
    expect(panel).toContain("skillsPrepareWorkspace");
    expect(ipc).toContain("export async function skillsPrepareWorkspace");
    expect(ipc).toContain("workspaceRoot");
    expect(ipc).toContain("workspaceReady");
    expect(ipc).toContain("workspaceMissingItems");
  });

  it("does not compress layered skill state into a generic current available label", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
    const badges = read("src/components/ai/skills/SkillStatusBadges.tsx");
    const statusState = read("src/components/ai/skills/skill-status-state.ts");
    const ui = `${panel}\n${badges}\n${statusState}`;

    expect(ui).not.toContain("当前可用");
    expect(ui).toContain("部分可用");
    expect(ui).toContain("不需要运行时");
    expect(ui).toContain("缺少或未启用 MCP profile");
  });

  it("splits Skills cards and MCP provider management into focused components", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
    const skillCard = read("src/components/ai/skills/SkillCard.tsx");
    const statusBadges = read("src/components/ai/skills/SkillStatusBadges.tsx");
    const statusState = read("src/components/ai/skills/skill-status-state.ts");
    const profilesPanel = read("src/components/ai/skills/McpProfilesPanel.tsx");
    const profileCard = read("src/components/ai/skills/McpProfileCard.tsx");

    expect(panel).toContain("MCP / Providers");
    expect(panel).toContain("<SkillCard");
    expect(panel).toContain("<McpProfilesPanel");
    expect(skillCard).toContain("SkillStatusBadges");
    expect(`${statusBadges}\n${statusState}`).toContain(
      "缺少或未启用 MCP profile",
    );
    expect(profilesPanel).toContain("mcpRuntimeProfilesList");
    expect(profilesPanel).toContain("mcpRuntimeToolInventoryList");
    expect(profilesPanel).toContain("mcpRuntimeHealthEventsList");
    expect(profilesPanel).toContain("mcpRuntimeProfileToggle");
    expect(profilesPanel).toContain("mcpRuntimeProfileDelete");
    expect(profilesPanel).toContain("mcpRuntimeHealthCheck");
    expect(profilesPanel).toContain("mcpRuntimeToolsList");
    expect(profileCard).toContain("MCP Profile");
    expect(profileCard).toContain("profile.transport");
    expect(profileCard).toContain("profile.scope");
    expect(profileCard).toContain("profile.trust_level");
    expect(profileCard).toContain("profile.credential_binding_status");
    expect(profileCard).toContain("删除 MCP profile");
  });

  it("surfaces manifest runtime and workspace summary in the Skills panel contract", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
    const card = read("src/components/ai/skills/SkillCard.tsx");
    const badges = read("src/components/ai/skills/SkillStatusBadges.tsx");
    const statusState = read("src/components/ai/skills/skill-status-state.ts");
    const ui = `${panel}\n${card}\n${badges}\n${statusState}`;
    const ipc = read("src/lib/ipc.ts");

    expect(ipc).toContain("export type SkillManifestKind");
    expect(ipc).toContain("kind: SkillManifestKind");
    expect(ipc).toContain("runtime_kind:");
    expect(ipc).toContain("runtime_status:");
    expect(ipc).toContain("runtime_ready:");
    expect(ipc).toContain("workspace_declared:");
    expect(ipc).toContain("workspace_prepared:");
    expect(ipc).toContain("generated_files_count:");
    expect(ipc).toContain("activated_sections: string[]");
    expect(ipc).toContain("blocked_sections: string[]");
    expect(ipc).toContain("degraded_reasons: string[]");
    expect(ipc).toContain("mcp_dependencies: string[]");

    expect(panel).toContain("sectionState");
    expect(panel).toContain("skill.activated_sections");
    expect(panel).toContain("skill.blocked_sections");
    expect(ui).toContain("可用片段");
    expect(ui).toContain("阻塞片段");
    expect(ui).toContain("runtimeState");
    expect(ui).toContain("skill.runtime_status");
    expect(panel).toContain("skill.workspace_declared");
    expect(panel).toContain("skill.workspace_prepared");
  });
});
