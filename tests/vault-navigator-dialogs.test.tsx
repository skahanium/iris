import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { RenameItemDialog } from "@/components/file/VaultNavigatorDialogs";

describe("RenameItemDialog", () => {
  it("重命名文档时只显示名称输入，不显示所在位置或新路径预览", () => {
    render(
      <RenameItemDialog
        target={{
          kind: "file",
          file: {
            isLocked: false,
            path: "大模型/MiniMax M3.md",
            title: "MiniMax M3",
            updatedAt: "2026-08-01T00:00:00Z",
          },
        }}
        onCancel={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );

    expect(screen.getByRole("textbox", { name: "文档名称" })).toBeTruthy();
    expect(screen.queryByText("所在位置")).toBeNull();
    expect(screen.queryByText("新路径")).toBeNull();
    expect(screen.queryByText("输入名称后预览")).toBeNull();
  });
});
