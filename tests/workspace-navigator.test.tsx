import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { WorkspaceNavigator } from "@/components/file/WorkspaceNavigator";
import type { WorkspaceNavigatorFileLifecycle } from "@/components/file/WorkspaceNavigator";
import {
  corpusList,
  fileDelete,
  fileRename,
  fileSetLock,
  folderCreate,
  folderList,
  folderRename,
  listenFileChanged,
  workspaceList,
} from "@/lib/ipc";

vi.mock("@/lib/ipc", () => ({
  workspaceList: vi.fn(),
  folderList: vi.fn(),
  corpusList: vi.fn(),
  listenFileChanged: vi.fn(),
  fileRename: vi.fn(),
  fileSetLock: vi.fn(),
  fileDelete: vi.fn(),
  folderCreate: vi.fn(),
  folderRename: vi.fn(),
}));

function lifecycle(): WorkspaceNavigatorFileLifecycle {
  return {
    handleBeforeFilePathChange: vi.fn(async () => undefined),
    handleFilePathChanged: vi.fn(),
    handleFilePathChangeFailed: vi.fn(),
    handleBeforeFileDelete: vi.fn(async () => undefined),
    handleFileDeleted: vi.fn(),
    handleBeforeFileLock: vi.fn(async () => undefined),
  };
}

const FILES = [
  {
    path: "root.md",
    title: "根笔记",
    updatedAt: "2026-01-01T00:00:00Z",
    isLocked: false,
  },
  {
    path: "notes/a.md",
    title: "A 笔记",
    updatedAt: "2026-01-02T00:00:00Z",
    isLocked: false,
  },
  {
    path: "notes/locked.md",
    title: "锁定文档",
    updatedAt: "2026-01-03T00:00:00Z",
    isLocked: true,
  },
  {
    path: "notes/图片.png",
    title: "图片",
    updatedAt: "2026-01-04T00:00:00Z",
    isLocked: false,
  },
  {
    path: "notes/sub/b.md",
    title: "后代文档",
    updatedAt: "2026-01-05T00:00:00Z",
    isLocked: false,
  },
];

