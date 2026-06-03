import { useCallback, useEffect, useRef } from "react";

import { versionSaveIdle } from "@/lib/ipc";
import { isNoteSubstantivelyEmpty } from "@/lib/note-substance";

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
 * After `VERSION_IDLE_MS` without `onActivity`, enqueues an `auto_idle` snapshot
 * using the last layer-1 persisted markdown (no editor re-serialization).
 *
 * Only schedules after the first `onActivity()` call — opening a note without
 * editing will never trigger an idle snapshot.
 */
export function useVersionIdle(
  path: string | null,
  getPersistedContent: () => string,
) {
  const pathRef = useRef(path);
  const getPersistedContentRef = useRef(getPersistedContent);
  pathRef.current = path;
  getPersistedContentRef.current = getPersistedContent;

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearTimer = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  useEffect(() => {
    clearTimer();
    return clearTimer;
  }, [path, clearTimer]);

  const schedule = useCallback(() => {
    clearTimer();
    timerRef.current = setTimeout(() => {
      timerRef.current = null;
      const target = pathRef.current;
      if (!target) return;
      runWhenIdle(() => {
        const content = getPersistedContentRef.current();
        if (isNoteSubstantivelyEmpty(content)) return;
        void versionSaveIdle(target, content);
      });
    }, VERSION_IDLE_MS);
  }, [clearTimer]);

  const onActivity = useCallback(() => {
    schedule();
  }, [schedule]);

  return { onActivity };
}
