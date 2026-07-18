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
    const helpers = read("src/components/ai/skills/mcpProfileHelpers.ts");
    const panel = read("src/components/ai/skills/McpProfilesPanel.tsx");
    const diagnostics = read("src-tauri/src/commands/ai_commands.rs");
    const broker = read("src-tauri/src/ai_runtime/web_evidence_broker.rs");
    const runtime = read("src-tauri/src/ai_runtime/mcp_host_runtime.rs");

    expect(helpers).toContain("未配置 Key，将使用匿名额度");
    expect(helpers).toContain("必填凭据缺失");
    expect(helpers).toContain("本次保存会更新 Key");
    expect(card).toContain("credentialStateText");
    expect(card).toContain("仅填原始 Key，不含 Bearer");
    expect(card).toContain("清除 Key");
    expect(panel).toContain("credentialDelete");
    expect(diagnostics).toContain("Key 已绑定，请求将携带鉴权");
    expect(diagnostics).toContain("未配置 Key，将使用匿名额度");
    expect(runtime).toContain("credential_unreadable");
    expect(runtime).toContain("系统凭据不可读取");
    expect(broker).toContain("auth header present");
    expect(diagnostics).toContain("authFingerprint");
    expect(diagnostics).toContain("搜索探针请求将携带 Authorization");
  });

  it("maps AnySearch result limits to its official MCP argument", () => {
    const presets = read("src/components/ai/skills/mcpProviderPresets.ts");

    expect(presets).toContain('maxResultsArg: "max_results"');
  });
});
