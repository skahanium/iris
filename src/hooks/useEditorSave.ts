import { useCallback, useEffect, useMemo, useRef } from "react";

import { fileWrite } from "@/lib/ipc";
import { debounce } from "@/lib/utils";

export function useEditorSave(path: string | null, onSaved?: () => void) {
  const dirtyRef = useRef(false);
  const pathRef = useRef(path);
  pathRef.current = path;

  const saveNow = useCallback(
    async (content: string) => {
      const target = pathRef.current;
      if (!target) return;
      await fileWrite(target, content);
      dirtyRef.current = false;
      onSaved?.();
    },
    [onSaved],
  );

  const saveDebounced = useMemo(
    () =>
      debounce((content: string) => {
        void saveNow(content);
      }, 500),
    [saveNow],
  );

  useEffect(() => {
    return () => {
      saveDebounced.flush();
    };
  }, [path, saveDebounced]);

  const scheduleSave = useCallback(
    (content: string) => {
      dirtyRef.current = true;
      saveDebounced(content);
    },
    [saveDebounced],
  );

  return { scheduleSave, saveNow, isDirty: () => dirtyRef.current };
}
