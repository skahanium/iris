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
    expect(source).toContain("API Key 保存在本地加密凭据");
    expect(source).not.toContain("permissions:");
    expect(source).not.toContain('data-testid="agent-permission-settings"');
  });

  it("does not expose raw permission codes or secret capabilities in Management Center", () => {
    const source = read("src/components/settings/ManagementCenterPanel.tsx");

    expect(source).toContain("权限边界");
    expect(source).not.toContain("vault.write.patch");
    expect(source).not.toContain("secret.read_plaintext");
    expect(source).not.toContain("Markdown Agent");
  });

  it("keeps tool confirmations compact and user-facing", () => {
    const source = read("src/components/ai/ToolConfirmDialog.tsx");

    expect(source).toContain("buildPermissionCard");
    expect(source).toContain("permissionEffects");
    expect(source).not.toContain("web_to_markdown");
    expect(source).not.toContain("process_run_readonly");
    expect(source).not.toContain("调用参数");
    expect(source).not.toContain("参数 JSON");
    expect(source).not.toContain("reversibleBy");
  });
});
