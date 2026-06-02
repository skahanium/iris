import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useVersionIdle, VERSION_IDLE_MS } from "@/hooks/useVersionIdle";

const versionSaveIdle = vi.fn().mockResolvedValue(null);

vi.mock("@/lib/ipc", () => ({
  versionSaveIdle: (...args: unknown[]) => versionSaveIdle(...args),
}));

function TestHarness({
  path,
  getContent,
  onReady,
}: {
  path: string | null;
  getContent: () => string;
  onReady: (api: { onActivity: () => void }) => void;
}) {
  const { onActivity } = useVersionIdle(path, getContent);
  onReady({ onActivity });
  return null;
}

describe("useVersionIdle", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.useFakeTimers();
    versionSaveIdle.mockClear();
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

  it("does not serialize or save just because a note is open", async () => {
    const getContent = vi.fn(() => "body");

    await act(async () => {
      root.render(
        createElement(TestHarness, {
          path: "note.md",
          getContent,
          onReady: () => {},
        }),
      );
    });

    await act(async () => {
      vi.advanceTimersByTime(VERSION_IDLE_MS);
      vi.runOnlyPendingTimers();
    });

    expect(getContent).not.toHaveBeenCalled();
    expect(versionSaveIdle).not.toHaveBeenCalled();
  });

  it("fires versionSaveIdle after idle interval", async () => {
    let onActivity!: () => void;
    const getContent = vi.fn(() => "body");

    await act(async () => {
      root.render(
        createElement(TestHarness, {
          path: "note.md",
          getContent,
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

    expect(getContent).toHaveBeenCalledTimes(1);
    expect(versionSaveIdle).toHaveBeenCalledTimes(1);
    expect(versionSaveIdle).toHaveBeenCalledWith("note.md", "body");
  });

  it("resets idle timer on activity", async () => {
    let onActivity!: () => void;
    const getContent = vi.fn(() => "body");

    await act(async () => {
      root.render(
        createElement(TestHarness, {
          path: "note.md",
          getContent,
          onReady: (api) => {
            onActivity = api.onActivity;
          },
        }),
      );
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
    expect(getContent).toHaveBeenCalledTimes(1);
  });

  it("defers serialization until the browser idle callback runs", async () => {
    let onActivity!: () => void;
    const getContent = vi.fn(() => "body");
    let idleCallback: IdleRequestCallback | null = null;
    const requestIdleCallback = vi.fn((cb: IdleRequestCallback) => {
      idleCallback = cb;
      return 1;
    });
    Object.assign(window, { requestIdleCallback });

    await act(async () => {
      root.render(
        createElement(TestHarness, {
          path: "note.md",
          getContent,
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
    expect(getContent).not.toHaveBeenCalled();
    expect(versionSaveIdle).not.toHaveBeenCalled();

    await act(async () => {
      idleCallback?.({
        didTimeout: false,
        timeRemaining: () => 50,
      });
    });

    expect(getContent).toHaveBeenCalledTimes(1);
    expect(versionSaveIdle).toHaveBeenCalledWith("note.md", "body");
  });

  it("changing path cancels pending idle saves without scheduling a new one", async () => {
    let onActivity!: () => void;
    const firstContent = vi.fn(() => "first");
    const secondContent = vi.fn(() => "second");

    await act(async () => {
      root.render(
        createElement(TestHarness, {
          path: "first.md",
          getContent: firstContent,
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
        createElement(TestHarness, {
          path: "second.md",
          getContent: secondContent,
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

    expect(firstContent).not.toHaveBeenCalled();
    expect(secondContent).not.toHaveBeenCalled();
    expect(versionSaveIdle).not.toHaveBeenCalled();
  });
});
