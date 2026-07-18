import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("MCP optional credential contract", () => {
  it("preserves optional credential metadata when saving provider cards", () => {
    const card = read("src/components/ai/skills/McpProfileCard.tsx");

    expect(card).toContain("optional: row.optional === true");
    expect(card).toContain("optional: credentialOptional(value)");
  });

  it("preserves optional credential metadata in preset drafts", () => {
    const panel = read("src/components/ai/skills/McpProfilesPanel.tsx");

    expect(panel).toContain("optional: item.optional === true");
  });

  it("shows anonymous and required credential states distinctly", () => {
    const card = read("src/components/ai/skills/McpProfileCard.tsx");
    const panel = read("src/components/ai/skills/McpProfilesPanel.tsx");
    const diagnostics = read("src-tauri/src/commands/ai_commands.rs");
    const runtime = read("src-tauri/src/ai_runtime/mcp_host_runtime.rs");

    expect(card).toContain("匿名模式");
    expect(card).toContain("必填凭据缺失");
    expect(card).toContain("本次保存会更新 Key");
    expect(card).toContain("仅填原始 Key，不含 Bearer");
    expect(card).toContain("清除 Key");
    expect(panel).toContain("credentialDelete");
    expect(diagnostics).toContain("Key 已绑定");
    expect(diagnostics).toContain("可选凭据未绑定，使用匿名模式");
    expect(runtime).toContain("credential_unreadable");
    expect(runtime).toContain("系统凭据不可读取");
    expect(diagnostics).toContain("auth header present");
  });

  it("maps AnySearch result limits to its official MCP argument", () => {
    const presets = read("src/components/ai/skills/mcpProviderPresets.ts");

    expect(presets).toContain('maxResultsArg: "max_results"');
  });
});
