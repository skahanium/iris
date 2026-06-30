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

    for (const label of [
      "只读笔记",
      "联网读取",
      "写入笔记",
      "管理 Skills",
      "运行命令",
      "凭据访问",
      "权限摘要",
    ]) {
      expect(panel).toContain(label);
    }

    expect(panel).not.toContain("Requested capabilities:");
    expect(panel).not.toContain("Blocked capabilities:");
    expect(panel).not.toContain("Compatibility warnings:");
  });

  it("keeps scope explicit when toggling or uninstalling skills", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");

    expect(panel).toContain("skillsToggle(skill.name, sc");
    expect(panel).toContain("skillsUninstall(skill.name, sc)");
  });

  it("shows workspace archive status and prepare path in the Skills panel", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
    const ipc = read("src/lib/ipc.ts");

    expect(panel).toContain("工作区");
    expect(panel).toContain("需要准备");
    expect(panel).toContain("准备工作区");
    expect(panel).toContain("skillsPrepareWorkspace");
    expect(ipc).toContain("export async function skillsPrepareWorkspace");
    expect(ipc).toContain("workspaceRoot");
    expect(ipc).toContain("workspaceReady");
    expect(ipc).toContain("workspaceMissingItems");
  });

  it("does not compress layered skill state into a generic current available label", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");

    expect(panel).not.toContain("当前可用");
    expect(panel).toContain("部分可用");
    expect(panel).toContain("不需要运行时");
    expect(panel).toContain("缺少或未启用 MCP profile");
  });
  it("surfaces manifest runtime and workspace summary in the Skills panel contract", () => {
    const panel = read("src/components/ai/SkillsPanel.tsx");
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
    expect(panel).toContain("可用片段");
    expect(panel).toContain("阻塞片段");
    expect(panel).toContain("runtimeState");
    expect(panel).toContain("skill.runtime_status");
    expect(panel).toContain("skill.workspace_declared");
    expect(panel).toContain("skill.workspace_prepared");
  });
});
