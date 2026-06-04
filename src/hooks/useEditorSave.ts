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
  const substantivelyEmpty = isNoteSubstantivelyEmpty(md);
  // #region agent log
  fetch("http://127.0.0.1:7413/ingest/3336dc9b-75d7-44cd-8238-25a3e4a38bb9", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Debug-Session-Id": "8589f0",
    },
    body: JSON.stringify({
      sessionId: "8589f0",
      location: "useEditorSave.ts:writeNoteAtPath",
      message: "writeNoteAtPath attempt",
      data: {
        targetPath,
        mdLen: md.length,
        substantivelyEmpty,
        mdPreview: md.slice(0, 80),
      },
      timestamp: Date.now(),
      hypothesisId: "H2",
    }),
  }).catch(() => {});
  // #endregion
  if (substantivelyEmpty) {
    console.debug(
      "[useEditorSave] skip save: note substantively empty",
      targetPath,
    );
    // #region agent log
    fetch("http://127.0.0.1:7413/ingest/3336dc9b-75d7-44cd-8238-25a3e4a38bb9", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Debug-Session-Id": "8589f0",
      },
      body: JSON.stringify({
        sessionId: "8589f0",
        location: "useEditorSave.ts:writeNoteAtPath",
        message: "skip save: substantively empty",
        data: { targetPath, mdLen: md.length },
        timestamp: Date.now(),
        hypothesisId: "H2",
        runId: "post-fix-v3",
      }),
    }).catch(() => {});
    // #endregion
    return null;
  }
  try {
    await fileWrite(targetPath, md);
    // #region agent log
    fetch("http://127.0.0.1:7413/ingest/3336dc9b-75d7-44cd-8238-25a3e4a38bb9", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Debug-Session-Id": "8589f0",
      },
      body: JSON.stringify({
        sessionId: "8589f0",
        location: "useEditorSave.ts:writeNoteAtPath",
        message: "fileWrite ok",
        data: { targetPath, mdLen: md.length },
        timestamp: Date.now(),
        hypothesisId: "H4",
      }),
    }).catch(() => {});
    // #endregion
  } catch (err) {
    // #region agent log
    fetch("http://127.0.0.1:7413/ingest/3336dc9b-75d7-44cd-8238-25a3e4a38bb9", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Debug-Session-Id": "8589f0",
      },
      body: JSON.stringify({
        sessionId: "8589f0",
        location: "useEditorSave.ts:writeNoteAtPath",
        message: "fileWrite failed",
        data: {
          targetPath,
          error: err instanceof Error ? err.message : String(err),
        },
        timestamp: Date.now(),
        hypothesisId: "H4",
      }),
    }).catch(() => {});
    // #endregion
    throw err;
  }
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
      // Wait for the current loop to finish, then run once more with latest content
      await saveInFlightRef.current;
      return runSaveOnce();
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
        // #region agent log
        fetch(
          "http://127.0.0.1:7413/ingest/3336dc9b-75d7-44cd-8238-25a3e4a38bb9",
          {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "X-Debug-Session-Id": "8589f0",
            },
            body: JSON.stringify({
              sessionId: "8589f0",
              location: "useEditorSave.ts:debouncedSave",
              message: "debounced save fired",
              data: { path: pathRef.current },
              timestamp: Date.now(),
              hypothesisId: "H5",
            }),
          },
        ).catch(() => {});
        // #endregion
        saveNote().catch((err) => {
          console.warn("[useEditorSave] save failed:", err);
        });
      }, EDITOR_SAVE_DEBOUNCE_MS),
    [saveNote],
  );

  /** Path changes are persisted via `persistBeforeLeave` in tab manager; do not flush here (pathRef race). */
  useEffect(() => {
    return () => {
      // #region agent log
      fetch(
        "http://127.0.0.1:7413/ingest/3336dc9b-75d7-44cd-8238-25a3e4a38bb9",
        {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "X-Debug-Session-Id": "8589f0",
          },
          body: JSON.stringify({
            sessionId: "8589f0",
            location: "useEditorSave.ts:pathEffect",
            message: "debounced save cancelled (path change/unmount)",
            data: { path },
            timestamp: Date.now(),
            hypothesisId: "H5",
          }),
        },
      ).catch(() => {});
      // #endregion
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
        // Fire-and-forget: best effort save before window closes.
        // The 1200ms debounced auto-save should have already persisted most changes.
        void fileWrite(target, md).catch((err: unknown) => {
          console.warn("[useEditorSave] beforeunload save failed:", err);
        });
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
