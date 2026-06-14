import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Phase5 permission UI", () => {
  it("keeps Management Center as a plain-language boundary summary", () => {
    const source = read("src/components/settings/ManagementCenterPanel.tsx");

    expect(source).toContain("系统边界");
    expect(source).toContain("权限边界");
    expect(source).toContain("凭据边界");
    expect(source).toContain("API Key 保存在系统凭据管理器");
    expect(source).not.toContain("Markdown Agent 权限");
    expect(source).not.toContain("permissions:");
    expect(source).not.toContain('data-testid="agent-permission-settings"');
  });

  it("does not expose raw permission codes or secret capabilities in Management Center", () => {
    const source = read("src/components/settings/ManagementCenterPanel.tsx");

    expect(source).toContain("凭据边界");
    expect(source).not.toContain("vault.write.patch");
    expect(source).not.toContain("secret.read_plaintext");
    expect(source).not.toContain("涉密面板");
  });

  it("renders permission effects in tool confirmation before raw arguments", () => {
    const source = read("src/components/ai/ToolConfirmDialog.tsx");

    expect(source).toContain("权限影响");
    expect(source).toContain("permissionEffects");
    expect(source.indexOf("权限影响")).toBeLessThan(source.indexOf("调用参数"));
    expect(source).toContain("reversibleBy");
    expect(source).toContain("blockedReason");
  });
});
