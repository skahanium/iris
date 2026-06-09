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
});
