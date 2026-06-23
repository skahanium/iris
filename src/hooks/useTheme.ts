import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { settingsGet, settingsSet } from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";

const THEME_STORAGE_KEY = "iris-theme";

type Theme = "dark" | "light";

function readStoredTheme(): Theme {
  try {
    return localStorage.getItem(THEME_STORAGE_KEY) === "light"
      ? "light"
      : "dark";
  } catch {
    return "dark";
  }
}

export function useTheme() {
  const [theme, setThemeState] = useState<"dark" | "light">(readStoredTheme);

  const applyThemeClass = useCallback((t: Theme) => {
    document.documentElement.classList.toggle("light", t === "light");
    try {
      localStorage.setItem(THEME_STORAGE_KEY, t);
    } catch {
      /* ignore quota / private mode */
    }
    if (isTauriRuntime()) {
      void getCurrentWindow().setTheme(t);
    }
  }, []);

  useEffect(() => {
    applyThemeClass(theme);
  }, [applyThemeClass, theme]);

  useEffect(() => {
    void settingsGet<string>("theme").then((t) => {
      if (t === "light" || t === "dark") {
        setThemeState(t);
        applyThemeClass(t);
      }
    });
  }, [applyThemeClass]);

  const setTheme = useCallback(
    async (t: Theme) => {
      setThemeState(t);
      applyThemeClass(t);
      await settingsSet("theme", t);
    },
    [applyThemeClass],
  );

  return { theme, setTheme };
}
