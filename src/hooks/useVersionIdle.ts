import { useCallback, useEffect, useRef } from "react";

import { versionSaveIdle } from "@/lib/ipc";

/** Matches `policy::AUTO_IDLE_MIN_INTERVAL_SECS` in the Rust backend. */
export const VERSION_IDLE_MS = 10 * 60 * 1000;

/**
 * After `VERSION_IDLE_MS` without `onActivity`, creates an `auto_idle` snapshot
 * when content is available.
 */
export function useVersionIdle(
  path: string | null,
  getContent: () => string,
) {
  const pathRef = useRef(path);
  const getContentRef = useRef(getContent);
  pathRef.current = path;
  getContentRef.current = getContent;

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const schedule = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
    }
    timerRef.current = setTimeout(() => {
      const target = pathRef.current;
      if (!target) return;
      const content = getContentRef.current();
      void versionSaveIdle(target, content);
    }, VERSION_IDLE_MS);
  }, []);

  useEffect(() => {
    schedule();
    return () => {
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
      }
    };
  }, [path, schedule]);

  const onActivity = useCallback(() => {
    schedule();
  }, [schedule]);

  return { onActivity };
}
