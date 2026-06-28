import { useEffect } from "react";

import { useHomeWorkspaceTransitions } from "@/hooks/useHomeWorkspaceTransitions";
import { usePreparedNoteOpener } from "@/hooks/usePreparedNoteOpener";
import { loadWorkspaceSessionSnapshot } from "@/lib/workspace-session-snapshot";
import type {
  NoteOpenBudgetKind,
  PrepareNoteOpenRequest,
  PreparedNoteOpen,
} from "@/lib/document-open-runtime";
import type { ClassifiedStatus } from "@/types/ipc";

interface CurrentRef<T> {
  current: T;
}

type MaybePromise<T> = T | Promise<T>;

interface OpenPreparedNoteOptions {
  allowClassified?: boolean;
  openBudgetKind?: NoteOpenBudgetKind;
  openStartedAt?: number;
  openTraceRequest?: PrepareNoteOpenRequest;
  preparedNote?: PreparedNoteOpen;
}

interface OpenTabLike {
  path: string;
}

interface UsePreparedWorkspaceTransitionsOptions<
  OpenOptions extends OpenPreparedNoteOptions,
> {
  activePathRef: CurrentRef<string | null>;
  activateArtifact: (id: string) => void;
  activateTab: (path: string) => MaybePromise<void>;
  classifiedVaultStatus: ClassifiedStatus;
  handleNewNote: () => Promise<void>;
  openNote: (
    path: string,
    titleHint?: string,
    options?: OpenOptions,
  ) => Promise<void>;
  setActiveArtifactId: (id: string | null) => void;
  setHomeActive: (active: boolean) => void;
  tabs: readonly OpenTabLike[];
  vaultPath: string | null;
}

export function usePreparedWorkspaceTransitions<
  OpenOptions extends OpenPreparedNoteOptions,
>({
  activePathRef,
  activateArtifact,
  activateTab,
  classifiedVaultStatus,
  handleNewNote,
  openNote,
  setActiveArtifactId,
  setHomeActive,
  tabs,
  vaultPath,
}: UsePreparedWorkspaceTransitionsOptions<OpenOptions>) {
  const prepared = usePreparedNoteOpener<OpenOptions>({
    openNote,
    openTabs: tabs,
  });

  const { clearPreparedNotes, openPreparedNote, warmNotePath } = prepared;

  useEffect(() => {
    clearPreparedNotes();
  }, [clearPreparedNotes, vaultPath]);

  useEffect(() => {
    if (classifiedVaultStatus !== "unlocked") {
      clearPreparedNotes("classified");
    }
  }, [classifiedVaultStatus, clearPreparedNotes]);

  useEffect(() => {
    if (!vaultPath) return;
    const timer = window.setTimeout(() => {
      const snapshot = loadWorkspaceSessionSnapshot(vaultPath);
      for (const note of snapshot?.openNotes ?? []) {
        warmNotePath(note.path, note.title, {
          isLocked: note.isLocked,
          priority: "background",
          source: "startup",
          useSignature: false,
        });
      }
    }, 0);
    return () => window.clearTimeout(timer);
  }, [vaultPath, warmNotePath]);

  const transitions = useHomeWorkspaceTransitions<OpenOptions>({
    activePathRef,
    activateArtifact,
    activateTab,
    handleNewNote,
    openNote: openPreparedNote,
    openTabs: tabs,
    setActiveArtifactId,
    setHomeActive,
  });

  return {
    ...prepared,
    ...transitions,
  };
}
