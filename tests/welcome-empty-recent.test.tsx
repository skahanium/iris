import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { WelcomeEmpty } from "@/components/layout/WelcomeEmpty";
import type { FileListItem } from "@/types/ipc";

const fileDelete = vi.fn();

vi.mock("@/lib/ipc", () => ({
  fileDelete: (...args: unknown[]) => fileDelete(...args),
}));

describe("WelcomeEmpty recent notes", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    fileDelete.mockReset();
    fileDelete.mockResolvedValue(undefined);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  async function renderWelcome(
    recentNotes: FileListItem[] = [],
    onOpen = vi.fn(),
    onRefreshRecent = vi.fn(),
  ) {
    await act(async () => {
      root.render(
        <WelcomeEmpty
          onNew={vi.fn()}
          onOpen={onOpen}
          onOpenAiManagement={vi.fn()}
          onQuickOpen={vi.fn()}
          onRefreshRecent={onRefreshRecent}
          onSearch={vi.fn()}
          recentNotes={recentNotes}
        />,
      );
    });
  }

  it("renders recent notes as title-only entries", async () => {
    await renderWelcome([
      {
        path: "未命名文档.md",
        title: "中国共产党组织处理规定（试行）",
        updatedAt: "2026-06-16T08:30:00+08:00",
        isLocked: false,
      },
    ]);

    expect(document.body.textContent).toContain(
      "中国共产党组织处理规定（试行）",
    );
    expect(document.body.textContent).not.toContain("06/16 08:30");
    expect(document.body.textContent).not.toContain("未命名文档.md");
    expect(document.body.textContent).not.toContain("最近更新");
  });

  it("prepares recent notes on hover or focus", async () => {
    const onPrepare = vi.fn();
    const recentNotes = [
      {
        path: "ready.md",
        title: "Ready",
        updatedAt: "2026-06-24T00:00:00Z",
        isLocked: false,
      },
    ];

    await act(async () => {
      root.render(
        <WelcomeEmpty
          onNew={vi.fn()}
          onOpen={vi.fn()}
          onOpenAiManagement={vi.fn()}
          onPrepare={onPrepare}
          onQuickOpen={vi.fn()}
          onRefreshRecent={vi.fn()}
          onSearch={vi.fn()}
          recentNotes={recentNotes}
        />,
      );
    });

    document
      .querySelector("li")
      ?.dispatchEvent(new MouseEvent("mouseenter", { bubbles: true }));
    document.querySelector<HTMLButtonElement>("li button")?.focus();

    expect(onPrepare).toHaveBeenCalledWith(
      {
        path: "ready.md",
        title: "Ready",
        updatedAt: "2026-06-24T00:00:00Z",
        isLocked: false,
      },
      "welcome",
    );
  });

  it("passes the recent note display title as an open hint", async () => {
    const onOpen = vi.fn();
    await renderWelcome(
      [
        {
          path: "MiMo.md",
          title: "MiMo-V2.5-Pro-UltraSpeed",
          updatedAt: "2026-06-16T08:30:00+08:00",
          isLocked: false,
        },
      ],
      onOpen,
    );

    document.querySelector<HTMLButtonElement>("li button")?.click();

    expect(onOpen).toHaveBeenCalledWith(
      "MiMo.md",
      "MiMo-V2.5-Pro-UltraSpeed",
      "welcome",
    );
  });

  it("refreshes recent notes after creating or deleting a note", async () => {
    const onRefreshRecent = vi.fn();
    await renderWelcome(
      [
        {
          path: "delete-me.md",
          title: "Delete Me",
          updatedAt: "2026-06-16T08:30:00+08:00",
          isLocked: false,
        },
      ],
      vi.fn(),
      onRefreshRecent,
    );

    await act(async () => {
      document.querySelector<HTMLButtonElement>("button")?.click();
    });
    await vi.waitFor(() => expect(onRefreshRecent).toHaveBeenCalledTimes(1));

    await act(async () => {
      document
        .querySelector<HTMLButtonElement>('button[aria-label="删除 Delete Me"]')
        ?.click();
    });
    await vi.waitFor(() => {
      expect(document.body.textContent).toContain("确定删除");
    });

    await act(async () => {
      Array.from(document.querySelectorAll<HTMLButtonElement>("button"))
        .find((button) => button.textContent?.trim() === "删除")
        ?.click();
    });

    await vi.waitFor(() => {
      expect(fileDelete).toHaveBeenCalledWith("delete-me.md");
      expect(onRefreshRecent).toHaveBeenCalledTimes(2);
    });
  });

  it("keeps recent notes stable when remounted with the same controlled data", async () => {
    const recentNotes = [
      {
        path: "delete-me.md",
        title: "Delete Me",
        updatedAt: "2026-06-16T08:30:00+08:00",
        isLocked: false,
      },
    ];

    await renderWelcome(recentNotes);
    expect(document.body.textContent).toContain("Delete Me");

    await act(async () => {
      root.render(<div data-testid="editor-placeholder" />);
    });

    await renderWelcome(recentNotes);
    expect(document.body.textContent).toContain("Delete Me");
    expect(document.body.textContent).not.toContain("暂无最近笔记");
  });

  it("does not show row-level opening text for pending recent-note opens", async () => {
    await act(async () => {
      root.render(
        <WelcomeEmpty
          onNew={vi.fn()}
          onOpen={vi.fn()}
          onOpenAiManagement={vi.fn()}
          onQuickOpen={vi.fn()}
          onRefreshRecent={vi.fn()}
          onSearch={vi.fn()}
          pendingOpen={{
            kind: "note",
            path: "pending.md",
            sequence: 1,
            startedAt: Date.now(),
            title: "Pending",
          }}
          recentNotes={[
            {
              path: "pending.md",
              title: "Pending",
              updatedAt: "2026-06-24T00:00:00Z",
              isLocked: false,
            },
          ]}
        />,
      );
    });

    expect(document.body.textContent).toContain("Pending");
    expect(document.body.textContent).not.toContain("Opening");
    expect(document.body.textContent).not.toContain("正在打开");
  });
});
