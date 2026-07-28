import { useCallback, useEffect, useState } from "react";

import { settingsGet, settingsSet } from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";

export const DEFAULT_CJK_PUNCTUATION_ENABLED = true;

/**
 * 中文标点自动转换开关。默认开启；持久化到 SQLite settings 表。
 * 镜像 useAutoVersionSettings 的加载/写入模式。
 */
export function useCjkPunctuationSettings() {
  const [cjkPunctuationEnabled, setCjkPunctuationEnabledState] = useState(
    DEFAULT_CJK_PUNCTUATION_ENABLED,
  );

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let cancelled = false;
    void settingsGet<boolean>("cjk_punctuation_enabled").then((value) => {
      if (cancelled) return;
      if (typeof value === "boolean") {
        setCjkPunctuationEnabledState(value);
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const setCjkPunctuationEnabled = useCallback((enabled: boolean) => {
    setCjkPunctuationEnabledState(enabled);
    if (isTauriRuntime()) {
      void settingsSet("cjk_punctuation_enabled", enabled);
    }
  }, []);

  return {
    cjkPunctuationEnabled,
    setCjkPunctuationEnabled,
  };
}
