import { useEffect } from "react";

import { useHomeWorkspaceTransitions } from "@/hooks/useHomeWorkspaceTransitions";
import { usePreparedNoteOpener } from "@/hooks/usePreparedNoteOpener";
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

  const { clearPreparedNotes, openPreparedNote } = prepared;

  useEffect(() => {
    clearPreparedNotes();
  }, [clearPreparedNotes, vaultPath]);

  useEffect(() => {
    if (classifiedVaultStatus !== "unlocked") {
      clearPreparedNotes("classified");
    }
  }, [classifiedVaultStatus, clearPreparedNotes]);

  const transitions = useHomeWorkspaceTransitions<OpenOptions>({
    activePathRef,
    activateArtifact,
    activateTab,
    handleNewNote,
    openNote: openPreparedNote,
    setActiveArtifactId,
    setHomeActive,
  });

  return {
    ...prepared,
    ...transitions,
  };
}
