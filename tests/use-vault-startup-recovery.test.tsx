import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const vaultMocks = vi.hoisted(() => ({
  vaultGet: vi.fn<() => Promise<string | null>>(),
  vaultSet: vi.fn<(path: string) => Promise<void>>(),
}));

vi.mock("@/lib/ipc", () => vaultMocks);
vi.mock("@/lib/tauri-runtime", () => ({
  isTauriRuntime: () => true,
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

import { useVault, VAULT_BOOTSTRAP_TIMEOUT_MS } from "@/hooks/useVault";

let root: Root | null = null;
let host: HTMLDivElement | null = null;

function deferred<T>() {
  let resolve: (value: T) => void = () => undefined;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function VaultProbe() {
  const { error, loading, refresh, vaultPath } = useVault();
  return createElement(
    "button",
    {
      type: "button",
      onClick: () => void refresh(),
      "data-error": error ?? "",
      "data-loading": String(loading),
      "data-vault-path": vaultPath ?? "",
    },
    "retry",
  );
}

beforeEach(() => {
  vi.useFakeTimers();
  vaultMocks.vaultSet.mockReset();
  vaultMocks.vaultGet.mockReset();
  vaultMocks.vaultGet.mockImplementation(() => new Promise(() => undefined));
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root?.unmount());
  host?.remove();
  root = null;
  host = null;
  vi.useRealTimers();
});

describe("useVault startup recovery", () => {
  it("leaves the startup gate and exposes a retryable error when vault_get never settles", async () => {
    await act(async () => {
      root?.render(createElement(VaultProbe));
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(VAULT_BOOTSTRAP_TIMEOUT_MS);
    });

    const probe = host?.querySelector("button");
    expect(probe?.getAttribute("data-loading")).toBe("false");
    expect(probe?.getAttribute("data-vault-path")).toBe("");
    expect(probe?.getAttribute("data-error")).toContain("启动服务未响应");
  });

  it("keeps a successful retry when the timed-out request resolves late", async () => {
    const firstRequest = deferred<string | null>();
    vaultMocks.vaultGet
      .mockImplementationOnce(() => firstRequest.promise)
      .mockResolvedValueOnce("/fresh-vault");

    await act(async () => {
      root?.render(createElement(VaultProbe));
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(VAULT_BOOTSTRAP_TIMEOUT_MS);
    });

    const probe = host?.querySelector("button");
    await act(async () => {
      probe?.click();
      await Promise.resolve();
    });
    expect(probe?.getAttribute("data-vault-path")).toBe("/fresh-vault");
    expect(probe?.getAttribute("data-error")).toBe("");

    await act(async () => {
      firstRequest.resolve("/stale-vault");
      await Promise.resolve();
    });
    expect(probe?.getAttribute("data-vault-path")).toBe("/fresh-vault");
  });
});
