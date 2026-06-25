import { useCallback, type RefObject } from "react";

interface UsePreparedNoteInvalidationCallbacksOptions {
  activePathRef: RefObject<string | null>;
  handleFileDeleted: (path?: string) => void;
  handleFilePathChanged: (
    oldPath: string,
    newPath: string,
    title?: string,
  ) => void;
  invalidatePreparedNote: (path: string) => void;
}

export function usePreparedNoteInvalidationCallbacks({
  activePathRef,
  handleFileDeleted,
  handleFilePathChanged,
  invalidatePreparedNote,
}: UsePreparedNoteInvalidationCallbacksOptions) {
  const invalidateActivePreparedNote = useCallback(() => {
    const path = activePathRef.current;
    if (path) invalidatePreparedNote(path);
  }, [activePathRef, invalidatePreparedNote]);

  const handlePreparedFilePathChanged = useCallback(
    (oldPath: string, newPath: string, title?: string) => {
      invalidatePreparedNote(oldPath);
      invalidatePreparedNote(newPath);
      handleFilePathChanged(oldPath, newPath, title);
    },
    [handleFilePathChanged, invalidatePreparedNote],
  );

  const handlePreparedFileDeleted = useCallback(
    (path: string) => {
      invalidatePreparedNote(path);
      handleFileDeleted(path);
    },
    [handleFileDeleted, invalidatePreparedNote],
  );

  return {
    handlePreparedFileDeleted,
    handlePreparedFilePathChanged,
    invalidateActivePreparedNote,
  };
}
