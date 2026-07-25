import { readFileSync } from "node:fs";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const clearPreparedNotes = vi.fn();
const openPreparedNote = vi.fn(async () => undefined);
const prepareClassifiedNotePath = vi.fn();
const prepareNotePath = vi.fn();
const prepareVisibleNote = vi.fn();
const warmNotePath = vi.fn();
const fileList = vi.fn();

vi.mock("@/lib/ipc", () => ({
  fileList: (...args: unknown[]) => fileList(...args),
}));

vi.mock("@/hooks/usePreparedNoteOpener", () => ({
  usePreparedNoteOpener: () => ({
    clearPreparedNotes,
    invalidatePreparedNote: vi.fn(),
    openPreparedNote,
    prepareClassifiedNotePath,
    prepareNotePath,
    prepareVisibleNote,
    warmNotePath,
    warmPreparedNotes: [],
  }),
}));

import { usePreparedWorkspaceTransitions } from "@/hooks/usePreparedWorkspaceTransitions";
import { saveWorkspaceSessionSnapshot } from "@/lib/workspace-session-snapshot";

function Harness({
  tabs = [] as { path: string }[],
  vaultPath,
  workspaceEmpty = true,
  setWorkspaceEmpty = vi.fn<(active: boolean) => void>(),
}: {
  tabs?: { path: string }[];
  vaultPath: string | null;
  workspaceEmpty?: boolean;
  setWorkspaceEmpty?: (active: boolean) => void;
}) {
  usePreparedWorkspaceTransitions({
    activateTab: vi.fn(),
    classifiedVaultStatus: "locked",
    handleNewNote: vi.fn(async () => undefined),
    openNote: vi.fn(async () => undefined),
    setWorkspaceEmpty,
    tabs,
    vaultPath,
    workspaceEmpty,
  });
  return null;
}

describe("usePreparedWorkspaceTransitions startup warmup", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.useFakeTimers();
    localStorage.clear();
    clearPreparedNotes.mockClear();
    warmNotePath.mockClear();
    openPreparedNote.mockClear();
    fileList.mockReset();
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    vi.useRealTimers();
    act(() => root.unmount());
    host.remove();
  });

  it("warms saved session notes as startup background work without activating them", async () => {
    saveWorkspaceSessionSnapshot("/vault", {
      activePath: "notes/a.md",
      openNotes: [
        { path: "notes/a.md", title: "A", isLocked: false, lastActiveAt: 2 },
        { path: "notes/b.md", title: "B", isLocked: true, lastActiveAt: 1 },
      ],
    });
    fileList.mockResolvedValue([
      {
        path: "notes/a.md",
        title: "A",
        updatedAt: "2026-01-01T00:00:00Z",
        isLocked: false,
      },
    ]);

    act(() => {
      root.render(<Harness vaultPath="/vault" />);
    });

    expect(warmNotePath).not.toHaveBeenCalled();

    act(() => {
      vi.runOnlyPendingTimers();
    });

    await act(async () => {
      await vi.runOnlyPendingTimersAsync();
    });

    expect(warmNotePath).toHaveBeenNthCalledWith(1, "notes/a.md", "A", {
      isLocked: false,
      priority: "background",
      source: "startup",
      useSignature: false,
    });
    expect(warmNotePath).toHaveBeenNthCalledWith(2, "notes/b.md", "B", {
      isLocked: true,
      priority: "background",
      source: "startup",
      useSignature: false,
    });
    expect(openPreparedNote).toHaveBeenCalledTimes(1);
    expect(openPreparedNote).toHaveBeenCalledWith("notes/a.md", "A", {
      source: "startup",
    });
  });

  it("leaves the empty workspace after startup auto-open succeeds", async () => {
    const setWorkspaceEmpty = vi.fn<(active: boolean) => void>();
    saveWorkspaceSessionSnapshot("/vault", {
      activePath: "notes/a.md",
      openNotes: [
        { path: "notes/a.md", title: "A", isLocked: false, lastActiveAt: 1 },
      ],
    });
    fileList.mockResolvedValue([
      {
        path: "notes/a.md",
        title: "A",
        updatedAt: "2026-01-01T00:00:00Z",
        isLocked: false,
      },
    ]);

    act(() => {
      root.render(
        <Harness vaultPath="/vault" setWorkspaceEmpty={setWorkspaceEmpty} />,
      );
    });

    await act(async () => {
      await vi.runOnlyPendingTimersAsync();
    });

    expect(setWorkspaceEmpty).toHaveBeenCalledWith(false);
  });
});

