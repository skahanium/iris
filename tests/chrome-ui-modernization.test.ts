import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("v0.4.1-ui chrome modernization", () => {
  it("UnifiedAssistantPanel replaces workflow tabs and scene picker", () => {
    const source = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(source).toContain("AiComposer");
    expect(source).toContain("AiMessageList");
    expect(source).toContain("ContextPacketDrawer");
    expect(source).toContain("AssistantActionState");
    expect(source).not.toContain("WORKFLOW_TASK_DEFINITIONS");
    expect(source).not.toContain("SceneSelector");
  });

  it("SlashCommandList uses IrisSurfaceMenu and Lucide icons", () => {
    const source = read("src/components/editor/SlashCommandList.tsx");
    expect(source).toContain("IrisSurfaceMenuItem");
    expect(source).toContain("IrisSurfaceMenuPanel");
    expect(source).toContain("resolveCommandIcon");
    expect(source).toContain("useListboxKeyboard");
    expect(source).not.toContain("CommandListOption");
    expect(source).not.toContain("📄");
    expect(source).not.toContain("💡");
  });

  it("ResearchFocusView avoids high-saturation colors", () => {
    const research = read("src/components/ai/assistant/ResearchFocusView.tsx");
    expect(research).not.toContain("emerald-");
    expect(research).not.toContain("purple-");
    expect(research).not.toContain("amber-");
  });

  it("ContextPacketCard avoids high-saturation trust colors", () => {
    const source = read("src/components/ai/ContextPacketCard.tsx");
    expect(source).toContain("SurfaceCard");
    expect(source).not.toContain("emerald-500");
    expect(source).not.toContain("purple-500");
  });

  it("StatusBar uses EditorZoomControl popover", () => {
    const source = read("src/components/layout/StatusBar.tsx");
    expect(source).toContain("EditorZoomControl");
    expect(source).not.toContain('aria-label="缩小"');
  });

  it("StatusBar exposes a compact theme switch", () => {
    const source = read("src/components/layout/StatusBar.tsx");
    const app = read("src/App.tsx");

    expect(source).toContain("onThemeChange");
    expect(source).toContain('role="switch"');
    expect(source).toContain(
      'aria-label={theme === "dark" ? "切换到亮色模式" : "切换到暗色模式"}',
    );
    expect(source).toContain('data-testid="status-bar-theme-switch"');
    expect(app).toContain(
      "onThemeChange={(nextTheme) => void setTheme(nextTheme)}",
    );
  });

  it("StatusBar exposes management gear instead of command palette shortcut", () => {
    const source = read("src/components/layout/StatusBar.tsx");
    const appSlot = read("src/components/layout/AppStatusBarSlot.tsx");

    expect(source).toContain('data-testid="status-bar-management-button"');
    expect(source).toContain("onOpenManagementCenter");
    expect(source).not.toContain("formatCommandPaletteShortcut");
    expect(source).not.toContain("打开命令面板");
    expect(appSlot).toContain("onOpenManagementCenter");
  });

  it("StatusBar exposes graph as a direct bottom-bar entry", () => {
    const source = read("src/components/layout/StatusBar.tsx");
    const appSlot = read("src/components/layout/AppStatusBarSlot.tsx");
    const app = read("src/App.impl.tsx");

    expect(source).toContain('data-testid="status-bar-graph-button"');
    expect(source).toContain("onOpenGraph");
    expect(appSlot).toContain("onOpenGraph");
    expect(app).toContain('onOpenGraph={() => overlays.openOverlay("graph")}');
  });

  it("StatusBar filters classified vault status from global chrome", () => {
    const source = read("src/components/layout/StatusBar.tsx");
    const app = read("src/App.impl.tsx");

    expect(source).toContain("safeStatusLine");
    expect(source).toContain("isClassifiedStatusLine");
    expect(app).not.toContain('setAiStatus("涉密保险库已锁定")');
  });

  it("ConnectivityIndicators groups unified status balls", () => {
    const source = read("src/components/layout/ConnectivityIndicators.tsx");
    const statusBar = read("src/components/layout/StatusBar.tsx");
    expect(source).toContain("size-2 shrink-0 rounded-full");
    expect(source).toContain("onWebSearchChange");
    expect(source).toContain("--status-inactive");
    expect(source).not.toContain("size-3.5");
    expect(source).toContain('label="LLM"');
    expect(source).toContain('label="联网"');
    expect(source).not.toContain('label="搜索"');
    expect(
      read("src/components/settings/ManagementCenterPanel.tsx"),
    ).not.toContain("Bing");
    expect(statusBar).toContain("onWebSearchChange={onWebSearchChange}");
    expect(statusBar).not.toContain("联网搜索");
  });
});
