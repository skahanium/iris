import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAppUpdate } from "@/hooks/useAppUpdate";

const appUpdateInstall = vi.hoisted(() => vi.fn());

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((nextResolve) => {
    resolve = nextResolve;
  });
  return { promise, resolve };
}

vi.mock("@/lib/ipc", () => ({
  appUpdateCheck: vi.fn(),
  appUpdateDownload: vi.fn(),
  appUpdateInstall: (...args: unknown[]) => appUpdateInstall(...args),
  appUpdatePreflight: vi.fn(),
  listenAppUpdateProgress: vi.fn(),
  listenAppUpdateStatus: vi.fn(),
}));

vi.mock("@/lib/tauri-runtime", () => ({
  isTauriRuntime: () => false,
}));

function Harness({
  beforeInstall,
  onReady,
}: {
  beforeInstall: () => Promise<void>;
  onReady: (api: ReturnType<typeof useAppUpdate>) => void;
}) {
  onReady(
    useAppUpdate({
      beforeInstall,
      enabled: false,
    }),
  );
  return null;
}

describe("useAppUpdate", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    appUpdateInstall.mockReset();
    appUpdateInstall.mockResolvedValue(undefined);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("runs the persistence barrier before installing an update", async () => {
    const beforeInstall = vi.fn(async () => undefined);
    let api!: ReturnType<typeof useAppUpdate>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          beforeInstall,
          onReady: (next) => {
            api = next;
          },
        }),
      );
    });

    await act(async () => {
      await api.install();
    });

    expect(beforeInstall).toHaveBeenCalledTimes(1);
    expect(beforeInstall.mock.invocationCallOrder[0]).toBeLessThan(
      appUpdateInstall.mock.invocationCallOrder[0]!,
    );
    expect(appUpdateInstall).toHaveBeenCalledTimes(1);
  });

  it("blocks installation when the persistence barrier rejects", async () => {
    const beforeInstall = vi
      .fn<() => Promise<void>>()
      .mockRejectedValueOnce(new Error("save failed"));
    let api!: ReturnType<typeof useAppUpdate>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          beforeInstall,
          onReady: (next) => {
            api = next;
          },
        }),
      );
    });

    await act(async () => {
      await api.install();
    });

    expect(appUpdateInstall).not.toHaveBeenCalled();
    expect(api.snapshot.status).toBe("ready_to_install");
    expect(api.snapshot.busy).toBe(false);
    expect(api.snapshot.installBlockedMessage).toContain("保存失败");

    beforeInstall.mockResolvedValueOnce(undefined);
    await act(async () => {
      await api.install();
    });
    expect(appUpdateInstall).toHaveBeenCalledTimes(1);
  });

  it("coalesces double-clicked installation attempts behind one persistence lease", async () => {
    const beforeInstallGate = deferred<void>();
    const beforeInstall = vi.fn(() => beforeInstallGate.promise);
    let api!: ReturnType<typeof useAppUpdate>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          beforeInstall,
          onReady: (next) => {
            api = next;
          },
        }),
      );
    });

    let first!: Promise<void>;
    let second!: Promise<void>;
    await act(async () => {
      first = api.install();
      second = api.install();
      await Promise.resolve();
    });

    expect(first).toBe(second);
    expect(beforeInstall).toHaveBeenCalledTimes(1);
    expect(api.snapshot.busy).toBe(true);

    await act(async () => {
      beforeInstallGate.resolve(undefined);
      await first;
    });

    expect(appUpdateInstall).toHaveBeenCalledTimes(1);
  });
});
