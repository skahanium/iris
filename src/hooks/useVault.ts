import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import { normalizeOpenDialogPath } from "@/lib/dialog-path";
import { invokeErrorMessage } from "@/lib/credentials";
import { vaultGet, vaultSet } from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";

/** Never let an unavailable native command bridge trap the user in startup. */
export const VAULT_BOOTSTRAP_TIMEOUT_MS = 8_000;

class VaultBootstrapTimeoutError extends Error {
  constructor() {
    super("vault bootstrap timed out");
  }
}

async function vaultGetWithDeadline(): Promise<string | null> {
  let timeout: ReturnType<typeof window.setTimeout> | undefined;
  try {
    return await Promise.race([
      vaultGet(),
      new Promise<string | null>((_, reject) => {
        timeout = window.setTimeout(
          () => reject(new VaultBootstrapTimeoutError()),
          VAULT_BOOTSTRAP_TIMEOUT_MS,
        );
      }),
    ]);
  } finally {
    if (timeout !== undefined) window.clearTimeout(timeout);
  }
}

function vaultBootstrapErrorMessage(error: unknown): string {
  return error instanceof VaultBootstrapTimeoutError
    ? "启动服务未响应。请重试；如仍失败，请重新选择笔记目录。"
    : invokeErrorMessage(error);
}

export function useVault() {
  const [vaultPath, setVaultPath] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const requestGenerationRef = useRef(0);

  const refresh = useCallback(async () => {
    if (!isTauriRuntime()) {
      setLoading(false);
      return;
    }
    const requestGeneration = ++requestGenerationRef.current;
    setLoading(true);
    try {
      const path = await vaultGetWithDeadline();
      if (requestGeneration !== requestGenerationRef.current) return;
      setVaultPath(path);
      if (path) setError(null);
    } catch (e) {
      if (requestGeneration !== requestGenerationRef.current) return;
      setError(vaultBootstrapErrorMessage(e));
      setVaultPath(null);
    } finally {
      if (requestGeneration === requestGenerationRef.current) {
        setLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const pickVault = useCallback(async () => {
    if (!isTauriRuntime()) return;
    setError(null);
    let requestGeneration: number | null = null;
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        recursive: true,
        title: "选择笔记目录",
      });
      const raw = normalizeOpenDialogPath(selected);
      if (!raw) return;

      requestGeneration = ++requestGenerationRef.current;
      setLoading(true);
      await vaultSet(raw);
      const persisted = await vaultGetWithDeadline();
      if (requestGeneration !== requestGenerationRef.current) return;
      if (!persisted) {
        setError("笔记目录未能保存，请重试或换一个文件夹。");
        return;
      }
      setVaultPath(persisted);
      setError(null);
    } catch (e) {
      if (
        requestGeneration !== null &&
        requestGeneration !== requestGenerationRef.current
      ) {
        return;
      }
      setError(vaultBootstrapErrorMessage(e));
    } finally {
      if (
        requestGeneration !== null &&
        requestGeneration === requestGenerationRef.current
      ) {
        setLoading(false);
      }
    }
  }, []);

  return {
    vaultPath,
    loading,
    pickVault,
    refresh,
    error,
    clearError: () => setError(null),
  };
}
