import { useCallback, useEffect, useRef, useState } from "react";

import {
  embeddingSchedulerSetForegroundBusy,
  embeddingSchedulerSetPaused,
  embeddingSchedulerStart,
  embeddingSchedulerStatus,
  listenEmbeddingSchedulerStatus,
} from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";
import type {
  EmbeddingIndexStatus,
  EmbeddingSchedulerStartResult,
} from "@/types/ipc";

const IDLE_DELAY_MS = 30_000;

export interface UseEmbeddingSchedulerOptions {
  hasDirtyDocuments: boolean;
}

export interface EmbeddingSchedulerController {
  status: EmbeddingIndexStatus | null;
  loading: boolean;
  error: "status_unavailable" | null;
  start: () => Promise<EmbeddingSchedulerStartResult | null>;
  setPaused: (paused: boolean) => Promise<void>;
  reportForegroundActivity: () => Promise<void>;
}

/**
 * React adapter for the scheduler's single backend-owned generation state.
 * It never derives a generation phase locally; status updates only come from
 * the initial read or the scheduler's complete status event.
 */
export function useEmbeddingScheduler({
  hasDirtyDocuments,
}: UseEmbeddingSchedulerOptions): EmbeddingSchedulerController {
  const [status, setStatus] = useState<EmbeddingIndexStatus | null>(null);
  const [loading, setLoading] = useState(isTauriRuntime);
  const [error, setError] = useState<"status_unavailable" | null>(null);
  const hasDirtyDocumentsRef = useRef(hasDirtyDocuments);
  const idleTimerRef = useRef<ReturnType<typeof window.setTimeout> | null>(
    null,
  );
  const foregroundBusyRef = useRef(false);

  hasDirtyDocumentsRef.current = hasDirtyDocuments;

  const clearIdleTimer = useCallback(() => {
    if (idleTimerRef.current === null) return;
    window.clearTimeout(idleTimerRef.current);
    idleTimerRef.current = null;
  }, []);

  const sendForegroundBusy = useCallback(async (busy: boolean) => {
    if (foregroundBusyRef.current === busy) return;
    foregroundBusyRef.current = busy;
    try {
      await embeddingSchedulerSetForegroundBusy(busy);
    } catch {
      setError("status_unavailable");
    }
  }, []);

  const scheduleIdleRelease = useCallback(() => {
    clearIdleTimer();
    if (hasDirtyDocumentsRef.current) return;
    idleTimerRef.current = window.setTimeout(() => {
      idleTimerRef.current = null;
      if (hasDirtyDocumentsRef.current) return;
      void sendForegroundBusy(false);
    }, IDLE_DELAY_MS);
  }, [clearIdleTimer, sendForegroundBusy]);

  const reportForegroundActivity = useCallback(async () => {
    clearIdleTimer();
    await sendForegroundBusy(true);
    if (!hasDirtyDocumentsRef.current) scheduleIdleRelease();
  }, [clearIdleTimer, scheduleIdleRelease, sendForegroundBusy]);

  useEffect(() => {
    if (!isTauriRuntime()) {
      setLoading(false);
      return undefined;
    }
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listenEmbeddingSchedulerStatus((next) => {
      if (disposed) return;
      setStatus(next);
      setError(null);
    })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {
        if (!disposed) setError("status_unavailable");
      });

    void embeddingSchedulerStatus()
      .then((next) => {
        if (disposed) return;
        setStatus(next);
        setError(null);
      })
      .catch(() => {
        if (!disposed) setError("status_unavailable");
      })
      .finally(() => {
        if (!disposed) setLoading(false);
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    void sendForegroundBusy(true);
    if (hasDirtyDocuments) {
      clearIdleTimer();
      return undefined;
    }
    scheduleIdleRelease();
    return clearIdleTimer;
  }, [
    clearIdleTimer,
    hasDirtyDocuments,
    scheduleIdleRelease,
    sendForegroundBusy,
  ]);

  useEffect(
    () => () => {
      clearIdleTimer();
    },
    [clearIdleTimer],
  );

  const start = useCallback(async () => {
    try {
      return await embeddingSchedulerStart();
    } catch {
      setError("status_unavailable");
      return null;
    }
  }, []);

  const setPaused = useCallback(async (paused: boolean) => {
    try {
      await embeddingSchedulerSetPaused(paused);
    } catch {
      setError("status_unavailable");
    }
  }, []);

  return {
    status,
    loading,
    error,
    start,
    setPaused,
    reportForegroundActivity,
  };
}
