import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { QuickOpen } from "@/components/file/QuickOpen";

const workspaceList = vi.fn();

vi.mock("@/lib/ipc", () => ({
  workspaceList: (...args: unknown[]) => workspaceList(...args),
}));

describe("QuickOpen note preparation", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    workspaceList.mockReset();
    workspaceList.mockResolvedValue([
      {
        attachmentRole: "formal",
        kind: "note",
        mediaKind: null,
        mimeType: null,
        path: "notes/a.md",
        sizeBytes: 12,
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

  it("prepares visible results and closes immediately when opening a result", async () => {
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
      expect(document.body.textContent).toContain("a");
    });
    expect(onPrepare).toHaveBeenCalledWith(
      {
        path: "notes/a.md",
        title: "Note A",
        updatedAt: "2026-06-24T00:00:00Z",
        isLocked: false,
      },
      "quick-open",
    );

    const option = Array.from(document.querySelectorAll("button")).find((b) =>
      b.textContent?.includes("a"),
    );
    expect(option).toBeTruthy();
    await act(async () => {
      option?.click();
      await Promise.resolve();
    });

    expect(onSelect).toHaveBeenCalledWith("notes/a.md", "quick-open");
    expect(onClose).toHaveBeenCalledOnce();

    await act(async () => {
      resolveOpen();
      await Promise.resolve();
    });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("limits speculative note preparation to the first few candidates", async () => {
    workspaceList.mockResolvedValue(
      Array.from({ length: 8 }, (_, index) => ({
        attachmentRole: "formal",
        kind: "note",
        mediaKind: null,
        mimeType: null,
        path: `notes/${index}.md`,
        sizeBytes: 12,
        title: `Note ${index}`,
        updatedAt: "2026-06-24T00:00:00Z",
        isLocked: false,
      })),
    );
    const onPrepare = vi.fn();

    await act(async () => {
      root.render(
        <QuickOpen
          open
          onClose={vi.fn()}
          onPrepare={onPrepare}
          onSelect={vi.fn()}
        />,
      );
    });

    await vi.waitFor(() => {
      expect(document.body.textContent).toContain("0");
    });

    expect(onPrepare).toHaveBeenCalledTimes(3);
    expect(onPrepare.mock.calls.map(([file]) => file.path)).toEqual([
      "notes/0.md",
      "notes/1.md",
      "notes/2.md",
    ]);
  });

  it("opens media results without note preparation", async () => {
    workspaceList.mockResolvedValue([
      {
        attachmentRole: "attachment",
        isLocked: false,
        kind: "media",
        mediaKind: "pdf",
        mimeType: "application/pdf",
        path: "assets/paper.pdf",
        sizeBytes: 10,
        title: "paper",
        updatedAt: "2026-06-24T00:00:00Z",
      },
    ]);
    const onPrepare = vi.fn();
    const onSelect = vi.fn(async () => undefined);
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
      expect(document.body.textContent).toContain("paper");
    });
    expect(onPrepare).not.toHaveBeenCalled();

    const option = Array.from(document.querySelectorAll("button")).find((b) =>
      b.textContent?.includes("paper"),
    );
    await act(async () => {
      option?.click();
      await Promise.resolve();
    });

    expect(onSelect).toHaveBeenCalledWith("assets/paper.pdf", "quick-open");
  });
});
