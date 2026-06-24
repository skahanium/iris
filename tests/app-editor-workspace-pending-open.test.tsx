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

  it("keeps the current complete document visible while another note is pending", () => {
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
    ).toBeNull();
    expect(
      document.querySelector('[data-testid="tiptap-editor"]'),
    ).not.toBeNull();
    expect(document.querySelector('[data-testid="home-workbench"]')).toBeNull();
  });

  it("does not replace the visible editor until the next document editor is ready", () => {
    act(() => {
      root.render(<AppEditorWorkspace {...baseProps()} />);
    });

    expect(
      document.querySelector('[data-testid="tiptap-editor"]')?.textContent,
    ).toContain("body:old");

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
      document.querySelector('[data-testid="tiptap-editor"]')?.textContent,
    ).toContain("body:old");

    act(() => {
      editorReadyCallbacks.get("new.md")?.({});
    });

    expect(
      document.querySelector('[data-testid="tiptap-editor"]')?.textContent,
    ).toContain("body:new");
  });

  it("does not mount classified warm notes while the committed surface is normal", () => {
    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          activeNoteIsClassified={false}
          warmPreparedNotes={[
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

    expect(editorPropsByPath.get(".classified/secret.md")).toBeUndefined();
    expect(document.body.textContent).not.toContain("classified secret body");
  });

  it("mounts a prepared note in a hidden warm slot without business callbacks", () => {
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
              bodyMarkdown: "spare",
              content: "spare",
              frontmatterYaml: null,
              isLocked: false,
              namespace: "normal",
              path: "spare.md",
              signature: "sig2",
              title: "Spare",
              traceKey: "normal:def",
            },
          ]}
        />,
      );
    });

    const editors = Array.from(
      document.querySelectorAll('[data-testid="tiptap-editor"]'),
    );
    expect(editors.map((node) => node.getAttribute("data-path"))).toEqual([
      "old.md",
      "warm.md",
      "spare.md",
    ]);

    const warmProps = editorPropsByPath.get("warm.md");
    expect(warmProps).toBeTruthy();
    expect(warmProps?.onDirty).toBeUndefined();
    expect(warmProps?.onSlashCommand).toBeUndefined();
    expect(warmProps?.onBodyContextMenu).toBeUndefined();
    expect(warmProps?.onBodyStatsChange).toBeUndefined();
    expect(warmProps?.onInlineAiRetry).toBeUndefined();
    expect(warmProps?.onInlineAiDismiss).toBeUndefined();
    expect(warmProps?.onInlineAiAccept).toBeUndefined();
    expect(warmProps?.onOpenWikiLink).toBeUndefined();
    expect(warmProps?.setLocked).toBeUndefined();
    expect(warmProps?.titleSlot).toBeNull();
    expect(warmProps?.locked).toBe(true);
  });

  it("does not wire business callbacks into the staging editor", () => {
    act(() => {
      root.render(<AppEditorWorkspace {...baseProps()} />);
    });

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

    const stagingProps = editorPropsByPath.get("new.md");
    expect(stagingProps).toBeTruthy();
    expect(stagingProps?.onDirty).toBeUndefined();
    expect(stagingProps?.onSlashCommand).toBeUndefined();
    expect(stagingProps?.onBodyContextMenu).toBeUndefined();
    expect(stagingProps?.onBodyStatsChange).toBeUndefined();
    expect(stagingProps?.onInlineAiRetry).toBeUndefined();
    expect(stagingProps?.onInlineAiDismiss).toBeUndefined();
    expect(stagingProps?.onInlineAiAccept).toBeUndefined();
    expect(stagingProps?.onOpenWikiLink).toBeUndefined();
    expect(stagingProps?.setLocked).toBeUndefined();
    expect(stagingProps?.titleSlot).toBeNull();
    expect(stagingProps?.locked).toBe(true);
  });
  it("commits a pending note only after the hidden staging editor is ready", () => {
    const commitPendingNoteOpen = vi.fn(() => true);
    const handleEditorReady = vi.fn();

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          activePath="old.md"
          editorBodyMarkdown="old"
          commitPendingNoteOpen={commitPendingNoteOpen}
          handleEditorReady={handleEditorReady}
          pendingNoteOpen={{
            bodyMarkdown: "new staged body",
            content: "# New\n\nnew staged body",
            frontmatterYaml: null,
            isLocked: false,
            namespace: "normal",
            path: "new.md",
            sequence: 42,
            title: "New",
          }}
        />,
      );
    });

    expect(commitPendingNoteOpen).not.toHaveBeenCalled();
    const editorsBeforeReady = Array.from(
      document.querySelectorAll('[data-testid="tiptap-editor"]'),
    );
    expect(
      editorsBeforeReady.map((node) => node.getAttribute("data-path")),
    ).toEqual(["old.md", "new.md"]);
    expect(
      document.querySelector('[data-editor-visibility="visible"]')?.textContent,
    ).toContain("body:old");

    act(() => {
      editorReadyCallbacks.get("new.md")?.({});
    });

    expect(commitPendingNoteOpen).toHaveBeenCalledWith("new.md", 42);
    expect(handleEditorReady).toHaveBeenCalledWith({});
    expect(
      document.querySelector('[data-editor-visibility="visible"]')?.textContent,
    ).toContain("body:new staged body");
  });

  it("promotes an already warm editor into staging without remounting it", () => {
    const commitPendingNoteOpen = vi.fn(() => true);

    const warmNote = {
      bodyMarkdown: "warm body",
      content: "# Warm\n\nwarm body",
      frontmatterYaml: null,
      isLocked: false,
      namespace: "normal" as const,
      path: "warm.md",
      signature: "sig",
      title: "Warm",
      traceKey: "normal:warm",
    };

    act(() => {
      root.render(
        <AppEditorWorkspace {...baseProps()} warmPreparedNotes={[warmNote]} />,
      );
    });

    const warmEditor = document.querySelector('[data-path="warm.md"]');
    expect(warmEditor).toBeTruthy();
    expect(commitPendingNoteOpen).not.toHaveBeenCalled();

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          commitPendingNoteOpen={commitPendingNoteOpen}
          pendingNoteOpen={{
            bodyMarkdown: "warm body",
            content: "# Warm\n\nwarm body",
            frontmatterYaml: null,
            isLocked: false,
            namespace: "normal",
            path: "warm.md",
            sequence: 61,
            title: "Warm",
          }}
          warmPreparedNotes={[warmNote]}
        />,
      );
    });

    const stagedEditor = document.querySelector(
      '[data-editor-visibility="staging"] [data-path="warm.md"]',
    );
    expect(stagedEditor).toBe(warmEditor);

    act(() => {
      editorReadyCallbacks.get("warm.md")?.({});
    });

    expect(commitPendingNoteOpen).toHaveBeenCalledWith("warm.md", 61);
    expect(
      document.querySelector(
        '[data-editor-visibility="visible"] [data-path="warm.md"]',
      ),
    ).toBe(warmEditor);
  });

  it("promotes the prepared staging editor without remounting it", () => {
    const commitPendingNoteOpen = vi.fn(() => true);

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          commitPendingNoteOpen={commitPendingNoteOpen}
          pendingNoteOpen={{
            bodyMarkdown: "new staged body",
            content: "# New\n\nnew staged body",
            frontmatterYaml: null,
            isLocked: false,
            namespace: "normal",
            path: "new.md",
            sequence: 51,
            title: "New",
          }}
        />,
      );
    });

    const stagedEditor = document.querySelector('[data-path="new.md"]');
    expect(stagedEditor).toBeTruthy();

    act(() => {
      editorReadyCallbacks.get("new.md")?.({});
    });

    const promotedEditor = document.querySelector(
      '[data-editor-visibility="visible"] [data-path="new.md"]',
    );
    expect(promotedEditor).toBe(stagedEditor);
  });

  it("ignores a stale pending staging editor that no longer commits", () => {
    const commitPendingNoteOpen = vi.fn(() => false);
    const handleEditorReady = vi.fn();

    act(() => {
      root.render(
        <AppEditorWorkspace
          {...baseProps()}
          commitPendingNoteOpen={commitPendingNoteOpen}
          handleEditorReady={handleEditorReady}
          pendingNoteOpen={{
            bodyMarkdown: "stale body",
            content: "stale body",
            frontmatterYaml: null,
            isLocked: false,
            namespace: "normal",
            path: "stale.md",
            sequence: 7,
            title: "Stale",
          }}
        />,
      );
    });

    act(() => {
      editorReadyCallbacks.get("stale.md")?.({});
    });

    expect(commitPendingNoteOpen).toHaveBeenCalledWith("stale.md", 7);
    expect(handleEditorReady).not.toHaveBeenCalled();
    expect(
      document.querySelector('[data-editor-visibility="visible"]')?.textContent,
    ).toContain("body:old");
  });
});
