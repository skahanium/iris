import { useCallback, useEffect, useRef } from "react";

import type { AppShortcutItem } from "@/lib/app-shortcuts";
import { matchesKeyChord } from "@/lib/utils";

interface UseAppKeyboardOptions {
  items: AppShortcutItem[];
  vaultPath: string | null;
  activePathRef: React.MutableRefObject<string | null>;
  onAction: (item: AppShortcutItem) => void;
}

export function useAppKeyboard(options: UseAppKeyboardOptions) {
  const { items, vaultPath, activePathRef, onAction } = options;
  const onActionRef = useRef(onAction);
  const itemsRef = useRef(items);
  const vaultPathRef = useRef(vaultPath);
  const activePathRefRef = useRef(activePathRef);
  const handledKeyDownShortcutRef = useRef<string | null>(null);

  onActionRef.current = onAction;
  itemsRef.current = items;
  vaultPathRef.current = vaultPath;
  activePathRefRef.current = activePathRef;

  const runShortcut = useCallback(
    (e: KeyboardEvent): AppShortcutItem | null => {
      for (const item of itemsRef.current) {
        const chord = item.chord;
        if (!chord) continue;
        if (!matchesKeyChord(e, chord)) continue;
        if (item.disabled) continue;
        if (chord.requireNote && !activePathRefRef.current.current) continue;
        if (chord.requireVault && !vaultPathRef.current) continue;
        e.preventDefault();
        onActionRef.current(item);
        return item;
      }
      return null;
    },
    [],
  );

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      const item = runShortcut(e);
      handledKeyDownShortcutRef.current = item?.id ?? null;
    },
    [runShortcut],
  );

  const handleKeyUp = useCallback(
    (e: KeyboardEvent) => {
      const handledId = handledKeyDownShortcutRef.current;
      if (handledId) {
        const handledItem = itemsRef.current.find(
          (item) => item.id === handledId,
        );
        if (handledItem?.chord && matchesKeyChord(e, handledItem.chord)) {
          handledKeyDownShortcutRef.current = null;
          return;
        }
        handledKeyDownShortcutRef.current = null;
      }
      void runShortcut(e);
    },
    [runShortcut],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown, { capture: true });
    window.addEventListener("keyup", handleKeyUp, { capture: true });
    return () => {
      window.removeEventListener("keydown", handleKeyDown, { capture: true });
      window.removeEventListener("keyup", handleKeyUp, { capture: true });
    };
  }, [handleKeyDown, handleKeyUp]);

  return {};
}
