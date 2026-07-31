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
    path: "notes/a.md",
    title: "A 笔记",
    updatedAt: "2026-01-01T00:00:00Z",
    isLocked: false,
  },
  {
    path: "notes/locked.md",
    title: "锁定文档",
    updatedAt: "2026-01-01T00:00:00Z",
    isLocked: true,
  },
  {
    path: "notes/图片.png",
    title: "图片",
    updatedAt: "2026-01-01T00:00:00Z",
    isLocked: false,
  },
];

describe("WorkspaceNavigator", () => {
  beforeEach(() => {
    vi.mocked(workspaceList).mockResolvedValue(
      FILES.map((f) => ({
        attachmentRole: "attachment" as const,
        isLocked: f.isLocked,
        kind: f.path.endsWith(".md") ? "note" : "media",
        mediaKind: null,
        mimeType: null,
        path: f.path,
        sizeBytes: 10,
        title: f.title,
        updatedAt: f.updatedAt,
      })),
    );
    vi.mocked(folderList).mockResolvedValue(["notes/"]);
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
    const result = render(
      <WorkspaceNavigator
        activePath={props.activePath ?? null}
        onOpenDocument={onOpenDocument}
        onPrepareNote={props.onPrepareNote}
        fileLifecycle={lifecycle()}
      />,
    );
    return { ...result, onOpenDocument };
  }

  it("加载 catalog 后渲染统一 folder/file 树并打开文件不关闭导航器", async () => {
    const { onOpenDocument } = renderNavigator();

    await screen.findByTestId("workspace-navigator-tree");
    const folder = screen.getByTestId("workspace-tree-folder");
    expect(folder.textContent).toContain("notes");

    // 文件夹默认收起；展开后出现文件行（排序沿用 buildVaultTree 的 zh-CN 规则）
    fireEvent.click(folder);
    const files = screen.getAllByTestId("workspace-tree-file");
    expect(new Set(files.map((f) => f.textContent))).toEqual(
      new Set(["A 笔记", "锁定文档", "图片"]),
    );

    fireEvent.click(files.find((f) => f.textContent?.includes("A 笔记"))!);
    expect(onOpenDocument).toHaveBeenCalledWith("notes/a.md", "A 笔记");
    // 导航器保持挂载（连续浏览）
    expect(screen.getByTestId("workspace-navigator-tree")).toBeTruthy();
  });

  it("当前文件行显示 brand marker，锁定文档显示锁定图标", async () => {
    renderNavigator({ activePath: "notes/locked.md" });

    await screen.findByTestId("workspace-navigator-tree");
    fireEvent.click(screen.getByTestId("workspace-tree-folder"));

    const locked = screen
      .getAllByTestId("workspace-tree-file")
      .find((f) => f.textContent?.includes("锁定文档"));
    expect(locked?.getAttribute("data-active")).toBe("true");
    // 祖先文件夹自动展开（activePath 的父目录）
    expect(
      screen.getByTestId("workspace-tree-folder").getAttribute("aria-expanded"),
    ).toBe("true");
  });

  it("catalog 加载失败显示可重试的安全错误，不卸载壳层", async () => {
    vi.mocked(workspaceList).mockRejectedValueOnce(new Error("IO error"));
    renderNavigator();

    await screen.findByTestId("workspace-navigator-error");
    expect(screen.getByTestId("workspace-navigator-error").textContent).toBe(
      "IO error",
    );
  });

  it("行菜单提供重命名/移动/锁定/移入回收站，且无批量与永久删除", async () => {
    renderNavigator();
    await screen.findByTestId("workspace-navigator-tree");
    fireEvent.click(screen.getByTestId("workspace-tree-folder"));

    const fileRow = screen
      .getAllByTestId("workspace-tree-file")
      .find((f) => f.textContent?.includes("A 笔记"))!;
    fireEvent.contextMenu(fileRow);

    expect(screen.getByRole("menu", { name: "文件操作" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "重命名" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "移动" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "锁定" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "移入回收站" })).toBeTruthy();
    // 轻量导航不承载批量/永久删除
    expect(screen.queryByText("批量删除")).toBeNull();
    expect(screen.queryByText("永久删除")).toBeNull();
  });

  it("移入回收站需确认，确认后走 fileDelete 与生命周期屏障", async () => {
    renderNavigator();
    await screen.findByTestId("workspace-navigator-tree");
    fireEvent.click(screen.getByTestId("workspace-tree-folder"));
    const fileRow = screen
      .getAllByTestId("workspace-tree-file")
      .find((f) => f.textContent?.includes("A 笔记"))!;
    fireEvent.contextMenu(fileRow);
    fireEvent.click(screen.getByRole("menuitem", { name: "移入回收站" }));

    fireEvent.click(screen.getByRole("button", { name: "移入回收站" }));
    await waitFor(() => expect(fileDelete).toHaveBeenCalledWith("notes/a.md"));
  });

  it("索引降级显示弱警示且不回滚动作", async () => {
    vi.mocked(fileRename).mockResolvedValueOnce({
      entry: { id: 1, path: "x.md", title: "x", updated_at: "", word_count: 0 },
      contentHash: "h",
      indexStatus: "degraded",
    });
    renderNavigator();
    await screen.findByTestId("workspace-navigator-tree");
    fireEvent.click(screen.getByTestId("workspace-tree-folder"));
    const fileRow = screen
      .getAllByTestId("workspace-tree-file")
      .find((f) => f.textContent?.includes("A 笔记"))!;
    fireEvent.contextMenu(fileRow);
    fireEvent.click(screen.getByRole("menuitem", { name: "重命名" }));

    const input = await screen.findByRole("textbox");
    fireEvent.change(input, { target: { value: "b" } });
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(screen.getByText(/索引待修复/)).toBeTruthy());
  });

  it("hover 文件行触发 prepared-note 预热", async () => {
    const onPrepareNote = vi.fn();
    renderNavigator({ onPrepareNote });
    await screen.findByTestId("workspace-navigator-tree");
    fireEvent.click(screen.getByTestId("workspace-tree-folder"));

    const fileRow = screen
      .getAllByTestId("workspace-tree-file")
      .find((f) => f.textContent?.includes("A 笔记"))!;
    fireEvent.mouseEnter(fileRow);

    expect(onPrepareNote).toHaveBeenCalledWith(
      expect.objectContaining({ path: "notes/a.md" }),
    );
  });

  it("点击媒体文件行走同一打开路由（media tab 分流由消费方处理）", async () => {
    const { onOpenDocument } = renderNavigator();
    await screen.findByTestId("workspace-navigator-tree");
    fireEvent.click(screen.getByTestId("workspace-tree-folder"));

    const mediaRow = screen
      .getAllByTestId("workspace-tree-file")
      .find((f) => f.textContent?.includes("图片"))!;
    fireEvent.click(mediaRow);

    expect(onOpenDocument).toHaveBeenCalledWith("notes/图片.png", "图片");
    // 导航器保持打开（连续浏览），媒体不伪装为 Markdown
    expect(screen.getByTestId("workspace-navigator-tree")).toBeTruthy();
  });
});
