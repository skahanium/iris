import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  clearAllEditorHtmlCache,
  editorHtmlDigest,
  getCachedEditorHtml,
} from "@/lib/editor-html-cache";
import {
  clearNoteOpenPerformanceEntries,
  clearNoteOpenPreparationCache,
  getPreparedNoteOpen,
  invalidateNoteOpenPreparation,
  NOTE_OPEN_HOT_PATH_BUDGET_MS,
  NOTE_OPEN_PERFORMANCE_ENTRY_PREFIX,
  NOTE_OPEN_WARM_PATH_BUDGET_MS,
  prepareNoteOpen,
  resetNoteOpenTraceSession,
  setNoteOpenTraceSink,
  warmNoteOpen,
} from "@/lib/note-open-preparation";

const fileRead = vi.fn();
const source = await import("node:fs").then(({ readFileSync }) =>
  readFileSync("src/lib/document-open-runtime.ts", "utf8"),
);

vi.mock("@/lib/ipc", () => ({
  fileRead: (...args: unknown[]) => fileRead(...args),
}));

describe("note open preparation", () => {
  beforeEach(() => {
    fileRead.mockReset();
    clearAllEditorHtmlCache();
    clearNoteOpenPreparationCache();
    clearNoteOpenPerformanceEntries();
    setNoteOpenTraceSink(null);
  });

  it("prepares note content and TipTap HTML once for a stable file signature", async () => {
    fileRead.mockResolvedValue({
      content: '---\ntitle: "Prepared"\n---\n\nBody',
      isLocked: false,
    });

    const prepared = await prepareNoteOpen({
      path: "a.md",
      meta: { updatedAt: "2026-06-24T00:00:00Z", isLocked: false },
    });
    const preparedAgain = await prepareNoteOpen({
      path: "a.md",
      meta: { updatedAt: "2026-06-24T00:00:00Z", isLocked: false },
    });

    expect(fileRead).toHaveBeenCalledTimes(1);
    expect(preparedAgain).toBe(prepared);
    expect(prepared.title).toBe("Prepared");
    expect(prepared.bodyMarkdown.trim()).toBe("Body");
    expect(
      getCachedEditorHtml("a.md", editorHtmlDigest(prepared.bodyMarkdown)),
    ).toContain("Body");
  });

  it("coalesces concurrent preparations for the same file signature", async () => {
    let resolveRead!: (value: { content: string; isLocked: boolean }) => void;
    fileRead.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveRead = resolve;
        }),
    );

    const first = prepareNoteOpen({
      path: "a.md",
      meta: { updatedAt: "same", isLocked: false },
    });
    const second = prepareNoteOpen({
      path: "a.md",
      meta: { updatedAt: "same", isLocked: false },
    });
    resolveRead({ content: "# A\n\nBody", isLocked: false });

    await expect(Promise.all([first, second])).resolves.toHaveLength(2);
    expect(fileRead).toHaveBeenCalledTimes(1);
  });

  it("invalidates prepared content when the file signature changes", async () => {
    fileRead
      .mockResolvedValueOnce({
        content: '---\ntitle: "A"\n---\n\nOld',
        isLocked: false,
      })
      .mockResolvedValueOnce({
        content: '---\ntitle: "A"\n---\n\nNew',
        isLocked: false,
      });

    await prepareNoteOpen({
      path: "a.md",
      meta: { updatedAt: "old", isLocked: false },
    });
    const prepared = await prepareNoteOpen({
      path: "a.md",
      meta: { updatedAt: "new", isLocked: false },
    });

    expect(fileRead).toHaveBeenCalledTimes(2);
    expect(prepared.bodyMarkdown.trim()).toBe("New");
  });

  it("invalidates one prepared path without relying on plaintext map keys", async () => {
    fileRead
      .mockResolvedValueOnce({
        content: "# A\n\nOld",
        isLocked: false,
      })
      .mockResolvedValueOnce({
        content: "# A\n\nNew",
        isLocked: false,
      });

    await prepareNoteOpen({ path: "a.md" });
    invalidateNoteOpenPreparation("a.md");
    const prepared = await prepareNoteOpen({ path: "a.md" });

    expect(fileRead).toHaveBeenCalledTimes(2);
    expect(prepared.bodyMarkdown).toContain("New");
  });

  it("does not warm classified notes without explicit classified permission", async () => {
    warmNoteOpen({ path: ".classified/secret.md" });

    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(fileRead).not.toHaveBeenCalled();
    expect(
      getPreparedNoteOpen({
        path: ".classified/secret.md",
        allowClassified: true,
      }),
    ).toBeNull();
  });

  it("keeps classified prepared entries out of the normal namespace", async () => {
    fileRead.mockResolvedValue({
      content: "# Secret\n\nClassified body",
      isLocked: true,
    });

    const prepared = await prepareNoteOpen({
      path: ".classified/secret.md",
      allowClassified: true,
      meta: { updatedAt: "classified-v1", isLocked: true },
    });

    expect(prepared.isLocked).toBe(true);
    expect(
      getPreparedNoteOpen({
        path: ".classified/secret.md",
        meta: { updatedAt: "classified-v1", isLocked: true },
      }),
    ).toBeNull();
    expect(
      getPreparedNoteOpen({
        path: ".classified/secret.md",
        allowClassified: true,
        meta: { updatedAt: "classified-v1", isLocked: true },
      }),
    ).toBe(prepared);
  });

  it("keeps classified prepared TipTap HTML out of the normal namespace", async () => {
    fileRead.mockResolvedValue({
      content: "# Secret\n\nClassified body",
      isLocked: true,
    });

    const prepared = await prepareNoteOpen({
      path: ".classified/secret.md",
      allowClassified: true,
      meta: { updatedAt: "classified-html-v1", isLocked: true },
    });
    const digest = editorHtmlDigest(prepared.bodyMarkdown);

    expect(
      getCachedEditorHtml(".classified/secret.md", digest, "normal"),
    ).toBeUndefined();
    expect(
      getCachedEditorHtml(".classified/secret.md", digest, "classified"),
    ).toContain("Classified body");
  });

  it("can clear classified preparation without dropping normal entries", async () => {
    fileRead
      .mockResolvedValueOnce({
        content: "# Normal\n\nNormal body",
        isLocked: false,
      })
      .mockResolvedValueOnce({
        content: "# Secret\n\nClassified body",
        isLocked: true,
      });

    const normal = await prepareNoteOpen({
      path: "normal.md",
      meta: { updatedAt: "normal-v1", isLocked: false },
    });
    await prepareNoteOpen({
      path: ".classified/secret.md",
      allowClassified: true,
      meta: { updatedAt: "classified-v1", isLocked: true },
    });

    clearNoteOpenPreparationCache("classified");

    expect(
      getPreparedNoteOpen({
        path: "normal.md",
        meta: { updatedAt: "normal-v1", isLocked: false },
      }),
    ).toBe(normal);
    expect(
      getPreparedNoteOpen({
        path: ".classified/secret.md",
        allowClassified: true,
        meta: { updatedAt: "classified-v1", isLocked: true },
      }),
    ).toBeNull();
  });

  it("rotates trace keys when the trace session resets", async () => {
    const traces: Array<{ key: string }> = [];
    setNoteOpenTraceSink((trace) => traces.push(trace));
    fileRead.mockResolvedValue({
      content: "# Trace\n\nBody",
      isLocked: false,
    });

    await prepareNoteOpen({
      path: "folder/private-note.md",
      meta: { updatedAt: "trace-v1", isLocked: false },
    });
    const firstKey = traces.at(-1)?.key;
    clearNoteOpenPreparationCache();
    resetNoteOpenTraceSession();
    await prepareNoteOpen({
      path: "folder/private-note.md",
      meta: { updatedAt: "trace-v1", isLocked: false },
    });

    expect(firstKey).toBeTruthy();
    expect(traces.at(-1)?.key).toBeTruthy();
    expect(traces.at(-1)?.key).not.toBe(firstKey);
  });

  it("emits machine-readable hot and warm path budgets", async () => {
    const traces: Array<{
      budgetExceeded: boolean;
      budgetKind: string;
      budgetMs: number | null;
      phase: string;
    }> = [];
    setNoteOpenTraceSink((trace) => traces.push(trace));
    fileRead.mockResolvedValue({
      content: "# Budget\n\nBody",
      isLocked: false,
    });

    await prepareNoteOpen({
      path: "budget.md",
      meta: { updatedAt: "budget-v1", isLocked: false },
    });

    expect(traces.find((trace) => trace.phase === "prepare-done")).toEqual(
      expect.objectContaining({
        budgetExceeded: false,
        budgetKind: "warm",
        budgetMs: NOTE_OPEN_WARM_PATH_BUDGET_MS,
      }),
    );

    traces.length = 0;
    const nowSpy = vi
      .spyOn(performance, "now")
      .mockReturnValueOnce(1000)
      .mockReturnValueOnce(1010)
      .mockReturnValueOnce(2000)
      .mockReturnValueOnce(2020);

    await prepareNoteOpen({
      path: "budget.md",
      meta: { updatedAt: "budget-v1", isLocked: false },
    });
    await prepareNoteOpen({
      path: "budget.md",
      meta: { updatedAt: "budget-v1", isLocked: false },
    });
    nowSpy.mockRestore();

    expect(traces[0]).toEqual(
      expect.objectContaining({
        budgetExceeded: false,
        budgetKind: "hot",
        budgetMs: NOTE_OPEN_HOT_PATH_BUDGET_MS,
        phase: "cache-hit",
      }),
    );
    expect(traces[1]).toEqual(
      expect.objectContaining({
        budgetExceeded: true,
        budgetKind: "hot",
        budgetMs: NOTE_OPEN_HOT_PATH_BUDGET_MS,
        phase: "cache-hit",
      }),
    );
  });

  it("emits anonymized traces without paths, titles, body, frontmatter, selection, or prompt", async () => {
    const traces: unknown[] = [];
    setNoteOpenTraceSink((trace) => traces.push(trace));
    fileRead.mockResolvedValue({
      content: '---\ntitle: "Trace Secret"\n---\n\nSensitive body',
      isLocked: false,
    });

    await prepareNoteOpen({
      path: "folder/private-note.md",
      titleHint: "Trace Secret",
      meta: { updatedAt: "trace-v1", isLocked: false },
    });

    const serialized = JSON.stringify(traces);
    expect(traces.length).toBeGreaterThan(0);
    expect(serialized).not.toContain("folder/private-note.md");
    expect(serialized).not.toContain("private-note");
    expect(serialized).not.toContain("Trace Secret");
    expect(serialized).not.toContain("Sensitive body");
    expect(serialized).not.toContain("frontmatter");
    expect(serialized).not.toContain("selection");
    expect(serialized).not.toContain("prompt");
  });

  it("records anonymized note-open phases in the Performance Timeline", async () => {
    fileRead.mockResolvedValue({
      content: '---\ntitle: "Perf Secret"\n---\n\nPrivate timeline body',
      isLocked: false,
    });

    await prepareNoteOpen({
      path: "folder/perf-private-note.md",
      titleHint: "Perf Secret",
      meta: { updatedAt: "perf-v1", isLocked: false },
    });

    const names = performance
      .getEntriesByType("measure")
      .map((entry) => entry.name)
      .filter((name) => name.startsWith(NOTE_OPEN_PERFORMANCE_ENTRY_PREFIX));
    const serialized = JSON.stringify(names);

    expect(names.length).toBeGreaterThan(0);
    expect(names.some((name) => name.includes("prepare-done"))).toBe(true);
    expect(serialized).not.toContain("folder/perf-private-note.md");
    expect(serialized).not.toContain("perf-private-note");
    expect(serialized).not.toContain("Perf Secret");
    expect(serialized).not.toContain("Private timeline body");
    expect(serialized).not.toContain("frontmatter");
    expect(serialized).not.toContain("selection");
    expect(serialized).not.toContain("prompt");
  });

  it("does not use plaintext paths as preparation map keys", () => {
    expect(source).not.toContain(".get(request.path)");
    expect(source).not.toContain(".set(path, entry)");
  });

  it("keeps warm preparation out of persistent storage and write/version/AI IPC", () => {
    expect(source).not.toContain("localStorage");
    expect(source).not.toContain("sessionStorage");
    expect(source).not.toContain("indexedDB");
    expect(source).not.toContain("fileWrite");
    expect(source).not.toContain("versionSave");
    expect(source).not.toContain("llm_");
  });
});
