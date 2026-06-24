import { useCallback, useMemo } from "react";

import { useMediaTabs } from "@/hooks/useMediaTabs";
import type { TabItem } from "@/components/layout/TabBar";
import type { ArtifactTab } from "@/types/assistant-artifact";

type MaybePromise<T> = T | Promise<T>;

interface NoteTabLike {
  dirty?: boolean;
  locked?: boolean;
  path: string;
  title: string;
}

interface UseWorkspaceTabRoutingOptions<OpenOptions> {
  activeArtifactTab: ArtifactTab | null;
  activePath: string | null;
  artifactTabs: readonly ArtifactTab[];
  closeArtifact: (id: string) => void;
  closeTab: (path: string) => MaybePromise<void>;
  currentNoteIsClassified: boolean;
  handleActivateNoteOrArtifactTab: (path: string) => void;
  handleNewNoteLeavingHome: () => MaybePromise<void>;
  openNoteLeavingHome: (
    path: string,
    titleHint?: string,
    options?: OpenOptions,
  ) => MaybePromise<void>;
  setActiveArtifactId: (id: string | null) => void;
  setHomeActive: (active: boolean) => void;
  tabs: readonly NoteTabLike[];
}

export function useWorkspaceTabRouting<OpenOptions>({
  activeArtifactTab,
  activePath,
  artifactTabs,
  closeArtifact,
  closeTab,
  currentNoteIsClassified,
  handleActivateNoteOrArtifactTab,
  handleNewNoteLeavingHome,
  openNoteLeavingHome,
  setActiveArtifactId,
  setHomeActive,
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
    !activeArtifactTab && !activeMediaTab && currentNoteIsClassified,
  );

  const openWorkspacePathLeavingHome = useCallback(
    (
      path: string,
      titleHint?: string,
      options?: OpenOptions,
    ): Promise<void> => {
      if (openMediaPath(path, titleHint)) {
        setActiveArtifactId(null);
        setHomeActive(false);
        return Promise.resolve();
      }
      setActiveMediaId(null);
      return Promise.resolve(openNoteLeavingHome(path, titleHint, options));
    },
    [
      openMediaPath,
      openNoteLeavingHome,
      setActiveArtifactId,
      setActiveMediaId,
      setHomeActive,
    ],
  );

  const handleActivateWorkspaceTab = useCallback(
    (path: string) => {
      if (path.startsWith("media:")) {
        setActiveArtifactId(null);
        setHomeActive(false);
        activateMedia(path);
        return;
      }
      setActiveMediaId(null);
      handleActivateNoteOrArtifactTab(path);
    },
    [
      activateMedia,
      handleActivateNoteOrArtifactTab,
      setActiveArtifactId,
      setActiveMediaId,
      setHomeActive,
    ],
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
      if (path.startsWith("artifact:")) {
        closeArtifact(path);
        return;
      }
      void closeTab(path);
    },
    [closeArtifact, closeMedia, closeTab],
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
      ...artifactTabs.map((tab) => ({
        path: tab.id,
        title: tab.title,
        kind: "artifact" as const,
        locked: true,
      })),
    ],
    [artifactTabs, mediaTabs, tabs],
  );

  const activeWorkspacePath =
    activeArtifactTab?.id ?? activeMediaTab?.id ?? activePath;

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
