import { useCallback, useEffect, useMemo, useRef } from "react";

import type { Editor } from "@tiptap/react";

import { fileWrite } from "@/lib/ipc";
import { htmlToMarkdown } from "@/lib/markdown";
import { debounce } from "@/lib/utils";

/**
 * Debounced editor save. Call `notifyDirty()` on every keystroke (zero-cost).
 * Actual HTML serialization + markdown conversion + IPC write only happen
 * when the 500ms debounce fires.
 */
export function useEditorSave(
  path: string | null,
  editorRef: React.RefObject<Editor | null>,
  onSaved?: (md: string) => void,
) {
  const pathRef = useRef(path);
  pathRef.current = path;

  const saveFromEditor = useCallback(async () => {
    const target = pathRef.current;
    const ed = editorRef.current;
    if (!target || !ed) return;
    const html = ed.getHTML();
    const md = htmlToMarkdown(html);
    await fileWrite(target, md);
    onSaved?.(md);
  }, [editorRef, onSaved]);

  const debouncedSave = useMemo(
    () =>
      debounce(() => {
        void saveFromEditor();
      }, 500),
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

  return { notifyDirty };
}
