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

  onActionRef.current = onAction;
  itemsRef.current = items;
  vaultPathRef.current = vaultPath;
  activePathRefRef.current = activePathRef;

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    for (const item of itemsRef.current) {
      const chord = item.chord;
      if (!chord) continue;
      if (!matchesKeyChord(e, chord)) continue;
      if (item.disabled) continue;
      if (chord.requireNote && !activePathRefRef.current.current) continue;
      if (chord.requireVault && !vaultPathRef.current) continue;
      e.preventDefault();
      onActionRef.current(item);
      return;
    }
  }, []);

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  return {};
}
