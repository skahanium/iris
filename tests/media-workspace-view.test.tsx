import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { MediaWorkspaceView } from "@/components/layout/MediaWorkspaceView";
import type { MediaTab } from "@/hooks/useMediaTabs";

const mediaResolve = vi.fn();
const mediaRelease = vi.fn();

vi.mock("@/lib/ipc", () => ({
  mediaResolve: (...args: unknown[]) => mediaResolve(...args),
  mediaRelease: (...args: unknown[]) => mediaRelease(...args),
}));

function tab(path: string, mediaKind: MediaTab["mediaKind"]): MediaTab {
  return {
    id: `media:${path}`,
    mediaKind,
    mimeType: null,
    path,
    sizeBytes: null,
    title: path.split("/").pop() ?? path,
    updatedAt: null,
  };
}

describe("MediaWorkspaceView", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    mediaResolve.mockReset();
    mediaRelease.mockReset();
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
  });

  it("renders image previews from opaque media leases", async () => {
    mediaResolve.mockResolvedValue({
      handle: "lease-image",
      mediaKind: "image",
      mimeType: "image/png",
      path: "assets/photo.png",
      sizeBytes: 123,
      updatedAt: "2026-06-24T00:00:00Z",
      url: "iris-media://localhost/lease-image",
    });

    await act(async () => {
      root.render(
        <MediaWorkspaceView tab={tab("assets/photo.png", "image")} />,
      );
    });

    const img = container.querySelector("img");
    expect(img?.getAttribute("src")).toBe("iris-media://localhost/lease-image");
    expect(img?.getAttribute("decoding")).toBe("async");

    act(() => root.unmount());
    expect(mediaRelease).toHaveBeenCalledWith("lease-image");
  });

  it("uses native PDF and video surfaces without eager byte reads", async () => {
    mediaResolve
      .mockResolvedValueOnce({
        handle: "lease-pdf",
        mediaKind: "pdf",
        mimeType: "application/pdf",
        path: "paper.pdf",
        sizeBytes: 10,
        updatedAt: null,
        url: "iris-media://localhost/lease-pdf",
      })
      .mockResolvedValueOnce({
        handle: "lease-video",
        mediaKind: "video",
        mimeType: "video/mp4",
        path: "clip.mp4",
        sizeBytes: 20,
        updatedAt: null,
        url: "iris-media://localhost/lease-video",
      });

    await act(async () => {
      root.render(<MediaWorkspaceView tab={tab("paper.pdf", "pdf")} />);
    });
    expect(container.querySelector("object")?.getAttribute("data")).toBe(
      "iris-media://localhost/lease-pdf",
    );

    await act(async () => {
      root.render(<MediaWorkspaceView tab={tab("clip.mp4", "video")} />);
    });
    const video = container.querySelector("video");
    expect(video?.getAttribute("src")).toBe(
      "iris-media://localhost/lease-video",
    );
    expect(video?.getAttribute("preload")).toBe("metadata");
    expect(mediaRelease).toHaveBeenCalledWith("lease-pdf");
  });
});
