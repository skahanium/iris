import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { VersionTimeline } from "@/components/version/VersionTimeline";

const versionFinalizeCurrent = vi.fn();
const versionList = vi.fn();

vi.mock("@/lib/ipc", () => ({
  versionFinalizeCurrent: (...args: unknown[]) =>
    versionFinalizeCurrent(...args),
  versionList: (...args: unknown[]) => versionList(...args),
  versionPreview: vi.fn(),
  versionRestore: vi.fn(),
  versionDelete: vi.fn(),
}));

describe("VersionTimeline finalize", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    versionList.mockResolvedValue([]);
    versionFinalizeCurrent.mockResolvedValue(null);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("marks high priority around finalize IPC", async () => {
    let resolveFinalize!: () => void;
    versionFinalizeCurrent.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          resolveFinalize = resolve;
        }),
    );

    const onHighPriorityStart = vi.fn();
    const onHighPriorityEnd = vi.fn();

    act(() => {
      root.render(
        <VersionTimeline
          open
          onClose={() => {}}
          notePath="notes/a.md"
          currentContent="body"
          onRestore={() => {}}
          onHighPriorityStart={onHighPriorityStart}
          onHighPriorityEnd={onHighPriorityEnd}
        />,
      );
    });

    const finalizeBtn = Array.from(document.querySelectorAll("button")).find(
      (b) => b.textContent === "定稿",
    );
    expect(finalizeBtn).toBeTruthy();

    await act(async () => {
      finalizeBtn?.click();
    });

    await vi.waitFor(() => {
      expect(onHighPriorityStart).toHaveBeenCalledWith("notes/a.md");
    });
    expect(onHighPriorityEnd).not.toHaveBeenCalled();

    await act(async () => {
      resolveFinalize();
    });

    await vi.waitFor(() => {
      expect(onHighPriorityEnd).toHaveBeenCalledWith("notes/a.md");
    });
  });
});
