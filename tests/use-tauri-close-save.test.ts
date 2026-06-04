import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useTauriCloseSave } from "@/hooks/useTauriCloseSave";

const tauriWindow = vi.hoisted(() => ({
  destroy: vi.fn(),
  onCloseRequested: vi.fn(),
  unlisten: vi.fn(),
  handler: null as
    | null
    | ((event: { preventDefault: () => void }) => void | Promise<void>),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    destroy: tauriWindow.destroy,
    onCloseRequested: tauriWindow.onCloseRequested,
  }),
}));

vi.mock("@/lib/tauri-runtime", () => ({
  isTauriRuntime: () => true,
}));

function Harness({
  flushBeforeClose,
  onError,
}: {
  flushBeforeClose: () => Promise<void>;
  onError?: (message: string) => void;
}) {
  useTauriCloseSave({ flushBeforeClose, onError });
  return null;
}

describe("useTauriCloseSave", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
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
  });

  it("prevents close, flushes open tabs, then destroys the window", async () => {
    const flushBeforeClose = vi.fn(async () => undefined);
    const preventDefault = vi.fn();

    await act(async () => {
      root.render(createElement(Harness, { flushBeforeClose }));
    });

    await act(async () => {
      await tauriWindow.handler?.({ preventDefault });
    });

    expect(preventDefault).toHaveBeenCalledTimes(1);
    expect(flushBeforeClose).toHaveBeenCalledTimes(1);
    expect(tauriWindow.destroy).toHaveBeenCalledTimes(1);
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
    expect(tauriWindow.destroy).not.toHaveBeenCalled();
    expect(onError).toHaveBeenCalledWith("disk write failed");
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
