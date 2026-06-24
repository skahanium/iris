import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

import {
  mediaMetadata,
  mediaRelease,
  mediaResolve,
  workspaceList,
} from "@/lib/ipc";

describe("media workspace IPC contract", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("lists workspace items with optional limits", async () => {
    invoke.mockResolvedValue([]);
    await workspaceList({ limit: 30, offset: 10 });
    expect(invoke).toHaveBeenCalledWith("workspace_list", {
      limit: 30,
      offset: 10,
    });
  });

  it("resolves media paths without exposing raw file reads to callers", async () => {
    invoke.mockResolvedValue({
      handle: "media-1",
      mediaKind: "pdf",
      mimeType: "application/pdf",
      path: "assets/paper.pdf",
      sizeBytes: 10,
      url: "asset://localhost/assets/paper.pdf",
    });
    await mediaResolve("assets/paper.pdf");
    expect(invoke).toHaveBeenCalledWith("media_resolve", {
      path: "assets/paper.pdf",
    });
  });

  it("loads media metadata and releases handles through dedicated commands", async () => {
    invoke.mockResolvedValueOnce({
      mediaKind: "image",
      mimeType: "image/png",
      path: "assets/a.png",
      sizeBytes: 12,
      updatedAt: "2026-06-24T00:00:00Z",
    });
    await mediaMetadata("assets/a.png");
    expect(invoke).toHaveBeenCalledWith("media_metadata", {
      path: "assets/a.png",
    });

    invoke.mockResolvedValueOnce(undefined);
    await mediaRelease("media-1");
    expect(invoke).toHaveBeenCalledWith("media_release", {
      handle: "media-1",
    });
  });
});
