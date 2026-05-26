import { useCallback, useEffect, useMemo, useRef } from "react";

import type { Editor } from "@tiptap/react";

import { fileWrite } from "@/lib/ipc";
import { htmlToMarkdown } from "@/lib/markdown";
import { debounce } from "@/lib/utils";

/** Debounce for layer-1 persistence to `.md` only (not version snapshots). */
export const EDITOR_SAVE_DEBOUNCE_MS = 1200;

/**
 * Debounced editor save. Call `notifyDirty()` on every keystroke (zero-cost).
 * Actual HTML serialization + markdown conversion + IPC write only happen
 * when the debounce fires. Version checkpoints use `versionSaveManual` / idle.
 *
 * `onSaved` is captured via ref so callers can pass an inline lambda without
 * destabilizing the debounced save (otherwise every parent re-render would
 * rebuild the debounce and the effect cleanup would `flush()` the pending
 * timer immediately, effectively making each keystroke trigger a full save).
 */
export function useEditorSave(
  path: string | null,
  editorRef: React.RefObject<Editor | null>,
  onSaved?: (md: string) => void,
) {
  const pathRef = useRef(path);
  pathRef.current = path;

  const onSavedRef = useRef(onSaved);
  onSavedRef.current = onSaved;

  const saveFromEditor = useCallback(async () => {
    const target = pathRef.current;
    const ed = editorRef.current;
    if (!target || !ed) return;
    const html = ed.getHTML();
    const md = htmlToMarkdown(html);
    await fileWrite(target, md);
    onSavedRef.current?.(md);
  }, [editorRef]);

  const debouncedSave = useMemo(
    () =>
      debounce(() => {
        void saveFromEditor();
      }, EDITOR_SAVE_DEBOUNCE_MS),
    [saveFromEditor],
  );

  useEffect(() => {
    return () => {
      debouncedSave.flush();
    };
  }, [path, debouncedSave]);

  const notifyDirty = useCallback(() => {
    debouncedSave();
  }, [debouncedSave]);

  const flushSave = useCallback(async () => {
    debouncedSave.cancel();
    await saveFromEditor();
  }, [debouncedSave, saveFromEditor]);

  return { notifyDirty, flushSave };
}
