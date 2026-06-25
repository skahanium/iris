import { useCallback, useRef, useState } from "react";

import {
  beginHomeOpenLoading,
  cancelHomeOpenTransitions,
  failHomeOpenLoading,
  type HomePendingOpen,
} from "@/lib/home-open-transition";
import { resolveNoteDisplayTitle } from "@/lib/note-display";

interface CurrentRef<T> {
  current: T;
}

type MaybePromise<T> = T | Promise<T>;

interface UseHomeWorkspaceTransitionsOptions<OpenNoteOptions> {
  activePathRef: CurrentRef<string | null>;
  activateArtifact: (id: string) => void;
  activateTab: (path: string) => MaybePromise<void>;
  handleNewNote: () => Promise<void>;
  openNote: (
    path: string,
    titleHint?: string,
    options?: OpenNoteOptions,
  ) => Promise<void>;
  openTabs?: readonly { path: string }[];
  setActiveArtifactId: (id: string | null) => void;
  setHomeActive: (active: boolean) => void;
}

export function useHomeWorkspaceTransitions<OpenNoteOptions>({
  activateArtifact,
  activateTab,
  handleNewNote,
  openNote,
  openTabs = [],
  setActiveArtifactId,
  setHomeActive,
}: UseHomeWorkspaceTransitionsOptions<OpenNoteOptions>) {
  const homeOpenSequenceRef = useRef(0);
  const [pendingOpen, setPendingOpenState] = useState<HomePendingOpen | null>(
    null,
  );
  const pendingOpenRef = useRef<HomePendingOpen | null>(null);

  const setPendingOpen = useCallback((next: HomePendingOpen | null) => {
    pendingOpenRef.current = next;
    setPendingOpenState(next);
  }, []);

  const showHome = useCallback(() => {
    cancelHomeOpenTransitions(homeOpenSequenceRef, setPendingOpen);
    setHomeActive(true);
  }, [setHomeActive, setPendingOpen]);

  const openNoteLeavingHome = useCallback(
    (
      path: string,
      titleHint?: string,
      options?: OpenNoteOptions,
    ): Promise<void> => {
      if (openTabs.some((tab) => tab.path === path)) {
        cancelHomeOpenTransitions(homeOpenSequenceRef, setPendingOpen);
        setActiveArtifactId(null);
        setHomeActive(false);
        return openNote(path, titleHint, options).catch(() => {
          setHomeActive(true);
        });
      }

      const title = resolveNoteDisplayTitle({ path, title: titleHint });
      const sequence = beginHomeOpenLoading({
        path,
        sequenceRef: homeOpenSequenceRef,
        setPendingOpen,
        title,
      });
      const pending = pendingOpenRef.current ?? {
        kind: "note" as const,
        path,
        sequence,
        startedAt: Date.now(),
        title,
      };
      setActiveArtifactId(null);
      setHomeActive(false);
      return openNote(path, titleHint, options)
        .then(() => {
          if (homeOpenSequenceRef.current !== sequence) return;
          setActiveArtifactId(null);
        })
        .catch((error: unknown) => {
          setHomeActive(true);
          failHomeOpenLoading({
            message: error instanceof Error ? error.message : "无法打开笔记",
            pending,
            sequence,
            sequenceRef: homeOpenSequenceRef,
            setPendingOpen,
          });
        });
    },
    [openNote, openTabs, setActiveArtifactId, setHomeActive, setPendingOpen],
  );

  const clearPendingOpenFromWorkspace = useCallback(
    (pending: HomePendingOpen): boolean => {
      const current = pendingOpenRef.current;
      if (
        homeOpenSequenceRef.current !== pending.sequence ||
        !current ||
        current.kind !== pending.kind ||
        current.path !== pending.path ||
        current.sequence !== pending.sequence
      ) {
        return false;
      }
      setPendingOpen(null);
      return true;
    },
    [setPendingOpen],
  );

  const handleActivateWorkspaceTab = useCallback(
    (path: string) => {
      cancelHomeOpenTransitions(homeOpenSequenceRef, setPendingOpen);
      setHomeActive(false);
      if (path.startsWith("artifact:")) {
        activateArtifact(path);
        return;
      }
      setActiveArtifactId(null);
      void activateTab(path);
    },
    [
      activateArtifact,
      activateTab,
      setActiveArtifactId,
      setHomeActive,
      setPendingOpen,
    ],
  );

  const handleNewNoteLeavingHome = useCallback((): Promise<void> => {
    const title = "新建笔记";
    const sequence = beginHomeOpenLoading({
      kind: "new-note",
      path: null,
      sequenceRef: homeOpenSequenceRef,
      setPendingOpen,
      title,
    });
    const pending = pendingOpenRef.current ?? {
      kind: "new-note" as const,
      path: null,
      sequence,
      startedAt: Date.now(),
      title,
    };
    setActiveArtifactId(null);
    setHomeActive(false);
    return handleNewNote()
      .then(() => {
        if (homeOpenSequenceRef.current !== sequence) return;
        setActiveArtifactId(null);
      })
      .catch((error: unknown) => {
        setHomeActive(true);
        failHomeOpenLoading({
          message: error instanceof Error ? error.message : "新建笔记失败",
          pending,
          sequence,
          sequenceRef: homeOpenSequenceRef,
          setPendingOpen,
        });
      });
  }, [handleNewNote, setActiveArtifactId, setHomeActive, setPendingOpen]);

  return {
    clearPendingOpenFromWorkspace,
    handleActivateWorkspaceTab,
    handleNewNoteLeavingHome,
    openNoteLeavingHome,
    pendingOpen,
    showHome,
  };
}
