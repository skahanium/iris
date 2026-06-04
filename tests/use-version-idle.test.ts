import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useVersionIdle, VERSION_IDLE_MS } from "@/hooks/useVersionIdle";

const versionSaveIdle = vi.fn().mockResolvedValue(undefined);

vi.mock("@/lib/ipc", () => ({
  versionSaveIdle: (...args: unknown[]) => versionSaveIdle(...args),
}));

function IdleHarness({
  path,
  flushSave,
  onReady,
}: {
  path: string | null;
  flushSave: () => Promise<string | null>;
  onReady: (api: { onActivity: () => void }) => void;
}) {
  const { onActivity } = useVersionIdle(path, flushSave);
  onReady({ onActivity });
  return null;
}

describe("useVersionIdle", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.useFakeTimers();
    versionSaveIdle.mockClear();
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
    const flushSave = vi.fn(async () => "body");

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "note.md",
          flushSave,
          onReady: () => {},
        }),
      );
    });

    await act(async () => {
      vi.advanceTimersByTime(VERSION_IDLE_MS);
      vi.runOnlyPendingTimers();
    });

    expect(flushSave).not.toHaveBeenCalled();
    expect(versionSaveIdle).not.toHaveBeenCalled();
  });

  it("fires versionSaveIdle after idle interval following activity", async () => {
    let onActivity!: () => void;
    const flushSave = vi.fn(async () => "body");

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "note.md",
          flushSave,
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

    expect(flushSave).toHaveBeenCalledTimes(1);
    expect(versionSaveIdle).toHaveBeenCalledTimes(1);
    expect(versionSaveIdle).toHaveBeenCalledWith("note.md", "body");
  });

  it("uses the markdown returned by flushSave for idle snapshots", async () => {
    let onActivity!: () => void;
    const flushSave = vi.fn(
      async () => '---\ntitle: "x"\n---\n\nFlushed body text.',
    );

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "note.md",
          flushSave,
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

    expect(flushSave).toHaveBeenCalled();
    expect(versionSaveIdle).toHaveBeenCalledTimes(1);
    expect(versionSaveIdle).toHaveBeenCalledWith(
      "note.md",
      '---\ntitle: "x"\n---\n\nFlushed body text.',
    );
  });

  it("resets idle timer on activity", async () => {
    let onActivity!: () => void;
    const flushSave = vi.fn(
      async () => '---\ntitle: "x"\n---\n\nPersisted body text.',
    );

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "note.md",
          flushSave,
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
    expect(versionSaveIdle).not.toHaveBeenCalled();

    act(() => {
      onActivity();
    });

    await act(async () => {
      vi.advanceTimersByTime(VERSION_IDLE_MS - 1000);
    });
    expect(versionSaveIdle).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(1000);
      vi.runOnlyPendingTimers();
    });

    expect(versionSaveIdle).toHaveBeenCalledTimes(1);
    expect(flushSave).toHaveBeenCalled();
  });

  it("defers snapshot until the browser idle callback runs", async () => {
    let onActivity!: () => void;
    const flushSave = vi.fn(async () => "body");
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
          flushSave,
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
    expect(versionSaveIdle).not.toHaveBeenCalled();

    await act(async () => {
      idleCallback?.({
        didTimeout: false,
        timeRemaining: () => 50,
      });
    });

    expect(flushSave).toHaveBeenCalledTimes(1);
    expect(versionSaveIdle).toHaveBeenCalledWith("note.md", "body");
  });

  it("changing path cancels pending idle saves without scheduling a new one", async () => {
    let onActivity!: () => void;
    const firstFlush = vi.fn(async () => "first");
    const secondFlush = vi.fn(async () => "second");

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "first.md",
          flushSave: firstFlush,
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
          flushSave: secondFlush,
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
    expect(versionSaveIdle).not.toHaveBeenCalled();
  });

  it("skips idle snapshot for substantively empty content", async () => {
    let onActivity!: () => void;

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "note.md",
          flushSave: async () => "",
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

    expect(versionSaveIdle).not.toHaveBeenCalled();
  });
});
