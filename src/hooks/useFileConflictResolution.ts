import { useCallback, type MutableRefObject } from "react";

import type { ConflictState } from "@/hooks/useCurrentFileChangeListener";
import { resolveNoteDisplayTitle } from "@/lib/note-display";

interface UseFileConflictResolutionOptions {
  activePathRef: MutableRefObject<string | null>;
  applyMarkdownToEditor: (content: string) => void;
  conflictState: ConflictState | null;
  dirtyRef: MutableRefObject<boolean>;
  flushSave: () => void | Promise<unknown>;
  invalidatePreparedNote: (path: string) => void;
  isMutationBlocked: () => boolean;
  markClean: (path: string, title: string) => void;
  openNoteLeavingHome: (path: string) => void | Promise<void>;
  setConflictState: (state: ConflictState | null) => void;
  syncTabMarkdownCache: (path: string, markdown: string) => void;
}

export function useFileConflictResolution({
  activePathRef,
  applyMarkdownToEditor,
  conflictState,
  dirtyRef,
  flushSave,
  invalidatePreparedNote,
  isMutationBlocked,
  markClean,
  openNoteLeavingHome,
  setConflictState,
  syncTabMarkdownCache,
}: UseFileConflictResolutionOptions) {
  const handleConflictKeepLocal = useCallback(() => {
    if (isMutationBlocked()) return;
    setConflictState(null);
    void flushSave();
  }, [flushSave, isMutationBlocked, setConflictState]);

  const handleConflictAcceptExternal = useCallback(() => {
    if (isMutationBlocked()) return;
    if (!conflictState) return;
    const { externalContent, filePath } = conflictState;
    setConflictState(null);
    invalidatePreparedNote(filePath);
    if (filePath === activePathRef.current) {
      dirtyRef.current = false;
      applyMarkdownToEditor(externalContent);
      syncTabMarkdownCache(filePath, externalContent);
      markClean(
        filePath,
        resolveNoteDisplayTitle({ path: filePath, markdown: externalContent }),
      );
      return;
    }
    void openNoteLeavingHome(filePath);
  }, [
    activePathRef,
    applyMarkdownToEditor,
    conflictState,
    dirtyRef,
    invalidatePreparedNote,
    isMutationBlocked,
    markClean,
    openNoteLeavingHome,
    setConflictState,
    syncTabMarkdownCache,
  ]);

  const handleConflictManualEdit = useCallback(() => {
    setConflictState(null);
  }, [setConflictState]);

  return {
    handleConflictAcceptExternal,
    handleConflictKeepLocal,
    handleConflictManualEdit,
  };
}
