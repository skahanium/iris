import { act } from "react";
import { createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const fileDiscard = vi.fn();
const fileRead = vi.fn();
const resolveDocumentTitle = vi.fn();

vi.mock("@/lib/ipc", () => ({
  fileDiscard: (...args: unknown[]) => fileDiscard(...args),
  fileRead: (...args: unknown[]) => fileRead(...args),
}));

vi.mock("@/lib/note-create", () => ({
  createDefaultNote: vi.fn(),
}));

vi.mock("@/lib/document-title", () => ({
  displayTitleFromMarkdown: (_md: string, fallback: string) => fallback,
  resolveDocumentTitle: (...args: unknown[]) => resolveDocumentTitle(...args),
}));

vi.mock("@/lib/markdown", async () => {
  const actual =
    await vi.importActual<typeof import("@/lib/markdown")>("@/lib/markdown");
  return {
    ...actual,
    extractFrontmatterYaml: actual.extractFrontmatterYaml,
    parseNoteForEditor: actual.parseNoteForEditor,
    stripLeadingBodyTitleHeading: (body: string) => body,
  };
});

vi.mock("@/lib/editor-html-cache", () => ({
  clearCachedEditorHtml: vi.fn(),
  editorHtmlDigest: vi.fn(() => "digest"),
  setCachedEditorHtml: vi.fn(),
}));

import { useTabManager } from "@/hooks/useTabManager";
import {
  NOTE_OPEN_HOT_PATH_BUDGET_MS,
  setNoteOpenTraceSink,
  type NoteOpenTrace,
} from "@/lib/note-open-preparation";

function Harness({
  apiRef,
}: {
  apiRef: { current: ReturnType<typeof useTabManager> | null };
}) {
  const api = useTabManager();
  apiRef.current = api;
  return null;
}

async function openAndWait(
  apiRef: { current: ReturnType<typeof useTabManager> | null },
  path: string,
  titleHint?: string,
  options?: Parameters<ReturnType<typeof useTabManager>["openFile"]>[2],
) {
  let openPromise!: Promise<void>;
  await act(async () => {
    openPromise = apiRef.current!.openFile(path, titleHint, options);
    await openPromise;
  });
  const pending = apiRef.current!.pendingNoteOpen;
  if (pending) {
    await act(async () => {
      apiRef.current!.commitPendingNoteOpen(pending.path, pending.sequence);
    });
  }
  expect(apiRef.current!.pendingNoteOpen).toBeNull();
}

describe("useTabManager activateTab / openNote", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    fileDiscard.mockReset();
    fileRead.mockReset();
    resolveDocumentTitle.mockReset();
    resolveDocumentTitle.mockImplementation(
      async (_path: string, hint?: string) => hint?.trim() || "Title",
    );
    fileDiscard.mockResolvedValue(undefined);
    setNoteOpenTraceSink(null);
    fileRead.mockImplementation(async (path: string) => {
      if (path === "a.md") {
        return { content: '---\ntitle: "A"\n---\n\nbody-a', isLocked: false };
      }
      if (path === "b.md") {
        return { content: '---\ntitle: "B"\n---\n\nbody-b', isLocked: false };
      }
      return { content: '---\ntitle: "X"\n---\n\nbody', isLocked: false };
    });
  });

  afterEach(() => {
    setNoteOpenTraceSink(null);
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("activateTab restores in-memory edits without re-reading disk", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await openAndWait(apiRef, "a.md", "A");
    const edited = '---\ntitle: "A"\n---\n\nedited-a';
    act(() => {
      apiRef.current!.setMarkdown(edited);
    });

    await openAndWait(apiRef, "b.md", "B");
    expect(fileRead).toHaveBeenCalledTimes(2);

    await act(async () => {
      await apiRef.current!.activateTab("a.md");
    });

    expect(fileRead).toHaveBeenCalledTimes(2);
    expect(apiRef.current!.activePath).toBe("a.md");
    expect(apiRef.current!.markdown).toBe(edited);
    expect(apiRef.current!.pendingNoteOpen).toBeNull();
  });

  it("starts reading the target note without waiting for old-note persistence", async () => {
    let resolvePersist: ((value: string | null) => void) | null = null;
    const persistBeforeLeave = vi.fn(
      () =>
        new Promise<string | null>((resolve) => {
          resolvePersist = resolve;
        }),
    );
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    function PersistHarness() {
      const api = useTabManager({ persistBeforeLeave });
      apiRef.current = api;
      return null;
    }

    await act(async () => {
      root.render(createElement(PersistHarness));
    });

    await openAndWait(apiRef, "a.md", "A");
    fileRead.mockClear();

    let openPromise!: Promise<void>;
    await act(async () => {
      openPromise = apiRef.current!.openFile("b.md", "B");
      await Promise.resolve();
    });

    expect(persistBeforeLeave).toHaveBeenCalledWith("a.md");
    expect(fileRead).toHaveBeenCalledWith("b.md", {
      allowClassified: false,
    });
    expect(apiRef.current!.activePath).toBe("a.md");
    expect(apiRef.current!.markdown).toContain("body-a");

    await act(async () => {
      (resolvePersist as unknown as (value: string | null) => void)("saved-a");
      await openPromise;
    });

    const pending = apiRef.current!.pendingNoteOpen;
    expect(apiRef.current!.activePath).toBe("a.md");
    expect(pending?.path).toBe("b.md");
    await act(async () => {
      apiRef.current!.commitPendingNoteOpen(pending!.path, pending!.sequence);
    });

    expect(apiRef.current!.activePath).toBe("b.md");
    expect(apiRef.current!.markdown).toContain("body-b");
  });

  it("openNote uses activateTab when the path is already open", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await openAndWait(apiRef, "a.md", "A");
    await openAndWait(apiRef, "b.md", "B");
    expect(fileRead).toHaveBeenCalledTimes(2);

    await act(async () => {
      await apiRef.current!.openNote("a.md");
    });

    expect(fileRead).toHaveBeenCalledTimes(2);
    expect(apiRef.current!.activePath).toBe("a.md");
  });

  it("openNote reads disk when the tab is not open yet", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await act(async () => {
      await apiRef.current!.openNote("a.md", "A");
    });

    expect(fileRead).toHaveBeenCalledTimes(1);
    const pending = apiRef.current!.pendingNoteOpen;
    expect(apiRef.current!.activePath).toBeNull();
    expect(pending?.path).toBe("a.md");
    await act(async () => {
      apiRef.current!.commitPendingNoteOpen(pending!.path, pending!.sequence);
    });
    expect(apiRef.current!.activePath).toBe("a.md");
    expect(apiRef.current!.pendingNoteOpen).toBeNull();
    expect(resolveDocumentTitle).not.toHaveBeenCalled();
  });

  it("derives prepared note namespace from the path instead of trusting payload metadata", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };
    const traces: NoteOpenTrace[] = [];
    const prepared = {
      bodyMarkdown: "classified body",
      content: '---\ntitle: "Secret"\n---\n\nclassified body',
      frontmatterYaml: 'title: "Secret"',
      isLocked: false,
      namespace: "normal" as const,
      path: ".classified/secret.md",
      signature: "sig",
      title: "Secret",
      traceKey: "normal:bad",
    } as const;

    setNoteOpenTraceSink((trace) => traces.push(trace));
    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await openAndWait(apiRef, ".classified/secret.md", "Secret", {
      allowClassified: true,
      openBudgetKind: "hot",
      openStartedAt: 1,
      openTraceRequest: {
        allowClassified: true,
        path: ".classified/secret.md",
      },
      preparedNote: prepared,
    });

    expect(fileRead).not.toHaveBeenCalled();
    expect(apiRef.current!.activePath).toBe(".classified/secret.md");
    expect(traces.find((trace) => trace.phase === "visible-commit")).toEqual(
      expect.objectContaining({ namespace: "classified" }),
    );
  });

  it("openFile can consume a prepared note without re-reading disk", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };
    const prepared = {
      bodyMarkdown: "prepared body",
      content: '---\ntitle: "Prepared"\n---\n\nprepared body',
      frontmatterYaml: 'title: "Prepared"',
      isLocked: true,
      namespace: "normal" as const,
      path: "prepared.md",
      signature: "sig",
      title: "Prepared",
      traceKey: "normal:abc",
    };

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await openAndWait(apiRef, "prepared.md", "Prepared", {
      preparedNote: prepared,
    });

    expect(fileRead).not.toHaveBeenCalled();
    expect(apiRef.current!.activePath).toBe("prepared.md");
    expect(apiRef.current!.markdown).toBe(prepared.content);
    expect(apiRef.current!.activeFileLocked).toBe(true);
  });

  it("emits a hot visible-commit trace when a prepared note becomes active", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };
    const traces: NoteOpenTrace[] = [];
    const nowSpy = vi.spyOn(performance, "now").mockReturnValue(1010);
    const prepared = {
      bodyMarkdown: "prepared body",
      content: '---\ntitle: "Prepared"\n---\n\nprepared body',
      frontmatterYaml: 'title: "Prepared"',
      isLocked: false,
      namespace: "normal" as const,
      path: "prepared.md",
      signature: "sig",
      title: "Prepared",
      traceKey: "normal:abc",
    };

    setNoteOpenTraceSink((trace) => traces.push(trace));
    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await openAndWait(apiRef, "prepared.md", "Prepared", {
      openBudgetKind: "hot",
      openStartedAt: 1000,
      openTraceRequest: { path: "prepared.md", titleHint: "Prepared" },
      preparedNote: prepared,
    });

    const visibleCommit = traces.find(
      (trace) => trace.phase === "visible-commit",
    );
    expect(visibleCommit).toMatchObject({
      budgetExceeded: false,
      budgetKind: "hot",
      budgetMs: NOTE_OPEN_HOT_PATH_BUDGET_MS,
      cache: "none",
      durationMs: 10,
      namespace: "normal" as const,
      phase: "visible-commit",
      status: "ok",
    });
    expect(visibleCommit?.key).not.toContain("prepared.md");
    expect(visibleCommit?.key).not.toContain("Prepared");
    expect(fileRead).not.toHaveBeenCalled();
    nowSpy.mockRestore();
  });

  it("discards an externally deleted open tab without persisting the deleted path", async () => {
    const persistBeforeLeave = vi.fn().mockResolvedValue("saved");
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    function PersistHarness() {
      const api = useTabManager({ persistBeforeLeave });
      apiRef.current = api;
      return null;
    }

    await act(async () => {
      root.render(createElement(PersistHarness));
    });

    await openAndWait(apiRef, "a.md", "A");
    await openAndWait(apiRef, "b.md", "B");
    await act(async () => {
      await apiRef.current!.activateTab("a.md");
    });
    persistBeforeLeave.mockClear();

    await act(async () => {
      await apiRef.current!.discardOpenTab("a.md");
    });

    expect(persistBeforeLeave).not.toHaveBeenCalledWith("a.md");
    expect(apiRef.current!.tabs.map((tab) => tab.path)).toEqual(["b.md"]);
    expect(apiRef.current!.activePath).toBe("b.md");
  });

  it("openFile stages unreadied note state until editor first-frame commit", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await openAndWait(apiRef, "a.md", "A");

    let openPromise!: Promise<void>;
    await act(async () => {
      openPromise = apiRef.current!.openFile("b.md", "B");
      await openPromise;
    });

    const pending = apiRef.current!.pendingNoteOpen;
    expect(apiRef.current!.activePath).toBe("a.md");
    expect(apiRef.current!.markdown).toContain("body-a");
    expect(pending).toEqual(
      expect.objectContaining({
        path: "b.md",
        bodyMarkdown: expect.stringContaining("body-b"),
      }),
    );
    expect(apiRef.current!.commitPendingNoteOpen("b.md", 999)).toBe(false);
    let committed = false;
    await act(async () => {
      committed = apiRef.current!.commitPendingNoteOpen(
        "b.md",
        pending!.sequence,
      );
    });
    expect(committed).toBe(true);
    expect(apiRef.current!.activePath).toBe("b.md");
    expect(apiRef.current!.markdown).toContain("body-b");
  });
});
