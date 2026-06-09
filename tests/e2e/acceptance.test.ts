/**
 * 核心功能验收（契约层）：在 CI 中验证统一助手接线与关键 testid；
 * 真机 Tauri/Playwright 驱动可在同一选择器上扩展。
 */
import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Iris 核心功能验收", () => {
  it("主界面包含编辑器、标签栏与状态栏 testid", () => {
    expect(read("src/components/editor/TipTapEditor.tsx")).toContain(
      'data-testid="editor"',
    );
    expect(read("src/components/layout/DesktopTitleBar.tsx")).toContain(
      'data-testid="desktop-title-bar"',
    );
    expect(read("src/components/layout/StatusBar.tsx")).toContain(
      'data-testid="status-bar"',
    );
  });

  it("统一助手 dock 与面板可被 E2E 定位", () => {
    const shell = read("src/components/layout/AppShell.tsx");
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(shell).toContain("unified-assistant-dock");
    expect(panel).toContain("unified-assistant-panel");
  });

  it("App 仅渲染一套 MinimalWindowChrome", () => {
    const source = read("src/App.tsx");
    const chromeMatches = source.match(/<MinimalWindowChrome\s*\/>/g) ?? [];
    expect(chromeMatches).toHaveLength(1);
  });

  it("AI 系统中心保留规则管理，设置页与主助手不含 AiRulesPanel", () => {
    const aiCenter = read("src/components/settings/AiSystemCenterPanel.tsx");
    const settings = read("src/components/settings/SettingsPanel.tsx");
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(aiCenter).toContain("AiRulesPanel");
    expect(settings).not.toContain("AiRulesPanel");
    expect(panel).not.toContain("AiRulesPanel");
  });
});
