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
    versionList.mockReset();
    versionFinalizeCurrent.mockReset();
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

    const finalizeLabelInput = document.querySelector("input");
    const finalizeBtn = finalizeLabelInput?.nextElementSibling as
      | HTMLButtonElement
      | undefined;
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

  it("flushes current note before finalizing and versions the flushed markdown", async () => {
    const onBeforeFinalizeCurrent = vi.fn(async () => "# saved markdown");
    const getCurrentContent = vi.fn(() => "# live fallback");

    act(() => {
      root.render(
        <VersionTimeline
          open
          onClose={() => {}}
          notePath="notes/a.md"
          currentContent="# stale state"
          getCurrentContent={getCurrentContent}
          onBeforeFinalizeCurrent={onBeforeFinalizeCurrent}
          onRestore={() => {}}
        />,
      );
    });

    const finalizeLabelInput = document.querySelector("input");
    const finalizeBtn = finalizeLabelInput?.nextElementSibling as
      | HTMLButtonElement
      | undefined;
    expect(finalizeBtn).toBeTruthy();

    await act(async () => {
      finalizeBtn?.click();
      await Promise.resolve();
    });

    await vi.waitFor(() => {
      expect(onBeforeFinalizeCurrent).toHaveBeenCalledTimes(1);
      expect(versionFinalizeCurrent).toHaveBeenCalledWith(
        "notes/a.md",
        "# saved markdown",
        null,
      );
    });
    expect(getCurrentContent).not.toHaveBeenCalled();
  });

  it("does not create a finalized version when the pre-finalize flush returns no markdown", async () => {
    const onBeforeFinalizeCurrent = vi.fn(async () => null);

    act(() => {
      root.render(
        <VersionTimeline
          open
          onClose={() => {}}
          notePath="notes/a.md"
          currentContent="# stale state"
          onBeforeFinalizeCurrent={onBeforeFinalizeCurrent}
          onRestore={() => {}}
        />,
      );
    });

    const finalizeLabelInput = document.querySelector("input");
    const finalizeBtn = finalizeLabelInput?.nextElementSibling as
      | HTMLButtonElement
      | undefined;
    expect(finalizeBtn).toBeTruthy();

    await act(async () => {
      finalizeBtn?.click();
      await Promise.resolve();
    });

    await vi.waitFor(() => {
      expect(onBeforeFinalizeCurrent).toHaveBeenCalledTimes(1);
    });
    expect(versionFinalizeCurrent).not.toHaveBeenCalled();
  });
});
