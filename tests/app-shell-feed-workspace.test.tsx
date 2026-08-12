import { readFileSync } from "node:fs";

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AppShell } from "@/components/layout/AppShell";
import { DesktopTitleBar } from "@/components/layout/DesktopTitleBar";
import type { AppWorkspaceMode } from "@/lib/workspace-chrome-layout";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

function renderShell(props: {
  aiPanelOpen?: boolean;
  onAiPanelOpenChange?: (open: boolean) => void;
  workspaceMode?: AppWorkspaceMode;
  onWorkspaceModeChange?: (mode: AppWorkspaceMode) => void;
  withFeedWorkspace?: boolean;
}) {
  const feedWorkspace = props.withFeedWorkspace ? (
    <div data-testid="feed-workspace">订阅工作区</div>
  ) : undefined;
  return render(
    <AppShell
      tabBar={<div data-testid="tab-bar">tabs</div>}
      editor={<div data-testid="editor-node">editor</div>}
      aiPanel={<div data-testid="agent-panel">agent</div>}
      statusBar={<div data-testid="status-bar">status</div>}
      aiPanelOpen={props.aiPanelOpen}
      onAiPanelOpenChange={props.onAiPanelOpenChange}
      workspaceMode={props.workspaceMode}
      onWorkspaceModeChange={props.onWorkspaceModeChange}
      feedWorkspace={feedWorkspace}
    />,
  );
}

