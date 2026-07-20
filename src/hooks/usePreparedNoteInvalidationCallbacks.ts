import { useCallback, type RefObject } from "react";

interface UsePreparedNoteInvalidationCallbacksOptions {
  activePathRef: RefObject<string | null>;
  handleFileDeleted: (path?: string) => void;
  handleFilePathChanged: (
    oldPath: string,
    newPath: string,
    title?: string,
  ) => void;
  invalidateDocumentRuntimeState: (path: string) => void;
  invalidatePreparedNote: (path: string) => void;
}

export function usePreparedNoteInvalidationCallbacks({
  activePathRef,
  handleFileDeleted,
  handleFilePathChanged,
  invalidateDocumentRuntimeState,
  invalidatePreparedNote,
}: UsePreparedNoteInvalidationCallbacksOptions) {
  const invalidateActivePreparedNote = useCallback(() => {
    const path = activePathRef.current;
    if (!path) return;
    invalidatePreparedNote(path);
    invalidateDocumentRuntimeState(path);
  }, [activePathRef, invalidateDocumentRuntimeState, invalidatePreparedNote]);

  const handlePreparedFilePathChanged = useCallback(
    (oldPath: string, newPath: string, title?: string) => {
      invalidatePreparedNote(oldPath);
      invalidatePreparedNote(newPath);
      invalidateDocumentRuntimeState(oldPath);
      invalidateDocumentRuntimeState(newPath);
      handleFilePathChanged(oldPath, newPath, title);
    },
    [
      handleFilePathChanged,
      invalidateDocumentRuntimeState,
      invalidatePreparedNote,
    ],
  );

  const handlePreparedFileDeleted = useCallback(
    (path: string) => {
      invalidatePreparedNote(path);
      invalidateDocumentRuntimeState(path);
      handleFileDeleted(path);
    },
    [handleFileDeleted, invalidateDocumentRuntimeState, invalidatePreparedNote],
  );

  /**
   * An Iris-owned atomic rename suppresses its watcher events. The active tab
   * has already moved its live cache to the new path, so retire only old-path
   * speculative/runtime entries and never reload or reset the editor.
   */
  const handleApplicationPathRenamed = useCallback(
    (oldPath: string) => {
      invalidatePreparedNote(oldPath);
      invalidateDocumentRuntimeState(oldPath);
    },
    [invalidateDocumentRuntimeState, invalidatePreparedNote],
  );

  return {
    handleApplicationPathRenamed,
    handlePreparedFileDeleted,
    handlePreparedFilePathChanged,
    invalidateActivePreparedNote,
  };
}
