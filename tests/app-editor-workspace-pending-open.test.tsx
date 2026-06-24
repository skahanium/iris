import { act } from "react";
import type { ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/components/editor/TipTapEditor", () => ({
  TipTapEditor: () => <div data-testid="tiptap-editor" />,
}));

vi.mock("@/components/editor/EditorOutline", () => ({
  EditorOutline: () => <div data-testid="editor-outline" />,
}));

vi.mock("@/components/editor/EditorFindReplaceBar", () => ({
  EditorFindReplaceBar: () => <div data-testid="find-replace" />,
}));

vi.mock("@/components/layout/WelcomeEmpty", () => ({
  WelcomeEmpty: () => <div data-testid="home-workbench" />,
}));

vi.mock("@/components/layout/ArtifactWorkspaceView", () => ({
  ArtifactWorkspaceView: () => <div data-testid="artifact-workspace" />,
}));

vi.mock("@/components/ErrorBoundary", () => ({
  ErrorBoundary: ({ children }: { children: ReactNode }) => (
    <div data-testid="error-boundary">{children}</div>
  ),
}));

vi.mock("@/components/ui/iris-context-menu", () => ({
  IrisContextMenu: () => <div data-testid="context-menu" />,
}));

import { AppEditorWorkspace } from "@/components/layout/AppEditorWorkspace";
import type { HomePendingOpen } from "@/lib/home-open-transition";

function baseProps() {
  return {
    activeFileLocked: false,
    activeArtifactTab: null,
    activeNoteIsClassified: false,
    activePath: "old.md",
    editorBodyMarkdown: "old",
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
    getNoteContent: vi.fn(() => ""),
    homeActive: false,
    inlineAi: {
      retry: vi.fn(async () => undefined),
      dismiss: vi.fn(),
      finish: vi.fn(),
    },
    onOutlineOpenChange: vi.fn(),
    onOpenAiManagement: vi.fn(),
    onOpenQuickOpen: vi.fn(),
    onOpenSearch: vi.fn(),
    openNoteLeavingHome: vi.fn(),
    outlineOpen: false,
    pendingOpen: null,
    runEditorActionById: vi.fn(),
    setFindReplaceMode: vi.fn(),
    setFindReplaceOpen: vi.fn(),
    updateEditorStats: vi.fn(),
    vaultIndexEpoch: 0,
    vaultPath: "/vault",
    zen: false,
  };
}

describe("AppEditorWorkspace pending Home opens", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("renders the requested target loading view instead of the old active document", () => {
    const pendingOpen: HomePendingOpen = {
      kind: "note",
      path: "new.md",
      sequence: 3,
      title: "New Note",
    };

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          activePath="old.md"
          pendingOpen={pendingOpen}
        />,
      );
    });

    expect(
      document.querySelector('[data-testid="target-open-loading"]'),
    ).not.toBeNull();
    expect(document.body.textContent).toContain("New Note");
    expect(document.body.textContent).toContain("new.md");
    expect(document.querySelector('[data-testid="tiptap-editor"]')).toBeNull();
    expect(document.querySelector('[data-testid="home-workbench"]')).toBeNull();
  });
});
