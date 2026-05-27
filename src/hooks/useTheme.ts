import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { settingsGet, settingsSet } from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";

export function useTheme() {
  const [theme, setThemeState] = useState<"dark" | "light">("dark");

  const applyThemeClass = useCallback((t: "dark" | "light") => {
    document.documentElement.classList.toggle("light", t === "light");
    try {
      localStorage.setItem("iris-theme", t);
    } catch {
      /* ignore quota / private mode */
    }
    if (isTauriRuntime()) {
      void getCurrentWindow().setTheme(t);
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
