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
  markClean,
  openNoteLeavingHome,
  setConflictState,
  syncTabMarkdownCache,
}: UseFileConflictResolutionOptions) {
  const handleConflictKeepLocal = useCallback(() => {
    setConflictState(null);
    void flushSave();
  }, [flushSave, setConflictState]);

  const handleConflictAcceptExternal = useCallback(() => {
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
