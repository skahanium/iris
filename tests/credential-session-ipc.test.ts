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

    expect(lib).toContain("commands::settings::credential_lock_session");
    expect(lib).toContain("commands::settings::credential_status");
  });

  it("credential_unlock_session has been removed (was a misleading no-op)", () => {
    const lib = readFileSync("src-tauri/src/lib.rs", "utf8");
    const credentials = readFileSync("src-tauri/src/credentials.rs", "utf8");
    const ipc = readFileSync("src/lib/ipc.ts", "utf8");

    // Must NOT exist anymore
    expect(lib).not.toContain("credential_unlock_session");
    expect(credentials).not.toContain("pub fn credential_unlock_session");
    expect(ipc).not.toContain("credentialUnlockSession");
  });

  it("invokes credential lock without exposing secret args", async () => {
    const { credentialLockSession } = await import("@/lib/ipc");
    invoke.mockResolvedValue(undefined);

    await credentialLockSession();

    expect(invoke).toHaveBeenCalledWith("credential_lock_session");
  });

  it("wraps credential status without exposing secret values", async () => {
    const { credentialStatus } = await import("@/lib/ipc");
    invoke.mockResolvedValue({
      service: "iris.llm.deepseek",
      state: "missing",
      configured: false,
      checkedAt: "2026-07-08T00:00:00Z",
    });

    await credentialStatus("iris.llm.deepseek");

    expect(invoke).toHaveBeenCalledWith("credential_status", {
      service: "iris.llm.deepseek",
    });
  });

  it("backend availability checks use the local encrypted credential store instead of marker-only state", () => {
    const settings = readFileSync("src-tauri/src/commands/settings.rs", "utf8");
    const statusCommand = settings.split("pub fn credential_status")[1] ?? "";
    const hasCommand = settings.split("pub fn credential_has")[1] ?? "";
    const config = readFileSync("src-tauri/src/llm/config.rs", "utf8");
    const runtimeContext = readFileSync(
      "src-tauri/src/ai_runtime/runtime_context.rs",
      "utf8",
    );
    const mcpRuntime = readFileSync(
      "src-tauri/src/ai_runtime/mcp_host_runtime.rs",
      "utf8",
    );

    expect(settings).toContain("credential_available_for_runtime");
    expect(settings).toContain("set_credential_marker");
    expect(statusCommand).toContain("credentials::credential_status(&service)");
    expect(hasCommand).toContain("credential_available_for_runtime");
    expect(config).toContain("credentials::credential_available(service)");
    expect(config).not.toContain("credential_marker_configured(db, service)");
    expect(runtimeContext).toContain(
      "credentials::credential_available(&service)",
    );
    expect(mcpRuntime).toContain("credentials::credential_available(service)");
  });
});
