import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const useHomeRecentNotes = vi.hoisted(() => vi.fn());

vi.mock("@/hooks/useHomeRecentNotes", () => ({
  useHomeRecentNotes: (...args: unknown[]) => useHomeRecentNotes(...args),
}));

vi.mock("@/components/editor/TipTapEditor", () => ({
  TipTapEditor: () => <div data-testid="tiptap-editor" />,
}));

vi.mock("@/components/editor/EditorOutline", () => ({
  EditorOutline: () => <div data-testid="editor-outline" />,
}));

vi.mock("@/components/editor/EditorFindReplaceBar", () => ({
  EditorFindReplaceBar: () => <div data-testid="find-replace" />,
}));

vi.mock("@/components/layout/MediaWorkspaceView", () => ({
  MediaWorkspaceView: () => <div data-testid="media-workspace" />,
}));

vi.mock("@/components/ErrorBoundary", () => ({
  ErrorBoundary: ({ children }: { children: React.ReactNode }) => (
    <>{children}</>
  ),
}));

vi.mock("@/components/ui/iris-context-menu", () => ({
  IrisContextMenu: () => <div data-testid="context-menu" />,
}));

import type { HomePendingOpen } from "@/lib/home-open-transition";
import { AppEditorWorkspace } from "@/components/layout/AppEditorWorkspace";

function baseProps() {
  return {
    activeFileLocked: false,
    activeMediaTab: null,
    activeNoteIsClassified: false,
    activePath: null,
    editorBodyMarkdown: "",
    editorContentTick: 0,
    editorContextMenu: {
      menu: { open: false, x: 0, y: 0 },
      groups: [],
      handleContextMenu: vi.fn(),
      close: vi.fn(),
    },
    editorInstance: null,
    editorTitleSlot: null,
    editorZoom: 1,
    findReplaceMode: "find" as const,
    findReplaceOpen: false,
    handleDirty: vi.fn(),
    handleEditorReady: vi.fn(),
    handleLockToggle: vi.fn(async () => undefined),
    handleNewNoteLeavingHome: vi.fn(),
    workspaceEmpty: true,
    inlineAi: {
      retry: vi.fn(async () => undefined),
      dismiss: vi.fn(),
      finish: vi.fn(),
    },
    onOutlineOpenChange: vi.fn(),
    openNoteLeavingHome: vi.fn(),
    outlineOpen: false,
    pendingOpen: null,
    pendingNoteOpen: null,
    commitPendingNoteOpen: vi.fn(() => true),
    runEditorActionById: vi.fn(),
    setFindReplaceMode: vi.fn(),
    setFindReplaceOpen: vi.fn(),
    updateEditorStats: vi.fn(),
    vaultIndexEpoch: 0,
    vaultPath: "/vault",
    openNotePaths: [],
    zen: false,
  };
}

describe("AppEditorWorkspace empty main surface", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    useHomeRecentNotes.mockReturnValue({
      catalogPaths: [],
      recentNotes: [],
      vaultHasNotes: false,
      refreshRecent: vi.fn(),
    });
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("renders vault empty when the catalog has no notes", () => {
    act(() => {
      root.render(<AppEditorWorkspace {...baseProps()} />);
    });

    const surface = document.querySelector('[data-testid="workspace-empty"]');
    expect(surface).toBeTruthy();
    expect(surface?.getAttribute("data-mode")).toBe("vault");
    expect(
      document.querySelector('[data-testid="workspace-empty-open-recent"]'),
    ).toBeNull();
  });

  it("renders workspace empty with open-recent when the vault has notes", () => {
    useHomeRecentNotes.mockReturnValue({
      catalogPaths: ["notes/a.md"],
      recentNotes: [{ path: "notes/a.md", title: "A" }],
      vaultHasNotes: true,
      refreshRecent: vi.fn(),
    });

    act(() => {
      root.render(<AppEditorWorkspace {...baseProps()} />);
    });

    const surface = document.querySelector('[data-testid="workspace-empty"]');
    expect(surface?.getAttribute("data-mode")).toBe("workspace");
    expect(
      document.querySelector('[data-testid="workspace-empty-open-recent"]'),
    ).toBeTruthy();
  });

  it("opens the resolved startup note when open-recent is clicked", async () => {
    const openNoteLeavingHome = vi.fn(async () => undefined);
    useHomeRecentNotes.mockReturnValue({
      catalogPaths: ["notes/a.md", "notes/b.md"],
      recentNotes: [
        { path: "notes/a.md", title: "A" },
        { path: "notes/b.md", title: "B" },
      ],
      vaultHasNotes: true,
      refreshRecent: vi.fn(),
    });

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          openNoteLeavingHome={openNoteLeavingHome}
        />,
      );
    });

    await userEvent.click(
      document.querySelector(
        '[data-testid="workspace-empty-open-recent"]',
      ) as Element,
    );

    expect(openNoteLeavingHome).toHaveBeenCalledWith(
      "notes/a.md",
      "A",
      expect.objectContaining({
        priority: "foreground",
        source: "workspace_empty",
      }),
    );
  });

  it("surfaces pending open errors on the empty main surface", () => {
    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          pendingOpen={
            {
              kind: "note",
              path: "missing.md",
              sequence: 1,
              startedAt: 1,
              error: "无法打开笔记",
            } as HomePendingOpen
          }
        />,
      );
    });

    expect(document.body.textContent).toContain("无法打开笔记");
  });
});
