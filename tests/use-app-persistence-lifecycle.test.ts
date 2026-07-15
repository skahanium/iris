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

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((nextResolve) => {
    resolve = nextResolve;
  });
  return { promise, resolve };
}

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
  getTabMarkdownCached = () => undefined,
  tabDirty = true,
  tabs,
  onReady,
  onPersistenceBarrierStart,
  persistBeforeLeaveRef,
  applySavedMarkdown = vi.fn(),
  setFileLocked,
}: {
  editorContentTick?: number;
  editorReady: boolean;
  markdown?: string;
  getTabMarkdownCached?: (path: string) => string | undefined;
  tabDirty?: boolean;
  tabs?: TabItem[];
  onReady?: (api: ReturnType<typeof useAppPersistenceLifecycle>) => void;
  onPersistenceBarrierStart?: () => void;
  persistBeforeLeaveRef: React.MutableRefObject<PersistBeforeLeave>;
  applySavedMarkdown?: (markdown: string) => void;
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
  const tabItems = tabs ?? [
    {
      dirty: tabDirty,
      locked: false,
      path,
      title: "Note",
    },
  ];
  const tabsRef = useRef<TabItem[]>(tabItems);
  tabsRef.current = tabItems;

  const api = useAppPersistenceLifecycle({
    activeFileLocked: false,
    activePath: path,
    activePathRef,
    applySavedMarkdown,
    autoSnapshotGenerationRef,
    autoVersionEnabled: false,
    autoVersionIdleMinutes: 5,
    dirtyRef,
    editorContentTick,
    editorReadyRef,
    editorRef,
    getLiveMarkdownRef,
    getTabMarkdownCached,
    markClean: vi.fn(),
    markdown,
    noteTitle: "Note",
    onPersistenceBarrierStart,
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

  it("persists a dirty cached snapshot while the editor is remounting", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorReady: false,
          getTabMarkdownCached: () =>
            '---\ntitle: "Renamed"\n---\n\nBody captured before remount.',
          persistBeforeLeaveRef,
        }),
      );
    });

    await act(async () => {
      await persistBeforeLeaveRef.current("note.md");
    });

    expect(fileWrite).toHaveBeenCalledWith(
      "note.md",
      '---\ntitle: "Renamed"\n---\n\nBody captured before remount.',
    );
    expect(setCachedEditorHtml).not.toHaveBeenCalled();
  });

  it("projects whether any coordinator-owned document still needs persistence", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;
    let api!: ReturnType<typeof useAppPersistenceLifecycle>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorContentTick: 1,
          editorReady: true,
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
        }),
      );
    });

    expect(api.hasDirtyDocuments).toBe(false);

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorContentTick: 1,
          editorReady: true,
          markdown: '---\ntitle: "Note"\n---\n\nUnsaved coordinator snapshot.',
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
        }),
      );
      await Promise.resolve();
    });

    await act(async () => {
      api.notifyDirty();
    });

    expect(api.hasDirtyDocuments).toBe(true);
  });

  it("uses the coordinator revision rather than a stale tab dirty flag before leaving", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;
    let api!: ReturnType<typeof useAppPersistenceLifecycle>;
    const markdown = '---\ntitle: "Note"\n---\n\nCoordinator is authoritative.';

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorContentTick: 1,
          editorReady: true,
          markdown,
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
          tabDirty: false,
        }),
      );
    });

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorContentTick: 1,
          editorReady: true,
          markdown:
            '---\ntitle: "Note"\n---\n\nMust write despite stale tab state.',
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
          tabDirty: false,
        }),
      );
      await Promise.resolve();
    });

    await act(async () => {
      api.notifyDirty();
      await persistBeforeLeaveRef.current("note.md");
    });

    expect(fileWrite).toHaveBeenCalledWith(
      "note.md",
      '---\ntitle: "Note"\n---\n\nMust write despite stale tab state.',
    );
  });

  it("freezes new captures until the close barrier reaches a durable fixed point", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;
    const firstWrite = deferred<{
      id: number;
      path: string;
      title: string;
      updated_at: string;
      word_count: number;
    }>();
    fileWrite.mockReturnValueOnce(firstWrite.promise);
    let api!: ReturnType<typeof useAppPersistenceLifecycle>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorContentTick: 1,
          editorReady: true,
          markdown: '---\ntitle: "Note"\n---\n\nOpened revision.',
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
          editorContentTick: 1,
          editorReady: true,
          markdown: '---\ntitle: "Note"\n---\n\nFirst captured revision.',
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
        }),
      );
      await Promise.resolve();
      api.notifyDirty();
    });
    let closing!: Promise<void>;
    await act(async () => {
      closing = api.flushAllOpenTabs();
      await Promise.resolve();
    });
    expect(api.isPersistenceBarrierActive).toBe(true);

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorContentTick: 1,
          editorReady: true,
          markdown:
            '---\ntitle: "Note"\n---\n\nMust not capture after close starts.',
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
        }),
      );
      await Promise.resolve();
      api.notifyDirty();
    });

    await act(async () => {
      firstWrite.resolve({
        id: 1,
        path: "note.md",
        title: "Note",
        updated_at: "",
        word_count: 3,
      });
      await closing;
    });

    expect(fileWrite.mock.calls).toEqual([
      ["note.md", '---\ntitle: "Note"\n---\n\nFirst captured revision.'],
    ]);
    expect(api.isPersistenceBarrierActive).toBe(true);

    await act(async () => {
      api.releasePersistenceBarrier();
    });
    expect(api.isPersistenceBarrierActive).toBe(false);
  });

  it("invokes the departure interaction lock synchronously before the close barrier yields", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;
    const setEditable = vi.fn();
    let api!: ReturnType<typeof useAppPersistenceLifecycle>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorContentTick: 1,
          editorReady: true,
          onPersistenceBarrierStart: () => setEditable(false),
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
        }),
      );
    });

    void api.flushAllOpenTabs();

    expect(setEditable).toHaveBeenCalledWith(false);
  });

  it("stages an uncaptured dirty background tab before the global close barrier", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;
    const backgroundMarkdown =
      '---\ntitle: "Background"\n---\n\nCached before close.';
    let api!: ReturnType<typeof useAppPersistenceLifecycle>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorContentTick: 1,
          editorReady: true,
          getTabMarkdownCached: (path) =>
            path === "background.md" ? backgroundMarkdown : undefined,
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
          tabs: [
            {
              dirty: false,
              locked: false,
              path: "note.md",
              title: "Note",
            },
            {
              dirty: true,
              locked: false,
              path: "background.md",
              title: "Background",
            },
          ],
        }),
      );
    });

    await act(async () => {
      await api.flushAllOpenTabs();
    });

    expect(fileWrite).toHaveBeenCalledWith("background.md", backgroundMarkdown);
  });

  it("releases the capture freeze after a close barrier write failure", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;
    let api!: ReturnType<typeof useAppPersistenceLifecycle>;
    const retryMarkdown =
      '---\ntitle: "Note"\n---\n\nRetry after the failed close barrier.';

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorContentTick: 1,
          editorReady: true,
          markdown: '---\ntitle: "Note"\n---\n\nFirst close attempt.',
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
          editorContentTick: 1,
          editorReady: true,
          markdown:
            '---\ntitle: "Note"\n---\n\nFirst close attempt that needs saving.',
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
        }),
      );
      api.notifyDirty();
    });
    fileWrite.mockRejectedValueOnce(new Error("disk unavailable"));

    await act(async () => {
      await expect(api.flushAllOpenTabs()).rejects.toThrow("disk unavailable");
    });

    expect(api.isPersistenceBarrierActive).toBe(false);

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorContentTick: 1,
          editorReady: true,
          markdown: retryMarkdown,
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
        }),
      );
    });
    await act(async () => {
      api.notifyDirty();
      await api.flushAllOpenTabs();
    });

    expect(fileWrite).toHaveBeenLastCalledWith("note.md", retryMarkdown);
  });

  it("rejects close persistence when a dirty remount has no recoverable snapshot", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorReady: false,
          getTabMarkdownCached: () => undefined,
          persistBeforeLeaveRef,
        }),
      );
    });

    await expect(
      persistBeforeLeaveRef.current("note.md", { reason: "app_close" }),
    ).rejects.toThrow("no recoverable snapshot");
    expect(fileWrite).not.toHaveBeenCalled();
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

  it("projects degraded index status after a title-driven path rename", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;
    let api!: ReturnType<typeof useAppPersistenceLifecycle>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorContentTick: 1,
          editorReady: true,
          markdown: "# loaded",
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
        }),
      );
    });

    await act(async () => {
      await expect(
        api.renamePath("note.md", "renamed.md", "# loaded", async () => ({
          path: "renamed.md",
          indexDegraded: true,
        })),
      ).resolves.toBe("# loaded");
    });

    expect(api.saveStatus).toBe("saved_index_degraded");
  });

  it("waits for an in-flight save before durably acknowledging a restored version", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;
    const applySavedMarkdown = vi.fn();
    const firstWrite = deferred<{
      id: number;
      path: string;
      title: string;
      updated_at: string;
      word_count: number;
    }>();
    const restoredWrite = deferred<{
      id: number;
      path: string;
      title: string;
      updated_at: string;
      word_count: number;
    }>();
    fileWrite.mockReturnValueOnce(firstWrite.promise);
    fileWrite.mockReturnValueOnce(restoredWrite.promise);
    let api!: ReturnType<typeof useAppPersistenceLifecycle>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          applySavedMarkdown,
          editorContentTick: 1,
          editorReady: true,
          markdown: "# loaded",
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
        }),
      );
    });
    applySavedMarkdown.mockClear();

    await act(async () => {
      root.render(
        createElement(Harness, {
          applySavedMarkdown,
          editorContentTick: 1,
          editorReady: true,
          markdown: "# unsaved before restore",
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
        }),
      );
      api.notifyDirty();
      await Promise.resolve();
    });
    applySavedMarkdown.mockClear();

    let initialSave!: Promise<string | null>;
    await act(async () => {
      initialSave = api.flushSave();
      await Promise.resolve();
    });
    expect(fileWrite).toHaveBeenCalledWith(
      "note.md",
      "# unsaved before restore",
    );

    let restored!: Promise<string>;
    await act(async () => {
      restored = api.restoreVersion("note.md", "# historical version");
      await Promise.resolve();
    });

    await act(async () => {
      firstWrite.resolve({
        id: 1,
        path: "note.md",
        title: "Note",
        updated_at: "",
        word_count: 3,
      });
      await Promise.resolve();
    });
    expect(applySavedMarkdown).not.toHaveBeenCalled();
    await vi.waitFor(() => {
      expect(fileWrite).toHaveBeenLastCalledWith(
        "note.md",
        "# historical version",
      );
    });

    await act(async () => {
      restoredWrite.resolve({
        id: 1,
        path: "note.md",
        title: "Note",
        updated_at: "",
        word_count: 3,
      });
      await expect(restored).resolves.toBe("# historical version");
    });
    await expect(initialSave).resolves.toBe("# historical version");
    expect(applySavedMarkdown).toHaveBeenCalledWith("# historical version");
  });
});
