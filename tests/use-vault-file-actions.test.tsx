import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  useVaultFileActions,
  type VaultFileActionCallbacks,
} from "@/hooks/useVaultFileActions";
import {
  fileDelete,
  fileRename,
  fileSetLock,
  folderCreate,
  folderRename,
} from "@/lib/ipc";
import { createDefaultNote } from "@/lib/note-create";
import { prepareNoteOpenFromContent } from "@/lib/note-open-preparation";
import type { FileListItem } from "@/types/ipc";

vi.mock("@/lib/ipc", () => ({
  fileRename: vi.fn(),
  fileSetLock: vi.fn(),
  fileDelete: vi.fn(),
  folderCreate: vi.fn(),
  folderRename: vi.fn(),
}));
vi.mock("@/lib/note-create", () => ({ createDefaultNote: vi.fn() }));
vi.mock("@/lib/note-open-preparation", () => ({
  prepareNoteOpenFromContent: vi.fn(),
}));

type HookApi = ReturnType<typeof useVaultFileActions>;

function Harness({
  apiRef,
  callbacks,
}: {
  apiRef: { current: HookApi | null };
  callbacks: VaultFileActionCallbacks;
}) {
  apiRef.current = useVaultFileActions(callbacks);
  return null;
}

const NOTE_A: FileListItem = {
  path: "policy/a.md",
  title: "A",
  updatedAt: "2026-01-01T00:00:00Z",
  isLocked: false,
};

const NOTE_B: FileListItem = {
  path: "policy/b.md",
  title: "B",
  updatedAt: "2026-01-01T00:00:00Z",
  isLocked: false,
};

