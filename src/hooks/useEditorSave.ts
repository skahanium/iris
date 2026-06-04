import { useCallback, useEffect, useMemo, useRef } from "react";

import { fileWrite } from "@/lib/ipc";
import { isNoteSubstantivelyEmpty } from "@/lib/note-substance";
import { debounce } from "@/lib/utils";

/** Debounce for layer-1 persistence to `.md` only (not version snapshots). */
export const EDITOR_SAVE_DEBOUNCE_MS = 1200;

async function writeNoteAtPath(
  targetPath: string,
  getMd: () => string,
): Promise<string | null> {
  const md = getMd();
  if (isNoteSubstantivelyEmpty(md)) {
    return null;
  }
  await fileWrite(targetPath, md);
  return md;
}

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

  const saveInFlightRef = useRef<Promise<string | null> | null>(null);
  const saveAgainRef = useRef(false);

  const runSaveOnce = useCallback(async (): Promise<string | null> => {
    const target = pathRef.current;
    if (!target) return null;
    const md = await writeNoteAtPath(target, () => getMarkdownRef.current());
    if (md) {
      onSavedRef.current?.(md);
    }
    return md;
  }, []);

  const saveNote = useCallback(async (): Promise<string | null> => {
    if (saveInFlightRef.current) {
      saveAgainRef.current = true;
      return saveInFlightRef.current;
    }

    const loop = async (): Promise<string | null> => {
      let last: string | null = null;
      do {
        saveAgainRef.current = false;
        last = await runSaveOnce();
      } while (saveAgainRef.current);
      return last;
    };

    const promise = loop().finally(() => {
      saveInFlightRef.current = null;
    });
    saveInFlightRef.current = promise;
    return promise;
  }, [runSaveOnce]);

  const debouncedSave = useMemo(
    () =>
      debounce(() => {
        saveNote().catch((err) => {
          console.warn("[useEditorSave] save failed:", err);
        });
      }, EDITOR_SAVE_DEBOUNCE_MS),
    [saveNote],
  );

  /** Path changes are persisted via `persistBeforeLeave` in tab manager; do not flush here (pathRef race). */
  useEffect(() => {
    return () => {
      debouncedSave.cancel();
    };
  }, [path, debouncedSave]);

  useEffect(() => {
    const handleBeforeUnload = () => {
      const target = pathRef.current;
      if (!target) return;
      debouncedSave.cancel();
      const md = getMarkdownRef.current();
      if (!isNoteSubstantivelyEmpty(md)) {
        void fileWrite(target, md).catch(() => {});
      }
    };
    window.addEventListener("beforeunload", handleBeforeUnload);
    return () => window.removeEventListener("beforeunload", handleBeforeUnload);
  }, [debouncedSave]);

  const notifyDirty = useCallback(() => {
    debouncedSave();
  }, [debouncedSave]);

  const flushSave = useCallback(async (): Promise<string | null> => {
    debouncedSave.cancel();
    return saveNote();
  }, [debouncedSave, saveNote]);

  /** Persist a specific path (e.g. tab being left) without relying on current `pathRef`. */
  const flushSaveForPath = useCallback(
    async (
      targetPath: string,
      getMarkdownOverride?: () => string,
    ): Promise<string | null> => {
      debouncedSave.cancel();
      const getMd = getMarkdownOverride ?? (() => getMarkdownRef.current());
      const md = await writeNoteAtPath(targetPath, getMd);
      if (md && targetPath === pathRef.current) {
        onSavedRef.current?.(md);
      }
      return md;
    },
    [debouncedSave],
  );

  const cancelPendingSave = useCallback(() => {
    debouncedSave.cancel();
  }, [debouncedSave]);

  return {
    notifyDirty,
    flushSave,
    flushSaveForPath,
    cancelPendingSave,
  };
}
