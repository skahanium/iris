import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { QuickOpen } from "@/components/file/QuickOpen";

const fileList = vi.fn();

vi.mock("@/lib/ipc", () => ({
  fileList: (...args: unknown[]) => fileList(...args),
}));

describe("QuickOpen note preparation", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    fileList.mockReset();
    fileList.mockResolvedValue([
      {
        path: "notes/a.md",
        title: "Note A",
        updatedAt: "2026-06-24T00:00:00Z",
        isLocked: false,
      },
    ]);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("prepares visible results and closes only after async open resolves", async () => {
    const onPrepare = vi.fn();
    let resolveOpen!: () => void;
    const onSelect = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveOpen = resolve;
        }),
    );
    const onClose = vi.fn();

    await act(async () => {
      root.render(
        <QuickOpen
          open
          onClose={onClose}
          onPrepare={onPrepare}
          onSelect={onSelect}
        />,
      );
    });

    await vi.waitFor(() => {
      expect(document.body.textContent).toContain("Note A");
    });
    expect(onPrepare).toHaveBeenCalledWith({
      path: "notes/a.md",
      title: "Note A",
      updatedAt: "2026-06-24T00:00:00Z",
      isLocked: false,
    });

    const option = Array.from(document.querySelectorAll("button")).find((b) =>
      b.textContent?.includes("Note A"),
    );
    expect(option).toBeTruthy();
    await act(async () => {
      option?.click();
      await Promise.resolve();
    });

    expect(onSelect).toHaveBeenCalledWith("notes/a.md");
    expect(onClose).not.toHaveBeenCalled();

    await act(async () => {
      resolveOpen();
      await Promise.resolve();
    });
    expect(onClose).toHaveBeenCalledOnce();
  });
});
