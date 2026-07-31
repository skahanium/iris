import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useVaultCatalog } from "@/hooks/useVaultCatalog";
import {
  corpusList,
  folderList,
  listenFileChanged,
  workspaceList,
} from "@/lib/ipc";
import type { FileChangedEvent, WorkspaceItem } from "@/types/ipc";

vi.mock("@/lib/ipc", () => ({
  workspaceList: vi.fn(),
  folderList: vi.fn(),
  corpusList: vi.fn(),
  listenFileChanged: vi.fn(),
}));

const ITEM: WorkspaceItem = {
  attachmentRole: "attachment",
  isLocked: false,
  kind: "note",
  mediaKind: null,
  mimeType: null,
  path: "notes/a.md",
  sizeBytes: 10,
  title: "A",
  updatedAt: "2026-01-01T00:00:00Z",
};

type HookApi = ReturnType<typeof useVaultCatalog>;

function Harness({
  apiRef,
  watch,
}: {
  apiRef: { current: HookApi | null };
  watch?: boolean;
}) {
  apiRef.current = useVaultCatalog({ watch });
  return null;
}

describe("useVaultCatalog", () => {
  let host: HTMLDivElement;
  let root: Root;
  let apiRef: { current: HookApi | null };
  let fileChangedHandler: ((payload: FileChangedEvent) => void) | null;

  beforeEach(() => {
    apiRef = { current: null };
    fileChangedHandler = null;
    vi.mocked(workspaceList).mockResolvedValue([ITEM]);
    vi.mocked(folderList).mockResolvedValue(["notes/"]);
    vi.mocked(corpusList).mockResolvedValue([]);
    vi.mocked(listenFileChanged).mockImplementation(async (handler) => {
      fileChangedHandler = handler;
      return () => {
        fileChangedHandler = null;
      };
    });

    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.clearAllMocks();
  });

  function renderHook(watch?: boolean) {
    act(() => {
      root.render(createElement(Harness, { apiRef, watch }));
    });
  }

  it("首次加载拉取 workspace/folder/corpus 并映射为 VaultFileItem", async () => {
    renderHook();
    await act(async () => {
      await Promise.resolve();
    });

    expect(workspaceList).toHaveBeenCalledTimes(1);
    expect(folderList).toHaveBeenCalledTimes(1);
    expect(corpusList).toHaveBeenCalledTimes(1);
    expect(apiRef.current?.files).toEqual([
      {
        isLocked: false,
        kind: "note",
        mediaKind: null,
        mimeType: null,
        path: "notes/a.md",
        title: "A",
        updatedAt: "2026-01-01T00:00:00Z",
      },
    ]);
    expect(apiRef.current?.folders).toEqual(["notes/"]);
    expect(apiRef.current?.loading).toBe(false);
    expect(apiRef.current?.error).toBeNull();
    expect(apiRef.current?.watcherEpoch).toBe(0);
  });

  it("refresh() 重新拉取 catalog", async () => {
    renderHook();
    await act(async () => {
      await Promise.resolve();
    });

    vi.mocked(workspaceList).mockResolvedValueOnce([
      { ...ITEM, path: "notes/b.md", title: "B" },
    ]);
    act(() => {
      apiRef.current?.refresh();
    });
    await act(async () => {
      await Promise.resolve();
    });

    expect(workspaceList).toHaveBeenCalledTimes(2);
    expect(apiRef.current?.files[0]?.path).toBe("notes/b.md");
  });

  it("加载失败设置错误且不中断后续 refresh", async () => {
    vi.mocked(workspaceList).mockRejectedValueOnce(new Error("IO error"));
    renderHook();
    await act(async () => {
      await Promise.resolve();
    });

    expect(apiRef.current?.error).toBe("IO error");
    expect(apiRef.current?.loading).toBe(false);

    act(() => {
      apiRef.current?.refresh();
    });
    await act(async () => {
      await Promise.resolve();
    });
    expect(apiRef.current?.error).toBeNull();
    expect(apiRef.current?.files).toHaveLength(1);
  });

  it("watch 时订阅外部 watcher：事件到达递增 epoch 并重新加载", async () => {
    renderHook(true);
    await act(async () => {
      await Promise.resolve();
    });

    expect(listenFileChanged).toHaveBeenCalledTimes(1);
    expect(apiRef.current?.watcherEpoch).toBe(0);

    await act(async () => {
      fileChangedHandler?.({ path: "notes/c.md", event_type: "create" });
    });
    await act(async () => {
      await Promise.resolve();
    });

    expect(apiRef.current?.watcherEpoch).toBe(1);
    expect(workspaceList).toHaveBeenCalledTimes(2);
  });

  it("卸载时注销 watcher", async () => {
    renderHook(true);
    await act(async () => {
      await Promise.resolve();
    });
    expect(fileChangedHandler).not.toBeNull();

    act(() => root.unmount());
    await act(async () => {
      await Promise.resolve();
    });
    expect(fileChangedHandler).toBeNull();
  });

  it("不 watch 时不订阅外部 watcher", async () => {
    renderHook();
    await act(async () => {
      await Promise.resolve();
    });
    expect(listenFileChanged).not.toHaveBeenCalled();
  });
});
