import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useTauriCloseSave } from "@/hooks/useTauriCloseSave";

const appExit = vi.hoisted(() => vi.fn());

const tauriWindow = vi.hoisted(() => ({
  close: vi.fn(),
  destroy: vi.fn(),
  onCloseRequested: vi.fn(),
  unlisten: vi.fn(),
  handler: null as
    | null
    | ((event: { preventDefault: () => void }) => void | Promise<void>),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    close: tauriWindow.close,
    destroy: tauriWindow.destroy,
    onCloseRequested: tauriWindow.onCloseRequested,
  }),
}));

vi.mock("@/lib/ipc", () => ({
  appExit: (...args: unknown[]) => appExit(...args),
}));

vi.mock("@/lib/tauri-runtime", () => ({
  isTauriRuntime: () => true,
}));

function Harness({
  flushBeforeClose,
  onBlocked,
  onError,
}: {
  flushBeforeClose: () => Promise<void>;
  onBlocked?: (retry: () => Promise<void>) => void;
  onError?: (message: string) => void;
}) {
  useTauriCloseSave({ flushBeforeClose, onBlocked, onError });
  return null;
}

describe("useTauriCloseSave", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    appExit.mockReset();
    appExit.mockResolvedValue(undefined);
    tauriWindow.close.mockReset();
    tauriWindow.close.mockResolvedValue(undefined);
    tauriWindow.destroy.mockReset();
    tauriWindow.destroy.mockResolvedValue(undefined);
    tauriWindow.unlisten.mockReset();
    tauriWindow.handler = null;
    tauriWindow.onCloseRequested.mockReset();
    tauriWindow.onCloseRequested.mockImplementation(async (handler) => {
      tauriWindow.handler = handler;
      return tauriWindow.unlisten;
    });
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
    vi.useRealTimers();
  });

  it("prevents the first close, flushes open tabs, then exits the app on the next tick", async () => {
    vi.useFakeTimers();
    const flushBeforeClose = vi.fn(async () => undefined);
    const firstPreventDefault = vi.fn();
    const secondPreventDefault = vi.fn();

    await act(async () => {
      root.render(createElement(Harness, { flushBeforeClose }));
    });

    await act(async () => {
      await tauriWindow.handler?.({ preventDefault: firstPreventDefault });
    });

    expect(appExit).not.toHaveBeenCalled();

    await act(async () => {
      vi.runOnlyPendingTimers();
      await Promise.resolve();
    });

    await act(async () => {
      await tauriWindow.handler?.({ preventDefault: secondPreventDefault });
    });

    expect(firstPreventDefault).toHaveBeenCalledTimes(1);
    expect(secondPreventDefault).not.toHaveBeenCalled();
    expect(flushBeforeClose).toHaveBeenCalledTimes(1);
    expect(appExit).toHaveBeenCalledTimes(1);
    expect(tauriWindow.close).not.toHaveBeenCalled();
    expect(tauriWindow.destroy).not.toHaveBeenCalled();
  });

  it("keeps the window open and reports an error when flushing fails", async () => {
    const flushBeforeClose = vi.fn(async () => {
      throw new Error("disk write failed");
    });
    const onError = vi.fn();
    const preventDefault = vi.fn();

    await act(async () => {
      root.render(createElement(Harness, { flushBeforeClose, onError }));
    });

    await act(async () => {
      await tauriWindow.handler?.({ preventDefault });
    });

    expect(preventDefault).toHaveBeenCalledTimes(1);
    expect(flushBeforeClose).toHaveBeenCalledTimes(1);
    expect(appExit).not.toHaveBeenCalled();
    expect(tauriWindow.close).not.toHaveBeenCalled();
    expect(tauriWindow.destroy).not.toHaveBeenCalled();
    expect(onError).toHaveBeenCalledWith("disk write failed");
  });

  it("supplies a retry action after a blocked close and exits only after that retry saves", async () => {
    vi.useFakeTimers();
    const flushBeforeClose = vi
      .fn<() => Promise<void>>()
      .mockRejectedValueOnce(new Error("disk write failed"))
      .mockResolvedValueOnce(undefined);
    const onBlocked = vi.fn();

    await act(async () => {
      root.render(createElement(Harness, { flushBeforeClose, onBlocked }));
    });

    await act(async () => {
      await tauriWindow.handler?.({ preventDefault: vi.fn() });
    });

    const retry = onBlocked.mock.calls[0]?.[0] as
      | (() => Promise<void>)
      | undefined;
    expect(retry).toBeTypeOf("function");
    expect(appExit).not.toHaveBeenCalled();

    await act(async () => {
      await retry?.();
      vi.runOnlyPendingTimers();
      await Promise.resolve();
    });

    expect(flushBeforeClose).toHaveBeenCalledTimes(2);
    expect(appExit).toHaveBeenCalledTimes(1);
  });

  it("unlistens on unmount", async () => {
    await act(async () => {
      root.render(
        createElement(Harness, {
          flushBeforeClose: async () => undefined,
        }),
      );
    });

    act(() => {
      root.unmount();
    });

    expect(tauriWindow.unlisten).toHaveBeenCalledTimes(1);
  });
});
