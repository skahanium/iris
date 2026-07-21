import { act } from "react";
import { createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const fileDiscard = vi.fn();
const fileRead = vi.fn();
const createDefaultNote = vi.fn();

vi.mock("@/lib/ipc", () => ({
  fileDiscard: (...args: unknown[]) => fileDiscard(...args),
  fileRead: (...args: unknown[]) => fileRead(...args),
}));

vi.mock("@/lib/note-create", () => ({
  createDefaultNote: (options: unknown) => createDefaultNote(options),
}));

vi.mock("@/lib/document-title", () => ({
  displayTitleFromMarkdown: (_md: string, fallback: string) => fallback,
  resolveDocumentTitle: async (_path: string, hint?: string) =>
    hint?.trim() || "未命名文档",
}));

vi.mock("@/lib/markdown", async () => {
  const actual =
    await vi.importActual<typeof import("@/lib/markdown")>("@/lib/markdown");
  return {
    ...actual,
    parseNoteForEditor: (markdown: string) => {
      const match = markdown.match(/^---\n([\s\S]*?)\n---\n?([\s\S]*)$/);
      const yaml = match?.[1] ?? null;
      const bodyMd = match?.[2] ?? markdown;
      const titleMatch = yaml?.match(/title:\s*"?([^"\n]+)"?/);
      return {
        bodyMarkdown: bodyMd,
        bodyMd,
        frontmatterYaml: yaml,
        title: titleMatch?.[1] ?? null,
        yaml,
      };
    },
  };
});

import type { TabItem } from "@/components/layout/TabBar";
import { useTabManager } from "@/hooks/useTabManager";

const EMPTY_MD = '---\ntitle: "未命名文档"\n---\n\n';

function fileReadResult(content: string, isLocked = false) {
  return { content, isLocked };
}

function Harness({
  apiRef,
  persistBeforeLeave,
  getLiveMarkdown,
}: {
  apiRef: { current: ReturnType<typeof useTabManager> | null };
  persistBeforeLeave?: (path: string) => Promise<string | null>;
  getLiveMarkdown?: () => string;
}) {
  const api = useTabManager({
    discardPristineNote: async (path) => {
      await fileDiscard(path);
    },
    getLiveMarkdown,
    persistBeforeLeave,
  });
  apiRef.current = api;
  return null;
}

async function runAndWait(
  apiRef: { current: ReturnType<typeof useTabManager> | null },
  path: string,
  action: () => Promise<unknown>,
) {
  await act(async () => {
    await action();
  });

  const pending = apiRef.current!.pendingNoteOpen;
  if (pending) {
    expect(pending.path).toBe(path);
    await act(async () => {
      apiRef.current!.commitPendingNoteOpen(pending.path, pending.sequence);
    });
  }

  expect(apiRef.current!.activePath).toBe(path);
  expect(apiRef.current!.pendingNoteOpen).toBeNull();
}

async function openAndCommit(
  apiRef: { current: ReturnType<typeof useTabManager> | null },
  path: string,
  titleHint?: string,
) {
  await runAndWait(apiRef, path, () =>
    apiRef.current!.openFile(path, titleHint),
  );
}

