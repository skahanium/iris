import { useCallback, useMemo } from "react";

import type { TabItem } from "@/components/layout/TabBar";
import type { CloseTabResult } from "@/hooks/useTabManager";
import { useMediaTabs } from "@/hooks/useMediaTabs";

type MaybePromise<T> = T | Promise<T>;

interface NoteTabLike {
  dirty?: boolean;
  locked?: boolean;
  path: string;
  title: string;
}

interface UseWorkspaceTabRoutingOptions<OpenOptions> {
  activePath: string | null;
  /** Resolves only after the tab lifecycle and active destination are settled. */
  closeTab: (path: string) => MaybePromise<CloseTabResult>;
  currentNoteIsClassified: boolean;
  handleActivateNoteTab: (path: string) => void;
  handleNewNoteLeavingHome: () => MaybePromise<void>;
  openNoteLeavingHome: (
    path: string,
    titleHint?: string,
    options?: OpenOptions,
  ) => MaybePromise<void>;
  setHomeActive: (active: boolean) => void;
  showHome: () => void;
  tabs: readonly NoteTabLike[];
}

export function useWorkspaceTabRouting<OpenOptions>({
  activePath,
  closeTab,
  currentNoteIsClassified,
  handleActivateNoteTab,
  handleNewNoteLeavingHome,
  openNoteLeavingHome,
  setHomeActive,
  showHome,
  tabs,
}: UseWorkspaceTabRoutingOptions<OpenOptions>) {
  const {
    activateMedia,
    activeMediaTab,
    closeMedia,
    mediaTabs,
    openMediaPath,
    setActiveMediaId,
  } = useMediaTabs();

  const activeNoteIsClassified = Boolean(
    !activeMediaTab && currentNoteIsClassified,
  );

  const openWorkspacePathLeavingHome = useCallback(
    (
      path: string,
      titleHint?: string,
      options?: OpenOptions,
    ): Promise<void> => {
      if (openMediaPath(path, titleHint)) {
        setHomeActive(false);
        return Promise.resolve();
      }
      setActiveMediaId(null);
      return Promise.resolve(openNoteLeavingHome(path, titleHint, options));
    },
    [openMediaPath, openNoteLeavingHome, setActiveMediaId, setHomeActive],
  );

  const handleActivateWorkspaceTab = useCallback(
    (path: string) => {
      if (path.startsWith("media:")) {
        setHomeActive(false);
        activateMedia(path);
        return;
      }
      setActiveMediaId(null);
      handleActivateNoteTab(path);
    },
    [activateMedia, handleActivateNoteTab, setActiveMediaId, setHomeActive],
  );

  const handleNewWorkspaceNote = useCallback((): Promise<void> => {
    setActiveMediaId(null);
    return Promise.resolve(handleNewNoteLeavingHome());
  }, [handleNewNoteLeavingHome, setActiveMediaId]);

  const handleCloseWorkspaceTab = useCallback(
    (path: string) => {
      if (path.startsWith("media:")) {
        closeMedia(path);
        return;
      }
      void Promise.resolve(closeTab(path))
        .then((result) => {
          if (!result.closed || result.remainingNoteCount > 0) return;
          const nextMedia = mediaTabs[0];
          if (nextMedia) {
            setHomeActive(false);
            activateMedia(nextMedia.id);
            return;
          }
          showHome();
        })
        .catch(() => undefined);
    },
    [activateMedia, closeMedia, closeTab, mediaTabs, setHomeActive, showHome],
  );

  const workspaceTabs: TabItem[] = useMemo(
    () => [
      ...tabs.map((tab) => ({ ...tab, kind: "note" as const })),
      ...mediaTabs.map((tab) => ({
        path: tab.id,
        title: tab.title,
        kind: "media" as const,
        locked: true,
      })),
    ],
    [mediaTabs, tabs],
  );

  const activeWorkspacePath = activeMediaTab?.id ?? activePath;

  return {
    activeMediaTab,
    activeNoteIsClassified,
    activeWorkspacePath,
    handleActivateWorkspaceTab,
    handleCloseWorkspaceTab,
    handleNewWorkspaceNote,
    openWorkspacePathLeavingHome,
    workspaceTabs,
  };
}
