import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import { vaultGet, vaultSet } from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";

export function useVault() {
  const [vaultPath, setVaultPath] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    if (!isTauriRuntime()) {
      setLoading(false);
      return;
    }
    try {
      const path = await vaultGet();
      setVaultPath(path);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const pickVault = useCallback(async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "选择笔记目录",
    });
    if (typeof selected === "string") {
      await vaultSet(selected);
      setVaultPath(selected);
    }
  }, []);

  return { vaultPath, loading, pickVault, refresh };
}
