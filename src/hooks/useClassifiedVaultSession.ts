import { useCallback, useEffect, useRef, useState } from "react";

import { classifiedLock, classifiedStatus } from "@/lib/ipc";
import type { ClassifiedStatus } from "@/types/ipc";

const AUTO_LOCK_MS = 10 * 60 * 1000;

interface UseClassifiedVaultSessionOptions {
  enabled: boolean;
  openClassifiedPaths: string[];
  onLocked?: () => void;
  abortClassifiedRequest?: () => void;
}

/**
 * Tracks vault unlock status and enforces 10-minute idle auto-lock (R9).
 * Defers locking while classified editor tabs remain open.
 */
export function useClassifiedVaultSession({
  enabled,
  openClassifiedPaths,
  onLocked,
  abortClassifiedRequest,
}: UseClassifiedVaultSessionOptions) {
  const [status, setStatus] = useState<ClassifiedStatus>("locked");
  const [waiting, setWaiting] = useState(false);
  const [idleDeadline, setIdleDeadline] = useState<number | null>(null);
  const idleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onLockedRef = useRef(onLocked);
  onLockedRef.current = onLocked;

  const abortClassifiedRequestRef = useRef(abortClassifiedRequest);
  abortClassifiedRequestRef.current = abortClassifiedRequest;

  const clearIdleTimer = useCallback(() => {
    if (idleTimerRef.current) {
      clearTimeout(idleTimerRef.current);
      idleTimerRef.current = null;
    }
    setIdleDeadline(null);
  }, []);

  const refreshStatus = useCallback(async () => {
    if (!enabled) {
      setStatus("locked");
      setWaiting(false);
      return "locked" as const;
    }
    try {
      const next = await classifiedStatus();
      setStatus(next);
      if (next !== "unlocked") {
        setWaiting(false);
        clearIdleTimer();
      }
      return next;
    } catch {
      setStatus("locked");
      setWaiting(false);
      clearIdleTimer();
      return "locked" as const;
    }
  }, [clearIdleTimer, enabled]);

  const performLock = useCallback(async () => {
    clearIdleTimer();
    abortClassifiedRequestRef.current?.();
    try {
      await classifiedLock();
    } catch {
      /* best-effort */
    }
    setStatus("locked");
    setWaiting(false);
    onLockedRef.current?.();
  }, [clearIdleTimer]);

  const requestLock = useCallback(async () => {
    if (openClassifiedPaths.length > 0) {
      setWaiting(true);
      return false;
    }
    await performLock();
    return true;
  }, [openClassifiedPaths.length, performLock]);

  const scheduleIdleLock = useCallback(() => {
    if (status !== "unlocked") return;
    clearIdleTimer();
    const deadline = Date.now() + AUTO_LOCK_MS;
    setIdleDeadline(deadline);
    idleTimerRef.current = setTimeout(() => {
      idleTimerRef.current = null;
      setIdleDeadline(null);
      void requestLock();
    }, AUTO_LOCK_MS);
  }, [clearIdleTimer, requestLock, status]);

  const touchActivity = useCallback(() => {
    if (status === "unlocked") {
      scheduleIdleLock();
    }
  }, [scheduleIdleLock, status]);

  useEffect(() => {
    if (!enabled) return;
    void refreshStatus();
  }, [enabled, refreshStatus]);

  useEffect(() => {
    if (status !== "unlocked") {
      clearIdleTimer();
      return;
    }
    scheduleIdleLock();
    const onActivity = () => {
      touchActivity();
    };
    window.addEventListener("mousemove", onActivity, { passive: true });
    window.addEventListener("keydown", onActivity);
    return () => {
      window.removeEventListener("mousemove", onActivity);
      window.removeEventListener("keydown", onActivity);
      clearIdleTimer();
    };
  }, [clearIdleTimer, scheduleIdleLock, status, touchActivity]);

  useEffect(() => {
    if (waiting && openClassifiedPaths.length === 0) {
      void performLock();
    }
  }, [openClassifiedPaths.length, performLock, waiting]);

  const onUnlocked = useCallback(async () => {
    const next = await refreshStatus();
    if (next === "unlocked") {
      setWaiting(false);
      scheduleIdleLock();
    }
  }, [refreshStatus, scheduleIdleLock]);

  return {
    status,
    waiting,
    idleDeadline,
    refreshStatus,
    touchActivity,
    requestLock,
    onUnlocked,
    setWaiting,
  };
}
