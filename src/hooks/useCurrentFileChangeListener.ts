import { useEffect, type MutableRefObject } from "react";

import { fileRead, listenFileChanged } from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";

export interface ConflictState {
  open: boolean;
  localContent: string;
  externalContent: string;
  filePath: string;
}

interface UseCurrentFileChangeListenerParams {
  activePathRef: MutableRefObject<string | null>;
  awaitSaveInFlight: () => Promise<void>;
  bumpVaultIndex: () => void;
  cancelPendingSave: () => void;
  discardOpenTab: (path: string) => Promise<void>;
  getLiveMarkdownRef: MutableRefObject<() => string>;
  onFileChanged?: (path: string) => void;
  setConflictState: (state: ConflictState | null) => void;
}

export function useCurrentFileChangeListener({
  activePathRef,
  awaitSaveInFlight,
  bumpVaultIndex,
  cancelPendingSave,
  discardOpenTab,
  getLiveMarkdownRef,
  onFileChanged,
  setConflictState,
}: UseCurrentFileChangeListenerParams) {
  useEffect(() => {
    if (!isTauriRuntime()) return;
    let unlisten: (() => void) | undefined;
    void listenFileChanged((event) => {
      onFileChanged?.(event.path);
      const currentPath = activePathRef.current;
      if (!currentPath || event.path !== currentPath) return;
      if (event.event_type === "removed") {
        cancelPendingSave();
        void awaitSaveInFlight()
          .then(() => discardOpenTab(event.path))
          .then(() => bumpVaultIndex())
          .catch((err: unknown) => {
            console.warn("[App] failed to discard removed file tab:", err);
          });
        return;
      }
      void fileRead(event.path)
        .then(({ content: externalContent }) => {
          const localContent = getLiveMarkdownRef.current();
          if (externalContent !== localContent) {
            setConflictState({
              open: true,
              localContent,
              externalContent,
              filePath: event.path,
            });
          }
        })
        .catch((err: unknown) => {
          console.warn("[App] failed to read external file for conflict:", err);
        });
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [
    activePathRef,
    awaitSaveInFlight,
    bumpVaultIndex,
    cancelPendingSave,
    discardOpenTab,
    getLiveMarkdownRef,
    onFileChanged,
    setConflictState,
  ]);
}
