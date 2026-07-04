import { readFileSync } from "node:fs";

import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

describe("credential session IPC contract", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("registers credential session commands in Tauri lib.rs", () => {
    const lib = readFileSync("src-tauri/src/lib.rs", "utf8");

    expect(lib).toContain("commands::settings::credential_unlock_session");
    expect(lib).toContain("commands::settings::credential_lock_session");
  });

  it("invokes credential unlock and lock without exposing secret args", async () => {
    const { credentialLockSession, credentialUnlockSession } =
      await import("@/lib/ipc");
    invoke.mockResolvedValue(undefined);

    await credentialUnlockSession();
    await credentialLockSession();

    expect(invoke).toHaveBeenNthCalledWith(1, "credential_unlock_session");
    expect(invoke).toHaveBeenNthCalledWith(2, "credential_lock_session");
  });
});
