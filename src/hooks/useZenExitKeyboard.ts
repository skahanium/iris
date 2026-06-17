import { useCallback, useEffect, useRef } from "react";

import { matchesKeyChord } from "@/lib/utils";

interface UseZenExitKeyboardOptions {
  zen: boolean;
  setZen: (updater: (zen: boolean) => boolean) => void;
}

export function useZenExitKeyboard({ zen, setZen }: UseZenExitKeyboardOptions) {
  const handledToggleKeyDownRef = useRef(false);
  const seenEventsRef = useRef<WeakSet<KeyboardEvent>>(new WeakSet());

  const toggleZen = useCallback(
    (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      setZen((current) => !current);
    },
    [setZen],
  );

  const handleKeyDown = useCallback(
    (event: KeyboardEvent) => {
      if (seenEventsRef.current.has(event)) return;
      seenEventsRef.current.add(event);

      if (matchesKeyChord(event, { key: ".", mod: true })) {
        handledToggleKeyDownRef.current = true;
        toggleZen(event);
        return;
      }

      if (event.key !== "Escape") return;
      if (!zen) return;
      event.preventDefault();
      event.stopPropagation();
      setZen(() => false);
    },
    [setZen, toggleZen, zen],
  );

  const handleKeyUp = useCallback(
    (event: KeyboardEvent) => {
      if (seenEventsRef.current.has(event)) return;
      seenEventsRef.current.add(event);

      if (!matchesKeyChord(event, { key: ".", mod: true })) {
        handledToggleKeyDownRef.current = false;
        return;
      }
      if (handledToggleKeyDownRef.current) {
        handledToggleKeyDownRef.current = false;
        return;
      }
      toggleZen(event);
    },
    [toggleZen],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown, { capture: true });
    window.addEventListener("keyup", handleKeyUp, { capture: true });
    document.addEventListener("keydown", handleKeyDown, { capture: true });
    document.addEventListener("keyup", handleKeyUp, { capture: true });
    return () => {
      window.removeEventListener("keydown", handleKeyDown, { capture: true });
      window.removeEventListener("keyup", handleKeyUp, { capture: true });
      document.removeEventListener("keydown", handleKeyDown, { capture: true });
      document.removeEventListener("keyup", handleKeyUp, { capture: true });
    };
  }, [handleKeyDown, handleKeyUp]);
}
