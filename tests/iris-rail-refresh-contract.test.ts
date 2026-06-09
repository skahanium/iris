import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Iris Rail complete interface contracts", () => {
  it("defines semantic tokens for the full Iris Rail interface system", () => {
    const css = read("src/styles/globals.css");
    expect(css).toContain("--knowledge-accent");
    expect(css).toContain("--iris-rail-bg");
    expect(css).toContain("--iris-rail-active");
    expect(css).toContain("--outline-rail-bg");
    expect(css).toContain("--outline-rail-active");
    expect(css).toContain("--ai-workspace-bg");
    expect(css).toContain("--ai-workspace-border");
    expect(css).toContain("--overlay-task-header");
  });

  it("documents the complete Iris Rail target surfaces", () => {
    const design = read("docs/design-system.md");
    expect(design).toContain("Iris Rail 完整刷新设计");
    expect(design).toContain("Rail Segments Tab");
    expect(design).toContain("Outline Rail");
    expect(design).toContain("AI Conversation Workspace");
    expect(design).toContain("Overlay Family");
  });

  it("defines persistent brand rail, Home view, and Rail Segments tabs", () => {
    const titleBar = read("src/components/layout/DesktopTitleBar.tsx");
    const app = read("src/App.tsx");
    const welcome = read("src/components/layout/WelcomeEmpty.tsx");
    const platform = read("src/lib/platform-chrome.ts");
    const macos = read("src-tauri/tauri.macos.conf.json");

    expect(titleBar).toContain('data-testid="iris-brand-rail"');
    expect(titleBar).toContain('data-testid="rail-segment-tab"');
    expect(titleBar).toContain('data-testid="home-segment"');
    expect(titleBar).toContain("onHome");
    expect(titleBar).toContain("isHomeActive");
    expect(app).toContain("homeActive");
    expect(welcome).toContain('data-testid="home-workbench"');
    expect(platform).toContain("showCustomWindowControls");
    expect(platform).toContain("return isTauriRuntime()");
    expect(macos).toContain('"decorations": false');
  });

  it("uses Outline Rail instead of a floating outline card", () => {
    const outline = read("src/components/editor/EditorOutline.tsx");
    const editor = read("src/components/editor/TipTapEditor.tsx");
    const css = read("src/styles/globals.css");

    expect(outline).toContain('data-testid="outline-rail"');
    expect(outline).toContain('data-testid="outline-rail-handle"');
    expect(outline).toContain("outline-rail-item--active");
    expect(outline).not.toContain("shadow-floating");
    expect(editor).toContain("editor-edge-control");
    expect(css).toContain(".outline-rail");
    expect(css).toContain(".outline-rail-handle");
  });

  it("moves AI configuration into AI System Center", () => {
    const settings = read("src/components/settings/SettingsPanel.tsx");
    const aiCenter = read("src/components/settings/AiSystemCenterPanel.tsx");
    const overlays = read("src/hooks/useOverlayManager.ts");
    const palette = read("src/lib/command-palette.ts");
    const app = read("src/App.tsx");

    expect(aiCenter).toContain('data-testid="ai-system-center"');
    expect(aiCenter).toContain("LlmRoutingSection");
    expect(aiCenter).toContain("MinimaxSearchSection");
    expect(aiCenter).toContain("PersonaSettingsPanel");
    expect(aiCenter).toContain("SkillsPanel");
    expect(aiCenter).toContain("AiRulesPanel");
    expect(settings).not.toContain("LlmRoutingSection");
    expect(settings).not.toContain("MinimaxSearchSection");
    expect(settings).not.toContain("AiRulesPanel");
    expect(overlays).toContain('"aiSystemCenter"');
    expect(palette).toContain("ai-system-center");
    expect(app).toContain("AiSystemCenterPanel");
  });

  it("defines AI collaboration sidecar surfaces", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    const bubble = read("src/components/ai/AiMessageBubble.tsx");
    const composer = read("src/components/ui/ai-composer.tsx");
    const css = read("src/styles/globals.css");

    expect(panel).toContain("ai-sidecar");
    expect(panel).toContain("ai-sidecar-header");
    expect(panel).toContain("ai-task-surface");
    expect(bubble).toContain("ai-message-surface-assistant");
    expect(bubble).toContain("ai-message-surface-user");
    expect(composer).toContain("ai-composer-workbench");
    expect(css).toContain(".ai-sidecar");
    expect(css).toContain(".ai-composer-workbench");
  });

  it("uses task-capsule overlay family hooks across command surfaces", () => {
    const overlay = read("src/components/ui/iris-overlay.tsx");
    const chrome = read("src/components/ui/overlay-chrome.tsx");
    const search = read("src/components/file/SearchPanel.tsx");
    const quickOpen = read("src/components/file/QuickOpen.tsx");
    const command = read("src/components/layout/CommandPalette.tsx");

    expect(overlay).toContain("task-overlay");
    expect(chrome).toContain("task-overlay-header");
    expect(chrome).toContain("task-overlay-footer");
    expect(search).toContain("task-overlay-filter");
    expect(quickOpen).toContain("task-overlay-results");
    expect(command).toContain("task-overlay-results");
  });

  it("ships a manual checklist for the complete Iris Rail refresh", () => {
    const checklist = read(
      "docs/testing/iris-rail-refresh-manual-checklist.md",
    );
    expect(checklist).toContain("macOS 顶栏与右侧窗口控制");
    expect(checklist).toContain("Rail Segments Tab");
    expect(checklist).toContain("Outline Rail 长文");
    expect(checklist).toContain("AI 协作侧车长对话");
    expect(checklist).toContain("任务舱 Overlay");
  });
});
