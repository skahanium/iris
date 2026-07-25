import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { WorkspaceEmpty } from "@/components/layout/WorkspaceEmpty";
import type { FileListItem } from "@/types/ipc";

const fileRead = vi.hoisted(() => vi.fn());

vi.mock("@/lib/ipc", () => ({
  fileRead: (...args: unknown[]) => fileRead(...args),
}));

function note(
  path: string,
  title: string,
  opts?: { isLocked?: boolean },
): FileListItem {
  return {
    path,
    title,
    updatedAt: new Date(Date.now() - 86_400_000).toISOString(),
    isLocked: opts?.isLocked ?? false,
  };
}

describe("WorkspaceEmpty", () => {
  it("vault mode shows muted empty hint and create-first CTA without hero heading", async () => {
    const onNew = vi.fn();
    render(<WorkspaceEmpty mode="vault" onNew={onNew} />);
    expect(screen.getByTestId("workspace-empty")).toHaveAttribute(
      "data-mode",
      "vault",
    );
    expect(screen.queryByText("开始写作")).toBeNull();
    expect(screen.queryByText("继续写作")).toBeNull();
    expect(screen.getByText("还没有笔记")).toBeTruthy();
    expect(screen.queryByTestId("workspace-empty-recent-grid")).toBeNull();
    await userEvent.click(screen.getByTestId("workspace-empty-new"));
    expect(onNew).toHaveBeenCalled();
  });

  it("workspace mode loads body excerpts onto recent cards", async () => {
    fileRead.mockReset();
    fileRead.mockImplementation(async (path: string) => {
      if (path === "notes/a.md") {
        return {
          content: "---\ntags: [x]\n---\n\n这是正文预览第一句，足够长。",
          isLocked: false,
        };
      }
      return { content: "# Root\n\n根目录笔记正文。", isLocked: false };
    });

    const onOpenNote = vi.fn();
    const files = [
      note("notes/a.md", "A"),
      note("root-note.md", "Root Note Title"),
    ];
    render(
      <WorkspaceEmpty
        mode="workspace"
        onNew={vi.fn()}
        recentNotes={files}
        onOpenNote={onOpenNote}
        onOpenSearch={vi.fn()}
      />,
    );
    expect(screen.queryByText("继续写作")).toBeNull();
    expect(screen.getByTestId("workspace-empty-search")).toBeTruthy();
    expect(screen.queryByTestId("workspace-empty-recent-folder")).toBeNull();

    await waitFor(() => {
      const excerpts = screen.getAllByTestId("workspace-empty-recent-excerpt");
      expect(excerpts.some((el) => el.textContent?.includes("正文预览"))).toBe(
        true,
      );
      expect(
        excerpts.some((el) => el.textContent?.includes("根目录笔记")),
      ).toBe(true);
    });

    const cards = screen.getAllByTestId("workspace-empty-recent-card");
    expect(cards).toHaveLength(2);
    await userEvent.click(cards[0]!);
    expect(onOpenNote).toHaveBeenCalledWith(files[0]);
  });
});
