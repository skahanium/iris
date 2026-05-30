import { useCallback, useEffect, useRef, useState } from "react";

import type { CommandPaletteItem } from "@/lib/command-palette";
import { isModKey, matchesKeyChord } from "@/lib/utils";

const LEADER_TIMEOUT_MS = 2000;

interface UseAppKeyboardOptions {
  items: CommandPaletteItem[];
  vaultPath: string | null;
  activePathRef: React.MutableRefObject<string | null>;
  onAction: (item: CommandPaletteItem) => void;
  /** Leader 等待第二键时回调（用于状态栏提示） */
  onLeaderPendingChange?: (pending: boolean) => void;
}

export function useAppKeyboard(options: UseAppKeyboardOptions) {
  const { items, vaultPath, activePathRef, onAction, onLeaderPendingChange } =
    options;
  const onActionRef = useRef(onAction);
  const itemsRef = useRef(items);
  const vaultPathRef = useRef(vaultPath);
  const activePathRefRef = useRef(activePathRef);
  const onLeaderPendingChangeRef = useRef(onLeaderPendingChange);
  const [pendingLeader, setPendingLeader] = useState<string | null>(null);
  const pendingLeaderRef = useRef<string | null>(null);

  onActionRef.current = onAction;
  itemsRef.current = items;
  vaultPathRef.current = vaultPath;
  activePathRefRef.current = activePathRef;
  onLeaderPendingChangeRef.current = onLeaderPendingChange;

  const setPending = useCallback((leader: string | null) => {
    pendingLeaderRef.current = leader;
    setPendingLeader(leader);
    onLeaderPendingChangeRef.current?.(leader !== null);
  }, []);

  useEffect(() => {
    if (!pendingLeader) return;
    const timer = window.setTimeout(() => {
      setPending(null);
    }, LEADER_TIMEOUT_MS);
    return () => window.clearTimeout(timer);
  }, [pendingLeader, setPending]);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      const pending = pendingLeaderRef.current;

      if (e.key === "Escape") {
        if (pending) {
          setPending(null);
          e.preventDefault();
          return;
        }
        return;
      }

      if (pending) {
        for (const item of itemsRef.current) {
          const chord = item.chord;
          if (!chord || chord.afterLeader !== pending) continue;
          if (e.key.toLowerCase() !== chord.key.toLowerCase()) continue;
          if (chord.mod !== isModKey(e)) continue;
          if (item.disabled) {
            setPending(null);
            return;
          }
          if (chord.requireNote && !activePathRefRef.current.current) {
            setPending(null);
            return;
          }
          if (chord.requireVault && !vaultPathRef.current) {
            setPending(null);
            return;
          }
          e.preventDefault();
          setPending(null);
          onActionRef.current(item);
          return;
        }
        setPending(null);
        return;
      }

      for (const item of itemsRef.current) {
        const chord = item.chord;
        if (!chord || chord.afterLeader) continue;
        if (!matchesKeyChord(e, chord)) continue;
        if (item.disabled) continue;
        if (chord.leader) {
          e.preventDefault();
          setPending(chord.leader);
          return;
        }
        if (chord.requireNote && !activePathRefRef.current.current) continue;
        if (chord.requireVault && !vaultPathRef.current) continue;
        e.preventDefault();
        onActionRef.current(item);
        return;
      }
    },
    [setPending],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  return { pendingLeader };
}
