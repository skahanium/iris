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

  it("defines persistent brand rail as the only Home entry plus Rail Segments tabs", () => {
    const titleBar = read("src/components/layout/DesktopTitleBar.tsx");
    const app = read("src/App.tsx");
    const welcome = read("src/components/layout/WelcomeEmpty.tsx");
    const platform = read("src/lib/platform-chrome.ts");
    const macos = read("src-tauri/tauri.macos.conf.json");

    expect(titleBar).toContain('data-testid="iris-brand-rail"');
    expect(titleBar).toContain('data-testid="rail-segment-tab"');
    expect(titleBar).not.toContain('data-testid="home-segment"');
    expect(titleBar).not.toContain("iris-home-segment");
    expect(titleBar).toContain("onHome");
    expect(titleBar).toContain("isHomeActive");
    expect(titleBar).toContain("iris-brand-rail--active");
    expect(app).toContain("homeActive");
    expect(welcome).toContain('data-testid="home-workbench"');
    expect(welcome).toContain("home-workbench-grid");
    expect(welcome).toContain('data-testid="home-quick-actions"');
    expect(welcome).toContain('className="grid gap-5"');
    expect(welcome).toContain("grid grid-cols-1 gap-5");
    expect(welcome).not.toContain('data-testid="home-status-summary"');
    expect(welcome).not.toContain("useConnectivityStatus");
    expect(welcome).not.toContain("Vault 已连接");
    expect(welcome).not.toContain("篇已索引");
    expect(welcome).not.toContain("LLM 可用");
    expect(welcome).not.toContain("MiniMax 检索");
    expect(welcome).not.toContain("shadow-floating");
    expect(welcome).not.toContain("max-w-md");
    expect(welcome).not.toContain("本地优先的知识工作台");
    expect(welcome).not.toContain("<IrisMark");
    expect(platform).toContain("showCustomWindowControls");
    expect(platform).toContain("isWindowsDesktopChrome");
    expect(macos).toContain('"decorations": false');
  });

  it("all document-opening overlay routes leave Home before opening a note", () => {
    const app = read("src/App.tsx");

    expect(app).toContain("openNoteLeavingHome");
    expect(app).not.toMatch(
      /on(?:Select|Open|Restored|OpenNote)=\{\(p\) => void openNote\(p\)\}/,
    );
    expect(app).not.toMatch(/onOpenWikiLink=\{\(title\) => void openNote/);
  });

  it("uses Ghost Spine outline instead of a minimap or floating outline card", () => {
    const outline = read("src/components/editor/EditorOutline.tsx");
    const editor = read("src/components/editor/TipTapEditor.tsx");
    const css = read("src/styles/globals.css");

    expect(outline).toContain('data-testid="outline-rail"');
    expect(outline).toContain('data-testid="outline-rail-handle"');
    expect(outline).toContain("outline-ghost--active");
    expect(outline).toContain("outline-ghost-item");
    expect(outline).not.toContain("useVirtualizer");
    expect(outline).toContain("ArrowDown");
    expect(outline).toContain("Escape");
    expect(outline).not.toContain("onPointerMove");
    expect(outline).not.toContain("wheelScrubIndex");
    expect(outline).not.toContain("outline-luminous-tick");
    expect(outline).not.toContain("OutlineLuminousCaption");
    expect(outline).not.toContain("outline-label-cloud");
    expect(outline).not.toContain("postDebugLog");
    expect(outline).not.toContain("outline-axis-label line-clamp-2");
    expect(outline).not.toContain("shadow-floating");
    expect(outline).not.toContain("backdrop-filter");
    expect(editor).toContain("editor-edge-control");
    expect(css).toContain("--editor-outline-rail-width: 12rem");
    expect(css).toContain("padding-left: var(--editor-outline-reserve);");
    expect(css).toContain(".outline-ghost");
    expect(css).toContain(".outline-ghost-item--level-1");
    expect(css).toContain(".outline-ghost-item--level-2");
    expect(css).toContain(".outline-ghost-item--level-3");
    expect(css).not.toContain(".outline-luminous-tick");
    expect(css).not.toContain(".outline-minimap-tick");
    expect(css).not.toContain("backdrop-filter: blur(12px)");
  });

  it("moves AI configuration into Management Center", () => {
    const managementCenter = read(
      "src/components/settings/ManagementCenterPanel.tsx",
    );
    const overlays = read("src/hooks/useOverlayManager.ts");
    const palette = read("src/lib/command-palette.ts");
    const app = read("src/App.tsx");

    expect(managementCenter).toContain('data-testid="management-center"');
    expect(managementCenter).toContain('data-testid="management-center-tabs"');
    expect(managementCenter).toContain('role="tablist"');
    expect(managementCenter).toContain("activeSection");
    expect(managementCenter).toContain('id: "ai"');
    expect(managementCenter).toContain("LlmRoutingSection");
    expect(managementCenter).toContain("MinimaxSearchSection");
    expect(managementCenter).toContain("PersonaSettingsBody");
    expect(managementCenter).toContain("SkillsPanelBody");
    expect(managementCenter).toContain("AiRulesPanel");
    expect(managementCenter).toContain("凭据边界");
    expect(managementCenter).toContain("自动版本追踪");
    expect(managementCenter).not.toContain(
      'data-testid="ai-system-center-nav"',
    );
    const rules = read("src/components/ai/AiRulesPanel.tsx");
    expect(rules).toContain('data-testid="ai-rules-workbench"');
    expect(rules).toContain('data-testid="ai-rules-summary-grid"');
    expect(rules).toContain("formatProfileEntry");
    expect(overlays).toContain('"managementCenter"');
    expect(overlays).not.toContain('"aiSystemCenter"');
    expect(palette).toContain("ai-system-center");
    expect(palette).toContain("openManagementCenter");
    expect(app).toContain("ManagementCenterPanel");
    expect(app).not.toContain("AiSystemCenterPanel");
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

    expect(overlay).toContain("task-overlay");
    expect(chrome).toContain("task-overlay-header");
    expect(chrome).toContain("task-overlay-footer");
    expect(search).toContain("task-overlay-filter");
    expect(quickOpen).toContain("task-overlay-results");
  });

  it("ships a manual checklist for the complete Iris Rail refresh", () => {
    const checklist = read(
      "docs/testing/iris-rail-refresh-manual-checklist.md",
    );
    expect(checklist).toContain("macOS 顶栏与右侧窗口控制");
    expect(checklist).toContain("Rail Segments Tab");
    expect(checklist).toContain("Outline Ghost Spine 长文");
    expect(checklist).toContain("AI 协作侧车长对话");
    expect(checklist).toContain("任务舱 Overlay");
  });
});
