import { useCallback, useEffect, useRef, useState } from "react";

import {
  corpusList,
  folderList,
  listenFileChanged,
  workspaceList,
} from "@/lib/ipc";
import type { CorpusListItem, FileListItem, WorkspaceItem } from "@/types/ipc";

/** catalog 条目：在 FileListItem 上叠加 workspace 的 kind/media/mime 元数据。 */
export type VaultFileItem = FileListItem & {
  kind?: WorkspaceItem["kind"];
  mediaKind?: WorkspaceItem["mediaKind"];
  mimeType?: string | null;
};

/** 把 WorkspaceItem 映射为 VaultFileItem（与管理中心 VaultNavigator 同一形状）。 */
export function vaultFileItem(item: WorkspaceItem): VaultFileItem {
  return {
    isLocked: item.isLocked,
    kind: item.kind,
    mediaKind: item.mediaKind,
    mimeType: item.mimeType,
    path: item.path,
    title: item.title,
    updatedAt: item.updatedAt ?? "",
  };
}

export interface UseVaultCatalogOptions {
  /** 订阅外部文件 watcher（轻量导航开启时）；事件到达递增 epoch 并重新加载。 */
  watch?: boolean;
}

export interface UseVaultCatalogResult {
  files: VaultFileItem[];
  folders: string[];
  corpora: CorpusListItem[];
  loading: boolean;
  error: string | null;
  /** 外部 watcher 事件计数（消费方可用作 refresh 的 epoch 信号）。 */
  watcherEpoch: number;
  refresh: () => void;
}

/**
 * 共享 vault catalog controller（v1.2.19 Task 6）。
 *
 * 固定 catalog 加载/refresh/watcher epoch 行为：加载失败保留壳层并可重试，
 * 外部 watcher 事件不丢失展开集合（展开状态由消费方按 path 保持）。
 */
export function useVaultCatalog(
  options: UseVaultCatalogOptions = {},
): UseVaultCatalogResult {
  const { watch = false } = options;
  const [files, setFiles] = useState<VaultFileItem[]>([]);
  const [folders, setFolders] = useState<string[]>([]);
  const [corpora, setCorpora] = useState<CorpusListItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [watcherEpoch, setWatcherEpoch] = useState(0);
  const generationRef = useRef(0);

  const refresh = useCallback(() => {
    const generation = ++generationRef.current;
    setLoading(true);
    setError(null);
    void Promise.all([workspaceList(), folderList(), corpusList()])
      .then(([nextFiles, nextFolders, nextCorpora]) => {
        if (generation !== generationRef.current) return;
        setFiles(nextFiles.map(vaultFileItem));
        setFolders(nextFolders);
        setCorpora(nextCorpora);
      })
      .catch((e) => {
        if (generation !== generationRef.current) return;
        setError(e instanceof Error ? e.message : "加载文件列表失败");
      })
      .finally(() => {
        if (generation === generationRef.current) {
          setLoading(false);
        }
      });
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // 外部 watcher：事件到达 → epoch + 1 → 重新加载；卸载时注销监听。
  useEffect(() => {
    if (!watch) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void listenFileChanged(() => {
      setWatcherEpoch((epoch) => epoch + 1);
      refresh();
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refresh, watch]);

  return { files, folders, corpora, loading, error, watcherEpoch, refresh };
}
