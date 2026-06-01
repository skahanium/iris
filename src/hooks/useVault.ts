import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import { normalizeOpenDialogPath } from "@/lib/dialog-path";
import { invokeErrorMessage } from "@/lib/credentials";
import { vaultGet, vaultSet } from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";

export function useVault() {
  const [vaultPath, setVaultPath] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!isTauriRuntime()) {
      setLoading(false);
      return;
    }
    try {
      const path = await vaultGet();
      setVaultPath(path);
      if (path) setError(null);
    } catch (e) {
      setError(invokeErrorMessage(e));
      setVaultPath(null);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const pickVault = useCallback(async () => {
    if (!isTauriRuntime()) return;
    setError(null);
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        recursive: true,
        title: "选择笔记目录",
      });
      const raw = normalizeOpenDialogPath(selected);
      if (!raw) return;

      await vaultSet(raw);
      const persisted = await vaultGet();
      if (!persisted) {
        setError("笔记目录未能保存，请重试或换一个文件夹。");
        return;
      }
      setVaultPath(persisted);
      setError(null);
    } catch (e) {
      setError(invokeErrorMessage(e));
    }
  }, []);

  return { vaultPath, loading, pickVault, refresh, error, clearError: () => setError(null) };
}
