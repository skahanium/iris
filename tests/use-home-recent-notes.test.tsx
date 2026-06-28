import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useHomeRecentNotes } from "@/hooks/useHomeRecentNotes";
import type { FileListItem } from "@/types/ipc";

const fileList = vi.fn();

vi.mock("@/lib/ipc", () => ({
  fileList: (...args: unknown[]) => fileList(...args),
}));

function note(path: string, title = path): FileListItem {
  return {
    path,
    title,
    updatedAt: "2026-06-24T00:00:00Z",
    isLocked: false,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, reject, resolve };
}

describe("useHomeRecentNotes", () => {
  let host: HTMLDivElement;
  let root: Root;
  let snapshots: FileListItem[][] = [];
  let refreshRecent: (() => Promise<void>) | null = null;
  let onPrepare: (file: FileListItem) => void;

  function Harness({
    vaultIndexEpoch,
    vaultPath,
  }: {
    vaultIndexEpoch: number;
    vaultPath: string | null;
  }) {
    const state = useHomeRecentNotes({
      onPrepare,
      vaultIndexEpoch,
      vaultPath,
    });
    useEffect(() => {
      snapshots.push([...state.recentNotes]);
      refreshRecent = state.refreshRecent;
    }, [state.recentNotes, state.refreshRecent]);
    return (
      <output data-testid="recent-count">{state.recentNotes.length}</output>
    );
  }

  beforeEach(() => {
    fileList.mockReset();
    snapshots = [];
    refreshRecent = null;
    onPrepare = vi.fn();
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("loads, dedupes, limits, and prepares recent notes", async () => {
    fileList.mockResolvedValue([
      note("a.md", "A"),
      note("a.md", "Duplicate"),
      note("b.md", "B"),
      note("c.md", "C"),
      note("d.md", "D"),
      note("e.md", "E"),
      note("f.md", "F"),
    ]);

    await act(async () => {
      root.render(<Harness vaultIndexEpoch={0} vaultPath="/vault" />);
    });

    await vi.waitFor(() => {
      expect(snapshots.at(-1)?.map((item) => item.path)).toEqual([
        "a.md",
        "b.md",
        "c.md",
        "d.md",
        "e.md",
      ]);
    });
    expect(onPrepare).toHaveBeenCalledTimes(5);
  });

  it("keeps previous notes visible while the same vault refresh is pending or fails", async () => {
    const initial = deferred<FileListItem[]>();
    const refresh = deferred<FileListItem[]>();
    fileList
      .mockReturnValueOnce(initial.promise)
      .mockReturnValueOnce(refresh.promise);

    await act(async () => {
      root.render(<Harness vaultIndexEpoch={0} vaultPath="/vault" />);
    });
    await act(async () => initial.resolve([note("old.md", "Old")]));
    await vi.waitFor(() => {
      expect(snapshots.at(-1)?.map((item) => item.path)).toEqual(["old.md"]);
    });

    await act(async () => {
      root.render(<Harness vaultIndexEpoch={1} vaultPath="/vault" />);
    });

    expect(snapshots.at(-1)?.map((item) => item.path)).toEqual(["old.md"]);

    await act(async () => refresh.reject(new Error("offline")));

    expect(snapshots.at(-1)?.map((item) => item.path)).toEqual(["old.md"]);
  });

  it("clears notes on vault changes and ignores stale results", async () => {
    const oldVault = deferred<FileListItem[]>();
    const newVault = deferred<FileListItem[]>();
    fileList
      .mockReturnValueOnce(oldVault.promise)
      .mockReturnValueOnce(newVault.promise);

    await act(async () => {
      root.render(<Harness vaultIndexEpoch={0} vaultPath="/old" />);
    });
    await act(async () => {
      root.render(<Harness vaultIndexEpoch={0} vaultPath="/new" />);
    });

    expect(snapshots.at(-1)).toEqual([]);

    await act(async () => oldVault.resolve([note("old.md", "Old")]));
    expect(snapshots.at(-1)).toEqual([]);

    await act(async () => newVault.resolve([note("new.md", "New")]));
    await vi.waitFor(() => {
      expect(snapshots.at(-1)?.map((item) => item.path)).toEqual(["new.md"]);
    });
  });

  it("exposes an explicit refresh that keeps previous notes until replacement data arrives", async () => {
    const initial = deferred<FileListItem[]>();
    const manual = deferred<FileListItem[]>();
    fileList
      .mockReturnValueOnce(initial.promise)
      .mockReturnValueOnce(manual.promise);

    await act(async () => {
      root.render(<Harness vaultIndexEpoch={0} vaultPath="/vault" />);
    });
    await act(async () => initial.resolve([note("old.md", "Old")]));
    await vi.waitFor(() => {
      expect(snapshots.at(-1)?.map((item) => item.path)).toEqual(["old.md"]);
    });

    await act(async () => {
      void refreshRecent?.();
    });
    expect(snapshots.at(-1)?.map((item) => item.path)).toEqual(["old.md"]);

    await act(async () => manual.resolve([note("new.md", "New")]));
    await vi.waitFor(() => {
      expect(snapshots.at(-1)?.map((item) => item.path)).toEqual(["new.md"]);
    });
  });
});
