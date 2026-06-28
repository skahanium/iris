import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

describe("document open IPC wrappers", () => {
  beforeEach(() => {
    invoke.mockReset();
  });
  it("requests file signatures through the typed ipc wrapper", async () => {
    const { fileSignature } = await import("../src/lib/ipc");
    invoke.mockResolvedValueOnce({
      byteLength: 3,
      contentHash: "abc",
      isLocked: false,
      modifiedMs: 10,
    });

    await expect(
      fileSignature("a.md", { allowClassified: true }),
    ).resolves.toEqual({
      byteLength: 3,
      contentHash: "abc",
      isLocked: false,
      modifiedMs: 10,
    });
    expect(invoke).toHaveBeenCalledWith("file_signature", {
      path: "a.md",
      allowClassified: true,
    });
  });

  it("opens and closes a foreground document-open scope", async () => {
    const { documentOpenBegin, documentOpenEnd } =
      await import("../src/lib/ipc");
    invoke.mockResolvedValueOnce({ token: "scope-1" });
    invoke.mockResolvedValueOnce(undefined);

    await expect(documentOpenBegin()).resolves.toEqual({ token: "scope-1" });
    await expect(documentOpenEnd("scope-1")).resolves.toBeUndefined();
    expect(invoke).toHaveBeenNthCalledWith(1, "document_open_begin");
    expect(invoke).toHaveBeenNthCalledWith(2, "document_open_end", {
      token: "scope-1",
    });
  });
});
