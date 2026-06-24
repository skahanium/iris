import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePreparedNoteOpener } from "@/hooks/usePreparedNoteOpener";
import { clearNoteOpenPreparationCache } from "@/lib/note-open-preparation";
import type { PreparedNoteOpen } from "@/lib/note-open-preparation";
import type { FileListItem } from "@/types/ipc";

const fileRead = vi.fn();

vi.mock("@/lib/ipc", () => ({
  fileRead: (...args: unknown[]) => fileRead(...args),
}));

interface OpenOptions {
  allowClassified?: boolean;
  preparedNote?: PreparedNoteOpen;
}

interface HookApi {
  openPreparedNote: (
    path: string,
    titleHint?: string,
    options?: OpenOptions,
  ) => Promise<void>;
  prepareVisibleNote: (file: FileListItem) => void;
}

function Harness({
  onReady,
  openNote,
}: {
  onReady: (api: HookApi) => void;
  openNote: (
    path: string,
    titleHint?: string,
    options?: OpenOptions,
  ) => Promise<void>;
}) {
  const api = usePreparedNoteOpener<OpenOptions>({
    openNote,
    openTabs: [],
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
    fileRead.mockReset();
    clearNoteOpenPreparationCache();
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
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
});
