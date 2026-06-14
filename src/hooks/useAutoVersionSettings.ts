import { useCallback, useEffect, useState } from "react";

import { settingsGet, settingsSet } from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";

export const DEFAULT_AUTO_VERSION_ENABLED = true;
export const DEFAULT_AUTO_VERSION_IDLE_MINUTES = 10;
export const MIN_AUTO_VERSION_IDLE_MINUTES = 1;
export const MAX_AUTO_VERSION_IDLE_MINUTES = 120;

function clampAutoVersionMinutes(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_AUTO_VERSION_IDLE_MINUTES;
  return Math.min(
    MAX_AUTO_VERSION_IDLE_MINUTES,
    Math.max(MIN_AUTO_VERSION_IDLE_MINUTES, Math.round(value)),
  );
}

export function useAutoVersionSettings() {
  const [autoVersionEnabled, setAutoVersionEnabledState] = useState(
    DEFAULT_AUTO_VERSION_ENABLED,
  );
  const [autoVersionIdleMinutes, setAutoVersionIdleMinutesState] = useState(
    DEFAULT_AUTO_VERSION_IDLE_MINUTES,
  );

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let cancelled = false;
    void Promise.all([
      settingsGet<boolean>("auto_version_enabled"),
      settingsGet<number>("auto_version_idle_minutes"),
    ]).then(([enabled, minutes]) => {
      if (cancelled) return;
      if (typeof enabled === "boolean") {
        setAutoVersionEnabledState(enabled);
      }
      if (typeof minutes === "number") {
        setAutoVersionIdleMinutesState(clampAutoVersionMinutes(minutes));
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const setAutoVersionEnabled = useCallback((enabled: boolean) => {
    setAutoVersionEnabledState(enabled);
    if (isTauriRuntime()) {
      void settingsSet("auto_version_enabled", enabled);
    }
  }, []);

  const setAutoVersionIdleMinutes = useCallback((minutes: number) => {
    const next = clampAutoVersionMinutes(minutes);
    setAutoVersionIdleMinutesState(next);
    if (isTauriRuntime()) {
      void settingsSet("auto_version_idle_minutes", next);
    }
  }, []);

  return {
    autoVersionEnabled,
    autoVersionIdleMinutes,
    setAutoVersionEnabled,
    setAutoVersionIdleMinutes,
  };
}
