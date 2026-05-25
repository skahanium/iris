import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import { settingsGet, settingsSet, vaultGet, vaultSet } from "@/lib/ipc";

export function useVault() {
  const [vaultPath, setVaultPath] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    const path = await vaultGet();
    setVaultPath(path);
    setLoading(false);
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

  useEffect(() => {
    void settingsGet<string>("theme").then((t) => {
      if (t === "light" || t === "dark") {
        setThemeState(t);
        document.documentElement.classList.toggle("light", t === "light");
      }
    });
  }, []);

  const setTheme = useCallback(async (t: "dark" | "light") => {
    setThemeState(t);
    document.documentElement.classList.toggle("light", t === "light");
    await settingsSet("theme", t);
  }, []);

  return { theme, setTheme };
}
