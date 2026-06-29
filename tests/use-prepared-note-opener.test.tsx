import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePreparedNoteOpener } from "@/hooks/usePreparedNoteOpener";
import { clearNoteOpenPreparationCache } from "@/lib/note-open-preparation";
import type { PreparedNoteOpen } from "@/lib/note-open-preparation";
import type {
  DocumentOpenPriority,
  NoteOpenSource,
  PrepareNoteOpenRequest,
} from "@/lib/document-open-runtime";
import type { FileListItem } from "@/types/ipc";

const documentOpen = vi.fn();
const documentOpenBegin = vi.fn();
const documentOpenEnd = vi.fn();
const fileRead = vi.fn();
const fileSignature = vi.fn();

vi.mock("@/lib/ipc", () => ({
  documentOpen: (...args: unknown[]) => documentOpen(...args),
  documentOpenBegin: (...args: unknown[]) => documentOpenBegin(...args),
  documentOpenEnd: (...args: unknown[]) => documentOpenEnd(...args),
  fileRead: (...args: unknown[]) => fileRead(...args),
  fileSignature: (...args: unknown[]) => fileSignature(...args),
}));

interface OpenOptions {
  allowClassified?: boolean;
  documentOpenToken?: string;
  onDocumentOpenTokenRetained?: () => void;
  openTraceRequest?: PrepareNoteOpenRequest;
  preparedNote?: PreparedNoteOpen;
  priority?: DocumentOpenPriority;
  source?: NoteOpenSource;
}

interface HookApi {
  openPreparedNote: (
    path: string,
    titleHint?: string,
    options?: OpenOptions,
  ) => Promise<void>;
  invalidatePreparedNote: (path: string) => void;
  prepareVisibleNote: (file: FileListItem, source?: NoteOpenSource) => void;
  warmNotePath: (
    path: string,
    titleHint?: string,
    options?: {
      isLocked?: boolean;
      priority?: DocumentOpenPriority;
      source?: NoteOpenSource;
    },
  ) => void;
  warmPreparedNotes: readonly PreparedNoteOpen[];
}

function Harness({
  onReady,
  openNote,
  openTabs = [],
}: {
  onReady: (api: HookApi) => void;
  openNote: (
    path: string,
    titleHint?: string,
    options?: OpenOptions,
  ) => Promise<void>;
  openTabs?: readonly { path: string }[];
}) {
  const api = usePreparedNoteOpener<OpenOptions>({
    openNote,
    openTabs,
  });
  useEffect(() => {
    onReady(api);
  }, [api, onReady]);
  return null;
}

