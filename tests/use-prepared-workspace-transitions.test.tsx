import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const clearPreparedNotes = vi.fn();
const openPreparedNote = vi.fn(async () => undefined);
const prepareClassifiedNotePath = vi.fn();
const prepareNotePath = vi.fn();
const prepareVisibleNote = vi.fn();
const warmNotePath = vi.fn();

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

function Harness({ vaultPath }: { vaultPath: string | null }) {
  usePreparedWorkspaceTransitions({
    activePathRef: { current: null },
    activateTab: vi.fn(),
    classifiedVaultStatus: "locked",
    handleNewNote: vi.fn(async () => undefined),
    openNote: vi.fn(async () => undefined),
    setHomeActive: vi.fn(),
    tabs: [],
    vaultPath,
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
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    vi.useRealTimers();
    act(() => root.unmount());
    host.remove();
  });

  it("warms saved session notes as startup background work without activating them", () => {
    saveWorkspaceSessionSnapshot("/vault", {
      activePath: "notes/a.md",
      openNotes: [
        { path: "notes/a.md", title: "A", isLocked: false, lastActiveAt: 2 },
        { path: "notes/b.md", title: "B", isLocked: true, lastActiveAt: 1 },
      ],
    });

    act(() => {
      root.render(<Harness vaultPath="/vault" />);
    });

    expect(warmNotePath).not.toHaveBeenCalled();

    act(() => {
      vi.runOnlyPendingTimers();
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
    expect(openPreparedNote).not.toHaveBeenCalled();
  });
});
