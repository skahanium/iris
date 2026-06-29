import { act, useEffect } from "react";
import type { ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const firstFrameCallbacks = vi.hoisted(
  () => new Map<string, (editor: unknown) => void>(),
);
const editorPropsByPath = vi.hoisted(
  () => new Map<string, Record<string, unknown>>(),
);
const documentOpenEnd = vi.hoisted(() => vi.fn());

vi.mock("@/lib/ipc", () => ({
  documentOpenEnd: (...args: unknown[]) => documentOpenEnd(...args),
}));

vi.mock("@/hooks/useHomeRecentNotes", () => ({
  useHomeRecentNotes: () => ({ recentNotes: [], refreshRecent: vi.fn() }),
}));

vi.mock("@/components/editor/TipTapEditor", () => ({
  TipTapEditor: (props: {
    contentCacheKey?: string | null;
    initialBodyMarkdown: string;
    mediaLoading?: unknown;
    onFirstFrameReady?: (editor: unknown) => void;
  }) => {
    const { contentCacheKey, initialBodyMarkdown, onFirstFrameReady } = props;
    if (contentCacheKey) editorPropsByPath.set(contentCacheKey, props);
    useEffect(() => {
      if (contentCacheKey && onFirstFrameReady) {
        firstFrameCallbacks.set(contentCacheKey, onFirstFrameReady);
      }
      return () => {
        if (contentCacheKey) {
          firstFrameCallbacks.delete(contentCacheKey);
          editorPropsByPath.delete(contentCacheKey);
        }
      };
    }, [contentCacheKey, onFirstFrameReady]);

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
  ErrorBoundary: ({ children }: { children: ReactNode }) => <>{children}</>,
}));

vi.mock("@/components/ui/iris-context-menu", () => ({
  IrisContextMenu: () => <div data-testid="context-menu" />,
}));

import { AppEditorWorkspace } from "@/components/layout/AppEditorWorkspace";
import { DOCUMENT_OPEN_BUDGETS } from "@/lib/document-open-runtime";
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
    openNotePaths: ["old.md"],
    zen: false,
  };
}