describe("usePreparedNoteOpener", () => {
  it("does not match warm prepared payloads by path only", async () => {
    const source = await import("node:fs").then(({ readFileSync }) =>
      readFileSync("src/hooks/usePreparedNoteOpener.ts", "utf8"),
    );

    expect(source).not.toContain(
      "warmPreparedNotes.find((note) => note.path === path)",
    );
  });

  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    documentOpen.mockReset();
    documentOpen.mockResolvedValue({
      token: "open-token",
      content: "# Direct\\n\\nBody",
      isLocked: false,
    });
    documentOpenBegin.mockReset();
    documentOpenBegin.mockResolvedValue({ token: "open-token" });
    documentOpenEnd.mockReset();
    documentOpenEnd.mockResolvedValue(undefined);
    fileRead.mockReset();
    fileSignature.mockReset();
    fileSignature.mockResolvedValue({
      byteLength: 18,
      contentHash: "warm-hash",
      isLocked: false,
      modifiedMs: 10,
    });
    clearNoteOpenPreparationCache();
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("marks duplicate opens as foreground work from their entry source", async () => {
    const openNote = vi.fn(async () => undefined);
    let api!: HookApi;

    await act(async () => {
      root.render(
        <Harness
          openNote={openNote}
          openTabs={[{ path: "notes/open.md" }]}
          onReady={(next) => (api = next)}
        />,
      );
    });

    await act(async () => {
      await api.openPreparedNote("notes/open.md", "Open", { source: "tab" });
    });

    expect(openNote).toHaveBeenCalledWith(
      "notes/open.md",
      "Open",
      expect.objectContaining({
        openTraceRequest: expect.objectContaining({
          path: "notes/open.md",
          priority: "foreground",
          source: "tab",
          titleHint: "Open",
        }),
      }),
    );
  });

  it("prepares single-IPC document_open content before opening", async () => {
    const openNote = vi.fn(async () => undefined);
    let api!: HookApi;
    documentOpen.mockResolvedValueOnce({
      token: "merged-token",
      content: '---\ntitle: "Merged"\n---\n\nMerged body',
      isLocked: true,
    });

    await act(async () => {
      root.render(
        <Harness openNote={openNote} onReady={(next) => (api = next)} />,
      );
    });

    await act(async () => {
      await api.openPreparedNote("notes/merged.md", "Merged", {
        source: "quick-open",
      });
    });

    expect(documentOpen).toHaveBeenCalledWith("notes/merged.md", false);
    expect(fileRead).not.toHaveBeenCalled();
    expect(openNote).toHaveBeenCalledWith(
      "notes/merged.md",
      "Merged",
      expect.objectContaining({
        documentOpenToken: "merged-token",
        preparedNote: expect.objectContaining({
          bodyMarkdown: expect.stringContaining("Merged body"),
          frontmatterYaml: expect.stringContaining('title: "Merged"'),
          isLocked: true,
          title: "Merged",
        }),
      }),
    );
  });
  it("wraps foreground opens in a backend document-open token", async () => {
    const openNote = vi.fn(async () => undefined);
    let api!: HookApi;
    fileRead.mockResolvedValue({
      content: "# Direct\n\nBody",
      isLocked: false,
    });

    await act(async () => {
      root.render(
        <Harness openNote={openNote} onReady={(next) => (api = next)} />,
      );
    });

    await act(async () => {
      await api.openPreparedNote("notes/direct.md", "Direct", {
        source: "quick-open",
      });
    });

    expect(documentOpen).toHaveBeenCalledWith("notes/direct.md", false);
    expect(documentOpenBegin).not.toHaveBeenCalled();
    expect(documentOpenEnd).toHaveBeenCalledWith("open-token");
    expect(openNote).toHaveBeenCalledWith(
      "notes/direct.md",
      "Direct",
      expect.objectContaining({
        openTraceRequest: expect.objectContaining({
          path: "notes/direct.md",
          priority: "foreground",
          source: "quick-open",
          titleHint: "Direct",
        }),
      }),
    );
  });

  it("does not reuse an older warm payload after visible metadata changes", async () => {
    const openNote = vi.fn(async () => undefined);
    let api!: HookApi;
    let resolveSecond!: (value: { content: string; isLocked: boolean }) => void;
    fileRead
      .mockResolvedValueOnce({
        content: "# Old\n\nOld body",
        isLocked: false,
      })
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveSecond = resolve;
          }),
      );

    await act(async () => {
      root.render(
        <Harness openNote={openNote} onReady={(next) => (api = next)} />,
      );
    });

    await act(async () => {
      api.prepareVisibleNote({
        path: "notes/warmed.md",
        title: "Old",
        updatedAt: "old-mtime",
        isLocked: false,
      });
      await Promise.resolve();
    });
    await vi.waitFor(() => expect(fileRead).toHaveBeenCalledTimes(1));

    await act(async () => {
      api.prepareVisibleNote({
        path: "notes/warmed.md",
        title: "New",
        updatedAt: "new-mtime",
        isLocked: false,
      });
      await Promise.resolve();
    });
    await vi.waitFor(() => expect(fileRead).toHaveBeenCalledTimes(2));

    const opening = api.openPreparedNote("notes/warmed.md", "New");
    await Promise.resolve();
    expect(openNote).not.toHaveBeenCalled();

    await act(async () => {
      resolveSecond({ content: "# New\n\nNew body", isLocked: false });
      await opening;
    });

    expect(openNote).toHaveBeenCalledWith(
      "notes/warmed.md",
      "New",
      expect.objectContaining({
        preparedNote: expect.objectContaining({
          bodyMarkdown: expect.stringContaining("New body"),
          signature: expect.stringContaining("new-mtime"),
        }),
      }),
    );
  });

  it("drops in-flight warm preparation results after invalidation", async () => {
    const openNote = vi.fn(async () => undefined);
    let api!: HookApi;
    let resolveSignature!: (value: {
      byteLength: number;
      contentHash: string;
      isLocked: boolean;
      modifiedMs: number;
    }) => void;
    fileSignature.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveSignature = resolve;
        }),
    );
    fileRead.mockResolvedValue({
      content: "# Stale\n\nOld body",
      isLocked: false,
    });

    await act(async () => {
      root.render(
        <Harness openNote={openNote} onReady={(next) => (api = next)} />,
      );
    });

    await act(async () => {
      api.prepareVisibleNote({
        path: "notes/stale.md",
        title: "Stale",
        updatedAt: "old",
        isLocked: false,
      });
      api.invalidatePreparedNote("notes/stale.md");
    });

    await act(async () => {
      resolveSignature({
        byteLength: 19,
        contentHash: "old-hash",
        isLocked: false,
        modifiedMs: 11,
      });
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(fileRead).not.toHaveBeenCalled();
    expect(api.warmPreparedNotes).toHaveLength(0);
  });

  it("does not eager-signature startup background warmups", async () => {
    const openNote = vi.fn(async () => undefined);
    let api!: HookApi;
    fileRead.mockResolvedValue({
      content: "# Startup\n\nPrepared body",
      isLocked: false,
    });

    await act(async () => {
      root.render(
        <Harness openNote={openNote} onReady={(next) => (api = next)} />,
      );
    });

    await act(async () => {
      api.warmNotePath("notes/startup.md", "Startup", {
        priority: "background",
        source: "startup",
      });
      await Promise.resolve();
    });

    await vi.waitFor(() => expect(fileRead).toHaveBeenCalledTimes(1));
    expect(fileSignature).not.toHaveBeenCalled();
  });

  it("keeps foreground document-open tokens alive when the open is retained for first frame", async () => {
    const openNote = vi.fn(async (_path, _title, options?: OpenOptions) => {
      options?.onDocumentOpenTokenRetained?.();
    });
    let api!: HookApi;
    fileRead.mockResolvedValue({
      content: "# Direct\n\nBody",
      isLocked: false,
    });

    await act(async () => {
      root.render(
        <Harness openNote={openNote} onReady={(next) => (api = next)} />,
      );
    });

    await act(async () => {
      await api.openPreparedNote("notes/direct.md", "Direct", {
        source: "quick-open",
      });
    });

    expect(openNote).toHaveBeenCalledWith(
      "notes/direct.md",
      "Direct",
      expect.objectContaining({ documentOpenToken: "open-token" }),
    );
    expect(documentOpenEnd).not.toHaveBeenCalled();
  });

  it("reuses a visible-item preparation when opening the same note", async () => {
    const openNote = vi.fn(async () => undefined);
    let api!: HookApi;
    fileRead.mockResolvedValue({
      content: "# Warmed\n\nPrepared body",
      isLocked: false,
    });

    await act(async () => {
      root.render(
        <Harness openNote={openNote} onReady={(next) => (api = next)} />,
      );
    });

    await act(async () => {
      api.prepareVisibleNote({
        path: "notes/warmed.md",
        title: "Warmed",
        updatedAt: "2026-06-24T00:00:00Z",
        isLocked: false,
      });
      await Promise.resolve();
    });

    await vi.waitFor(() => expect(fileRead).toHaveBeenCalledTimes(1));

    await act(async () => {
      await api.openPreparedNote("notes/warmed.md", "Warmed");
    });

    expect(fileRead).toHaveBeenCalledTimes(1);
    expect(openNote).toHaveBeenCalledWith(
      "notes/warmed.md",
      "Warmed",
      expect.objectContaining({
        preparedNote: expect.objectContaining({
          bodyMarkdown: expect.stringContaining("Prepared body"),
          path: "notes/warmed.md",
        }),
      }),
    );
  });

  it("keeps the latest eight warm prepared notes with newest entries first", async () => {
    const openNote = vi.fn(async () => undefined);
    let api!: HookApi;
    fileSignature.mockImplementation(async (path: string) => ({
      byteLength: path.length,
      contentHash: `hash:${path}`,
      isLocked: false,
      modifiedMs: Number(path.match(/\d+/)?.[0] ?? 0),
    }));
    fileRead.mockImplementation(async (path: string) => ({
      content: `# ${path}\n\nPrepared body`,
      isLocked: false,
    }));

    await act(async () => {
      root.render(
        <Harness openNote={openNote} onReady={(next) => (api = next)} />,
      );
    });

    await act(async () => {
      for (let index = 0; index < 9; index++) {
        api.prepareVisibleNote({
          path: `notes/${index}.md`,
          title: `Note ${index}`,
          updatedAt: `mtime-${index}`,
          isLocked: false,
        });
      }
    });

    await vi.waitFor(() => expect(api.warmPreparedNotes).toHaveLength(8));
    expect(api.warmPreparedNotes.map((note) => note.path)).toEqual([
      "notes/8.md",
      "notes/7.md",
      "notes/6.md",
      "notes/5.md",
      "notes/4.md",
      "notes/3.md",
      "notes/2.md",
      "notes/1.md",
    ]);

    await act(async () => {
      api.prepareVisibleNote({
        path: "notes/4.md",
        title: "Note 4 refreshed",
        updatedAt: "mtime-4b",
        isLocked: false,
      });
    });

    await vi.waitFor(() =>
      expect(api.warmPreparedNotes[0]?.path).toBe("notes/4.md"),
    );
    expect(api.warmPreparedNotes).toHaveLength(8);
    expect(new Set(api.warmPreparedNotes.map((note) => note.path)).size).toBe(
      8,
    );
  });

  it("uses explicit file signatures for visible warm preparations", async () => {
    const openNote = vi.fn(async () => undefined);
    let api!: HookApi;
    fileSignature.mockResolvedValueOnce({
      byteLength: 21,
      contentHash: "visible-hash",
      isLocked: false,
      modifiedMs: 42,
    });
    fileRead.mockResolvedValue({
      content: "# Warmed\n\nPrepared body",
      isLocked: false,
    });

    await act(async () => {
      root.render(
        <Harness openNote={openNote} onReady={(next) => (api = next)} />,
      );
    });

    await act(async () => {
      api.prepareVisibleNote({
        path: "notes/signed.md",
        title: "Signed",
        updatedAt: "metadata-mtime",
        isLocked: false,
      });
    });

    await vi.waitFor(() =>
      expect(fileSignature).toHaveBeenCalledWith("notes/signed.md", {
        allowClassified: false,
      }),
    );
    await vi.waitFor(() => expect(fileRead).toHaveBeenCalledTimes(1));

    await act(async () => {
      await api.openPreparedNote("notes/signed.md", "Signed");
    });

    expect(openNote).toHaveBeenCalledWith(
      "notes/signed.md",
      "Signed",
      expect.objectContaining({
        preparedNote: expect.objectContaining({
          signature: expect.stringContaining("visible-hash"),
        }),
      }),
    );
  });
});
