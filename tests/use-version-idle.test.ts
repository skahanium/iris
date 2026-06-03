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
  getPersistedContent,
  onReady,
}: {
  path: string | null;
  getPersistedContent: () => string;
  onReady: (api: { onActivity: () => void }) => void;
}) {
  const { onActivity } = useVersionIdle(path, getPersistedContent);
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

  it("does not save just because a note is open", async () => {
    const getPersistedContent = vi.fn(() => "body");

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "note.md",
          getPersistedContent,
          onReady: () => {},
        }),
      );
    });

    await act(async () => {
      vi.advanceTimersByTime(VERSION_IDLE_MS);
      vi.runOnlyPendingTimers();
    });

    expect(getPersistedContent).not.toHaveBeenCalled();
    expect(versionSaveIdle).not.toHaveBeenCalled();
  });

  it("fires versionSaveIdle after idle interval following activity", async () => {
    let onActivity!: () => void;
    const getPersistedContent = vi.fn(() => "body");

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "note.md",
          getPersistedContent,
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

    expect(getPersistedContent).toHaveBeenCalledTimes(1);
    expect(versionSaveIdle).toHaveBeenCalledTimes(1);
    expect(versionSaveIdle).toHaveBeenCalledWith("note.md", "body");
  });

  it("uses persisted markdown ref content, not live editor serialization", async () => {
    let onActivity!: () => void;
    const getPersistedContent = vi.fn(
      () => '---\ntitle: "x"\n---\n\nPersisted body text.',
    );

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "note.md",
          getPersistedContent,
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

    expect(getPersistedContent).toHaveBeenCalled();
    expect(versionSaveIdle).toHaveBeenCalledTimes(1);
    expect(versionSaveIdle).toHaveBeenCalledWith(
      "note.md",
      '---\ntitle: "x"\n---\n\nPersisted body text.',
    );
  });

  it("resets idle timer on activity", async () => {
    let onActivity!: () => void;
    const getPersistedContent = vi.fn(
      () => '---\ntitle: "x"\n---\n\nPersisted body text.',
    );

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "note.md",
          getPersistedContent,
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
    expect(getPersistedContent).toHaveBeenCalled();
  });

  it("defers snapshot until the browser idle callback runs", async () => {
    let onActivity!: () => void;
    const getPersistedContent = vi.fn(() => "body");
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
          getPersistedContent,
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
    expect(getPersistedContent).not.toHaveBeenCalled();
    expect(versionSaveIdle).not.toHaveBeenCalled();

    await act(async () => {
      idleCallback?.({
        didTimeout: false,
        timeRemaining: () => 50,
      });
    });

    expect(getPersistedContent).toHaveBeenCalledTimes(1);
    expect(versionSaveIdle).toHaveBeenCalledWith("note.md", "body");
  });

  it("changing path cancels pending idle saves without scheduling a new one", async () => {
    let onActivity!: () => void;
    const firstContent = vi.fn(() => "first");
    const secondContent = vi.fn(() => "second");

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "first.md",
          getPersistedContent: firstContent,
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
          getPersistedContent: secondContent,
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

  it("skips idle snapshot for substantively empty content", async () => {
    let onActivity!: () => void;

    await act(async () => {
      root.render(
        createElement(IdleHarness, {
          path: "note.md",
          getPersistedContent: () => "",
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
