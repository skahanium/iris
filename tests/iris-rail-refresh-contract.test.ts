import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

const removedVendor = ["mini", "max"].join("");

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
    const css = read("src/styles/globals.css");
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
    expect(titleBar).toContain("iris-brand-rail flex h-8");
    expect(titleBar).toContain("min-w-[6.75rem]");
    expect(titleBar).not.toContain("iris-brand-rail flex h-full");
    expect(css).toContain(".iris-brand-rail:hover");
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
    expect(welcome.toLowerCase()).not.toContain(`${removedVendor} 检索`);
    expect(welcome).not.toContain("shadow-floating");
    expect(welcome).not.toContain("max-w-md");
    expect(welcome).not.toContain("本地优先的知识工作台");
    expect(welcome).not.toContain("<IrisMark");
    expect(platform).toContain("showCustomWindowControls");
    expect(platform).toContain("isWindowsDesktopChrome");
    expect(macos).toContain('"decorations": true');
  });

  it("all document-opening overlay routes leave Home before opening a note", () => {
    const app = read("src/App.tsx");

    expect(app).toContain("openNoteLeavingHome");
    expect(app).not.toMatch(
      /on(?:Select|Open|Restored|OpenNote)=\{\(p\) => void openNote\(p\)\}/,
    );
    expect(app).not.toMatch(/onOpenWikiLink=\{\(title\) => void openNote/);
  });

  it("uses a centered floating Ghost Spine bar island with single-title preview", () => {
    const outline = read("src/components/editor/EditorOutline.tsx");
    const editor = read("src/components/editor/TipTapEditor.tsx");
    const css = read("src/styles/globals.css");
    const aiPanelWidth = read("src/lib/ai-panel-width.ts");

    expect(outline).toContain('data-testid="outline-rail"');
    expect(outline).not.toContain('data-testid="outline-rail-handle"');
    expect(outline).toContain('data-testid="outline-ghost-popover"');
    expect(outline).toContain("outline-ghost--active");
    expect(outline).toContain("outline-ghost-item");
    expect(outline).toContain("outline-ghost-items");
    expect(outline).toContain("outline-ghost-item-line");
    expect(outline).toContain("previewEntry.text");
    expect(outline).not.toContain("ListTree");
    expect(outline).not.toContain("outline-ghost-handle");
    expect(outline).not.toContain("显示目录");
    expect(outline).not.toContain("隐藏目录");
    expect(outline).not.toContain("outline-ghost-popover-list");
    expect(outline).not.toContain("outline-ghost-popover-item");
    expect(outline).toContain("useVirtualizer");
    expect(outline).toContain("renderedOutlineItems");
    expect(outline).toContain("ArrowDown");
    expect(outline).not.toContain("Escape");
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
    expect(css).toContain("--editor-outline-rail-width: 3.25rem");
    expect(css).not.toContain("padding-left: var(--editor-outline-reserve);");
    expect(css).toContain(".outline-ghost-popover");
    expect(css).toMatch(/\.outline-ghost \{[\s\S]*top: 50%/);
    expect(css).toMatch(
      /\.outline-ghost \{[\s\S]*transform: translateY\(-50%\)/,
    );
    expect(css).toContain("height: min(74.4dvh, 33.6rem)");
    expect(css).toContain("--outline-bar-width: 0.95rem");
    expect(css).toContain("--outline-row-gap: 0.84rem");
    expect(css).toContain("--outline-bar-active-width: 3rem");
    expect(css).toContain("--outline-bar-candidate-width: 3.5rem");
    expect(css).toMatch(/\.outline-ghost-list \{[\s\S]*min-height: 100%;/);
    expect(css).toMatch(/\.outline-ghost-list \{[\s\S]*overflow-y: auto;/);
    expect(css).toMatch(/\.outline-ghost-spine \{[\s\S]*bottom: 0;/);
    expect(css).toMatch(/\.outline-ghost-items \{[\s\S]*margin-block: auto;/);
    expect(css).toMatch(
      /\.outline-ghost-items \{[\s\S]*row-gap: var\(--outline-row-gap\);/,
    );
    expect(css).toContain(".outline-ghost-item-line");
    expect(css).toContain(".outline-ghost");
    expect(css).not.toContain(".outline-ghost-list::before");
    expect(css).not.toContain(".outline-ghost-handle");
    expect(css).not.toContain(".outline-ghost-popover-list");
    expect(css).not.toContain(".outline-ghost-popover-item");
    expect(css).toContain(".outline-ghost-item--level-1");
    expect(css).toContain(".outline-ghost-item--level-2");
    expect(css).toContain(".outline-ghost-item--level-3");
    expect(css).not.toContain(".outline-luminous-tick");
    expect(css).not.toContain(".outline-minimap-tick");
    expect(css).not.toContain("backdrop-filter: blur(12px)");
    expect(aiPanelWidth).toContain("AI_PANEL_WIDTH_MAX = 720");
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
    expect(managementCenter.toLowerCase()).not.toContain(
      `${removedVendor}searchsection`,
    );
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

  it("defines the Run-backed AI collaboration sidecar surfaces", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
    const sender = read("src/components/ai/hooks/useUnifiedAssistantSend.ts");
    const bubble = read("src/components/ai/AiMessageBubble.tsx");
    const composer = read("src/components/ui/ai-composer.tsx");
    const css = read("src/styles/globals.css");

    expect(panel).toContain("ai-sidecar");
    expect(panel).toContain("useAssistantRun");
    expect(sender).toContain("explicitReferences");
    expect(sender).toContain("securityDomain: aiDomain");
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
