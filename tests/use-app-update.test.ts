import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAppUpdate } from "@/hooks/useAppUpdate";

const appUpdateInstall = vi.hoisted(() => vi.fn());

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
      hasUnsaved: () => true,
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
    const beforeInstall = vi.fn(async () => {
      throw new Error("save failed");
    });
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
  });
});
