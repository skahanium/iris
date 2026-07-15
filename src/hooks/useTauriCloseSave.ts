import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useRef } from "react";

import { appExit } from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";

interface UseTauriCloseSaveOptions {
  enabled?: boolean;
  flushBeforeClose: () => Promise<void>;
  releaseAfterCloseFailure?: () => void;
  onBlocked?: (retry: () => Promise<void>) => void;
  onError?: (message: string) => void;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function useTauriCloseSave({
  enabled = true,
  flushBeforeClose,
  releaseAfterCloseFailure,
  onBlocked,
  onError,
}: UseTauriCloseSaveOptions): void {
  const closingRef = useRef(false);
  const flushBeforeCloseRef = useRef(flushBeforeClose);
  const onBlockedRef = useRef(onBlocked);
  const onErrorRef = useRef(onError);
  const releaseAfterCloseFailureRef = useRef(releaseAfterCloseFailure);
  flushBeforeCloseRef.current = flushBeforeClose;
  onBlockedRef.current = onBlocked;
  onErrorRef.current = onError;
  releaseAfterCloseFailureRef.current = releaseAfterCloseFailure;

  useEffect(() => {
    if (!enabled || !isTauriRuntime()) return;

    let disposed = false;
    let unlisten: (() => void) | null = null;
    const win = getCurrentWindow();

    let closeTask: Promise<void> | null = null;
    const completeClose = (): Promise<void> => {
      if (closeTask) return closeTask;
      closingRef.current = true;
      closeTask = (async () => {
        try {
          await flushBeforeCloseRef.current();
          window.setTimeout(() => {
            void appExit().catch((err: unknown) => {
              closingRef.current = false;
              closeTask = null;
              releaseAfterCloseFailureRef.current?.();
              onErrorRef.current?.(errorMessage(err));
            });
          }, 0);
        } catch (err) {
          closingRef.current = false;
          closeTask = null;
          releaseAfterCloseFailureRef.current?.();
          onErrorRef.current?.(errorMessage(err));
          onBlockedRef.current?.(completeClose);
        }
      })();
      return closeTask;
    };

    void win
      .onCloseRequested(async (event) => {
        event.preventDefault();
        await completeClose();
      })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlisten = fn;
      })
      .catch((err: unknown) => {
        onErrorRef.current?.(errorMessage(err));
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [enabled]);
}
