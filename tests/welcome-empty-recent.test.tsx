import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { WelcomeEmpty } from "@/components/layout/WelcomeEmpty";

const fileDelete = vi.fn();
const fileList = vi.fn();

vi.mock("@/lib/ipc", () => ({
  fileDelete: (...args: unknown[]) => fileDelete(...args),
  fileList: (...args: unknown[]) => fileList(...args),
}));

describe("WelcomeEmpty recent notes", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    fileDelete.mockReset();
    fileList.mockReset();
    fileDelete.mockResolvedValue(undefined);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  async function renderWelcome() {
    await act(async () => {
      root.render(
        <WelcomeEmpty
          onNew={vi.fn()}
          onOpen={vi.fn()}
          onOpenAiManagement={vi.fn()}
          onQuickOpen={vi.fn()}
          onSearch={vi.fn()}
        />,
      );
    });
  }

  it("renders recent notes as title-only entries", async () => {
    fileList.mockResolvedValue([
      {
        path: "未命名文档.md",
        title: "中国共产党组织处理规定（试行）",
        updatedAt: "2026-06-16T08:30:00+08:00",
        isLocked: false,
      },
    ]);

    await renderWelcome();

    await vi.waitFor(() => {
      expect(document.body.textContent).toContain(
        "中国共产党组织处理规定（试行）",
      );
    });
    expect(document.body.textContent).not.toContain("06/16 08:30");
    expect(document.body.textContent).not.toContain("未命名文档.md");
    expect(document.body.textContent).not.toContain("最近更新");
  });
});
