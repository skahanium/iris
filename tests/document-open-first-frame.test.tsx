import { act, useEffect } from "react";
import type { ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const firstFrameCallbacks = vi.hoisted(
  () => new Map<string, (editor: unknown) => void>(),
);
const editorReadyCallbacks = vi.hoisted(
  () => new Map<string, (editor: unknown) => void>(),
);
const contentReadyCallbacks = vi.hoisted(
  () => new Map<string, (editor: unknown) => void>(),
);
const dirtyCallbacks = vi.hoisted(() => new Map<string, () => void>());
const editorMountsByPath = vi.hoisted(() => new Map<string, number>());

vi.mock("@/components/editor/TipTapEditor", () => ({
  TipTapEditor: (props: {
    contentCacheKey?: string | null;
    initialBodyMarkdown: string;
    initialEditorHtml?: string;
    mediaLoading?: unknown;
    onContentReady?: (editor: unknown) => void;
    onDirty?: () => void;
    onEditorReady?: (editor: unknown) => void;
    onFirstFrameReady?: (editor: unknown) => void;
  }) => {
    const {
      contentCacheKey,
      initialBodyMarkdown,
      initialEditorHtml,
      onContentReady,
      onDirty,
      onEditorReady,
      onFirstFrameReady,
    } = props;
    useEffect(() => {
      if (!contentCacheKey) return;
      editorMountsByPath.set(
        contentCacheKey,
        (editorMountsByPath.get(contentCacheKey) ?? 0) + 1,
      );
      return undefined;
    }, [contentCacheKey]);

    useEffect(() => {
      if (!contentCacheKey) return;
      if (onEditorReady)
        editorReadyCallbacks.set(contentCacheKey, onEditorReady);
      if (onContentReady) {
        contentReadyCallbacks.set(contentCacheKey, onContentReady);
      }
      if (onDirty) dirtyCallbacks.set(contentCacheKey, onDirty);
      if (onFirstFrameReady) {
        firstFrameCallbacks.set(contentCacheKey, onFirstFrameReady);
      }
      return () => {
        contentReadyCallbacks.delete(contentCacheKey);
        dirtyCallbacks.delete(contentCacheKey);
        editorReadyCallbacks.delete(contentCacheKey);
        firstFrameCallbacks.delete(contentCacheKey);
      };
    }, [contentCacheKey, onContentReady, onEditorReady, onFirstFrameReady]);

    return (
      <div data-testid="tiptap-editor" data-path={contentCacheKey ?? ""}>
        {initialEditorHtml
          ? "prepared-html"
          : `markdown:${initialBodyMarkdown}`}
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

vi.mock("@/hooks/useHomeRecentNotes", () => ({
  useHomeRecentNotes: () => ({
    catalogPaths: [],
    recentNotes: [],
    vaultHasNotes: false,
    refreshRecent: vi.fn(),
  }),
}));

vi.mock("@/components/layout/WorkspaceEmpty", () => ({
  WorkspaceEmpty: () => <div data-testid="workspace-empty" />,
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
    activeMediaTab: null,
    activeNoteIsClassified: false,
    activePath: "old.md",
    editorBodyMarkdown: "old body",
    editorContentTick: 0,
    editorContextMenu: {
      menu: { open: false, x: 0, y: 0 },
      groups: [],
      handleContextMenu: vi.fn(),
      close: vi.fn(),
    },
    editorInstance: null,
    editorPreparedHtml: null,
    editorTitleSlot: null,
    editorZoom: 1,
    findReplaceMode: "find" as const,
    findReplaceOpen: false,
    handleDirty: vi.fn(),
    handleEditorReady: vi.fn(),
    handleLockToggle: vi.fn(async () => undefined),
    handleNewNoteLeavingHome: vi.fn(),
    getNoteContent: vi.fn(() => ""),
    workspaceEmpty: false,
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
    warmPreparedNotes: [],
    openNotePaths: ["old.md"],
    zen: false,
  };
}

async function flushFrame() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

describe("document open first frame surface", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    firstFrameCallbacks.clear();
    contentReadyCallbacks.clear();
    dirtyCallbacks.clear();
    editorReadyCallbacks.clear();
    editorMountsByPath.clear();
  });

  afterEach(() => {
    vi.useRealTimers();
    act(() => root.unmount());
    host.remove();
  });

  it("shows the document loading surface until the target editor first frame is ready", async () => {
    vi.useFakeTimers();
    const handleEditorReady = vi.fn();

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          activePath="new.md"
          editorBodyMarkdown="new body that must not be visible yet"
          editorContentTick={1}
          editorPreparedHtml="<p>prepared new body</p>"
          handleEditorReady={handleEditorReady}
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
    expect(handleEditorReady).not.toHaveBeenCalledWith(
      expect.objectContaining({ path: "new.md" }),
    );

    act(() => {
      firstFrameCallbacks.get("new.md")?.({ path: "new.md" });
    });

    expect(
      document.querySelector('[data-testid="document-open-loading"]'),
    ).toBeNull();
    expect(
      document
        .querySelector('[data-path="new.md"]')
        ?.getAttribute("data-editor-visibility"),
    ).toBe("visible");
    expect(handleEditorReady).toHaveBeenCalledWith({ path: "new.md" });
  });

  it("delays loading for a staged pending note open and skips it when the first frame is fast", async () => {
    vi.useFakeTimers();
    const commitPendingNoteOpen = vi.fn(() => true);

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          pendingNoteOpen={{
            path: "new.md",
            title: "New",
            bodyMarkdown: "new body",
            content: "new body",
            frontmatterYaml: null,
            isLocked: false,
            namespace: "normal",
            sequence: 1,
            preparedEditorHtml: "<p>prepared new body</p>",
          }}
          commitPendingNoteOpen={commitPendingNoteOpen}
          openNotePaths={["old.md", "new.md"]}
        />,
      );
    });

    expect(
      document.querySelector('[data-testid="document-open-loading"]'),
    ).toBeNull();

    act(() => {
      firstFrameCallbacks.get("new.md")?.({ path: "new.md" });
    });

    expect(commitPendingNoteOpen).toHaveBeenCalledWith("new.md", 1, {
      skipContentTick: true,
    });
    expect(
      document.querySelector('[data-testid="document-open-loading"]'),
    ).toBeNull();
  });

  it("shows loading only after the cold-open delay budget is exceeded", async () => {
    vi.useFakeTimers();

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          pendingNoteOpen={{
            path: "new.md",
            title: "New",
            bodyMarkdown: "new body",
            content: "new body",
            frontmatterYaml: null,
            isLocked: false,
            namespace: "normal",
            sequence: 1,
            preparedEditorHtml: "<p>prepared new body</p>",
          }}
          openNotePaths={["old.md", "new.md"]}
        />,
      );
    });

    expect(
      document.querySelector('[data-testid="document-open-loading"]'),
    ).toBeNull();

    act(() => {
      vi.advanceTimersByTime(DOCUMENT_OPEN_BUDGETS.coldLoadingVisibleMs - 1);
    });
    expect(
      document.querySelector('[data-testid="document-open-loading"]'),
    ).toBeNull();

    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(
      document.querySelector('[data-testid="document-open-loading"]'),
    ).toBeTruthy();
  });

  it("does not delay a Home pending open after the first frame is ready", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    const commitPendingNoteOpen = vi.fn(() => true);

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          pendingOpen={
            {
              kind: "note",
              path: "new.md",
              sequence: 7,
              startedAt: 0,
              title: "New",
            } as HomePendingOpen
          }
          pendingNoteOpen={{
            path: "new.md",
            title: "New",
            bodyMarkdown: "new body",
            content: "new body",
            frontmatterYaml: null,
            isLocked: false,
            namespace: "normal",
            sequence: 7,
            preparedEditorHtml: "<p>prepared new body</p>",
          }}
          commitPendingNoteOpen={commitPendingNoteOpen}
          openNotePaths={["old.md", "new.md"]}
        />,
      );
    });

    expect(
      document.querySelector('[data-testid="document-open-loading"]'),
    ).toBeNull();

    act(() => {
      vi.advanceTimersByTime(DOCUMENT_OPEN_BUDGETS.coldLoadingVisibleMs);
      firstFrameCallbacks.get("new.md")?.({ path: "new.md" });
    });

    expect(commitPendingNoteOpen).toHaveBeenCalledWith("new.md", 7, {
      skipContentTick: true,
    });
  });

  it("releases a staged open as soon as the first frame is ready", async () => {
    vi.useFakeTimers();
    const commitPendingNoteOpen = vi.fn(() => true);

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          pendingNoteOpen={{
            path: "new.md",
            title: "New",
            bodyMarkdown: "new body",
            content: "new body",
            frontmatterYaml: null,
            isLocked: false,
            namespace: "normal",
            sequence: 2,
            preparedEditorHtml: "<p>prepared new body</p>",
          }}
          commitPendingNoteOpen={commitPendingNoteOpen}
          openNotePaths={["old.md", "new.md"]}
        />,
      );
    });

    expect(
      document.querySelector('[data-testid="document-open-loading"]'),
    ).toBeNull();

    act(() => {
      firstFrameCallbacks.get("new.md")?.({ path: "new.md" });
    });

    expect(commitPendingNoteOpen).toHaveBeenCalledWith("new.md", 2, {
      skipContentTick: true,
    });
  });

  it("releases a staged open through a watchdog when editor ready fires but first-frame is lost", async () => {
    vi.useFakeTimers();
    const commitPendingNoteOpen = vi.fn(() => true);

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          pendingNoteOpen={{
            path: "new.md",
            title: "New",
            bodyMarkdown: "new body",
            content: "new body",
            frontmatterYaml: null,
            isLocked: false,
            namespace: "normal",
            sequence: 3,
            preparedEditorHtml: "<p>prepared new body</p>",
          }}
          commitPendingNoteOpen={commitPendingNoteOpen}
          openNotePaths={["old.md", "new.md"]}
        />,
      );
    });

    act(() => {
      editorReadyCallbacks.get("new.md")?.({ path: "new.md" });
      contentReadyCallbacks.get("new.md")?.({ path: "new.md" });
      vi.advanceTimersByTime(5000);
    });

    expect(commitPendingNoteOpen).toHaveBeenCalledWith("new.md", 3, {
      skipContentTick: true,
    });
    expect(
      document.querySelector('[data-testid="document-open-loading"]'),
    ).toBeNull();
  });

  it("does not show delayed loading after a staged surface is already ready", () => {
    vi.useFakeTimers();
    const commitPendingNoteOpen = vi.fn(() => true);

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          pendingNoteOpen={{
            path: "new.md",
            title: "New",
            bodyMarkdown: "new body",
            content: "new body",
            frontmatterYaml: null,
            isLocked: false,
            namespace: "normal",
            sequence: 4,
            preparedEditorHtml: "<p>prepared new body</p>",
          }}
          commitPendingNoteOpen={commitPendingNoteOpen}
          openNotePaths={["old.md", "new.md"]}
        />,
      );
    });

    act(() => {
      firstFrameCallbacks.get("new.md")?.({ path: "new.md" });
    });
    expect(
      document.querySelector('[data-testid="document-open-loading"]'),
    ).toBeNull();

    act(() => {
      vi.advanceTimersByTime(DOCUMENT_OPEN_BUDGETS.coldLoadingVisibleMs);
    });

    expect(
      document.querySelector('[data-testid="document-open-loading"]'),
    ).toBeNull();
  });

  it("keeps ready tab surfaces mounted so switching back has no loading page", async () => {
    const handleEditorReady = vi.fn();

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          handleEditorReady={handleEditorReady}
          openNotePaths={["old.md", "new.md"]}
        />,
      );
    });
    act(() => {
      firstFrameCallbacks.get("old.md")?.({ path: "old.md" });
    });
    await flushFrame();

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          activePath="new.md"
          editorBodyMarkdown="new body"
          editorContentTick={1}
          handleEditorReady={handleEditorReady}
          openNotePaths={["old.md", "new.md"]}
        />,
      );
    });
    act(() => {
      firstFrameCallbacks.get("new.md")?.({ path: "new.md" });
    });
    await flushFrame();

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          handleEditorReady={handleEditorReady}
          openNotePaths={["old.md", "new.md"]}
        />,
      );
    });

    expect(
      document.querySelector('[data-testid="document-open-loading"]'),
    ).toBeNull();
    expect(editorMountsByPath.get("old.md")).toBe(1);
    expect(
      document
        .querySelector('[data-path="old.md"]')
        ?.getAttribute("data-editor-visibility"),
    ).toBe("visible");
  });

  it("attributes a retained background surface dirty event to its own path", async () => {
    const handleDirty = vi.fn();

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          handleDirty={handleDirty}
          openNotePaths={["old.md", "new.md"]}
        />,
      );
    });
    act(() => {
      firstFrameCallbacks.get("old.md")?.({ path: "old.md" });
    });
    await flushFrame();

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          activePath="new.md"
          editorBodyMarkdown="new body"
          editorContentTick={1}
          handleDirty={handleDirty}
          openNotePaths={["old.md", "new.md"]}
        />,
      );
    });
    act(() => {
      firstFrameCallbacks.get("new.md")?.({ path: "new.md" });
      dirtyCallbacks.get("old.md")?.();
    });

    expect(handleDirty).toHaveBeenCalledWith("old.md");
  });

  it("keeps a ready surface visible when prepared html warm cache clears after first edit", async () => {
    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          editorPreparedHtml="<p>prepared old body</p>"
          warmPreparedNotes={[
            {
              bodyMarkdown: "old body",
              content: "old body",
              frontmatterYaml: null,
              isLocked: false,
              namespace: "normal",
              path: "old.md",
              preparedEditorHtml: "<p>prepared old body</p>",
              signature: "sig-old",
              title: "old",
              traceKey: "trace-old",
            },
          ]}
        />,
      );
    });
    act(() => {
      firstFrameCallbacks.get("old.md")?.({ path: "old.md" });
    });
    await flushFrame();

    expect(
      document
        .querySelector('[data-path="old.md"]')
        ?.getAttribute("data-editor-visibility"),
    ).toBe("visible");

    // First dirty invalidates warm prepared HTML — must not restage the live surface.
    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          editorPreparedHtml={null}
          warmPreparedNotes={[]}
        />,
      );
    });
    await flushFrame();

    expect(
      document
        .querySelector('[data-path="old.md"]')
        ?.getAttribute("data-editor-visibility"),
    ).toBe("visible");
  });

  it("does not restage when only the live title slot element changes", async () => {
    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          editorTitleSlot={<div data-testid="title-slot">A</div>}
        />,
      );
    });
    act(() => {
      firstFrameCallbacks.get("old.md")?.({ path: "old.md" });
    });
    await flushFrame();

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          editorTitleSlot={<div data-testid="title-slot">B</div>}
        />,
      );
    });
    await flushFrame();

    expect(
      document
        .querySelector('[data-path="old.md"]')
        ?.getAttribute("data-editor-visibility"),
    ).toBe("visible");
    expect(editorMountsByPath.get("old.md")).toBe(1);
  });

  it("caps retained clean ready editor surfaces while keeping the active surface", async () => {
    const openNotePaths = Array.from(
      { length: 10 },
      (_, index) => `note-${index + 1}.md`,
    );

    for (const [index, path] of openNotePaths.entries()) {
      act(() => {
        root.render(
          <AppEditorWorkspace
            {...baseProps()}
            activePath={path}
            editorBodyMarkdown={`body ${index + 1}`}
            editorContentTick={index + 1}
            openNotePaths={openNotePaths}
          />,
        );
      });
      act(() => {
        firstFrameCallbacks.get(path)?.({ path });
      });
      await flushFrame();
    }

    expect(
      document.querySelectorAll('[data-testid="tiptap-editor"]'),
    ).toHaveLength(9);
    expect(document.querySelector('[data-path="note-10.md"]')).toBeTruthy();
    expect(document.querySelector('[data-path="note-1.md"]')).toBeNull();
  });
});