describe("useTabManager handleNewNote", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    fileDiscard.mockReset();
    fileRead.mockReset();
    createDefaultNote.mockReset();
    fileDiscard.mockResolvedValue(undefined);
    fileRead.mockResolvedValue(fileReadResult(EMPTY_MD));
    createDefaultNote.mockResolvedValue({
      content: EMPTY_MD,
      path: "未命名文档.md",
      title: "未命名文档",
    });
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("creates a note when no tab is active", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await runAndWait(apiRef, "未命名文档.md", () =>
      apiRef.current!.handleNewNote(),
    );

    expect(createDefaultNote).toHaveBeenCalledTimes(1);
    expect(apiRef.current!.activePath).toBe("未命名文档.md");
  });

  it("stages a newly created note from its authoritative Markdown without rereading or pre-ingesting it", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await act(async () => {
      await apiRef.current!.handleNewNote();
    });

    expect(fileRead).not.toHaveBeenCalled();
    expect(apiRef.current!.pendingNoteOpen).toMatchObject({
      bodyMarkdown: "\n",
      content: EMPTY_MD,
      frontmatterYaml: 'title: "未命名文档"',
      openBudgetKind: "hot",
      path: "未命名文档.md",
      title: "未命名文档",
    });

    const pending = apiRef.current!.pendingNoteOpen!;
    await act(async () => {
      apiRef.current!.commitPendingNoteOpen(pending.path, pending.sequence);
    });

    expect(apiRef.current!.activePath).toBe("未命名文档.md");
    expect(apiRef.current!.pendingNoteOpen).toBeNull();
  });

  it("preserves the home open sequence separately from the tab open sequence", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await act(async () => {
      await apiRef.current!.handleNewNote({ homeOpenSequence: 41 });
    });

    expect(apiRef.current!.pendingNoteOpen).toMatchObject({
      homeOpenSequence: 41,
      path: "未命名文档.md",
    });
    expect(apiRef.current!.pendingNoteOpen?.sequence).not.toBe(41);
  });

  it("registers a new note before first frame and does not bump editorContentTick again on commit", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    const initialTick = apiRef.current!.editorContentTick;

    await act(async () => {
      await apiRef.current!.handleNewNote();
    });

    expect(apiRef.current!.editorContentTick).toBe(initialTick + 1);

    const pending = apiRef.current!.pendingNoteOpen!;
    await act(async () => {
      apiRef.current!.commitPendingNoteOpen(pending.path, pending.sequence, {
        skipContentTick: true,
      });
    });

    expect(apiRef.current!.activePath).toBe("未命名文档.md");
    expect(apiRef.current!.editorContentTick).toBe(initialTick + 1);
  });

  it("still bumps editorContentTick when committing a direct disk open", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    fileRead.mockResolvedValueOnce(fileReadResult("# Disk\n\nBody"));

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    const initialTick = apiRef.current!.editorContentTick;

    await openAndCommit(apiRef, "disk.md", "Disk");

    expect(apiRef.current!.activePath).toBe("disk.md");
    expect(apiRef.current!.editorContentTick).toBe(initialTick + 1);
  });

  it("never permanently discards an existing empty tab before creating the next note", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    createDefaultNote.mockResolvedValueOnce({
      content: '---\ntitle: "未命名文档（1）"\n---\n\n',
      path: "未命名文档（1）.md",
      title: "未命名文档（1）",
    });

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await openAndCommit(apiRef, "未命名文档.md", "未命名文档");
    expect(apiRef.current!.activePath).toBe("未命名文档.md");

    await runAndWait(apiRef, "未命名文档（1）.md", () =>
      apiRef.current!.handleNewNote(),
    );

    expect(fileDiscard).not.toHaveBeenCalled();
    expect(createDefaultNote).toHaveBeenCalledWith({
      extraTakenTitles: ["未命名文档"],
    });
    expect(apiRef.current!.activePath).toBe("未命名文档（1）.md");
    expect(
      apiRef.current!.tabs.some((t: TabItem) => t.path === "未命名文档.md"),
    ).toBe(true);
    expect(
      apiRef.current!.tabs.some(
        (t: TabItem) => t.path === "未命名文档（1）.md",
      ),
    ).toBe(true);
  });

  it("registers every rapid new-note request as a distinct tab before either editor first frame commits", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };
    createDefaultNote
      .mockResolvedValueOnce({
        content: EMPTY_MD,
        path: "未命名文档.md",
        title: "未命名文档",
      })
      .mockResolvedValueOnce({
        content: '---\ntitle: "未命名文档（1）"\n---\n\n',
        path: "未命名文档（1）.md",
        title: "未命名文档（1）",
      });
    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    const first = apiRef.current!.handleNewNote();
    await Promise.resolve();
    const second = apiRef.current!.handleNewNote();

    await act(async () => {
      await Promise.all([first, second]);
    });

    expect(createDefaultNote).toHaveBeenCalledTimes(2);
    expect(apiRef.current!.tabs.map((tab) => tab.path)).toEqual([
      "未命名文档.md",
      "未命名文档（1）.md",
    ]);
    expect(apiRef.current!.tabs.map((tab) => tab.lifecycle)).toEqual([
      "session_pristine",
      "session_pristine",
    ]);
  });

  it("discards a never-edited temporary tab only when it is closed", async () => {
    const persistBeforeLeave = vi.fn(async () => "should-not-save");
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    await act(async () => {
      root.render(createElement(Harness, { apiRef, persistBeforeLeave }));
    });
    await act(async () => {
      await apiRef.current!.handleNewNote();
    });

    const pending = apiRef.current!.pendingNoteOpen!;
    await act(async () => {
      apiRef.current!.commitPendingNoteOpen(pending.path, pending.sequence);
    });

    let closeResult: {
      closed: boolean;
      discardedPristine: boolean;
      nextActivePath: string | null;
      remainingNoteCount: number;
    };
    await act(async () => {
      closeResult = await apiRef.current!.closeTab("未命名文档.md");
    });

    expect(persistBeforeLeave).not.toHaveBeenCalled();
    expect(fileDiscard).toHaveBeenCalledTimes(1);
    expect(closeResult!).toEqual({
      closed: true,
      discardedPristine: true,
      nextActivePath: null,
      remainingNoteCount: 0,
    });
    expect(apiRef.current!.tabs).toEqual([]);
    expect(apiRef.current!.activePath).toBeNull();
  });

  it("persists a pristine tab that already has substantive live content", async () => {
    createDefaultNote.mockResolvedValueOnce({
      content: "",
      path: "未命名文档.md",
      title: "未命名文档",
    });
    const persistBeforeLeave = vi.fn(async () => "typed body");
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };
    const getLiveMarkdown = vi.fn(() => "typed body");

    await act(async () => {
      root.render(
        createElement(Harness, {
          apiRef,
          persistBeforeLeave,
          getLiveMarkdown,
        }),
      );
    });
    await act(async () => {
      await apiRef.current!.handleNewNote();
    });
    const pending = apiRef.current!.pendingNoteOpen!;
    await act(async () => {
      apiRef.current!.commitPendingNoteOpen(pending.path, pending.sequence);
    });

    let closeResult: {
      closed: boolean;
      discardedPristine: boolean;
      nextActivePath: string | null;
      remainingNoteCount: number;
    };
    await act(async () => {
      closeResult = await apiRef.current!.closeTab("未命名文档.md");
    });

    expect(persistBeforeLeave).toHaveBeenCalledWith("未命名文档.md", {
      reason: "tab_leave",
      suppressShellUi: true,
      retainSuppressShellUi: true,
    });
    expect(fileDiscard).not.toHaveBeenCalled();
    expect(closeResult!).toEqual({
      closed: true,
      discardedPristine: false,
      nextActivePath: null,
      remainingNoteCount: 0,
    });
  });

  it("closes a blank pristine new note whose markdown is empty string", async () => {
    createDefaultNote.mockResolvedValueOnce({
      content: "",
      path: "未命名文档.md",
      title: "未命名文档",
    });
    const persistBeforeLeave = vi.fn(async () => "should-not-save");
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    await act(async () => {
      root.render(createElement(Harness, { apiRef, persistBeforeLeave }));
    });
    await act(async () => {
      await apiRef.current!.handleNewNote();
    });
    const pending = apiRef.current!.pendingNoteOpen!;
    await act(async () => {
      apiRef.current!.commitPendingNoteOpen(pending.path, pending.sequence);
    });

    let closeResult: {
      closed: boolean;
      discardedPristine: boolean;
      nextActivePath: string | null;
      remainingNoteCount: number;
    };
    await act(async () => {
      closeResult = await apiRef.current!.closeTab("未命名文档.md");
    });

    expect(persistBeforeLeave).not.toHaveBeenCalled();
    expect(fileDiscard).toHaveBeenCalledWith("未命名文档.md");
    expect(closeResult!).toEqual({
      closed: true,
      discardedPristine: true,
      nextActivePath: null,
      remainingNoteCount: 0,
    });
    expect(apiRef.current!.tabs).toEqual([]);
  });

  it("never reclassifies a temporary note as disposable after user edits are deleted", async () => {
    const persistBeforeLeave = vi.fn(async () => EMPTY_MD);
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    await act(async () => {
      root.render(createElement(Harness, { apiRef, persistBeforeLeave }));
    });
    await act(async () => {
      await apiRef.current!.handleNewNote();
    });
    const pending = apiRef.current!.pendingNoteOpen!;
    await act(async () => {
      apiRef.current!.commitPendingNoteOpen(pending.path, pending.sequence);
      apiRef.current!.setMarkdown(
        '---\ntitle: "未命名文档"\n---\n\nA user edit',
      );
      apiRef.current!.markDirty();
      apiRef.current!.setMarkdown(EMPTY_MD);
      await apiRef.current!.closeTab("未命名文档.md");
    });

    expect(persistBeforeLeave).toHaveBeenCalledWith("未命名文档.md", {
      reason: "tab_leave",
      suppressShellUi: true,
      retainSuppressShellUi: true,
    });
    expect(fileDiscard).not.toHaveBeenCalled();
  });

  it("keeps a pristine tab active when its safe discard fails", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };
    fileDiscard.mockRejectedValueOnce(new Error("discard unavailable"));

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });
    await act(async () => {
      await apiRef.current!.handleNewNote();
    });
    const pending = apiRef.current!.pendingNoteOpen!;
    await act(async () => {
      apiRef.current!.commitPendingNoteOpen(pending.path, pending.sequence);
    });
    const result = await apiRef.current!.closeTab("未命名文档.md");

    expect(result.closed).toBe(false);
    expect(apiRef.current!.activePath).toBe("未命名文档.md");
    expect(apiRef.current!.tabs).toHaveLength(1);
  });

  it("closes a tab and switches to the neighbor when closing the active tab", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    fileRead.mockImplementation(async (path: string) => {
      if (path === "a.md") {
        return fileReadResult("# A\n");
      }
      return fileReadResult(EMPTY_MD);
    });

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await openAndCommit(apiRef, "a.md", "A");
    await openAndCommit(apiRef, "b.md", "B");

    await runAndWait(apiRef, "a.md", () => apiRef.current!.closeTab("b.md"));

    expect(apiRef.current!.activePath).toBe("a.md");
    expect(apiRef.current!.tabs.map((t: TabItem) => t.path)).toEqual(["a.md"]);
  });

  it("keeps unsaved live markdown when a placeholder note path is renamed", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };
    const liveMarkdown =
      '---\ntitle: "调度优化"\n---\n\n# 调度优化\n\n从网页粘贴进来的正文';

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await openAndCommit(apiRef, "未命名文档.md", "未命名文档");

    await act(async () => {
      apiRef.current!.replaceOpenTabPath(
        "未命名文档.md",
        "调度优化.md",
        "调度优化",
        liveMarkdown,
      );
    });

    expect(apiRef.current!.activePath).toBe("调度优化.md");
    expect(apiRef.current!.markdown).toBe(liveMarkdown);
    expect(apiRef.current!.getEditorMarkdown()).toBe(liveMarkdown);
    expect(apiRef.current!.getTabMarkdownCached("调度优化.md")).toBe(
      liveMarkdown,
    );
  });

  it("preserves a dirty destination cache when merging onto an open clean source tab", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };
    const destinationDirty =
      '---\ntitle: "Target"\n---\n\nDestination unsaved body';

    fileRead.mockImplementation(async (path: string) => {
      if (path === "source.md") {
        return fileReadResult("# Source\n");
      }
      if (path === "target.md") {
        return fileReadResult("# Target\n");
      }
      throw new Error(`unexpected read: ${path}`);
    });

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await openAndCommit(apiRef, "source.md", "Source");
    await openAndCommit(apiRef, "target.md", "Target");

    await act(async () => {
      apiRef.current!.syncTabMarkdownCache("target.md", destinationDirty);
      apiRef.current!.markDirty();
    });
    expect(
      apiRef.current!.tabs.find((tab) => tab.path === "target.md")?.dirty,
    ).toBe(true);

    await act(async () => {
      apiRef.current!.replaceOpenTabPath("source.md", "target.md", "Target");
    });

    expect(apiRef.current!.tabs.map((tab) => tab.path)).toEqual(["target.md"]);
    expect(apiRef.current!.getTabMarkdownCached("target.md")).toBe(
      destinationDirty,
    );
    expect(apiRef.current!.tabs[0]?.dirty).toBe(true);
  });

  it("keeps close successful when switching to the next tab fails after removal", async () => {
    const persistBeforeLeave = vi.fn(async () => "# A\n");
    const onStatusChange = vi.fn();
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    fileRead.mockImplementation(async (path: string) => {
      if (path === "a.md") {
        return fileReadResult("# A\n");
      }
      if (path === "b.md") {
        return fileReadResult("# B\n");
      }
      throw new Error(`unexpected read: ${path}`);
    });

    function CloseHarness() {
      const api = useTabManager({
        discardPristineNote: async (path) => {
          await fileDiscard(path);
        },
        onStatusChange,
        persistBeforeLeave,
      });
      apiRef.current = api;
      return null;
    }

    await act(async () => {
      root.render(createElement(CloseHarness));
    });

    await openAndCommit(apiRef, "a.md", "A");
    await openAndCommit(apiRef, "b.md", "B");

    await act(async () => {
      apiRef.current!.invalidateDocumentRuntimeState("a.md");
    });
    expect(apiRef.current!.getTabMarkdownCached("a.md")).toBeUndefined();

    fileRead.mockImplementation(async (path: string) => {
      if (path === "a.md") {
        throw new Error("disk read failed for next tab");
      }
      throw new Error(`unexpected read: ${path}`);
    });

    let closeResult!: { closed: boolean; remainingNoteCount: number };
    await act(async () => {
      closeResult = await apiRef.current!.closeTab("b.md");
    });

    expect(closeResult.closed).toBe(true);
    expect(apiRef.current!.tabs.map((tab) => tab.path)).toEqual(["a.md"]);
    expect(onStatusChange).toHaveBeenCalledWith(
      expect.stringContaining("切换到相邻标签失败"),
    );
  });

  it("still closes the tab when its path is renamed during close persistence", async () => {
    let releasePersist!: (value: string) => void;
    const persistBeforeLeave = vi.fn(
      () =>
        new Promise<string>((resolve) => {
          releasePersist = resolve;
        }),
    );
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    fileRead.mockImplementation(async (path: string) => {
      if (path === "note.md") {
        return fileReadResult("# Note\n");
      }
      throw new Error(`unexpected read: ${path}`);
    });

    await act(async () => {
      root.render(createElement(Harness, { apiRef, persistBeforeLeave }));
    });
    await openAndCommit(apiRef, "note.md", "note");

    let closePromise!: Promise<{
      closed: boolean;
      remainingNoteCount: number;
      nextActivePath: string | null;
    }>;
    await act(async () => {
      closePromise = apiRef.current!.closeTab("note.md");
      await Promise.resolve();
    });
    expect(persistBeforeLeave).toHaveBeenCalled();

    await act(async () => {
      apiRef.current!.replaceOpenTabPath(
        "note.md",
        "note-renamed.md",
        "note-renamed",
        "# Note\n",
      );
    });
    expect(apiRef.current!.tabs.map((tab) => tab.path)).toEqual([
      "note-renamed.md",
    ]);

    await act(async () => {
      releasePersist("# Note\n");
      await closePromise;
    });

    expect(apiRef.current!.tabs).toEqual([]);
    expect(apiRef.current!.activePath).toBeNull();
    await expect(closePromise).resolves.toMatchObject({
      closed: true,
      remainingNoteCount: 0,
      nextActivePath: null,
    });
  });
});
