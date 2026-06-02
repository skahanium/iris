import { useCallback, useEffect, useMemo, useRef } from "react";

import { fileWrite } from "@/lib/ipc";
import { isNoteSubstantivelyEmpty } from "@/lib/note-substance";
import { debounce } from "@/lib/utils";

/** Debounce for layer-1 persistence to `.md` only (not version snapshots). */
export const EDITOR_SAVE_DEBOUNCE_MS = 1200;

/**
 * Debounced note save via a single `getMarkdown` serializer (title + body).
 * Call `notifyDirty()` on edits; serialization runs only when the debounce fires.
 */
export function useEditorSave(
  path: string | null,
  getMarkdown: () => string,
  onSaved?: (md: string) => void,
) {
  const pathRef = useRef(path);
  pathRef.current = path;

  const getMarkdownRef = useRef(getMarkdown);
  getMarkdownRef.current = getMarkdown;

  const onSavedRef = useRef(onSaved);
  onSavedRef.current = onSaved;

  const saveNote = useCallback(async (): Promise<string | null> => {
    const target = pathRef.current;
    if (!target) return null;
    const md = getMarkdownRef.current();
    if (isNoteSubstantivelyEmpty(md)) {
      return null;
    }
    await fileWrite(target, md);
    onSavedRef.current?.(md);
    return md;
  }, []);

  const debouncedSave = useMemo(
    () =>
      debounce(() => {
        saveNote().catch((err) => {
          console.warn("[useEditorSave] save failed:", err);
        });
      }, EDITOR_SAVE_DEBOUNCE_MS),
    [saveNote],
  );

  useEffect(() => {
    return () => {
      debouncedSave.flush();
    };
  }, [path, debouncedSave]);

  const notifyDirty = useCallback(() => {
    debouncedSave();
  }, [debouncedSave]);

  const flushSave = useCallback(async (): Promise<string | null> => {
    debouncedSave.cancel();
    return saveNote();
  }, [debouncedSave, saveNote]);

  const cancelPendingSave = useCallback(() => {
    debouncedSave.cancel();
  }, [debouncedSave]);

  return { notifyDirty, flushSave, cancelPendingSave };
}
