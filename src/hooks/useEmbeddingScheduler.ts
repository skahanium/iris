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
  const foregroundBusyRef = useRef(false);
  const foregroundTargetRef = useRef(false);
  const foregroundUpdateRef = useRef<Promise<void>>(Promise.resolve());

  hasDirtyDocumentsRef.current = hasDirtyDocuments;

  const sendForegroundBusy = useCallback(async (busy: boolean) => {
    if (foregroundTargetRef.current === busy) {
      return foregroundUpdateRef.current;
    }
    foregroundTargetRef.current = busy;
    const update = foregroundUpdateRef.current
      .catch(() => undefined)
      .then(async () => {
        if (foregroundBusyRef.current === busy) return;
        try {
          await embeddingSchedulerSetForegroundBusy(busy);
          foregroundBusyRef.current = busy;
        } catch {
          if (foregroundTargetRef.current === busy) {
            foregroundTargetRef.current = foregroundBusyRef.current;
          }
          setError("status_unavailable");
        }
      });
    foregroundUpdateRef.current = update;
    return update;
  }, []);

  const reportForegroundActivity = useCallback(async () => {
    await sendForegroundBusy(true);
    if (!hasDirtyDocumentsRef.current) await sendForegroundBusy(false);
  }, [sendForegroundBusy]);

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
    void (async () => {
      await sendForegroundBusy(true);
      if (!hasDirtyDocumentsRef.current) await sendForegroundBusy(false);
    })();
  }, [hasDirtyDocuments, sendForegroundBusy]);

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
