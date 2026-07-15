import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useRef } from "react";

import { appExit } from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";

interface UseTauriCloseSaveOptions {
  enabled?: boolean;
  flushBeforeClose: () => Promise<void>;
  onBlocked?: (retry: () => Promise<void>) => void;
  onError?: (message: string) => void;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function useTauriCloseSave({
  enabled = true,
  flushBeforeClose,
  onBlocked,
  onError,
}: UseTauriCloseSaveOptions): void {
  const closingRef = useRef(false);
  const flushBeforeCloseRef = useRef(flushBeforeClose);
  const onBlockedRef = useRef(onBlocked);
  const onErrorRef = useRef(onError);
  flushBeforeCloseRef.current = flushBeforeClose;
  onBlockedRef.current = onBlocked;
  onErrorRef.current = onError;

  useEffect(() => {
    if (!enabled || !isTauriRuntime()) return;

    let disposed = false;
    let unlisten: (() => void) | null = null;
    const win = getCurrentWindow();

    const completeClose = async (): Promise<void> => {
      try {
        await flushBeforeCloseRef.current();
        closingRef.current = true;
        window.setTimeout(() => {
          void appExit().catch((err: unknown) => {
            closingRef.current = false;
            onErrorRef.current?.(errorMessage(err));
          });
        }, 0);
      } catch (err) {
        closingRef.current = false;
        onErrorRef.current?.(errorMessage(err));
        onBlockedRef.current?.(completeClose);
      }
    };

    void win
      .onCloseRequested(async (event) => {
        if (closingRef.current) return;
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
