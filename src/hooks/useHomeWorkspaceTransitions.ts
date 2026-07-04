import { useCallback, useEffect, useRef, useState } from "react";

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

function openTransitionNow(): number {
  return globalThis.performance?.now?.() ?? Date.now();
}

const HOME_OPEN_WATCHDOG_MS = 15_000;

interface UseHomeWorkspaceTransitionsOptions<OpenNoteOptions> {
  activePathRef: CurrentRef<string | null>;
  activateArtifact: (id: string) => void;
  activateTab: (path: string, options?: OpenNoteOptions) => MaybePromise<void>;
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
  const openWatchdogTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );

  const clearOpenWatchdog = useCallback(() => {
    if (!openWatchdogTimerRef.current) return;
    clearTimeout(openWatchdogTimerRef.current);
    openWatchdogTimerRef.current = null;
  }, []);

  const setPendingOpen = useCallback((next: HomePendingOpen | null) => {
    pendingOpenRef.current = next;
    setPendingOpenState(next);
  }, []);

  const scheduleOpenWatchdog = useCallback(
    (pending: HomePendingOpen) => {
      clearOpenWatchdog();
      openWatchdogTimerRef.current = setTimeout(() => {
        openWatchdogTimerRef.current = null;
        const current = pendingOpenRef.current;
        if (
          homeOpenSequenceRef.current !== pending.sequence ||
          !current ||
          current.sequence !== pending.sequence ||
          current.error
        ) {
          return;
        }
        homeOpenSequenceRef.current += 1;
        setPendingOpen({
          ...current,
          error: "文档打开超时，未修改文件内容",
        });
        setHomeActive(true);
      }, HOME_OPEN_WATCHDOG_MS);
    },
    [clearOpenWatchdog, setHomeActive, setPendingOpen],
  );

  const showHome = useCallback(() => {
    clearOpenWatchdog();
    cancelHomeOpenTransitions(homeOpenSequenceRef, setPendingOpen);
    setHomeActive(true);
  }, [clearOpenWatchdog, setHomeActive, setPendingOpen]);

  useEffect(() => clearOpenWatchdog, [clearOpenWatchdog]);

  const openNoteLeavingHome = useCallback(
    (
      path: string,
      titleHint?: string,
      options?: OpenNoteOptions,
    ): Promise<void> => {
      if (openTabs.some((tab) => tab.path === path)) {
        cancelHomeOpenTransitions(homeOpenSequenceRef, setPendingOpen);
        const sequence = homeOpenSequenceRef.current;
        setActiveArtifactId(null);
        return Promise.resolve(
          activateTab(path, {
            ...options,
            openBudgetKind: "hot",
          } as OpenNoteOptions),
        )
          .then(() => {
            if (homeOpenSequenceRef.current !== sequence) return;
            setHomeActive(false);
          })
          .catch(() => undefined);
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
        startedAt: openTransitionNow(),
        title,
      };
      setActiveArtifactId(null);
      setHomeActive(false);
      scheduleOpenWatchdog(pending);
      return openNote(path, titleHint, options)
        .then(() => {
          clearOpenWatchdog();
          if (homeOpenSequenceRef.current !== sequence) return;
          setActiveArtifactId(null);
        })
        .catch((error: unknown) => {
          clearOpenWatchdog();
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
    [
      activateTab,
      clearOpenWatchdog,
      openNote,
      openTabs,
      scheduleOpenWatchdog,
      setActiveArtifactId,
      setHomeActive,
      setPendingOpen,
    ],
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
    async (path: string) => {
      cancelHomeOpenTransitions(homeOpenSequenceRef, setPendingOpen);
      if (path.startsWith("artifact:")) {
        setHomeActive(false);
        activateArtifact(path);
        return;
      }
      setActiveArtifactId(null);
      await activateTab(path, {
        openBudgetKind: "hot",
        openTraceRequest: {
          path,
          priority: "hot",
          source: "tab",
        },
        priority: "hot",
        source: "tab",
      } as OpenNoteOptions);
      setHomeActive(false);
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
      startedAt: openTransitionNow(),
      title,
    };
    setActiveArtifactId(null);
    setHomeActive(false);
    scheduleOpenWatchdog(pending);
    return handleNewNote()
      .then(() => {
        clearOpenWatchdog();
        if (homeOpenSequenceRef.current !== sequence) return;
        setActiveArtifactId(null);
      })
      .catch((error: unknown) => {
        clearOpenWatchdog();
        setHomeActive(true);
        failHomeOpenLoading({
          message: error instanceof Error ? error.message : "新建笔记失败",
          pending,
          sequence,
          sequenceRef: homeOpenSequenceRef,
          setPendingOpen,
        });
      });
  }, [
    clearOpenWatchdog,
    handleNewNote,
    scheduleOpenWatchdog,
    setActiveArtifactId,
    setHomeActive,
    setPendingOpen,
  ]);

  return {
    clearPendingOpenFromWorkspace,
    handleActivateWorkspaceTab,
    handleNewNoteLeavingHome,
    openNoteLeavingHome,
    pendingOpen,
    showHome,
  };
}
