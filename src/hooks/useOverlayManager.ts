import { useCallback, useState } from "react";

export type OverlayId =
  | "quickOpen"
  | "fileSheet"
  | "search"
  | "managementCenter"
  | "knowledgeRelations"
  | "graph"
  | "version"
  | "recycleBin";

export type ManagementCenterSection = "overview" | "notes" | "knowledge" | "ai";

export type ManagementCenterDetail =
  | "quick-open"
  | "global-search"
  | "knowledge-relations"
  | "graph"
  | "file-sheet"
  | "recycle-bin"
  | "reindex"
  | "version"
  | "outline"
  | "zoom"
  | "models"
  | "web-search"
  | "persona"
  | "skills"
  | "permissions"
  | "memory"
  | null;

export interface OverlayState {
  activeOverlay: OverlayId | null;
  managementCenterSection?: ManagementCenterSection;
  managementCenterDetail?: ManagementCenterDetail;
}

export function openOverlayState(
  _state: OverlayState,
  id: OverlayId,
): OverlayState {
  return {
    activeOverlay: id,
    managementCenterSection: "overview",
    managementCenterDetail: null,
  };
}

export function openManagementCenterState(
  _state: OverlayState,
  section: ManagementCenterSection = "overview",
  detail: ManagementCenterDetail = null,
): OverlayState {
  return {
    activeOverlay: "managementCenter",
    managementCenterSection: section,
    managementCenterDetail: detail,
  };
}

export function closeOverlayState(
  state: OverlayState,
  id?: OverlayId,
): OverlayState {
  if (id && state.activeOverlay !== id) return state;
  return {
    activeOverlay: null,
    managementCenterSection: "overview",
    managementCenterDetail: null,
  };
}

export function toggleOverlayState(
  state: OverlayState,
  id: OverlayId,
): OverlayState {
  if (state.activeOverlay === id) return closeOverlayState(state);
  return openOverlayState(state, id);
}

export function useOverlayManager() {
  const [overlayState, setOverlayState] = useState<OverlayState>({
    activeOverlay: null,
    managementCenterSection: "overview",
    managementCenterDetail: null,
  });
  const {
    activeOverlay,
    managementCenterSection = "overview",
    managementCenterDetail = null,
  } = overlayState;

  const updateOverlay = useCallback(
    (recipe: (state: OverlayState) => OverlayState) => {
      setOverlayState((current) => recipe(current));
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

  const openManagementCenter = useCallback(
    (
      section: ManagementCenterSection = "overview",
      detail: ManagementCenterDetail = null,
    ) =>
      updateOverlay((state) =>
        openManagementCenterState(state, section, detail),
      ),
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

  const quickOpen = activeOverlay === "quickOpen";
  const fileSheet = activeOverlay === "fileSheet";
  const searchOpen = activeOverlay === "search";
  const managementCenterOpen = activeOverlay === "managementCenter";
  const knowledgeRelationsOpen = activeOverlay === "knowledgeRelations";
  const graphOpen = activeOverlay === "graph";
  const versionOpen = activeOverlay === "version";
  const recycleBinOpen = activeOverlay === "recycleBin";

  return {
    activeOverlay,
    openOverlay,
    openManagementCenter,
    closeOverlay,
    toggleOverlay,
    isOverlayOpen: (id: OverlayId) => activeOverlay === id,
    managementCenterSection,
    managementCenterDetail,
    quickOpen,
    setQuickOpen: (open: boolean) => setOverlayOpen("quickOpen", open),
    fileSheet,
    setFileSheet: (open: boolean) => setOverlayOpen("fileSheet", open),
    searchOpen,
    setSearchOpen: (open: boolean) => setOverlayOpen("search", open),
    managementCenterOpen,
    setManagementCenterOpen: (open: boolean) =>
      setOverlayOpen("managementCenter", open),
    knowledgeRelationsOpen,
    setKnowledgeRelationsOpen: (open: boolean) =>
      setOverlayOpen("knowledgeRelations", open),
    graphOpen,
    setGraphOpen: (open: boolean) => setOverlayOpen("graph", open),
    versionOpen,
    setVersionOpen: (open: boolean) => setOverlayOpen("version", open),
    recycleBinOpen,
    setRecycleBinOpen: (open: boolean) => setOverlayOpen("recycleBin", open),
  };
}
