import type { Editor } from "@tiptap/react";
import { act, createElement, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  useAppPersistenceLifecycle,
  type PersistBeforeLeave,
} from "@/hooks/useAppPersistenceLifecycle";
import type { TabItem } from "@/components/layout/TabBar";

const fileWrite = vi.fn();
const setCachedEditorHtml = vi.fn();
const fileSetLock = vi.fn();
const versionSaveManual = vi.fn();

vi.mock("@/lib/ipc", () => ({
  appExit: vi.fn(),
  fileSetLock: (...args: unknown[]) => fileSetLock(...args),
  fileWrite: (...args: unknown[]) => fileWrite(...args),
  versionSaveIdle: vi.fn(),
  versionSaveManual: (...args: unknown[]) => versionSaveManual(...args),
}));

vi.mock("@/lib/editor-html-cache", () => ({
  editorHtmlDigest: vi.fn(() => "body-digest"),
  setCachedEditorHtml: (...args: unknown[]) => setCachedEditorHtml(...args),
}));

vi.mock("@/lib/tauri-runtime", () => ({
  isTauriRuntime: () => false,
}));

function Harness({
  editorContentTick = 0,
  editorReady,
  markdown = '---\ntitle: "Note"\n---\n\nOriginal body that must remain authoritative.',
  onReady,
  persistBeforeLeaveRef,
  setFileLocked,
}: {
  editorContentTick?: number;
  editorReady: boolean;
  markdown?: string;
  onReady?: (api: ReturnType<typeof useAppPersistenceLifecycle>) => void;
  persistBeforeLeaveRef: React.MutableRefObject<PersistBeforeLeave>;
  setFileLocked?: (path: string, locked: boolean) => void;
}) {
  const path = "note.md";
  const activePathRef = useRef<string | null>(path);
  const dirtyRef = useRef(true);
  const autoSnapshotGenerationRef = useRef(0);
  const editorReadyRef = useRef(editorReady);
  editorReadyRef.current = editorReady;
  const editorRef = useRef({
    getHTML: () => "<p></p>",
    isDestroyed: false,
  } as Editor);
  const getLiveMarkdownRef = useRef(() => markdown);
  getLiveMarkdownRef.current = () => markdown;
  const tabsRef = useRef<TabItem[]>([
    {
      dirty: true,
      locked: false,
      path,
      title: "Note",
    },
  ]);

  const api = useAppPersistenceLifecycle({
    activeFileLocked: false,
    activePath: path,
    activePathRef,
    applySavedMarkdown: vi.fn(),
    autoSnapshotGenerationRef,
    autoVersionEnabled: false,
    autoVersionIdleMinutes: 5,
    dirtyRef,
    editorContentTick,
    editorReadyRef,
    editorRef,
    getLiveMarkdownRef,
    getTabMarkdownCached: vi.fn(),
    markClean: vi.fn(),
    markdown,
    noteTitle: "Note",
    persistBeforeLeaveRef,
    schedulePathSync: vi.fn(),
    setAiStatus: vi.fn(),
    setFileLocked: setFileLocked ?? vi.fn(),
    setMarkdown: vi.fn(),
    syncTabMarkdownCache: vi.fn(),
    tabsRef,
  });
  onReady?.(api);
  return null;
}

describe("useAppPersistenceLifecycle", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    fileSetLock.mockReset();
    fileSetLock.mockResolvedValue(undefined);
    fileWrite.mockReset();
    fileWrite.mockResolvedValue({
      id: 1,
      path: "note.md",
      title: "Note",
      updated_at: "",
      word_count: 6,
    });
    setCachedEditorHtml.mockReset();
    versionSaveManual.mockReset();
    versionSaveManual.mockResolvedValue(undefined);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("does not write or cache the active note while the editor is not ready for persistence", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorReady: false,
          persistBeforeLeaveRef,
        }),
      );
    });

    await act(async () => {
      await persistBeforeLeaveRef.current("note.md");
    });

    expect(fileWrite).not.toHaveBeenCalled();
    expect(setCachedEditorHtml).not.toHaveBeenCalled();
  });

  it("does not create a manual version snapshot while the editor is not ready", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;
    let api!: ReturnType<typeof useAppPersistenceLifecycle>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorReady: false,
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
        }),
      );
    });

    await act(async () => {
      await api.handleSaveVersion();
    });

    expect(fileWrite).not.toHaveBeenCalled();
    expect(versionSaveManual).not.toHaveBeenCalled();
  });

  it("does not persist a lock change while the editor is not ready", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;
    let api!: ReturnType<typeof useAppPersistenceLifecycle>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorReady: false,
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
        }),
      );
    });

    await act(async () => {
      await api.handleLockToggle(true);
    });

    expect(fileWrite).not.toHaveBeenCalled();
    expect(fileSetLock).not.toHaveBeenCalled();
  });

  it("persists lock changes without invalidating prepared editor html", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;
    const setFileLocked = vi.fn();
    let api!: ReturnType<typeof useAppPersistenceLifecycle>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorReady: true,
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
          setFileLocked,
        }),
      );
    });

    await act(async () => {
      await api.handleLockToggle(true);
    });

    expect(setFileLocked).toHaveBeenCalledWith("note.md", true);
    expect(fileSetLock).toHaveBeenCalledWith("note.md", true);
  });

  it("records same-path loaded markdown as the saved baseline", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;
    const loadedMarkdown =
      '---\ntitle: "Note"\n---\n\nLoaded body from the prepared surface.';
    let api!: ReturnType<typeof useAppPersistenceLifecycle>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorReady: true,
          markdown: loadedMarkdown,
          editorContentTick: 1,
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
        }),
      );
    });

    await act(async () => {
      await api.flushWhenEditorReady("保存");
    });

    expect(fileWrite).not.toHaveBeenCalled();
  });

  it("does not record plain markdown state changes as a saved baseline", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;
    const originalMarkdown = '---\ntitle: "Note"\n---\n\nOriginal loaded body.';
    const localPatchMarkdown =
      '---\ntitle: "Note"\n---\n\nLocal patch that still needs persistence.';
    let api!: ReturnType<typeof useAppPersistenceLifecycle>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorReady: true,
          markdown: originalMarkdown,
          editorContentTick: 1,
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
        }),
      );
    });

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorReady: true,
          markdown: localPatchMarkdown,
          editorContentTick: 1,
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
        }),
      );
    });

    await act(async () => {
      await api.flushWhenEditorReady("保存");
    });

    expect(fileWrite).toHaveBeenCalledWith("note.md", localPatchMarkdown);
  });
});
