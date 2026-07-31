import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { WorkspaceNavigatorTree } from "@/components/file/WorkspaceNavigatorTree";
import { buildVaultTree } from "@/lib/vault-tree";

function treeFixture() {
  return buildVaultTree(
    [
      { path: "notes/a.md", title: "A", updatedAt: "", isLocked: false },
      { path: "notes/sub/b.md", title: "B", updatedAt: "", isLocked: false },
      { path: "z.md", title: "Z", updatedAt: "", isLocked: false },
    ],
    ["notes/"],
  );
}

function renderTree(
  props: {
    expanded?: string[];
    activePath?: string | null;
  } = {},
) {
  const expanded = new Set(props.expanded ?? []);
  const onToggleFolder = vi.fn();
  const onOpenFile = vi.fn();
  const onPrepareFile = vi.fn();
  const onRowMenu = vi.fn();
  render(
    <WorkspaceNavigatorTree
      tree={treeFixture()}
      expanded={expanded}
      activePath={props.activePath ?? null}
      onToggleFolder={onToggleFolder}
      onOpenFile={onOpenFile}
      onPrepareFile={onPrepareFile}
      onRowMenu={onRowMenu}
    />,
  );
  return { expanded, onToggleFolder, onOpenFile, onPrepareFile, onRowMenu };
}

describe("WorkspaceNavigatorTree 键盘语义", () => {
  afterEach(() => {
    cleanup();
  });

  it("暴露 tree/treeitem 层级与文件夹 aria-expanded", () => {
    renderTree({ expanded: ["notes/"] });

    const tree = screen.getByRole("tree", { name: "笔记库导航" });
    expect(tree).toBeTruthy();
    const folder = screen.getByRole("treeitem", { name: /notes/ });
    expect(folder.getAttribute("aria-expanded")).toBe("true");
    const file = screen.getByRole("treeitem", { name: "A" });
    expect(file.getAttribute("aria-level")).toBe("2");
  });

  it("↑/↓ 在可见行间移动焦点（aria-activedescendant 语义由焦点行 ring 表达）", () => {
    const { onOpenFile } = renderTree({ expanded: ["notes/"] });
    const tree = screen.getByRole("tree");

    fireEvent.keyDown(tree, { key: "ArrowDown" });
    fireEvent.keyDown(tree, { key: "ArrowDown" });
    fireEvent.keyDown(tree, { key: "ArrowDown" });
    // 第 4 行（z.md）应获得焦点样式
    const zRow = screen.getByRole("treeitem", { name: "Z" });
    expect(zRow.className).toContain("ring-1");

    fireEvent.keyDown(tree, { key: "Enter" });
    expect(onOpenFile).toHaveBeenCalledWith(
      expect.objectContaining({ path: "z.md" }),
    );
  });

  it("← 折叠文件夹；→ 展开文件夹；Enter 切换文件夹", () => {
    const { onToggleFolder } = renderTree({ expanded: ["notes/"] });
    const tree = screen.getByRole("tree");

    fireEvent.keyDown(tree, { key: "ArrowDown" }); // notes/（已展开）
    fireEvent.keyDown(tree, { key: "ArrowRight" }); // → 进第一个子行
    const sub = screen.getByRole("treeitem", { name: /sub/ });
    expect(sub.className).toContain("ring-1");

    fireEvent.keyDown(tree, { key: "ArrowLeft" }); // ← 折叠 sub/
    expect(onToggleFolder).toHaveBeenCalledWith("notes/sub/");

    fireEvent.keyDown(tree, { key: "ArrowLeft" }); // ← 折叠 notes/
    expect(onToggleFolder).toHaveBeenCalledWith("notes/");

    // Enter 在文件夹上切换展开
    fireEvent.keyDown(tree, { key: "Enter" });
    expect(onToggleFolder).toHaveBeenCalledWith("notes/");
  });

  it("Shift+F10 与菜单键打开当前行操作菜单", () => {
    const { onRowMenu } = renderTree({ expanded: ["notes/"] });
    const tree = screen.getByRole("tree");

    fireEvent.keyDown(tree, { key: "F10", shiftKey: true });
    expect(onRowMenu).toHaveBeenCalledWith(
      expect.objectContaining({ path: "notes/" }),
      expect.any(Number),
    );

    fireEvent.keyDown(tree, { key: "ContextMenu" });
    expect(onRowMenu).toHaveBeenCalledTimes(2);
  });

  it("当前文件行自动显露：请求展开祖先并标记激活行", () => {
    const { onToggleFolder } = renderTree({ activePath: "notes/sub/b.md" });

    // 祖先缺失时向父级请求展开（实际展开由父级状态决定）
    expect(onToggleFolder).toHaveBeenCalledWith("notes/");
    expect(onToggleFolder).toHaveBeenCalledWith("notes/sub/");

    // 展开集合就绪后：祖先 aria-expanded 与激活行 brand marker 生效
    cleanup();
    renderTree({
      expanded: ["notes/", "notes/sub/"],
      activePath: "notes/sub/b.md",
    });
    const folder = screen.getByRole("treeitem", { name: /notes/ });
    const sub = screen.getByRole("treeitem", { name: /sub/ });
    expect(folder.getAttribute("aria-expanded")).toBe("true");
    expect(sub.getAttribute("aria-expanded")).toBe("true");
    const bRow = screen.getByRole("treeitem", { name: "B" });
    expect(bRow.getAttribute("data-active")).toBe("true");
  });
});
