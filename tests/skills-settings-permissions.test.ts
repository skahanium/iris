import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Skills settings permission UX contract", () => {
  it("does not expose external skill installation entry points", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
    const ipc = read("src/lib/ipc.ts");
    const lib = read("src-tauri/src/lib.rs");
    const catalog = read("src-tauri/src/ai_runtime/tool_catalog/skills.rs");

    expect(panel).not.toContain("skillsInstall");
    expect(panel).not.toContain("从 Git 安装");
    expect(panel).not.toContain("选择本地文件");
    expect(ipc).not.toContain("export async function skillsInstall");
    expect(ipc).not.toContain('"skills_install"');
    expect(lib).not.toContain("commands::ai_commands::skills_install");
    expect(catalog).toContain("skills_list");
    expect(catalog).not.toContain("skills_install");
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
      'title="卸载"',
      'title="编辑 SKILL.md"',
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

  it("keeps prompt-only skill state visible without prepare/install actions", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
    const card = read("src/components/ai/skills/SkillCard.tsx");
    const ui = `${panel}\n${card}`;
    const ipc = read("src/lib/ipc.ts");

    expect(ipc).not.toContain("export async function skillsPrepareWorkspace");
    expect(ipc).not.toContain('"skills_prepare_workspace"');
    expect(ipc).toContain("confirmation_status");
    expect(ipc).toContain("scope_rules");
    expect(ui).toContain("需要确认");
    expect(ui).toContain("已确认");
  });

  it("does not compress layered skill state into a generic current available label", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
    const badges = read("src/components/ai/skills/SkillStatusBadges.tsx");
    const statusState = read("src/components/ai/skills/skill-status-state.ts");
    const ui = `${panel}\n${badges}\n${statusState}`;

    expect(ui).not.toContain("当前可用");
    expect(ui).toContain("部分可用");
    expect(ui).toContain("不需要运行时");
    expect(ui).toContain("需要确认");
  });

  it("splits Skills cards from web evidence provider IPC management", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
    const skillCard = read("src/components/ai/skills/SkillCard.tsx");
    const statusBadges = read("src/components/ai/skills/SkillStatusBadges.tsx");
    const statusState = read("src/components/ai/skills/skill-status-state.ts");
    const ipc = read("src/lib/ipc.ts");

    expect(panel).toContain("<SkillCard");
    expect(skillCard).toContain("SkillStatusBadges");
    expect(`${statusBadges}\n${statusState}`).toContain("需要确认");
    expect(panel).not.toContain("MCP / Providers");
    expect(ipc).toContain("export async function webEvidenceProvidersList");
    expect(ipc).toContain(
      "export async function webEvidenceProviderDiagnostics",
    );
    expect(ipc).not.toContain("mcpRuntimeProfilesList");
  });

  it("surfaces prompt-only manifest summary in the Skills panel contract", () => {
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
    expect(ipc).toContain("activated_sections: string[]");
    expect(ipc).toContain("blocked_sections: string[]");
    expect(ipc).toContain("degraded_reasons: string[]");
    expect(ipc).not.toContain("mcp_dependencies: string[]");

    expect(panel).toContain("sectionState");
    expect(panel).toContain("skill.activated_sections");
    expect(panel).toContain("skill.blocked_sections");
    expect(ui).toContain("可用片段");
    expect(ui).toContain("阻塞片段");
    expect(ui).toContain("confirmationState");
    expect(ui).toContain("skill.confirmation_status");
  });
});
