import { useCallback, useEffect, useRef } from "react";

import { indexRescan } from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";

export type VaultIndexSource = "auto" | "manual";

interface UseAutoVaultIndexOptions {
  onStatus: (message: string) => void;
  onIndexed: () => void;
}

/**
 * Rescan vault on load and expose manual rescan. Runs in background; does not block UI.
 */
export function useAutoVaultIndex(
  vaultPath: string | null,
  vaultLoading: boolean,
  { onStatus, onIndexed }: UseAutoVaultIndexOptions,
) {
  const onStatusRef = useRef(onStatus);
  const onIndexedRef = useRef(onIndexed);
  onStatusRef.current = onStatus;
  onIndexedRef.current = onIndexed;

  const rescanVault = useCallback(
    async (source: VaultIndexSource) => {
      if (!vaultPath || !isTauriRuntime()) return;
      onStatusRef.current(
        source === "manual" ? "正在重建索引…" : "正在同步笔记库…",
      );
      try {
        const entries = await indexRescan();
        onIndexedRef.current();
        onStatusRef.current(
          source === "manual"
            ? `索引完成 · ${entries.length} 篇`
            : `笔记库已同步 · ${entries.length} 篇`,
        );
      } catch {
        onStatusRef.current(
          source === "manual" ? "索引失败" : "笔记库同步失败",
        );
      }
    },
    [vaultPath],
  );

  useEffect(() => {
    if (!vaultPath || vaultLoading || !isTauriRuntime()) return;
    let cancelled = false;
    void (async () => {
      await rescanVault("auto");
      if (cancelled) return;
    })();
    return () => {
      cancelled = true;
    };
  }, [vaultPath, vaultLoading, rescanVault]);

  return { rescanVault };
}
