import { useCallback, useRef } from "react";

import { fileWrite } from "@/lib/ipc";
import { debounce } from "@/lib/utils";

export function useEditorSave(path: string | null, onSaved?: () => void) {
  const dirtyRef = useRef(false);

  const saveNow = useCallback(
    async (content: string) => {
      if (!path) return;
      await fileWrite(path, content);
      dirtyRef.current = false;
      onSaved?.();
    },
    [path, onSaved],
  );

  const saveDebounced = useRef(
    debounce((content: string) => {
      void saveNow(content);
    }, 500),
  ).current;

  const scheduleSave = useCallback(
    (content: string) => {
      dirtyRef.current = true;
      saveDebounced(content);
    },
    [saveDebounced],
  );

  return { scheduleSave, saveNow, isDirty: () => dirtyRef.current };
}
