import { useCallback, useRef, useState } from "react";

import {
  clearNoteOpenPreparationCache,
  getPreparedNoteOpen,
  invalidateNoteOpenPreparation,
  prepareNoteOpen,
} from "@/lib/document-open-runtime";
import type {
  PrepareNoteOpenRequest,
  NoteOpenBudgetKind,
  NoteOpenNamespace,
  PreparedNoteOpen,
} from "@/lib/document-open-runtime";
import type { FileListItem } from "@/types/ipc";

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

interface UsePreparedNoteOpenerOptions<
  OpenOptions extends OpenPreparedNoteOptions,
> {
  openNote: (
    path: string,
    titleHint?: string,
    options?: OpenOptions,
  ) => Promise<void>;
  openTabs: readonly OpenTabLike[];
}

export function usePreparedNoteOpener<
  OpenOptions extends OpenPreparedNoteOptions,
>({ openNote, openTabs }: UsePreparedNoteOpenerOptions<OpenOptions>) {
  const preparedRequestsRef = useRef(new Map<string, PrepareNoteOpenRequest>());
  const prepareSequenceRef = useRef(0);
  const [warmPreparedNotes, setWarmPreparedNotes] = useState<
    PreparedNoteOpen[]
  >([]);

  const rememberPreparedRequest = useCallback(
    (request: PrepareNoteOpenRequest) => {
      preparedRequestsRef.current.set(request.path, request);
      const sequence = ++prepareSequenceRef.current;
      void prepareNoteOpen(request)
        .then((prepared) => {
          setWarmPreparedNotes((previous) =>
            [
              prepared,
              ...previous.filter((note) => note.path !== prepared.path),
            ].slice(0, 2),
          );
        })
        .catch(() => {
          if (sequence === prepareSequenceRef.current) {
            setWarmPreparedNotes([]);
          }
        });
    },
    [],
  );

  const openPreparedNote = useCallback(
    async (path: string, titleHint?: string, options?: OpenOptions) => {
      const openStartedAt = options?.openStartedAt ?? performance.now();
      if (openTabs.some((tab) => tab.path === path)) {
        await openNote(path, titleHint, {
          ...options,
          openBudgetKind: options?.openBudgetKind ?? "warm",
          openStartedAt,
          openTraceRequest: options?.openTraceRequest ?? { path, titleHint },
        } as OpenOptions);
        return;
      }

      const remembered = preparedRequestsRef.current.get(path);
      const request: PrepareNoteOpenRequest = {
        ...remembered,
        allowClassified:
          options?.allowClassified ?? remembered?.allowClassified,
        path,
        titleHint: titleHint ?? remembered?.titleHint,
      };
      const providedPreparedNote = options?.preparedNote;
      const cachedPreparedNote =
        providedPreparedNote ?? getPreparedNoteOpen(request);
      const preparedNote =
        cachedPreparedNote ?? (await prepareNoteOpen(request));

      await openNote(path, titleHint, {
        ...options,
        openBudgetKind:
          options?.openBudgetKind ?? (cachedPreparedNote ? "hot" : "none"),
        openStartedAt,
        openTraceRequest: options?.openTraceRequest ?? request,
        preparedNote,
      } as OpenOptions);
    },
    [openNote, openTabs],
  );

  const prepareVisibleNote = useCallback(
    (file: FileListItem) => {
      rememberPreparedRequest({
        meta: { isLocked: file.isLocked, updatedAt: file.updatedAt },
        path: file.path,
        titleHint: file.title,
      });
    },
    [rememberPreparedRequest],
  );

  const prepareNotePath = useCallback(
    (path: string, titleHint?: string) => {
      rememberPreparedRequest({ path, titleHint });
    },
    [rememberPreparedRequest],
  );

  const prepareClassifiedNotePath = useCallback(
    (path: string, titleHint?: string) => {
      rememberPreparedRequest({ allowClassified: true, path, titleHint });
    },
    [rememberPreparedRequest],
  );

  const invalidatePreparedNote = useCallback((path: string) => {
    invalidateNoteOpenPreparation(path);
    preparedRequestsRef.current.delete(path);
    setWarmPreparedNotes((previous) =>
      previous.filter((note) => note.path !== path),
    );
  }, []);

  const clearPreparedNotes = useCallback((namespace?: NoteOpenNamespace) => {
    clearNoteOpenPreparationCache(namespace);
    if (!namespace) {
      preparedRequestsRef.current.clear();
      setWarmPreparedNotes([]);
      return;
    }
    for (const [path, request] of preparedRequestsRef.current) {
      const isClassified = request.allowClassified === true;
      if (namespace === "classified" && isClassified) {
        preparedRequestsRef.current.delete(path);
      }
      if (namespace === "normal" && !isClassified) {
        preparedRequestsRef.current.delete(path);
      }
    }
    setWarmPreparedNotes((previous) =>
      previous.filter((note) => note.namespace !== namespace),
    );
  }, []);

  return {
    clearPreparedNotes,
    invalidatePreparedNote,
    openPreparedNote,
    prepareVisibleNote,
    prepareNotePath,
    prepareClassifiedNotePath,
    warmPreparedNotes,
  };
}
