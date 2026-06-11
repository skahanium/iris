import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("SettingsPanel about and legal notice", () => {
  it("shows control-center sections, copyright, and AGPL license information", () => {
    const source = read("src/components/settings/SettingsPanel.tsx");

    expect(source).toContain('data-testid="settings-control-center"');
    expect(source).toContain("工作台偏好");
    expect(source).toContain("联网搜索");
    expect(source).toContain("系统状态");
    expect(source).toContain("数据与隐私");
    expect(source).toContain("关于 Iris");
    expect(source).toContain("Iris");
    expect(source).toContain("版本 1.1.0");
    expect(source).toContain("Copyright (C) 2026 Iris Contributors");
    expect(source).toContain("GNU Affero General Public License v3.0");
    expect(source).not.toContain("AI 系统中心");
    expect(source).not.toContain("LlmRoutingSection");
    expect(source).not.toContain("MinimaxSearchSection");
    expect(source).not.toContain("AiRulesPanel");
    expect(source).not.toContain("开发者水印");
  });
});
