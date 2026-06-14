import { useCallback, useMemo, type MutableRefObject } from "react";

import {
  buildCommandPaletteItems,
  recordCommandUsage,
  type CommandPaletteItem,
} from "@/lib/command-palette";
import type {
  ManagementCenterDetail,
  ManagementCenterSection,
  OverlayId,
} from "@/hooks/useOverlayManager";

interface OverlayPort {
  closeOverlay: (overlay?: OverlayId) => void;
  openManagementCenter: (
    section?: ManagementCenterSection,
    detail?: ManagementCenterDetail,
  ) => void;
  openOverlay: (overlay: OverlayId) => void;
}

interface UseAppCommandPaletteParams {
  activePath: string | null;
  activePathRef: MutableRefObject<string | null>;
  closeTab: (path: string) => void;
  handleNewNote: () => Promise<unknown>;
  handleSaveNote: () => Promise<void>;
  handleVaultRescan: () => void;
  openFindReplace: (mode: "find" | "replace") => void;
  overlays: OverlayPort;
  resetZoom: () => void;
  saveOutlineOpen: (open: boolean) => void;
  sendSelectionToAi: () => void;
  setAiPanelOpen: (updater: (open: boolean) => boolean) => void;
  setClassifiedOpen: (open: boolean) => void;
  setOutlineOpen: (updater: (open: boolean) => boolean) => void;
  setTheme: (theme: "dark" | "light") => Promise<void>;
  setZen: (updater: (zen: boolean) => boolean) => void;
  theme: "dark" | "light";
  toggleWebSearch: () => void;
  vaultPath: string | null;
  zoomIn: () => void;
  zoomOut: () => void;
}

export function useAppCommandPalette({
  activePath,
  activePathRef,
  closeTab,
  handleNewNote,
  handleSaveNote,
  handleVaultRescan,
  openFindReplace,
  overlays,
  resetZoom,
  saveOutlineOpen,
  sendSelectionToAi,
  setAiPanelOpen,
  setClassifiedOpen,
  setOutlineOpen,
  setTheme,
  setZen,
  theme,
  toggleWebSearch,
  vaultPath,
  zoomIn,
  zoomOut,
}: UseAppCommandPaletteParams) {
  const commandPaletteItems = useMemo(
    () =>
      buildCommandPaletteItems({
        hasVault: Boolean(vaultPath),
        hasActiveNote: Boolean(activePath),
      }),
    [vaultPath, activePath],
  );

  const handleCommandPaletteSelect = useCallback(
    (item: CommandPaletteItem) => {
      const action = item.action;
      recordCommandUsage(item.id);
      switch (action.type) {
        case "openOverlay":
          overlays.openOverlay(action.overlay);
          break;
        case "openManagementCenter":
          overlays.openManagementCenter(action.section, action.detail ?? null);
          break;
        case "openClassifiedPanel":
          setClassifiedOpen(true);
          break;
        case "openFindReplace":
          openFindReplace(action.mode);
          break;
        case "newNote":
          void handleNewNote();
          break;
        case "saveNote":
          void handleSaveNote();
          break;
        case "closeTab":
          if (activePathRef.current) closeTab(activePathRef.current);
          break;
        case "toggleAiPanel":
          setAiPanelOpen((open) => !open);
          break;
        case "toggleZen":
          setZen((z) => !z);
          break;
        case "toggleOutline":
          setOutlineOpen((open) => {
            const next = !open;
            saveOutlineOpen(next);
            return next;
          });
          break;
        case "toggleTheme":
          void setTheme(theme === "dark" ? "light" : "dark");
          break;
        case "toggleWebSearch":
          toggleWebSearch();
          break;
        case "rescanVault":
          void handleVaultRescan();
          break;
        case "zoomIn":
          zoomIn();
          break;
        case "zoomOut":
          zoomOut();
          break;
        case "zoomReset":
          resetZoom();
          break;
        case "sendSelectionToAi":
          sendSelectionToAi();
          break;
        case "noop":
          break;
        default: {
          const _exhaustive: never = action;
          return _exhaustive;
        }
      }
    },
    [
      activePathRef,
      closeTab,
      handleNewNote,
      handleSaveNote,
      handleVaultRescan,
      openFindReplace,
      overlays,
      resetZoom,
      saveOutlineOpen,
      sendSelectionToAi,
      setAiPanelOpen,
      setClassifiedOpen,
      setOutlineOpen,
      setTheme,
      setZen,
      theme,
      toggleWebSearch,
      zoomIn,
      zoomOut,
    ],
  );

  return { commandPaletteItems, handleCommandPaletteSelect };
}
