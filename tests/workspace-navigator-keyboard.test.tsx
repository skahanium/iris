import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { WorkspaceNavigatorTree } from "@/components/file/WorkspaceNavigatorTree";
import { buildFolderTree } from "@/lib/vault-tree";

function treeFixture() {
  return buildFolderTree(
    [
      { path: "notes/a.md", title: "A", updatedAt: "", isLocked: false },
      {
        path: "notes/sub/b.md",
        title: "B",
        updatedAt: "",
        isLocked: false,
      },
      {
        path: "notes/other/c.md",
        title: "C",
        updatedAt: "",
        isLocked: false,
      },
      { path: "z.md", title: "Z", updatedAt: "", isLocked: false },
    ],
    ["notes/", "z/"],
    () => true,
  );
}

function renderTree(
  props: {
    expanded?: string[];
    selectedFolder?: string;
  } = {},
) {
  const expanded = new Set(props.expanded ?? []);
  const onToggleFolder = vi.fn();
  const onSelectFolder = vi.fn();
  const onRowMenu = vi.fn();
  render(
    <WorkspaceNavigatorTree
      tree={treeFixture()}
      expanded={expanded}
      selectedFolder={props.selectedFolder ?? ""}
      onToggleFolder={onToggleFolder}
      onSelectFolder={onSelectFolder}
      onRowMenu={onRowMenu}
    />,
  );
  return { expanded, onToggleFolder, onSelectFolder, onRowMenu };
}

describe("WorkspaceNavigatorTree 文件夹键盘语义", () => {
  afterEach(() => {
    cleanup();
  });

  it("上层仅渲染文件夹和直属 Markdown 数量", () => {
    renderTree({ expanded: ["notes/"] });

    const tree = screen.getByRole("tree", { name: "文件夹" });
    expect(tree).toBeTruthy();
    expect(screen.getByTestId("workspace-tree-root").textContent).toContain(
      "根目录",
    );
    expect(screen.getAllByTestId("workspace-tree-folder")).toHaveLength(4);
    expect(screen.queryByText("A")).toBeNull();
  });

  it("目录名称只选择，箭头独立展开或收起", () => {
    const { onSelectFolder, onToggleFolder } = renderTree();
    const folder = screen.getByRole("treeitem", { name: /notes/ });

    fireEvent.click(folder);
    expect(onSelectFolder).toHaveBeenCalledWith("notes/");
    expect(onToggleFolder).not.toHaveBeenCalled();

    fireEvent.click(folder.querySelector("button")!);
    expect(onToggleFolder).toHaveBeenCalledWith("notes/");
  });

  it("嵌套目录显示连续导轨和展开图标", () => {
    renderTree({ expanded: ["notes/", "notes/sub/"] });

    expect(
      screen.getAllByTestId("workspace-tree-guide").length,
    ).toBeGreaterThan(0);
    const notes = screen.getByRole("treeitem", { name: /notes/ });
    expect(notes.querySelector("[data-icon='folder-open']")).not.toBeNull();
  });

  it("←/→ 控制展开，Enter 选择目录", () => {
    const { onSelectFolder, onToggleFolder } = renderTree();
    const tree = screen.getByRole("tree");

    fireEvent.keyDown(tree, { key: "ArrowDown" });
    fireEvent.keyDown(tree, { key: "ArrowRight" });
    expect(onToggleFolder).toHaveBeenCalledWith("notes/");

    fireEvent.keyDown(tree, { key: "Enter" });
    expect(onSelectFolder).toHaveBeenCalledWith("notes/");

    cleanup();
    const expandedTree = renderTree({ expanded: ["notes/"] });
    fireEvent.keyDown(screen.getByRole("tree"), { key: "ArrowDown" });
    fireEvent.keyDown(screen.getByRole("tree"), { key: "ArrowLeft" });
    expect(expandedTree.onToggleFolder).toHaveBeenCalledWith("notes/");
  });

  it("选中目录只使用弱 brand tint 与细 marker", () => {
    renderTree({ selectedFolder: "notes/" });

    const active = screen.getByRole("treeitem", { name: /notes/ });
    expect(active.getAttribute("data-selected")).toBe("true");
    expect(active.className).toContain("before:bg-[hsl(var(--brand))]");
    expect(active.className).not.toContain("border-2");
  });

  it("Shift+F10 与菜单键打开当前文件夹操作菜单", () => {
    const { onRowMenu } = renderTree();
    const tree = screen.getByRole("tree");

    fireEvent.keyDown(tree, { key: "ArrowDown" });
    fireEvent.keyDown(tree, { key: "F10", shiftKey: true });
    expect(onRowMenu).toHaveBeenCalledWith(
      expect.objectContaining({ path: "notes/" }),
      expect.any(Number),
    );

    fireEvent.keyDown(tree, { key: "ContextMenu" });
    expect(onRowMenu).toHaveBeenCalledTimes(2);
  });
});
