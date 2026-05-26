import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import { settingsGet, settingsSet, vaultGet, vaultSet } from "@/lib/ipc";
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

export function useTheme() {
  const [theme, setThemeState] = useState<"dark" | "light">("dark");

  const applyThemeClass = useCallback((t: "dark" | "light") => {
    document.documentElement.classList.toggle("light", t === "light");
    try {
      localStorage.setItem("iris-theme", t);
    } catch {
      /* ignore quota / private mode */
    }
  }, []);

  useEffect(() => {
    void settingsGet<string>("theme").then((t) => {
      if (t === "light" || t === "dark") {
        setThemeState(t);
        applyThemeClass(t);
      }
    });
  }, [applyThemeClass]);

  const setTheme = useCallback(
    async (t: "dark" | "light") => {
      setThemeState(t);
      applyThemeClass(t);
      await settingsSet("theme", t);
    },
    [applyThemeClass],
  );

  return { theme, setTheme };
}
