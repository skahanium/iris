import { useCallback, useState } from "react";

export type OverlayId =
  | "quickOpen"
  | "fileSheet"
  | "search"
  | "settings"
  | "backlinks"
  | "tags"
  | "graph"
  | "version";

const SIDE_PANELS: OverlayId[] = [
  "fileSheet",
  "search",
  "settings",
  "backlinks",
  "tags",
  "version",
];

export function useOverlayManager() {
  const [quickOpen, setQuickOpen] = useState(false);
  const [fileSheet, setFileSheet] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [backlinksOpen, setBacklinksOpen] = useState(false);
  const [tagViewOpen, setTagViewOpen] = useState(false);
  const [graphOpen, setGraphOpen] = useState(false);
  const [versionOpen, setVersionOpen] = useState(false);

  const closeSidePanels = useCallback(() => {
    setFileSheet(false);
    setSearchOpen(false);
    setSettingsOpen(false);
    setBacklinksOpen(false);
    setTagViewOpen(false);
    setVersionOpen(false);
  }, []);

  const openSidePanel = useCallback(
    (id: OverlayId) => {
      if (id === "quickOpen") {
        setQuickOpen(true);
        return;
      }
      if (id === "graph") {
        closeSidePanels();
        setGraphOpen(true);
        return;
      }
      if (SIDE_PANELS.includes(id)) {
        closeSidePanels();
        setGraphOpen(false);
        switch (id) {
          case "fileSheet":
            setFileSheet(true);
            break;
          case "search":
            setSearchOpen(true);
            break;
          case "settings":
            setSettingsOpen(true);
            break;
          case "backlinks":
            setBacklinksOpen(true);
            break;
          case "tags":
            setTagViewOpen(true);
            break;
          case "version":
            setVersionOpen(true);
            break;
        }
      }
    },
    [closeSidePanels],
  );

  const toggleSidePanel = useCallback(
    (id: OverlayId) => {
      const isOpen =
        (id === "fileSheet" && fileSheet) ||
        (id === "search" && searchOpen) ||
        (id === "settings" && settingsOpen) ||
        (id === "backlinks" && backlinksOpen) ||
        (id === "tags" && tagViewOpen) ||
        (id === "version" && versionOpen) ||
        (id === "graph" && graphOpen);

      if (isOpen) {
        if (id === "graph") setGraphOpen(false);
        else closeSidePanels();
        return;
      }
      openSidePanel(id);
    },
    [
      fileSheet,
      searchOpen,
      settingsOpen,
      backlinksOpen,
      tagViewOpen,
      versionOpen,
      graphOpen,
      closeSidePanels,
      openSidePanel,
    ],
  );

  return {
    quickOpen,
    setQuickOpen,
    fileSheet,
    setFileSheet,
    searchOpen,
    setSearchOpen,
    settingsOpen,
    setSettingsOpen,
    backlinksOpen,
    setBacklinksOpen,
    tagViewOpen,
    setTagViewOpen,
    graphOpen,
    setGraphOpen,
    versionOpen,
    setVersionOpen,
    openSidePanel,
    toggleSidePanel,
    closeSidePanels,
  };
}
