import { useEffect, useRef } from "react";

import { useHomeWorkspaceTransitions } from "@/hooks/useHomeWorkspaceTransitions";
import { usePreparedNoteOpener } from "@/hooks/usePreparedNoteOpener";
import { fileList } from "@/lib/ipc";
import { resolveStartupNote } from "@/lib/resolve-startup-note";
import { loadWorkspaceSessionSnapshot } from "@/lib/workspace-session-snapshot";
import type {
  NoteOpenBudgetKind,
  NoteOpenSource,
  PrepareNoteOpenRequest,
  PreparedNoteOpen,
} from "@/lib/document-open-runtime";
import type { ClassifiedStatus } from "@/types/ipc";

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
  activateTab: (path: string) => MaybePromise<void>;
  cancelPendingDocumentOpen?: () => void;
  classifiedVaultStatus: ClassifiedStatus;
  handleNewNote: () => Promise<void>;
  openNote: (
    path: string,
    titleHint?: string,
    options?: OpenOptions,
  ) => Promise<void>;
  setWorkspaceEmpty: (active: boolean) => void;
  tabs: readonly OpenTabLike[];
  vaultPath: string | null;
  workspaceEmpty: boolean;
}

export function usePreparedWorkspaceTransitions<
  OpenOptions extends OpenPreparedNoteOptions,
>({
  activateTab,
  cancelPendingDocumentOpen,
  classifiedVaultStatus,
  handleNewNote,
  openNote,
  setWorkspaceEmpty,
  tabs,
  vaultPath,
  workspaceEmpty,
}: UsePreparedWorkspaceTransitionsOptions<OpenOptions>) {
  const startupAutoOpenDoneRef = useRef(false);

  const prepared = usePreparedNoteOpener<OpenOptions>({
    openNote,
    openTabs: tabs,
  });

  const { clearPreparedNotes, openPreparedNote, warmNotePath } = prepared;

  useEffect(() => {
    startupAutoOpenDoneRef.current = false;
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

  useEffect(() => {
    if (!vaultPath || !workspaceEmpty || tabs.length > 0) {
      return;
    }
    if (startupAutoOpenDoneRef.current) {
      return;
    }

    const timer = window.setTimeout(() => {
      if (
        startupAutoOpenDoneRef.current ||
        tabs.length > 0 ||
        !workspaceEmpty
      ) {
        return;
      }
      startupAutoOpenDoneRef.current = true;

      void (async () => {
        const snapshot = loadWorkspaceSessionSnapshot(vaultPath);
        const openNotePaths =
          snapshot?.openNotes.map((note) => note.path) ?? [];
        let recentFiles: Awaited<ReturnType<typeof fileList>> = [];
        try {
          recentFiles = await fileList();
        } catch (error) {
          console.warn("[Workspace] startup recent notes load failed:", error);
          return;
        }

        const candidate = resolveStartupNote({
          activePath: snapshot?.activePath ?? null,
          openNotePaths,
          recentPaths: recentFiles.map((file) => file.path),
        });
        if (!candidate) {
          return;
        }

        const titleHint =
          snapshot?.openNotes.find((note) => note.path === candidate.path)
            ?.title ??
          recentFiles.find((file) => file.path === candidate.path)?.title;

        await openPreparedNote(candidate.path, titleHint, {
          source: "startup" as NoteOpenSource,
        } as unknown as OpenOptions);
      })();
    }, 0);

    return () => window.clearTimeout(timer);
  }, [openPreparedNote, tabs.length, vaultPath, workspaceEmpty]);

  const transitions = useHomeWorkspaceTransitions<OpenOptions>({
    activateTab,
    cancelPendingDocumentOpen,
    handleNewNote,
    openNote: openPreparedNote,
    openTabs: tabs,
    setWorkspaceEmpty,
  });

  return {
    ...prepared,
    ...transitions,
  };
}