describe("WorkspaceNavigator", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.mocked(workspaceList).mockResolvedValue(
      FILES.map((file) => ({
        attachmentRole: "attachment" as const,
        isLocked: file.isLocked,
        kind: file.path.endsWith(".md") ? "note" : "media",
        mediaKind: file.path.endsWith(".png") ? "image" : null,
        mimeType: null,
        path: file.path,
        sizeBytes: 10,
        title: file.title,
        updatedAt: file.updatedAt,
      })),
    );
    vi.mocked(folderList).mockResolvedValue(["notes/", "notes/sub/"]);
    vi.mocked(corpusList).mockResolvedValue([]);
    vi.mocked(listenFileChanged).mockResolvedValue(() => undefined);
    vi.mocked(fileRename).mockResolvedValue({
      entry: { id: 1, path: "x.md", title: "x", updated_at: "", word_count: 0 },
      contentHash: "h",
      indexStatus: "synced",
    });
    vi.mocked(fileSetLock).mockResolvedValue(undefined);
    vi.mocked(fileDelete).mockResolvedValue(undefined);
    vi.mocked(folderCreate).mockResolvedValue(undefined);
    vi.mocked(folderRename).mockResolvedValue("synced");
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  function renderNavigator(
    props: {
      activePath?: string | null;
      onOpenDocument?: (path: string) => void;
      onPrepareNote?: () => void;
    } = {},
  ) {
    const onOpenDocument = props.onOpenDocument ?? vi.fn();
    const navigator = (
      <WorkspaceNavigator
        activePath={props.activePath ?? null}
        onOpenDocument={onOpenDocument}
        onPrepareNote={props.onPrepareNote}
        fileLifecycle={lifecycle()}
      />
    );
    const result = render(navigator);
    return { ...result, onOpenDocument };
  }

  it("分层显示：上层只有文件夹，下层只显示根目录直属 Markdown", async () => {
    renderNavigator();

    await screen.findByTestId("workspace-navigator-tree");
    expect(screen.getByTestId("workspace-tree-root").textContent).toContain(
      "根目录",
    );
    expect(screen.getAllByTestId("workspace-tree-folder")).toHaveLength(1);
    expect(screen.queryByText("A 笔记")).toBeNull();

    const files = await screen.findByTestId("workspace-navigator-file-list");
    expect(files.textContent).toContain("根笔记");
    expect(files.textContent).not.toContain("A 笔记");
  });

  it("选择文件夹后下层只展示该目录直属文件，不递归显示后代", async () => {
    const { onOpenDocument } = renderNavigator();
    await screen.findByTestId("workspace-navigator-tree");

    fireEvent.click(screen.getByRole("treeitem", { name: /notes/ }));
    const list = screen.getByTestId("workspace-navigator-file-list");
    expect(list.textContent).toContain("A 笔记");
    expect(list.textContent).toContain("锁定文档");
    expect(list.textContent).not.toContain("后代文档");

    fireEvent.click(screen.getByRole("listitem", { name: /A 笔记/ }));
    expect(onOpenDocument).toHaveBeenCalledWith("notes/a.md", "A 笔记");
  });

  it("外部活动文档自动选择并展开其父目录", async () => {
    renderNavigator({ activePath: "notes/sub/b.md" });

    await screen.findByTestId("workspace-navigator-tree");
    await waitFor(() =>
      expect(screen.getByText("sub", { selector: "span" })).toBeTruthy(),
    );
    expect(
      screen
        .getByRole("treeitem", { name: /notes/ })
        .getAttribute("aria-expanded"),
    ).toBe("true");
    expect(
      screen.getByTestId("workspace-navigator-file-list").textContent,
    ).toContain("后代文档");
  });

  it("媒体开关、搜索和 Escape 仅影响当前文件夹下层列表", async () => {
    renderNavigator();
    await screen.findByTestId("workspace-navigator-tree");
    fireEvent.click(screen.getByRole("treeitem", { name: /notes/ }));

    expect(screen.queryByText("图片")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "显示直属媒体文件" }));
    expect(screen.getByText("图片")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "搜索当前文件夹" }));
    const search = screen.getByRole("textbox", { name: "搜索当前文件夹文件" });
    fireEvent.change(search, { target: { value: "锁定" } });
    expect(screen.getByText("锁定文档")).toBeTruthy();
    expect(screen.queryByText("A 笔记")).toBeNull();
    fireEvent.keyDown(search, { key: "Escape" });
    expect(
      screen.queryByRole("textbox", { name: "搜索当前文件夹文件" }),
    ).toBeNull();
  });

  it("两层工具栏使用当前目录创建，支持排序和全部展开/折叠", async () => {
    renderNavigator();
    await screen.findByTestId("workspace-navigator-tree");
    fireEvent.click(screen.getByRole("treeitem", { name: /notes/ }));

    expect(
      screen.getByRole("button", { name: "在当前文件夹新建文件夹" }),
    ).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "在当前文件夹新建笔记" }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "展开全部文件夹" }));
    expect(screen.getByRole("treeitem", { name: /sub/ })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "折叠全部文件夹" }));
    expect(screen.queryByRole("treeitem", { name: /sub/ })).toBeNull();

    fireEvent.click(
      screen.getByRole("button", { name: "在当前文件夹新建文件夹" }),
    );
    fireEvent.change(screen.getByRole("textbox", { name: "文件夹名称" }), {
      target: { value: "收件箱" },
    });
    fireEvent.click(screen.getByRole("button", { name: "创建文件夹" }));
    await waitFor(() =>
      expect(folderCreate).toHaveBeenCalledWith("notes/收件箱"),
    );
  });

  it("分隔线可通过键盘调整、双击复位并持久化无路径偏好", async () => {
    renderNavigator();
    await screen.findByTestId("workspace-navigator-tree");
    const separator = screen.getByRole("separator", {
      name: "调整文件夹与文件列表比例",
    });
    expect(separator.getAttribute("aria-valuenow")).toBe("45");
    fireEvent.keyDown(separator, { key: "ArrowDown" });
    expect(separator.getAttribute("aria-valuenow")).toBe("50");
    fireEvent.doubleClick(separator);
    expect(separator.getAttribute("aria-valuenow")).toBe("45");
    expect(
      localStorage.getItem("iris.workspaceNavigator.preferences"),
    ).not.toContain("notes/");
  });

  it("标题行只保留笔记库名称，不显示图钉或快捷键提示", async () => {
    renderNavigator();
    await screen.findByTestId("workspace-navigator-tree");

    expect(screen.getByText("笔记库")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /固定笔记库导航/ })).toBeNull();
    expect(screen.queryByText("Ctrl/Cmd+\\")).toBeNull();
  });

  it("文件右键菜单在触发位置打开，并保留重命名、锁定和回收站生命周期", async () => {
    renderNavigator();
    await screen.findByTestId("workspace-navigator-tree");
    fireEvent.click(screen.getByRole("treeitem", { name: /notes/ }));
    const fileRow = screen.getByRole("listitem", { name: /A 笔记/ });
    expect(fileRow.className).toContain("select-none");
    fireEvent.contextMenu(fileRow, {
      clientX: 312,
      clientY: 196,
    });

    const menu = screen.getByRole("menu", { name: "文件操作" });
    expect(menu.parentElement?.style.left).toBe("312px");
    expect(menu.parentElement?.style.top).toBe("196px");
    expect(screen.getByRole("menuitem", { name: "锁定" })).toBeTruthy();
    fireEvent.click(screen.getByRole("menuitem", { name: "移入回收站" }));
    fireEvent.click(screen.getByRole("button", { name: "移入回收站" }));
    await waitFor(() => expect(fileDelete).toHaveBeenCalledWith("notes/a.md"));
  });
});