describe("usePreparedWorkspaceTransitions cold-start auto-open", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.useFakeTimers();
    localStorage.clear();
    clearPreparedNotes.mockClear();
    warmNotePath.mockClear();
    openPreparedNote.mockClear();
    fileList.mockReset();
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    vi.useRealTimers();
    act(() => root.unmount());
    host.remove();
  });

  it("opens the resolved startup note once from snapshot and fileList", async () => {
    saveWorkspaceSessionSnapshot("/vault", {
      activePath: "notes/active.md",
      openNotes: [
        {
          path: "notes/active.md",
          title: "Active",
          isLocked: false,
          lastActiveAt: 1,
        },
      ],
    });
    fileList.mockResolvedValue([
      {
        path: "notes/recent.md",
        title: "Recent",
        updatedAt: "2026-01-02T00:00:00Z",
        isLocked: false,
      },
      {
        path: "notes/active.md",
        title: "Active",
        updatedAt: "2026-01-01T00:00:00Z",
        isLocked: false,
      },
    ]);

    act(() => {
      root.render(<Harness vaultPath="/vault" workspaceEmpty />);
    });

    await act(async () => {
      await vi.runOnlyPendingTimersAsync();
    });

    expect(fileList).toHaveBeenCalledTimes(1);
    expect(openPreparedNote).toHaveBeenCalledTimes(1);
    expect(openPreparedNote).toHaveBeenCalledWith("notes/active.md", "Active", {
      source: "startup",
    });
  });

  it("does not auto-open library recents when the last session had no open tabs", async () => {
    saveWorkspaceSessionSnapshot("/vault", {
      activePath: null,
      openNotes: [],
    });
    fileList.mockResolvedValue([
      {
        path: "notes/only.md",
        title: "Only",
        updatedAt: "2026-01-01T00:00:00Z",
        isLocked: false,
      },
    ]);

    act(() => {
      root.render(<Harness vaultPath="/vault" workspaceEmpty tabs={[]} />);
    });

    await act(async () => {
      await vi.runOnlyPendingTimersAsync();
    });

    expect(fileList).not.toHaveBeenCalled();
    expect(openPreparedNote).not.toHaveBeenCalled();
  });

  it("does not auto-open again after the user returns to an empty workspace", async () => {
    saveWorkspaceSessionSnapshot("/vault", {
      activePath: "notes/only.md",
      openNotes: [
        {
          path: "notes/only.md",
          title: "Only",
          isLocked: false,
          lastActiveAt: 1,
        },
      ],
    });
    fileList.mockResolvedValue([
      {
        path: "notes/only.md",
        title: "Only",
        updatedAt: "2026-01-01T00:00:00Z",
        isLocked: false,
      },
    ]);

    act(() => {
      root.render(<Harness vaultPath="/vault" workspaceEmpty tabs={[]} />);
    });

    await act(async () => {
      await vi.runOnlyPendingTimersAsync();
    });

    expect(openPreparedNote).toHaveBeenCalledTimes(1);

    act(() => {
      root.render(
        <Harness
          vaultPath="/vault"
          workspaceEmpty={false}
          tabs={[{ path: "notes/only.md" }]}
        />,
      );
    });

    act(() => {
      root.render(<Harness vaultPath="/vault" workspaceEmpty tabs={[]} />);
    });

    await act(async () => {
      await vi.runOnlyPendingTimersAsync();
    });

    expect(openPreparedNote).toHaveBeenCalledTimes(1);
  });

  it("opens snapshot activePath when fileList rejects", async () => {
    saveWorkspaceSessionSnapshot("/vault", {
      activePath: "notes/snapshot-only.md",
      openNotes: [
        {
          path: "notes/snapshot-only.md",
          title: "Snapshot Only",
          isLocked: false,
          lastActiveAt: 1,
        },
      ],
    });
    fileList.mockRejectedValue(new Error("ipc unavailable"));

    act(() => {
      root.render(<Harness vaultPath="/vault" workspaceEmpty />);
    });

    await act(async () => {
      await vi.runOnlyPendingTimersAsync();
    });

    expect(fileList).toHaveBeenCalledTimes(1);
    expect(openPreparedNote).toHaveBeenCalledTimes(1);
    expect(openPreparedNote).toHaveBeenCalledWith(
      "notes/snapshot-only.md",
      "Snapshot Only",
      { source: "startup" },
    );
  });

  it("skips startup auto-open when tabs are already open", async () => {
    fileList.mockResolvedValue([
      {
        path: "notes/a.md",
        title: "A",
        updatedAt: "2026-01-01T00:00:00Z",
        isLocked: false,
      },
    ]);

    act(() => {
      root.render(
        <Harness
          vaultPath="/vault"
          workspaceEmpty={false}
          tabs={[{ path: "notes/a.md" }]}
        />,
      );
    });

    await act(async () => {
      await vi.runOnlyPendingTimersAsync();
    });

    expect(openPreparedNote).not.toHaveBeenCalled();
  });
});

describe("usePreparedWorkspaceTransitions startup routing", () => {
  it("auto-opens through openNoteLeavingHome so the workspace leaves empty state", () => {
    const src = readFileSync(
      "src/hooks/usePreparedWorkspaceTransitions.ts",
      "utf8",
    );
    expect(src).toContain("await openNoteLeavingHome(candidate.path");
    expect(src).not.toMatch(/await openPreparedNote\(candidate\.path/);
  });
});
