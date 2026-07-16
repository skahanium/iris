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

  it("delegates finalize to the supplied serialized version writer", async () => {
    let resolveFinalize!: (value: null) => void;
    const onFinalizeCurrent = vi.fn(
      () =>
        new Promise<null>((resolve) => {
          resolveFinalize = resolve;
        }),
    );

    act(() => {
      root.render(
        <VersionTimeline
          open
          onClose={() => {}}
          notePath="notes/a.md"
          currentContent="body"
          onRestore={async () => {}}
          onFinalizeCurrent={onFinalizeCurrent}
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
      expect(onFinalizeCurrent).toHaveBeenCalledWith(
        "notes/a.md",
        "body",
        null,
      );
    });
    expect(versionFinalizeCurrent).not.toHaveBeenCalled();

    await act(async () => {
      resolveFinalize(null);
    });

    await vi.waitFor(() => {
      expect(finalizeBtn?.disabled).toBe(false);
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
          onRestore={async () => {}}
          onFinalizeCurrent={(path, content, label) =>
            versionFinalizeCurrent(path, content, label)
          }
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
          onRestore={async () => {}}
          onFinalizeCurrent={(path, content, label) =>
            versionFinalizeCurrent(path, content, label)
          }
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
