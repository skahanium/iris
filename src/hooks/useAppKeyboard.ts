import { useCallback, useEffect } from "react";

import type { useOverlayManager } from "@/hooks/useOverlayManager";
import { isModKey } from "@/lib/utils";

interface UseAppKeyboardOptions {
  overlays: ReturnType<typeof useOverlayManager>;
  activePathRef: React.MutableRefObject<string | null>;
  onSaveVersion: () => void;
  onCloseTab: (path: string) => void;
  onToggleAiPanel: () => void;
  onToggleZen: () => void;
  onToggleOutline: () => void;
  onToggleWebSearch: () => void;
  onRescanVault: () => void;
  zoomIn: () => void;
  zoomOut: () => void;
  resetZoom: () => void;
  vaultPath: string | null;
}

export function useAppKeyboard(options: UseAppKeyboardOptions) {
  const {
    overlays,
    activePathRef,
    onSaveVersion,
    onCloseTab,
    onToggleAiPanel,
    onToggleZen,
    onToggleOutline,
    onToggleWebSearch,
    onRescanVault,
    zoomIn,
    zoomOut,
    resetZoom,
    vaultPath,
  } = options;

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (
        isModKey(e) &&
        !e.shiftKey &&
        (e.key === "s" || e.key === "S") &&
        activePathRef.current
      ) {
        e.preventDefault();
        onSaveVersion();
      }
      if (isModKey(e) && e.shiftKey && (e.key === "P" || e.key === "p")) {
        e.preventDefault();
        overlays.openOverlay("commandPalette");
      }
      if (isModKey(e) && !e.shiftKey && e.key === "p") {
        e.preventDefault();
        overlays.openOverlay("quickOpen");
      }
      if (isModKey(e) && e.shiftKey && (e.key === "E" || e.key === "e")) {
        e.preventDefault();
        overlays.openOverlay("fileSheet");
      }
      if (isModKey(e) && e.shiftKey && (e.key === "U" || e.key === "u")) {
        e.preventDefault();
        overlays.openOverlay("recycleBin");
      }
      if (isModKey(e) && e.shiftKey && (e.key === "F" || e.key === "f")) {
        e.preventDefault();
        overlays.openOverlay("search");
      }
      if (isModKey(e) && e.shiftKey && (e.key === "A" || e.key === "a")) {
        e.preventDefault();
        onToggleAiPanel();
      }
      if (
        isModKey(e) &&
        e.shiftKey &&
        (e.key === "V" || e.key === "v") &&
        activePathRef.current
      ) {
        e.preventDefault();
        overlays.toggleOverlay("version");
      }
      if (isModKey(e) && e.key === "w" && activePathRef.current) {
        e.preventDefault();
        onCloseTab(activePathRef.current);
      }
      if (isModKey(e) && e.key === ",") {
        e.preventDefault();
        overlays.toggleOverlay("settings");
      }
      if (
        isModKey(e) &&
        e.shiftKey &&
        (e.key === "I" || e.key === "i") &&
        vaultPath
      ) {
        e.preventDefault();
        onRescanVault();
      }
      if (isModKey(e) && e.shiftKey && (e.key === "B" || e.key === "b")) {
        e.preventDefault();
        overlays.toggleOverlay("backlinks");
      }
      if (isModKey(e) && e.shiftKey && (e.key === "G" || e.key === "g")) {
        e.preventDefault();
        overlays.toggleOverlay("graph");
      }
      if (isModKey(e) && e.shiftKey && (e.key === "T" || e.key === "t")) {
        e.preventDefault();
        overlays.toggleOverlay("tags");
      }
      if (isModKey(e) && e.key === ".") {
        e.preventDefault();
        onToggleZen();
      }
      if (
        isModKey(e) &&
        e.shiftKey &&
        (e.key === "O" || e.key === "o") &&
        activePathRef.current
      ) {
        e.preventDefault();
        onToggleOutline();
      }
      if (isModKey(e) && !e.shiftKey && (e.key === "=" || e.key === "+")) {
        e.preventDefault();
        zoomIn();
      }
      if (isModKey(e) && !e.shiftKey && e.key === "-") {
        e.preventDefault();
        zoomOut();
      }
      if (isModKey(e) && !e.shiftKey && e.key === "0") {
        e.preventDefault();
        resetZoom();
      }
      if (isModKey(e) && e.shiftKey && (e.key === "W" || e.key === "w")) {
        e.preventDefault();
        onToggleWebSearch();
      }
    },
    [
      overlays,
      activePathRef,
      onSaveVersion,
      onCloseTab,
      onToggleAiPanel,
      onToggleZen,
      onToggleOutline,
      onToggleWebSearch,
      onRescanVault,
      zoomIn,
      zoomOut,
      resetZoom,
      vaultPath,
    ],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);
}
