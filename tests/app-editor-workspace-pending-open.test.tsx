import { act, useEffect } from "react";
import type { ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const editorReadyCallbacks = vi.hoisted(
  () => new Map<string, (editor: unknown) => void>(),
);
const editorPropsByPath = vi.hoisted(
  () => new Map<string, Record<string, unknown>>(),
);

vi.mock("@/components/editor/TipTapEditor", () => ({
  TipTapEditor: (props: {
    contentCacheKey?: string | null;
    initialBodyMarkdown: string;
    onBodyContextMenu?: unknown;
    onDirty?: unknown;
    onInlineAiAccept?: unknown;
    onInlineAiDismiss?: unknown;
    onInlineAiRetry?: unknown;
    mediaLoading?: unknown;
    onOpenWikiLink?: unknown;
    onSlashCommand?: unknown;
    onBodyStatsChange?: unknown;
    setLocked?: unknown;
    titleSlot?: unknown;
    onEditorReady?: (editor: unknown) => void;
  }) => {
    const { contentCacheKey, initialBodyMarkdown, onEditorReady } = props;
    if (contentCacheKey) editorPropsByPath.set(contentCacheKey, props);
    useEffect(() => {
      if (contentCacheKey && onEditorReady) {
        editorReadyCallbacks.set(contentCacheKey, onEditorReady);
      }
      return () => {
        if (contentCacheKey) {
          editorReadyCallbacks.delete(contentCacheKey);
          editorPropsByPath.delete(contentCacheKey);
        }
      };
    }, [contentCacheKey, onEditorReady]);

    return (
      <div data-testid="tiptap-editor" data-path={contentCacheKey ?? ""}>
        body:{initialBodyMarkdown}
      </div>
    );
  },
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

vi.mock("@/components/layout/MediaWorkspaceView", () => ({
  MediaWorkspaceView: ({ tab }: { tab: { path: string } }) => (
    <div data-testid="media-workspace" data-path={tab.path} />
  ),
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
    activeMediaTab: null,
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
    pendingNoteOpen: null,
    commitPendingNoteOpen: vi.fn(() => true),
    runEditorActionById: vi.fn(),
    setFindReplaceMode: vi.fn(),
    setFindReplaceOpen: vi.fn(),
    updateEditorStats: vi.fn(),
    vaultIndexEpoch: 0,
    vaultPath: "/vault",
    zen: false,
  };
}

async function flushEditorHydration() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

describe("AppEditorWorkspace content-first note opens", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    editorReadyCallbacks.clear();
    editorPropsByPath.clear();
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("keeps the current readable document visible while a Home open is pending", () => {
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

    expect(document.querySelector('[data-testid="home-workbench"]')).toBeNull();
    expect(
      document.querySelector('[data-testid="readable-note-preview"]'),
    ).toBeTruthy();
    expect(document.body.textContent).toContain("old");
  });

  it("renders active note content before TipTap editor hydration", () => {
    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          activePath="new.md"
          editorBodyMarkdown="new staged body"
          editorContentTick={1}
        />,
      );
    });

    expect(
      document.querySelector('[data-testid="readable-note-preview"]'),
    ).toBeTruthy();
    expect(document.querySelector('[data-testid="tiptap-editor"]')).toBeNull();
    expect(document.body.textContent).toContain("new staged body");
  });

  it("hydrates only the visible TipTap editor after the readable paint", async () => {
    act(() => {
      root.render(<AppEditorWorkspace {...baseProps()} />);
    });

    expect(editorPropsByPath.get("old.md")).toBeUndefined();
    await flushEditorHydration();

    expect(editorPropsByPath.get("old.md")?.mediaLoading).toBe("visible");
    expect(
      document.querySelector('[data-editor-visibility="visible"]'),
    ).toBeTruthy();
  });

  it("does not mount warm prepared notes as hidden TipTap editors", async () => {
    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          warmPreparedNotes={[
            {
              bodyMarkdown: "warm",
              content: "warm",
              frontmatterYaml: null,
              isLocked: false,
              namespace: "normal",
              path: "warm.md",
              signature: "sig",
              title: "Warm",
              traceKey: "normal:abc",
            },
            {
              bodyMarkdown: "classified secret body",
              content: "classified secret body",
              frontmatterYaml: null,
              isLocked: true,
              namespace: "classified",
              path: ".classified/secret.md",
              signature: "classified-sig",
              title: "Secret",
              traceKey: "classified:abc",
            },
          ]}
        />,
      );
    });
    await flushEditorHydration();

    const editors = Array.from(
      document.querySelectorAll('[data-testid="tiptap-editor"]'),
    );
    expect(editors.map((node) => node.getAttribute("data-path"))).toEqual([
      "old.md",
    ]);
    expect(editorPropsByPath.get("warm.md")).toBeUndefined();
    expect(editorPropsByPath.get(".classified/secret.md")).toBeUndefined();
    expect(document.body.textContent).not.toContain("warm");
    expect(document.body.textContent).not.toContain("classified secret body");
  });

  it("does not mount deferred staging or warm editors when switching notes", async () => {
    act(() => {
      root.render(<AppEditorWorkspace {...baseProps()} />);
    });
    await flushEditorHydration();
    expect(editorPropsByPath.get("old.md")?.mediaLoading).toBe("visible");

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          activePath="new.md"
          editorBodyMarkdown="new"
          editorContentTick={1}
        />,
      );
    });

    expect(
      document.querySelector('[data-testid="readable-note-preview"]'),
    ).toBeTruthy();
    expect(editorPropsByPath.get("new.md")).toBeUndefined();
    await flushEditorHydration();

    expect(editorPropsByPath.get("old.md")).toBeUndefined();
    expect(editorPropsByPath.get("new.md")?.mediaLoading).toBe("visible");
    expect(
      document.querySelectorAll('[data-testid="tiptap-editor"]'),
    ).toHaveLength(1);
  });

  it("renders the active media tab instead of mounting editor surfaces", async () => {
    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          activeMediaTab={{
            id: "media:assets/paper.pdf",
            mediaKind: "pdf",
            mimeType: "application/pdf",
            path: "assets/paper.pdf",
            sizeBytes: null,
            title: "paper.pdf",
            updatedAt: null,
          }}
          warmPreparedNotes={[
            {
              bodyMarkdown: "warm",
              content: "warm",
              frontmatterYaml: null,
              isLocked: false,
              namespace: "normal",
              path: "warm.md",
              signature: "sig",
              title: "Warm",
              traceKey: "normal:warm",
            },
          ]}
        />,
      );
    });
    await flushEditorHydration();

    expect(
      document.querySelector('[data-testid="media-workspace"]'),
    ).toBeTruthy();
    expect(document.querySelector('[data-testid="tiptap-editor"]')).toBeNull();
    expect(
      document.querySelector('[data-testid="readable-note-preview"]'),
    ).toBeNull();
    expect(editorPropsByPath.size).toBe(0);
  });
});