describe("useVaultFileActions", () => {
  let host: HTMLDivElement;
  let root: Root;
  let apiRef: { current: HookApi | null };
  let callbacks: VaultFileActionCallbacks;
  let refresh: () => void;

  beforeEach(() => {
    apiRef = { current: null };
    refresh = vi.fn();
    callbacks = {
      onOpen: vi.fn() as VaultFileActionCallbacks["onOpen"],
      onBeforeFilePathChange:
        vi.fn() as VaultFileActionCallbacks["onBeforeFilePathChange"],
      onFilePathChanged:
        vi.fn() as VaultFileActionCallbacks["onFilePathChanged"],
      onFilePathChangeFailed:
        vi.fn() as VaultFileActionCallbacks["onFilePathChangeFailed"],
      onBeforeFileDelete:
        vi.fn() as VaultFileActionCallbacks["onBeforeFileDelete"],
      onFileDeleted: vi.fn() as VaultFileActionCallbacks["onFileDeleted"],
      onBeforeFileLock: vi.fn() as VaultFileActionCallbacks["onBeforeFileLock"],
      onFileLockChanged:
        vi.fn() as VaultFileActionCallbacks["onFileLockChanged"],
      onIndexDegraded: vi.fn() as VaultFileActionCallbacks["onIndexDegraded"],
      onIndexChange: vi.fn() as VaultFileActionCallbacks["onIndexChange"],
      refresh,
    };
    vi.mocked(fileRename).mockResolvedValue({
      entry: {
        id: 1,
        path: "policy/b.md",
        title: "B",
        updated_at: "",
        word_count: 0,
      },
      contentHash: "h",
      indexStatus: "synced",
    });
    vi.mocked(fileSetLock).mockResolvedValue(undefined);
    vi.mocked(fileDelete).mockResolvedValue(undefined);
    vi.mocked(folderCreate).mockResolvedValue(undefined);
    vi.mocked(folderRename).mockResolvedValue("synced");

    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.clearAllMocks();
  });

  function renderHook() {
    act(() => {
      root.render(createElement(Harness, { apiRef, callbacks }));
    });
  }

  it("重命名文件：before → fileRename → 回执 → changed → refresh", async () => {
    renderHook();
    await act(async () => {
      await apiRef.current?.rename({ kind: "file", file: NOTE_A }, "b", {
        files: [NOTE_A],
        fileTitle: (f) => f.title,
      });
    });

    expect(callbacks.onBeforeFilePathChange).toHaveBeenCalledWith(
      "policy/a.md",
      "policy/b.md",
    );
    expect(fileRename).toHaveBeenCalledWith("policy/a.md", "policy/b.md");
    expect(callbacks.onFilePathChanged).toHaveBeenCalledWith(
      "policy/a.md",
      "policy/b.md",
      "b",
    );
    expect(callbacks.onIndexChange).toHaveBeenCalled();
    expect(refresh).toHaveBeenCalled();
    expect(apiRef.current?.error).toBeNull();
  });

  it("索引降级回执触发 onIndexDegraded，但动作视为成功", async () => {
    vi.mocked(fileRename).mockResolvedValueOnce({
      entry: {
        id: 1,
        path: "policy/b.md",
        title: "B",
        updated_at: "",
        word_count: 0,
      },
      contentHash: "h",
      indexStatus: "degraded",
    });
    renderHook();
    await act(async () => {
      await apiRef.current?.rename({ kind: "file", file: NOTE_A }, "b", {
        files: [NOTE_A],
        fileTitle: (f) => f.title,
      });
    });

    expect(callbacks.onIndexDegraded).toHaveBeenCalledTimes(1);
    expect(apiRef.current?.error).toBeNull();
    expect(refresh).toHaveBeenCalled();
  });

  it("重命名失败：对已开始迁移的路径逐个回执 failed，并设置错误", async () => {
    vi.mocked(fileRename).mockRejectedValueOnce(new Error("IO error"));
    renderHook();
    await act(async () => {
      await apiRef.current?.rename({ kind: "file", file: NOTE_A }, "b", {
        files: [NOTE_A],
        fileTitle: (f) => f.title,
      });
    });

    expect(callbacks.onFilePathChangeFailed).toHaveBeenCalledWith(
      "policy/a.md",
    );
    expect(apiRef.current?.error).toBe("IO error");
  });

  it("重命名文件夹：remap 前缀、folderRename、逐文件 changed", async () => {
    renderHook();
    await act(async () => {
      await apiRef.current?.rename(
        { kind: "folder", path: "policy/" },
        "archive",
        {
          files: [NOTE_A, NOTE_B],
          fileTitle: (f) => f.title,
        },
      );
    });

    expect(callbacks.onBeforeFilePathChange).toHaveBeenCalledTimes(2);
    expect(callbacks.onBeforeFilePathChange).toHaveBeenNthCalledWith(
      1,
      "policy/a.md",
      "archive/a.md",
    );
    expect(folderRename).toHaveBeenCalledWith("policy/", "archive");
    expect(callbacks.onFilePathChanged).toHaveBeenNthCalledWith(
      2,
      "policy/b.md",
      "archive/b.md",
      "B",
    );
    expect(refresh).toHaveBeenCalled();
  });

  it("移动文件：防重名分配目标并走完整链", async () => {
    renderHook();
    await act(async () => {
      await apiRef.current?.move({ kind: "file", file: NOTE_A }, "archive/", {
        files: [NOTE_A, NOTE_B],
        fileTitle: (f) => f.title,
      });
    });

    expect(fileRename).toHaveBeenCalledWith("policy/a.md", "archive/a.md");
    expect(callbacks.onFilePathChanged).toHaveBeenCalledWith(
      "policy/a.md",
      "archive/a.md",
      "A",
    );
  });

  it("锁定：before → fileSetLock → changed → refresh", async () => {
    renderHook();
    await act(async () => {
      await apiRef.current?.setLock("policy/a.md", true);
    });

    expect(callbacks.onBeforeFileLock).toHaveBeenCalledWith("policy/a.md");
    expect(fileSetLock).toHaveBeenCalledWith("policy/a.md", true);
    expect(callbacks.onFileLockChanged).toHaveBeenCalledWith(
      "policy/a.md",
      true,
    );
    expect(refresh).toHaveBeenCalled();
  });

  it("移入回收站：before → fileDelete → deleted → refresh", async () => {
    renderHook();
    await act(async () => {
      await apiRef.current?.deleteToRecycleBin("policy/a.md");
    });

    expect(callbacks.onBeforeFileDelete).toHaveBeenCalledWith("policy/a.md");
    expect(fileDelete).toHaveBeenCalledWith("policy/a.md");
    expect(callbacks.onFileDeleted).toHaveBeenCalledWith("policy/a.md");
    expect(callbacks.onIndexChange).toHaveBeenCalled();
    expect(refresh).toHaveBeenCalled();
  });

  it("新建文件夹：校验非法名不调用 IPC 并设置错误", async () => {
    renderHook();
    let created: string | null = "unset";
    await act(async () => {
      created = await apiRef.current!.createFolder("policy/", "a/b");
    });
    expect(created).toBeNull();
    expect(folderCreate).not.toHaveBeenCalled();
    expect(apiRef.current?.error).toContain("文件夹名称");

    await act(async () => {
      created = await apiRef.current!.createFolder("", "drafts");
    });
    expect(created).toBe("drafts/");
    expect(folderCreate).toHaveBeenCalledWith("drafts");
    expect(refresh).toHaveBeenCalled();
  });

  it("新建笔记：createDefaultNote + prepare + onOpen(source=file-tree)", async () => {
    vi.mocked(createDefaultNote).mockResolvedValue({
      path: "notes/新文档.md",
      title: "新文档",
      content: "# 新文档",
    });
    vi.mocked(prepareNoteOpenFromContent).mockResolvedValue({
      bodyMarkdown: "# 新文档",
      content: "# 新文档",
      frontmatterYaml: null,
      isLocked: false,
      namespace: "normal",
      path: "notes/新文档.md",
      signature: "sig",
      title: "新文档",
      traceKey: "trace",
    });
    renderHook();
    await act(async () => {
      await apiRef.current?.createNote({ folderPrefix: "notes/" });
    });

    expect(createDefaultNote).toHaveBeenCalledWith({
      folderPrefix: "notes/",
    });
    expect(callbacks.onOpen).toHaveBeenCalledWith(
      "notes/新文档.md",
      "file-tree",
      expect.objectContaining({ openBudgetKind: "hot", priority: "hot" }),
    );
    expect(refresh).toHaveBeenCalled();
  });
});