describe("AppEditorWorkspace complete-frame note opens", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    firstFrameCallbacks.clear();
    editorPropsByPath.clear();
    documentOpenEnd.mockReset();
    documentOpenEnd.mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.useRealTimers();
    act(() => root.unmount());
    host.remove();
  });

  it("renders the document loading surface instead of Home while a Home open is pending", () => {
    vi.useFakeTimers();

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          activePath={null}
          homeActive={false}
          openNotePaths={[]}
          pendingOpen={
            {
              kind: "note",
              path: "new.md",
              sequence: 1,
              startedAt: 1000,
              title: "New",
            } as HomePendingOpen
          }
        />,
      );
    });

    expect(
      document.querySelector('[data-testid="document-open-loading"]'),
    ).toBeNull();
    expect(document.querySelector('[data-testid="home-workbench"]')).toBeNull();

    act(() => {
      vi.advanceTimersByTime(DOCUMENT_OPEN_BUDGETS.coldLoadingVisibleMs);
    });

    expect(
      document.querySelector('[data-testid="document-open-loading"]'),
    ).toBeTruthy();
    expect(document.querySelector('[data-testid="home-workbench"]')).toBeNull();
    expect(document.querySelector("[data-opening]")).toBeNull();
  });

  it("keeps the current ready editor surface instead of flashing Home while a new note is pending", () => {
    vi.useFakeTimers();

    act(() => {
      root.render(<AppEditorWorkspace {...baseProps()} />);
    });

    act(() => {
      firstFrameCallbacks.get("old.md")?.({ path: "old.md" });
    });

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          pendingOpen={
            {
              kind: "new-note",
              path: null,
              sequence: 1,
              startedAt: 1000,
              title: "新建笔记",
            } as HomePendingOpen
          }
        />,
      );
    });

    expect(document.querySelector('[data-testid="home-workbench"]')).toBeNull();
    expect(
      document
        .querySelector('[data-path="old.md"]')
        ?.getAttribute("data-editor-visibility"),
    ).toBe("visible");
  });

  it("commits a new-note pending open after the new document first frame is ready", () => {
    vi.useFakeTimers();
    const commitPendingNoteOpen = vi.fn(() => true);

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          pendingOpen={
            {
              kind: "new-note",
              path: null,
              sequence: 7,
              startedAt: 1000,
              title: "新建笔记",
            } as HomePendingOpen
          }
          pendingNoteOpen={{
            bodyMarkdown: "new staged body",
            content: "# New\n\nnew staged body",
            frontmatterYaml: null,
            isLocked: false,
            namespace: "normal",
            path: "new.md",
            sequence: 7,
            title: "New",
          }}
          commitPendingNoteOpen={commitPendingNoteOpen}
          openNotePaths={["old.md", "new.md"]}
        />,
      );
    });

    act(() => {
      firstFrameCallbacks.get("new.md")?.({ path: "new.md" });
    });

    expect(commitPendingNoteOpen).toHaveBeenCalledWith("new.md", 7);
    expect(document.querySelector('[data-testid="home-workbench"]')).toBeNull();
  });

  it("shows loading instead of readable markdown before the first editor frame", () => {
    vi.useFakeTimers();

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          activePath="new.md"
          editorBodyMarkdown="new staged body"
          editorContentTick={1}
          openNotePaths={["old.md", "new.md"]}
        />,
      );
    });

    expect(
      document.querySelector('[data-testid="document-open-loading"]'),
    ).toBeNull();

    act(() => {
      vi.advanceTimersByTime(DOCUMENT_OPEN_BUDGETS.coldLoadingVisibleMs);
    });

    expect(
      document.querySelector('[data-testid="document-open-loading"]'),
    ).toBeTruthy();
    expect(
      document.querySelector('[data-testid="readable-note-preview"]'),
    ).toBeNull();
    expect(
      document
        .querySelector('[data-path="new.md"]')
        ?.getAttribute("data-editor-visibility"),
    ).toBe("staging");
  });

  it("ends retained document-open tokens after the first editor frame settles", () => {
    vi.useFakeTimers();
    const commitPendingNoteOpen = vi.fn(() => true);

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          activePath={null}
          editorBodyMarkdown="old"
          openNotePaths={["old.md", "new.md"]}
          pendingNoteOpen={{
            bodyMarkdown: "new staged body",
            content: "# New\n\nnew staged body",
            documentOpenToken: "open-token",
            frontmatterYaml: null,
            isLocked: false,
            namespace: "normal",
            path: "new.md",
            sequence: 7,
            title: "New",
          }}
          commitPendingNoteOpen={commitPendingNoteOpen}
        />,
      );
    });

    const firstFrame = firstFrameCallbacks.get("new.md");
    expect(firstFrame).toBeDefined();

    act(() => {
      firstFrame?.({ can: () => ({ undo: () => false, redo: () => false }) });
    });

    expect(commitPendingNoteOpen).toHaveBeenCalledWith("new.md", 7);
    expect(documentOpenEnd).toHaveBeenCalledWith("open-token");
  });

  it("does not mount warm prepared notes as hidden TipTap editors", () => {
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

    expect(editorPropsByPath.get("warm.md")).toBeUndefined();
    expect(editorPropsByPath.get(".classified/secret.md")).toBeUndefined();
    expect(document.body.textContent).not.toContain("warm");
    expect(document.body.textContent).not.toContain("classified secret body");
  });

  it("renders the active media tab instead of mounting editor surfaces", () => {
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

    expect(
      document.querySelector('[data-testid="media-workspace"]'),
    ).toBeTruthy();
    expect(document.querySelector('[data-testid="tiptap-editor"]')).toBeNull();
    expect(
      document.querySelector('[data-testid="document-open-loading"]'),
    ).toBeNull();
    expect(editorPropsByPath.size).toBe(0);
  });
});
