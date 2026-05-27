import { useCallback, useState } from "react";

export type OverlayId =
  | "commandPalette"
  | "quickOpen"
  | "fileSheet"
  | "search"
  | "settings"
  | "backlinks"
  | "tags"
  | "graph"
  | "version"
  | "recycleBin";

const SIDE_PANELS: OverlayId[] = [
  "commandPalette",
  "fileSheet",
  "search",
  "settings",
  "backlinks",
  "tags",
  "version",
  "recycleBin",
];

export interface OverlayState {
  activeOverlay: OverlayId | null;
}

export function openOverlayState(
  _state: OverlayState,
  id: OverlayId,
): OverlayState {
  return { activeOverlay: id };
}

export function closeOverlayState(
  state: OverlayState,
  id?: OverlayId,
): OverlayState {
  if (id && state.activeOverlay !== id) return state;
  return { activeOverlay: null };
}

export function toggleOverlayState(
  state: OverlayState,
  id: OverlayId,
): OverlayState {
  if (state.activeOverlay === id) return { activeOverlay: null };
  return { activeOverlay: id };
}

export function useOverlayManager() {
  const [activeOverlay, setActiveOverlay] =
    useState<OverlayState["activeOverlay"]>(null);

  const updateOverlay = useCallback(
    (recipe: (state: OverlayState) => OverlayState) => {
      setActiveOverlay(
        (current) => recipe({ activeOverlay: current }).activeOverlay,
      );
    },
    [],
  );

  const openOverlay = useCallback(
    (id: OverlayId) => updateOverlay((state) => openOverlayState(state, id)),
    [updateOverlay],
  );

  const closeOverlay = useCallback(
    (id?: OverlayId) => updateOverlay((state) => closeOverlayState(state, id)),
    [updateOverlay],
  );

  const toggleOverlay = useCallback(
    (id: OverlayId) => updateOverlay((state) => toggleOverlayState(state, id)),
    [updateOverlay],
  );

  const setOverlayOpen = useCallback(
    (id: OverlayId, open: boolean) => {
      if (open) openOverlay(id);
      else closeOverlay(id);
    },
    [closeOverlay, openOverlay],
  );

  const closeSidePanels = useCallback(() => {
    setActiveOverlay((current) =>
      current && SIDE_PANELS.includes(current) ? null : current,
    );
  }, []);

  const openSidePanel = openOverlay;

  const toggleSidePanel = toggleOverlay;

  const commandPaletteOpen = activeOverlay === "commandPalette";
  const quickOpen = activeOverlay === "quickOpen";
  const fileSheet = activeOverlay === "fileSheet";
  const searchOpen = activeOverlay === "search";
  const settingsOpen = activeOverlay === "settings";
  const backlinksOpen = activeOverlay === "backlinks";
  const tagViewOpen = activeOverlay === "tags";
  const graphOpen = activeOverlay === "graph";
  const versionOpen = activeOverlay === "version";
  const recycleBinOpen = activeOverlay === "recycleBin";

  return {
    activeOverlay,
    openOverlay,
    closeOverlay,
    toggleOverlay,
    isOverlayOpen: (id: OverlayId) => activeOverlay === id,
    commandPaletteOpen,
    setCommandPaletteOpen: (open: boolean) =>
      setOverlayOpen("commandPalette", open),
    quickOpen,
    setQuickOpen: (open: boolean) => setOverlayOpen("quickOpen", open),
    fileSheet,
    setFileSheet: (open: boolean) => setOverlayOpen("fileSheet", open),
    searchOpen,
    setSearchOpen: (open: boolean) => setOverlayOpen("search", open),
    settingsOpen,
    setSettingsOpen: (open: boolean) => setOverlayOpen("settings", open),
    backlinksOpen,
    setBacklinksOpen: (open: boolean) => setOverlayOpen("backlinks", open),
    tagViewOpen,
    setTagViewOpen: (open: boolean) => setOverlayOpen("tags", open),
    graphOpen,
    setGraphOpen: (open: boolean) => setOverlayOpen("graph", open),
    versionOpen,
    setVersionOpen: (open: boolean) => setOverlayOpen("version", open),
    recycleBinOpen,
    setRecycleBinOpen: (open: boolean) => setOverlayOpen("recycleBin", open),
    openSidePanel,
    toggleSidePanel,
    closeSidePanels,
  };
}
