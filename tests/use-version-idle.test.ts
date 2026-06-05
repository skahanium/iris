import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useVersionIdle, VERSION_IDLE_MS } from "@/hooks/useVersionIdle";
import type { LastSavedSnapshot } from "@/hooks/useEditorSave";

function IdleHarness({
  path,
  getLastSavedSnapshot,
  enqueueIdleSnapshot,
  onReady,
}: {
  path: string | null;
  getLastSavedSnapshot: () => LastSavedSnapshot | null;
  enqueueIdleSnapshot: (snapshot: LastSavedSnapshot) => void;
  onReady: (api: { onActivity: () => void }) => void;
}) {
  const { onActivity } = useVersionIdle(
    path,
    getLastSavedSnapshot,
    enqueueIdleSnapshot,
  );
  onReady({ onActivity });
  return null;
}

describe("useVersionIdle", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.useFakeTimers();
    delete (window as { requestIdleCallback?: unknown }).requestIdleCallback;
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
    vi.useRealTimers();
  });

  it("does not save just because a note is open", async () => {
    const enqueueIdleSnapshot = vi.fn();

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "note.md",
          getLastSavedSnapshot: () => ({
            path: "note.md",
            markdown: "body",
            savedAt: 1,
            dirtyGeneration: 1,
          }),
          enqueueIdleSnapshot,
          onReady: () => {},
        }),
      );
    });

    await act(async () => {
      vi.advanceTimersByTime(VERSION_IDLE_MS);
      vi.runOnlyPendingTimers();
    });

    expect(enqueueIdleSnapshot).not.toHaveBeenCalled();
  });

  it("enqueues the latest saved snapshot after idle without flushing", async () => {
    let onActivity!: () => void;
    const flushSave = vi.fn(async () => "body");
    const enqueueIdleSnapshot = vi.fn();
    const snapshot: LastSavedSnapshot = {
      path: "note.md",
      markdown: "body",
      savedAt: 1,
      dirtyGeneration: 1,
    };

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "note.md",
          getLastSavedSnapshot: () => snapshot,
          enqueueIdleSnapshot,
          onReady: (api) => {
            onActivity = api.onActivity;
          },
        }),
      );
    });

    act(() => {
      onActivity();
    });

    await act(async () => {
      vi.advanceTimersByTime(VERSION_IDLE_MS);
      vi.runOnlyPendingTimers();
    });

    expect(flushSave).not.toHaveBeenCalled();
    expect(enqueueIdleSnapshot).toHaveBeenCalledTimes(1);
    expect(enqueueIdleSnapshot).toHaveBeenCalledWith(snapshot);
  });

  it("skips idle snapshot when no saved markdown is available", async () => {
    let onActivity!: () => void;
    const enqueueIdleSnapshot = vi.fn();

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "note.md",
          getLastSavedSnapshot: () => null,
          enqueueIdleSnapshot,
          onReady: (api) => {
            onActivity = api.onActivity;
          },
        }),
      );
    });

    act(() => {
      onActivity();
    });

    await act(async () => {
      vi.advanceTimersByTime(VERSION_IDLE_MS);
      vi.runOnlyPendingTimers();
    });

    expect(enqueueIdleSnapshot).not.toHaveBeenCalled();
  });

  it("skips stale saved markdown from another path", async () => {
    let onActivity!: () => void;
    const enqueueIdleSnapshot = vi.fn();

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "note.md",
          getLastSavedSnapshot: () => ({
            path: "other.md",
            markdown: "body",
            savedAt: 1,
            dirtyGeneration: 1,
          }),
          enqueueIdleSnapshot,
          onReady: (api) => {
            onActivity = api.onActivity;
          },
        }),
      );
    });

    act(() => {
      onActivity();
    });

    await act(async () => {
      vi.advanceTimersByTime(VERSION_IDLE_MS);
      vi.runOnlyPendingTimers();
    });

    expect(enqueueIdleSnapshot).not.toHaveBeenCalled();
  });

  it("resets idle timer on activity", async () => {
    let onActivity!: () => void;
    const enqueueIdleSnapshot = vi.fn();
    const snapshot: LastSavedSnapshot = {
      path: "note.md",
      markdown: '---\ntitle: "x"\n---\n\nPersisted body text.',
      savedAt: 1,
      dirtyGeneration: 1,
    };

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "note.md",
          getLastSavedSnapshot: () => snapshot,
          enqueueIdleSnapshot,
          onReady: (api) => {
            onActivity = api.onActivity;
          },
        }),
      );
    });

    act(() => {
      onActivity();
    });

    await act(async () => {
      vi.advanceTimersByTime(VERSION_IDLE_MS / 2);
    });
    expect(enqueueIdleSnapshot).not.toHaveBeenCalled();

    act(() => {
      onActivity();
    });

    await act(async () => {
      vi.advanceTimersByTime(VERSION_IDLE_MS - 1000);
    });
    expect(enqueueIdleSnapshot).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(1000);
      vi.runOnlyPendingTimers();
    });

    expect(enqueueIdleSnapshot).toHaveBeenCalledTimes(1);
    expect(enqueueIdleSnapshot).toHaveBeenCalledWith(snapshot);
  });

  it("defers snapshot until the browser idle callback runs", async () => {
    let onActivity!: () => void;
    const flushSave = vi.fn(async () => "body");
    const enqueueIdleSnapshot = vi.fn();
    const snapshot: LastSavedSnapshot = {
      path: "note.md",
      markdown: "body",
      savedAt: 1,
      dirtyGeneration: 1,
    };
    let idleCallback: IdleRequestCallback | null = null;
    const requestIdleCallback = vi.fn((cb: IdleRequestCallback) => {
      idleCallback = cb;
      return 1;
    });
    Object.assign(window, { requestIdleCallback });

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "note.md",
          getLastSavedSnapshot: () => snapshot,
          enqueueIdleSnapshot,
          onReady: (api) => {
            onActivity = api.onActivity;
          },
        }),
      );
    });

    act(() => {
      onActivity();
    });

    await act(async () => {
      vi.advanceTimersByTime(VERSION_IDLE_MS);
    });

    expect(requestIdleCallback).toHaveBeenCalledTimes(1);
    expect(flushSave).not.toHaveBeenCalled();
    expect(enqueueIdleSnapshot).not.toHaveBeenCalled();

    await act(async () => {
      idleCallback?.({
        didTimeout: false,
        timeRemaining: () => 50,
      });
    });

    expect(flushSave).not.toHaveBeenCalled();
    expect(enqueueIdleSnapshot).toHaveBeenCalledWith(snapshot);
  });

  it("changing path cancels pending idle saves without scheduling a new one", async () => {
    let onActivity!: () => void;
    const firstFlush = vi.fn(async () => "first");
    const secondFlush = vi.fn(async () => "second");

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "first.md",
          getLastSavedSnapshot: () => ({
            path: "first.md",
            markdown: "first",
            savedAt: 1,
            dirtyGeneration: 1,
          }),
          enqueueIdleSnapshot: () => {},
          onReady: (api) => {
            onActivity = api.onActivity;
          },
        }),
      );
    });

    act(() => {
      onActivity();
    });

    await act(async () => {
      vi.advanceTimersByTime(VERSION_IDLE_MS / 2);
      root.render(
        createElement(IdleHarness, {
          path: "second.md",
          getLastSavedSnapshot: () => ({
            path: "second.md",
            markdown: "second",
            savedAt: 1,
            dirtyGeneration: 1,
          }),
          enqueueIdleSnapshot: () => {},
          onReady: (api) => {
            onActivity = api.onActivity;
          },
        }),
      );
    });

    await act(async () => {
      vi.advanceTimersByTime(VERSION_IDLE_MS);
      vi.runOnlyPendingTimers();
    });

    expect(firstFlush).not.toHaveBeenCalled();
    expect(secondFlush).not.toHaveBeenCalled();
  });

  it("skips idle snapshot for substantively empty content", async () => {
    let onActivity!: () => void;
    const enqueueIdleSnapshot = vi.fn();

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "note.md",
          getLastSavedSnapshot: () => ({
            path: "note.md",
            markdown: "",
            savedAt: 1,
            dirtyGeneration: 1,
          }),
          enqueueIdleSnapshot,
          onReady: (api) => {
            onActivity = api.onActivity;
          },
        }),
      );
    });

    act(() => {
      onActivity();
    });

    await act(async () => {
      vi.advanceTimersByTime(VERSION_IDLE_MS);
      vi.runOnlyPendingTimers();
    });

    expect(enqueueIdleSnapshot).not.toHaveBeenCalled();
  });
});
