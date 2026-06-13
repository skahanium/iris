import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Phase5 permission UI", () => {
  it("adds a Markdown Agent permissions section to AI System Center", () => {
    const source = read("src/components/settings/AiSystemCenterPanel.tsx");

    expect(source).toContain("Markdown Agent 权限");
    expect(source).toContain('id: "permissions"');
    expect(source).toContain('data-testid="agent-permission-settings"');
    for (const label of [
      "Vault",
      "外部文件",
      "文档处理",
      "Web",
      "Skills",
      "Shell/Git",
      "Clipboard/Browser",
      "Secrets",
    ]) {
      expect(source).toContain(label);
    }
  });

  it("keeps the main settings panel as a concise permission entry point", () => {
    const source = read("src/components/settings/SettingsPanel.tsx");

    expect(source).toContain("Markdown Agent 权限");
    expect(source).toContain("vault.write.patch");
    expect(source).toContain("secret.read_plaintext");
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
