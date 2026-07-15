import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useEmbeddingScheduler } from "@/hooks/useEmbeddingScheduler";
import type { EmbeddingIndexStatus } from "@/types/ipc";

const schedulerStatus = vi.hoisted(() => vi.fn());
const schedulerStart = vi.hoisted(() => vi.fn());
const schedulerSetPaused = vi.hoisted(() => vi.fn());
const schedulerSetForegroundBusy = vi.hoisted(() => vi.fn());
const listenSchedulerStatus = vi.hoisted(() => vi.fn());

vi.mock("@/lib/ipc", () => ({
  embeddingSchedulerStatus: (...args: unknown[]) => schedulerStatus(...args),
  embeddingSchedulerStart: (...args: unknown[]) => schedulerStart(...args),
  embeddingSchedulerSetPaused: (...args: unknown[]) =>
    schedulerSetPaused(...args),
  embeddingSchedulerSetForegroundBusy: (...args: unknown[]) =>
    schedulerSetForegroundBusy(...args),
  listenEmbeddingSchedulerStatus: (...args: unknown[]) =>
    listenSchedulerStatus(...args),
}));

vi.mock("@/lib/tauri-runtime", () => ({
  isTauriRuntime: () => true,
}));

const legacyReady: EmbeddingIndexStatus = {
  activeModelId: "fastembed/AllMiniLML6V2",
  targetModelId: "Xenova/bge-small-zh-v1.5",
  dimension: 512,
  phase: "legacy_ready",
  indexedItems: 0,
  totalItems: 4,
  lastError: null,
  failureCode: null,
  automaticAttempted: false,
};

const running: EmbeddingIndexStatus = {
  ...legacyReady,
  automaticAttempted: true,
  indexedItems: 2,
  phase: "running",
};

function Harness({
  hasDirtyDocuments,
  onReady,
}: {
  hasDirtyDocuments: boolean;
  onReady: (api: ReturnType<typeof useEmbeddingScheduler>) => void;
}) {
  onReady(useEmbeddingScheduler({ hasDirtyDocuments }));
  return null;
}

describe("useEmbeddingScheduler", () => {
  let host: HTMLDivElement;
  let root: Root;
  let emit: ((status: EmbeddingIndexStatus) => void) | undefined;
  const unlisten = vi.fn();

  beforeEach(() => {
    vi.useFakeTimers();
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    emit = undefined;
    unlisten.mockReset();
    schedulerStatus.mockReset();
    schedulerStatus.mockResolvedValue(legacyReady);
    schedulerStart.mockReset();
    schedulerStart.mockResolvedValue("started");
    schedulerSetPaused.mockReset();
    schedulerSetPaused.mockResolvedValue(undefined);
    schedulerSetForegroundBusy.mockReset();
    schedulerSetForegroundBusy.mockResolvedValue(undefined);
    listenSchedulerStatus.mockReset();
    listenSchedulerStatus.mockImplementation(
      async (handler: (status: EmbeddingIndexStatus) => void) => {
        emit = handler;
        return unlisten;
      },
    );
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.useRealTimers();
  });

  it("loads the scheduler state, replaces it from the one status event, and unlistens", async () => {
    let api!: ReturnType<typeof useEmbeddingScheduler>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          hasDirtyDocuments: false,
          onReady: (next) => {
            api = next;
          },
        }),
      );
      await Promise.resolve();
    });

    expect(schedulerStatus).toHaveBeenCalledTimes(1);
    expect(api.status).toEqual(legacyReady);

    await act(async () => {
      emit?.(running);
    });

    expect(api.status).toEqual(running);

    act(() => root.unmount());
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("treats already_running as an idempotent start result and only invokes the pause command", async () => {
    schedulerStart.mockResolvedValue("already_running");
    let api!: ReturnType<typeof useEmbeddingScheduler>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          hasDirtyDocuments: false,
          onReady: (next) => {
            api = next;
          },
        }),
      );
      await Promise.resolve();
    });

    await act(async () => {
      await expect(api.start()).resolves.toBe("already_running");
      await api.setPaused(true);
    });

    expect(schedulerStart).toHaveBeenCalledTimes(1);
    expect(schedulerSetPaused).toHaveBeenCalledWith(true);
    expect(api.status?.phase).toBe("legacy_ready");
  });

  it("reports clean immediately so the backend owns the single 30 second idle clock", async () => {
    await act(async () => {
      root.render(
        createElement(Harness, {
          hasDirtyDocuments: true,
          onReady: () => {},
        }),
      );
      await Promise.resolve();
    });

    expect(schedulerSetForegroundBusy).toHaveBeenCalledWith(true);

    expect(schedulerSetForegroundBusy).toHaveBeenCalledTimes(1);

    await act(async () => {
      root.render(
        createElement(Harness, {
          hasDirtyDocuments: false,
          onReady: () => {},
        }),
      );
    });

    await act(async () => {
      await Promise.resolve();
    });
    expect(schedulerSetForegroundBusy).toHaveBeenLastCalledWith(false);
  });

  it("resets the idle period when foreground activity is reported", async () => {
    let api!: ReturnType<typeof useEmbeddingScheduler>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          hasDirtyDocuments: false,
          onReady: (next) => {
            api = next;
          },
        }),
      );
      await Promise.resolve();
    });

    await act(async () => {
      await api.reportForegroundActivity();
      await Promise.resolve();
    });
    expect(schedulerSetForegroundBusy).toHaveBeenCalledWith(true);
    expect(schedulerSetForegroundBusy).toHaveBeenLastCalledWith(false);
  });

  it("serializes rapid dirty-to-clean foreground facts so stale busy cannot arrive after idle", async () => {
    let releaseBusy!: () => void;
    schedulerSetForegroundBusy.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          releaseBusy = resolve;
        }),
    );
    schedulerSetForegroundBusy.mockResolvedValue(undefined);

    await act(async () => {
      root.render(
        createElement(Harness, {
          hasDirtyDocuments: true,
          onReady: () => {},
        }),
      );
      await Promise.resolve();
    });
    await act(async () => {
      root.render(
        createElement(Harness, {
          hasDirtyDocuments: false,
          onReady: () => {},
        }),
      );
      await Promise.resolve();
    });

    expect(schedulerSetForegroundBusy.mock.calls).toEqual([[true]]);

    await act(async () => {
      releaseBusy();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(schedulerSetForegroundBusy.mock.calls).toEqual([[true], [false]]);
  });
});
