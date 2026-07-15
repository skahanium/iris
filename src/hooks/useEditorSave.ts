import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { fileWrite } from "@/lib/ipc";
import { isNoteSubstantivelyEmpty } from "@/lib/note-substance";
import type { LastSavedSnapshot } from "@/lib/version-snapshot-scheduler";
import { debounce } from "@/lib/utils";

/** Debounce for layer-1 persistence to `.md` only (not version snapshots). */
export const EDITOR_SAVE_DEBOUNCE_MS = 1200;
export type { LastSavedSnapshot } from "@/lib/version-snapshot-scheduler";

export type DocumentSaveStatus =
  | "clean"
  | "dirty"
  | "saving"
  | "saved"
  | "saved_index_degraded"
  | "failed";

async function writeNoteAtPath(
  targetPath: string,
  md: string,
): Promise<{ markdown: string; indexDegraded: boolean } | null> {
  const substantivelyEmpty = isNoteSubstantivelyEmpty(md);
  if (substantivelyEmpty) {
    console.debug(
      "[useEditorSave] skip save: note substantively empty",
      targetPath,
    );
    return null;
  }
  const result = await fileWrite(targetPath, md);
  return {
    markdown: md,
    indexDegraded: result.indexStatus === "degraded",
  };
}

/**
 * Debounced note save via a single `getMarkdown` serializer (title + body).
 * Call `notifyDirty()` on edits; serialization runs only when the debounce fires.
 */
export function useEditorSave(
  path: string | null,
  getMarkdown: () => string,
  onSaved?: (md: string, currentRevision: boolean) => void,
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
  const [saveStatus, setSaveStatus] = useState<DocumentSaveStatus>("clean");
  const [saveError, setSaveError] = useState<string | null>(null);

  const recordSavedSnapshot = useCallback(
    (
      targetPath: string,
      markdown: string,
      dirtyGeneration = dirtyGenerationRef.current,
    ) => {
      lastSavedSnapshotRef.current = {
        path: targetPath,
        markdown,
        savedAt: Date.now(),
        dirtyGeneration,
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
    const savingGeneration = dirtyGenerationRef.current;
    const last = lastSavedSnapshotRef.current;
    if (last && last.path === target && last.markdown === md) {
      if (last.dirtyGeneration !== dirtyGenerationRef.current) {
        recordSavedSnapshot(target, md);
        onSavedRef.current?.(md, true);
      }
      return last.markdown;
    }
    setSaveStatus("saving");
    setSaveError(null);
    try {
      const saved = await writeNoteAtPath(target, md);
      if (saved) {
        recordSavedSnapshot(target, saved.markdown, savingGeneration);
        const currentRevision =
          savingGeneration === dirtyGenerationRef.current;
        onSavedRef.current?.(saved.markdown, currentRevision);
        setSaveStatus(
          currentRevision
            ? saved.indexDegraded
              ? "saved_index_degraded"
              : "saved"
            : "dirty",
        );
      }
      return saved?.markdown ?? null;
    } catch (error) {
      setSaveStatus("failed");
      setSaveError(error instanceof Error ? error.message : String(error));
      throw error;
    }
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
    setSaveStatus("dirty");
    setSaveError(null);
    if (saveInFlightRef.current) {
      saveAgainRef.current = true;
    }
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
      setSaveStatus("saving");
      setSaveError(null);
      const savingGeneration = dirtyGenerationRef.current;
      let saved: Awaited<ReturnType<typeof writeNoteAtPath>>;
      try {
        saved = await writeNoteAtPath(targetPath, getMd());
      } catch (error) {
        setSaveStatus("failed");
        setSaveError(error instanceof Error ? error.message : String(error));
        throw error;
      }
      const md = saved?.markdown ?? null;
      if (md) {
        recordSavedSnapshot(targetPath, md, savingGeneration);
      }
      const currentRevision = savingGeneration === dirtyGenerationRef.current;
      if (md && targetPath === pathRef.current) {
        onSavedRef.current?.(md, currentRevision);
      }
      if (md) {
        setSaveStatus(
          currentRevision
            ? saved?.indexDegraded
              ? "saved_index_degraded"
              : "saved"
            : "dirty",
        );
      }
      return md;
    },
    [debouncedSave, recordSavedSnapshot],
  );

  const cancelPendingSave = useCallback(() => {
    debouncedSave.cancel();
  }, [debouncedSave]);

  const awaitSaveInFlight = useCallback(async (): Promise<void> => {
    await saveInFlightRef.current;
  }, []);

  const getLastSavedSnapshot = useCallback(
    () => lastSavedSnapshotRef.current,
    [],
  );

  const rebindSavedSnapshot = useCallback(
    (oldPath: string, newPath: string, markdown: string) => {
      const last = lastSavedSnapshotRef.current;
      if (
        last?.path === oldPath &&
        last.markdown === markdown
      ) {
        lastSavedSnapshotRef.current = { ...last, path: newPath };
      }
    },
    [],
  );

  return {
    notifyDirty,
    flushSave,
    flushSaveForPath,
    cancelPendingSave,
    awaitSaveInFlight,
    getLastSavedSnapshot,
    recordSavedSnapshot,
    rebindSavedSnapshot,
    saveStatus,
    saveError,
  };
}
