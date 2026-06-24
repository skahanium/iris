import { useCallback, useRef, useState } from "react";

import {
  beginHomeOpenLoading,
  cancelHomeOpenTransitions,
  clearHomeNewNoteLoading,
  clearHomeOpenLoading,
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
  setActiveArtifactId: (id: string | null) => void;
  setHomeActive: (active: boolean) => void;
}

export function useHomeWorkspaceTransitions<OpenNoteOptions>({
  activePathRef,
  activateArtifact,
  activateTab,
  handleNewNote,
  openNote,
  setActiveArtifactId,
  setHomeActive,
}: UseHomeWorkspaceTransitionsOptions<OpenNoteOptions>) {
  const homeOpenSequenceRef = useRef(0);
  const [pendingOpen, setPendingOpen] = useState<HomePendingOpen | null>(null);

  const showHome = useCallback(() => {
    cancelHomeOpenTransitions(homeOpenSequenceRef, setPendingOpen);
    setHomeActive(true);
  }, [setHomeActive]);

  const openNoteLeavingHome = useCallback(
    (path: string, titleHint?: string, options?: OpenNoteOptions) => {
      const title = resolveNoteDisplayTitle({
        path,
        title: titleHint,
      });
      const sequence = beginHomeOpenLoading({
        path,
        sequenceRef: homeOpenSequenceRef,
        setHomeActive,
        setPendingOpen,
        title,
      });
      const pending: HomePendingOpen = {
        kind: "note",
        path,
        sequence,
        title,
      };
      setActiveArtifactId(null);
      void openNote(path, titleHint, options)
        .then(() => {
          if (
            !clearHomeOpenLoading({
              activePath: activePathRef.current,
              path,
              sequence,
              sequenceRef: homeOpenSequenceRef,
              setPendingOpen,
            })
          ) {
            failHomeOpenLoading({
              message: "无法打开笔记",
              pending,
              sequence,
              sequenceRef: homeOpenSequenceRef,
              setPendingOpen,
            });
          }
        })
        .catch((error: unknown) => {
          failHomeOpenLoading({
            message: error instanceof Error ? error.message : "无法打开笔记",
            pending,
            sequence,
            sequenceRef: homeOpenSequenceRef,
            setPendingOpen,
          });
        });
    },
    [activePathRef, openNote, setActiveArtifactId, setHomeActive],
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
    [activateArtifact, activateTab, setActiveArtifactId, setHomeActive],
  );

  const handleNewNoteLeavingHome = useCallback(() => {
    const previousPath = activePathRef.current;
    const title = "新建笔记";
    const sequence = beginHomeOpenLoading({
      kind: "new-note",
      path: null,
      sequenceRef: homeOpenSequenceRef,
      setHomeActive,
      setPendingOpen,
      title,
    });
    const pending: HomePendingOpen = {
      kind: "new-note",
      path: null,
      sequence,
      title,
    };
    setActiveArtifactId(null);
    void handleNewNote()
      .then(() => {
        if (
          !clearHomeNewNoteLoading({
            activePath: activePathRef.current,
            previousPath,
            sequence,
            sequenceRef: homeOpenSequenceRef,
            setPendingOpen,
          })
        ) {
          failHomeOpenLoading({
            message: "新建笔记失败",
            pending,
            sequence,
            sequenceRef: homeOpenSequenceRef,
            setPendingOpen,
          });
        }
      })
      .catch((error: unknown) => {
        failHomeOpenLoading({
          message: error instanceof Error ? error.message : "新建笔记失败",
          pending,
          sequence,
          sequenceRef: homeOpenSequenceRef,
          setPendingOpen,
        });
      });
  }, [activePathRef, handleNewNote, setActiveArtifactId, setHomeActive]);

  return {
    handleActivateWorkspaceTab,
    handleNewNoteLeavingHome,
    openNoteLeavingHome,
    pendingOpen,
    showHome,
  };
}
