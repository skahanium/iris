import { useCallback, useEffect, useRef } from "react";

import { versionSaveIdle } from "@/lib/ipc";

/** Matches `policy::AUTO_IDLE_MIN_INTERVAL_SECS` in the Rust backend. */
export const VERSION_IDLE_MS = 10 * 60 * 1000;

function runWhenIdle(fn: () => void): void {
  if (typeof requestIdleCallback === "function") {
    requestIdleCallback(() => fn());
  } else {
    setTimeout(fn, 0);
  }
}

/**
 * After `VERSION_IDLE_MS` without `onActivity`, creates an `auto_idle` snapshot
 * when content is available.
 *
 * Only schedules after the first `onActivity()` call — opening a note without
 * editing will never trigger an idle snapshot.
 */
export function useVersionIdle(path: string | null, getContent: () => string) {
  const pathRef = useRef(path);
  const getContentRef = useRef(getContent);
  pathRef.current = path;
  getContentRef.current = getContent;

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearTimer = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  useEffect(() => {
    return () => {
      clearTimer();
    };
  }, [path, clearTimer]);

  const schedule = useCallback(() => {
    clearTimer();
    timerRef.current = setTimeout(() => {
      timerRef.current = null;
      const target = pathRef.current;
      if (!target) return;
      runWhenIdle(() => {
        const content = getContentRef.current();
        void versionSaveIdle(target, content);
      });
    }, VERSION_IDLE_MS);
  }, [clearTimer]);

  const onActivity = useCallback(() => {
    schedule();
  }, [schedule]);

  return { onActivity };
}
