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
    expect(read("src/components/settings/SettingsPanel.tsx")).not.toContain(
      "Bing",
    );
    expect(statusBar).toContain("onWebSearchChange={onWebSearchChange}");
    expect(statusBar).not.toContain("联网搜索");
  });
});
