import { useCallback, useEffect, useMemo, useRef } from "react";

import { fileWrite } from "@/lib/ipc";
import { isNoteSubstantivelyEmpty } from "@/lib/note-substance";
import type { LastSavedSnapshot } from "@/lib/version-snapshot-scheduler";
import { debounce } from "@/lib/utils";

/** Debounce for layer-1 persistence to `.md` only (not version snapshots). */
export const EDITOR_SAVE_DEBOUNCE_MS = 1200;
export type { LastSavedSnapshot } from "@/lib/version-snapshot-scheduler";

async function writeNoteAtPath(
  targetPath: string,
  md: string,
): Promise<string | null> {
  const substantivelyEmpty = isNoteSubstantivelyEmpty(md);
  if (substantivelyEmpty) {
    console.debug(
      "[useEditorSave] skip save: note substantively empty",
      targetPath,
    );
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
  const dirtyGenerationRef = useRef(0);
  const lastSavedSnapshotRef = useRef<LastSavedSnapshot | null>(null);

  const recordSavedSnapshot = useCallback(
    (targetPath: string, markdown: string) => {
      lastSavedSnapshotRef.current = {
        path: targetPath,
        markdown,
        savedAt: Date.now(),
        dirtyGeneration: dirtyGenerationRef.current,
      };
    },
    [],
  );

  const runSaveOnce = useCallback(async (): Promise<string | null> => {
    const target = pathRef.current;
    if (!target) return null;
    // Skip save if content unchanged from last persisted snapshot.
    // This avoids sending full content over IPC for no-op auto-saves.
    const md = getMarkdownRef.current();
    const last = lastSavedSnapshotRef.current;
    if (last && last.path === target && last.markdown === md) {
      return last.markdown;
    }
    const saved = await writeNoteAtPath(target, md);
    if (saved) {
      recordSavedSnapshot(target, saved);
      onSavedRef.current?.(saved);
    }
    return saved;
  }, [recordSavedSnapshot]);

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

  const notifyDirty = useCallback(() => {
    dirtyGenerationRef.current += 1;
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
      if (saveInFlightRef.current) {
        await saveInFlightRef.current;
      }
      const getMd = getMarkdownOverride ?? (() => getMarkdownRef.current());
      const md = await writeNoteAtPath(targetPath, getMd());
      if (md) {
        recordSavedSnapshot(targetPath, md);
      }
      if (md && targetPath === pathRef.current) {
        onSavedRef.current?.(md);
      }
      return md;
    },
    [debouncedSave, recordSavedSnapshot],
  );

  const cancelPendingSave = useCallback(() => {
    debouncedSave.cancel();
  }, [debouncedSave]);

  const getLastSavedSnapshot = useCallback(
    () => lastSavedSnapshotRef.current,
    [],
  );

  return {
    notifyDirty,
    flushSave,
    flushSaveForPath,
    cancelPendingSave,
    getLastSavedSnapshot,
  };
}
