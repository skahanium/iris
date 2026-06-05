import { useCallback, useEffect, useRef } from "react";

import { isNoteSubstantivelyEmpty } from "@/lib/note-substance";
import type { LastSavedSnapshot } from "@/lib/version-snapshot-scheduler";

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
 * after flushing layer-1 persistence and reusing the exact markdown that was written.
 *
 * Only schedules after the first `onActivity()` call — opening a note without
 * editing will never trigger an idle snapshot.
 */
export function useVersionIdle(
  path: string | null,
  getLastSavedSnapshot: () => LastSavedSnapshot | null,
  enqueueIdleSnapshot: (snapshot: LastSavedSnapshot) => void,
) {
  const pathRef = useRef(path);
  const getLastSavedSnapshotRef = useRef(getLastSavedSnapshot);
  const enqueueIdleSnapshotRef = useRef(enqueueIdleSnapshot);
  pathRef.current = path;
  getLastSavedSnapshotRef.current = getLastSavedSnapshot;
  enqueueIdleSnapshotRef.current = enqueueIdleSnapshot;

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
        const snapshot = getLastSavedSnapshotRef.current();
        if (!snapshot || snapshot.path !== target) return;
        if (isNoteSubstantivelyEmpty(snapshot.markdown)) return;
        enqueueIdleSnapshotRef.current(snapshot);
      });
    }, VERSION_IDLE_MS);
  }, [clearTimer]);

  const onActivity = useCallback(() => {
    schedule();
  }, [schedule]);

  return { onActivity, clearTimer };
}