describe("订阅工作区模式：不卸载编辑器", () => {
  beforeEach(() => {
    vi.spyOn(window, "getComputedStyle").mockReturnValue({
      fontSize: "16px",
      getPropertyValue: (prop: string) =>
        prop === "--prose-measure" ? "52rem" : "",
    } as CSSStyleDeclaration);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("进入 feeds 时 editor DOM 仍挂载但 aria-hidden 且不可交互", () => {
    const { container, rerender } = renderShell({ withFeedWorkspace: true });
    const editor = container.querySelector("[data-testid=editor-node]")!;
    expect(editor.getAttribute("aria-hidden")).toBeNull();

    rerender(
      <AppShell
        tabBar={<div data-testid="tab-bar">tabs</div>}
        editor={<div data-testid="editor-node">editor</div>}
        aiPanel={<div data-testid="agent-panel">agent</div>}
        statusBar={<div data-testid="status-bar">status</div>}
        workspaceMode="feeds"
        feedWorkspace={<div data-testid="feed-workspace">订阅工作区</div>}
      />,
    );

    const editorAfter = container.querySelector("[data-testid=editor-node]")!;
    expect(editorAfter).not.toBeNull();
    const mainAfter = container.querySelector("[data-testid=workspace-main]")!;
    expect(mainAfter.getAttribute("aria-hidden")).toBe("true");
    expect(mainAfter.className).toContain("pointer-events-none");
    expect(mainAfter.className).toContain("invisible");

    const feedMain = container.querySelector(
      "[data-testid=workspace-feed-main]",
    )!;
    expect(feedMain).not.toBeNull();
    expect(feedMain.getAttribute("aria-hidden")).toBeNull();
    expect(screen.getByTestId("feed-workspace")).toBeTruthy();
  });

  it("返回 documents 后是同一 editor 节点，Agent 意图与文档 Tab 不变", () => {
    const onAiPanelOpenChange = vi.fn();
    const { container, rerender } = renderShell({
      aiPanelOpen: true,
      onAiPanelOpenChange,
      withFeedWorkspace: true,
    });
    const editorBefore = container.querySelector("[data-testid=editor-node]")!;
    const tabBefore = container.querySelector("[data-testid=tab-bar]")!;

    rerender(
      <AppShell
        tabBar={<div data-testid="tab-bar">tabs</div>}
        editor={<div data-testid="editor-node">editor</div>}
        aiPanel={<div data-testid="agent-panel">agent</div>}
        statusBar={<div data-testid="status-bar">status</div>}
        aiPanelOpen={true}
        onAiPanelOpenChange={onAiPanelOpenChange}
        workspaceMode="feeds"
        feedWorkspace={<div data-testid="feed-workspace">订阅工作区</div>}
      />,
    );
    // feeds 模式折叠 Agent 的有效 presentation，但不写回 aiPanelOpen。
    const dock = container.querySelector(
      "[data-testid=unified-assistant-dock]",
    )!;
    expect(dock.getAttribute("data-presentation")).toBe("collapsed");
    expect(onAiPanelOpenChange).not.toHaveBeenCalled();

    rerender(
      <AppShell
        tabBar={<div data-testid="tab-bar">tabs</div>}
        editor={<div data-testid="editor-node">editor</div>}
        aiPanel={<div data-testid="agent-panel">agent</div>}
        statusBar={<div data-testid="status-bar">status</div>}
        aiPanelOpen={true}
        onAiPanelOpenChange={onAiPanelOpenChange}
        workspaceMode="documents"
        feedWorkspace={<div data-testid="feed-workspace">订阅工作区</div>}
      />,
    );

    const editorAfter = container.querySelector("[data-testid=editor-node]")!;
    const tabAfter = container.querySelector("[data-testid=tab-bar]")!;
    expect(editorAfter).toBe(editorBefore);
    expect(tabAfter).toBe(tabBefore);
    expect(editorAfter.getAttribute("aria-hidden")).toBeNull();
    expect(onAiPanelOpenChange).not.toHaveBeenCalled();
  });

  it("feeds 模式两个 main 都保持挂载，只切换可见性", () => {
    const { container, rerender } = renderShell({ withFeedWorkspace: true });

    rerender(
      <AppShell
        tabBar={<div data-testid="tab-bar">tabs</div>}
        editor={<div data-testid="editor-node">editor</div>}
        aiPanel={<div data-testid="agent-panel">agent</div>}
        statusBar={<div data-testid="status-bar">status</div>}
        workspaceMode="feeds"
        feedWorkspace={<div data-testid="feed-workspace">订阅工作区</div>}
      />,
    );
    const feedMainInFeeds = container.querySelector(
      "[data-testid=workspace-feed-main]",
    )!;
    expect(feedMainInFeeds.getAttribute("aria-hidden")).toBeNull();

    rerender(
      <AppShell
        tabBar={<div data-testid="tab-bar">tabs</div>}
        editor={<div data-testid="editor-node">editor</div>}
        aiPanel={<div data-testid="agent-panel">agent</div>}
        statusBar={<div data-testid="status-bar">status</div>}
        workspaceMode="documents"
        feedWorkspace={<div data-testid="feed-workspace">订阅工作区</div>}
      />,
    );
    // feed main 不卸载：仍挂载但不可见。
    const feedMainInDocs = container.querySelector(
      "[data-testid=workspace-feed-main]",
    )!;
    expect(feedMainInDocs).toBe(feedMainInFeeds);
    expect(feedMainInDocs.getAttribute("aria-hidden")).toBe("true");
    expect(feedMainInDocs.className).toContain("pointer-events-none");
  });

  it("Rss 标题栏按钮以 aria-pressed 表达模式并可切换", () => {
    const onWorkspaceModeChange = vi.fn();
    const titleBar = (workspaceMode: AppWorkspaceMode) => (
      <DesktopTitleBar
        tabs={[]}
        activePath={null}
        onSelect={() => undefined}
        onClose={() => undefined}
        onNew={() => undefined}
        workspaceMode={workspaceMode}
        onWorkspaceModeChange={onWorkspaceModeChange}
      />
    );
    const { rerender } = render(
      <AppShell
        tabBar={titleBar("documents")}
        editor={<div data-testid="editor-node">editor</div>}
        aiPanel={<div />}
        statusBar={<div />}
      />,
    );
    const rssButton = screen.getByTestId("titlebar-feed-entry");
    expect(rssButton.getAttribute("aria-pressed")).toBe("false");
    expect(rssButton.getAttribute("title")).toBe("打开订阅");
    fireEvent.click(rssButton);
    expect(onWorkspaceModeChange).toHaveBeenCalledWith("feeds");

    rerender(
      <AppShell
        tabBar={titleBar("feeds")}
        editor={<div data-testid="editor-node">editor</div>}
        aiPanel={<div />}
        statusBar={<div />}
      />,
    );
    expect(rssButton.getAttribute("aria-pressed")).toBe("true");
    expect(rssButton.getAttribute("title")).toBe("返回笔记库");
  });

  it("契约：AppShell 与 DesktopTitleBar 声明 workspaceMode 边界", () => {
    const shell = read("src/components/layout/AppShell.tsx");
    expect(shell).toContain("workspaceMode");
    expect(shell).toContain("feedWorkspace");
    expect(shell).toContain("onWorkspaceModeChange");
    const titleBar = read("src/components/layout/DesktopTitleBar.tsx");
    expect(titleBar).toContain("titlebar-feed-entry");
    const layout = read("src/lib/workspace-chrome-layout.ts");
    expect(layout).toContain('"documents" | "feeds"');
  });
});
