import { useCallback, useRef, useState } from "react";

import {
  clearNoteOpenPreparationCache,
  getPreparedNoteOpen,
  invalidateNoteOpenPreparation,
  prepareNoteOpen,
  prepareNoteOpenFromContent,
} from "@/lib/document-open-runtime";
import { clearCachedEditorHtml } from "@/lib/editor-html-cache";
import {
  documentOpen,
  documentOpenBegin,
  documentOpenEnd,
  fileSignature,
} from "@/lib/ipc";
import type {
  DocumentOpenPriority,
  PrepareNoteOpenRequest,
  NoteOpenBudgetKind,
  NoteOpenNamespace,
  NoteOpenSource,
  PreparedNoteOpen,
} from "@/lib/document-open-runtime";
import type { FileListItem } from "@/types/ipc";

interface OpenPreparedNoteOptions {
  allowClassified?: boolean;
  documentOpenToken?: string;
  onDocumentOpenTokenRetained?: () => void;
  openBudgetKind?: NoteOpenBudgetKind;
  openStartedAt?: number;
  openTraceRequest?: PrepareNoteOpenRequest;
  preparedNote?: PreparedNoteOpen;
  priority?: DocumentOpenPriority;
  source?: NoteOpenSource;
}

interface OpenTabLike {
  path: string;
}

interface RememberPreparedRequestOptions {
  useSignature?: boolean;
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

const WARM_PREPARED_NOTES_LIMIT = 8;

export function usePreparedNoteOpener<
  OpenOptions extends OpenPreparedNoteOptions,
>({ openNote, openTabs }: UsePreparedNoteOpenerOptions<OpenOptions>) {
  const preparedRequestsRef = useRef(new Map<string, PrepareNoteOpenRequest>());
  const prepareTokensRef = useRef(new Map<string, symbol>());
  const [warmPreparedNotes, setWarmPreparedNotes] = useState<
    PreparedNoteOpen[]
  >([]);

  const enrichRequestSignature = useCallback(
    async (
      request: PrepareNoteOpenRequest,
    ): Promise<PrepareNoteOpenRequest> => {
      if (request.signature) return request;
      try {
        const signature = await fileSignature(request.path, {
          allowClassified: request.allowClassified === true,
        });
        return {
          ...request,
          meta: {
            ...request.meta,
            isLocked: signature.isLocked,
          },
          signature: {
            byteLength: signature.byteLength,
            contentHash: signature.contentHash,
            modifiedMs: signature.modifiedMs,
          },
        };
      } catch {
        return request;
      }
    },
    [],
  );

  const rememberPreparedRequest = useCallback(
    (
      request: PrepareNoteOpenRequest,
      options: RememberPreparedRequestOptions = {},
    ) => {
      const token = Symbol(request.path);
      const shouldUseSignature = options.useSignature !== false;
      preparedRequestsRef.current.set(request.path, request);
      prepareTokensRef.current.set(request.path, token);

      const isLatest = (path: string) =>
        prepareTokensRef.current.get(path) === token;
      const requestPromise = shouldUseSignature
        ? enrichRequestSignature(request)
        : Promise.resolve(request);

      void requestPromise
        .then((enriched) => {
          if (!isLatest(enriched.path)) return null;
          preparedRequestsRef.current.set(enriched.path, enriched);
          return prepareNoteOpen(enriched);
        })
        .then((prepared) => {
          if (!prepared || !isLatest(prepared.path)) return;
          setWarmPreparedNotes((previous) =>
            [
              prepared,
              ...previous.filter((note) => note.path !== prepared.path),
            ].slice(0, WARM_PREPARED_NOTES_LIMIT),
          );
        })
        .catch(() => {
          if (isLatest(request.path)) {
            preparedRequestsRef.current.delete(request.path);
            prepareTokensRef.current.delete(request.path);
            setWarmPreparedNotes((previous) =>
              previous.filter((note) => note.path !== request.path),
            );
          }
        });
    },
    [enrichRequestSignature],
  );

  const openPreparedNote = useCallback(
    async (path: string, titleHint?: string, options?: OpenOptions) => {
      const openStartedAt = options?.openStartedAt ?? performance.now();
      const remembered = preparedRequestsRef.current.get(path);
      const source = options?.source ?? remembered?.source ?? "tab";
      const priority = options?.priority ?? "foreground";
      const baseRequest: PrepareNoteOpenRequest = {
        ...remembered,
        allowClassified:
          options?.allowClassified ?? remembered?.allowClassified,
        path,
        priority,
        source,
        titleHint: titleHint ?? remembered?.titleHint,
      };
      const requiresFreshSignature =
        priority === "foreground" || priority === "hot";
      const lookupRequest = requiresFreshSignature
        ? await enrichRequestSignature(baseRequest)
        : baseRequest;
      const openTraceRequest = options?.openTraceRequest ?? lookupRequest;
      if (source === "welcome" || source === "workspace_empty") {
        // Empty-surface recent open is user-facing recovery after startup.
        // Warm HTML is strictly speculative: it must never decide whether that
        // path opens. Re-read Markdown through the normal open pipeline so a
        // stale/prepared editor surface cannot strand the user on Home.
        const { preparedNote: _preparedNote, ...authoritativeOptions } =
          options ?? {};
        void _preparedNote;
        await openNote(path, titleHint, {
          ...authoritativeOptions,
          openBudgetKind: options?.openBudgetKind ?? "none",
          openStartedAt,
          openTraceRequest,
        } as OpenOptions);
        return;
      }
      const shouldScopeOpen = priority === "foreground" || priority === "hot";
      let documentOpenToken: string | null = null;
      let documentOpenTokenRetained = false;
      let documentOpenResult: Awaited<ReturnType<typeof documentOpen>> | null =
        null;
      const providedPreparedNote = options?.preparedNote;
      const cachedPreparedNote =
        providedPreparedNote ?? getPreparedNoteOpen(lookupRequest);
      const usePreparationPipeline = Boolean(
        cachedPreparedNote || (remembered && lookupRequest.signature),
      );

      if (shouldScopeOpen) {
        try {
          if (usePreparationPipeline) {
            documentOpenToken = (await documentOpenBegin()).token;
          } else {
            documentOpenResult = await documentOpen(
              path,
              baseRequest.allowClassified === true,
            );
            documentOpenToken = documentOpenResult.token;
          }
        } catch {
          documentOpenToken = null;
          documentOpenResult = null;
        }
      }

      const openOptionsWithScope = {
        ...options,
        ...(documentOpenToken ? { documentOpenToken } : {}),
        onDocumentOpenTokenRetained: () => {
          documentOpenTokenRetained = true;
          options?.onDocumentOpenTokenRetained?.();
        },
      } as OpenOptions;

      try {
        if (openTabs.some((tab) => tab.path === path)) {
          await openNote(path, titleHint, {
            ...openOptionsWithScope,
            openBudgetKind: options?.openBudgetKind ?? "warm",
            openStartedAt,
            openTraceRequest,
          } as OpenOptions);
          return;
        }

        let preparedNote: PreparedNoteOpen | undefined;
        try {
          preparedNote =
            cachedPreparedNote ??
            (documentOpenResult
              ? await prepareNoteOpenFromContent(lookupRequest, {
                  content: documentOpenResult.content,
                  isLocked: documentOpenResult.isLocked,
                })
              : await prepareNoteOpen(lookupRequest));
        } catch {
          // Markdown remains authoritative and `openNote` can reread it. A
          // cache/parser failure must never turn a valid user click into an
          // invisible return to Home.
          await openNote(path, titleHint, {
            ...openOptionsWithScope,
            openBudgetKind: options?.openBudgetKind ?? "none",
            openStartedAt,
            openTraceRequest,
          } as OpenOptions);
          return;
        }

        await openNote(path, titleHint, {
          ...openOptionsWithScope,
          openBudgetKind:
            options?.openBudgetKind ??
            (cachedPreparedNote || documentOpenResult ? "hot" : "none"),
          openStartedAt,
          openTraceRequest,
          preparedNote,
        } as OpenOptions);
      } finally {
        if (documentOpenToken && !documentOpenTokenRetained) {
          try {
            await documentOpenEnd(documentOpenToken);
          } catch {
            /* Best-effort scheduler hint cleanup; opening must not fail here. */
          }
        }
      }
    },
    [enrichRequestSignature, openNote, openTabs],
  );
  const prepareVisibleNote = useCallback(
    (file: FileListItem, source: NoteOpenSource = "file-tree") => {
      rememberPreparedRequest({
        meta: { isLocked: file.isLocked, updatedAt: file.updatedAt },
        path: file.path,
        priority: "warm",
        source,
        titleHint: file.title,
      });
    },
    [rememberPreparedRequest],
  );

  const prepareNotePath = useCallback(
    (path: string, titleHint?: string, source: NoteOpenSource = "link") => {
      rememberPreparedRequest({ path, priority: "warm", source, titleHint });
    },
    [rememberPreparedRequest],
  );

  const warmNotePath = useCallback(
    (
      path: string,
      titleHint?: string,
      options: {
        isLocked?: boolean;
        priority?: DocumentOpenPriority;
        source?: NoteOpenSource;
        useSignature?: boolean;
      } = {},
    ) => {
      const source = options.source ?? "startup";
      rememberPreparedRequest(
        {
          meta: { isLocked: options.isLocked },
          path,
          priority: options.priority ?? "background",
          source,
          titleHint,
        },
        { useSignature: options.useSignature ?? source !== "startup" },
      );
    },
    [rememberPreparedRequest],
  );

  const prepareClassifiedNotePath = useCallback(
    (
      path: string,
      titleHint?: string,
      source: NoteOpenSource = "file-tree",
    ) => {
      rememberPreparedRequest({
        allowClassified: true,
        path,
        priority: "warm",
        source,
        titleHint,
      });
    },
    [rememberPreparedRequest],
  );

  const invalidatePreparedNote = useCallback((path: string) => {
    invalidateNoteOpenPreparation(path);
    clearCachedEditorHtml(path);
    preparedRequestsRef.current.delete(path);
    prepareTokensRef.current.delete(path);
    setWarmPreparedNotes((previous) =>
      previous.filter((note) => note.path !== path),
    );
  }, []);

  const clearPreparedNotes = useCallback((namespace?: NoteOpenNamespace) => {
    clearNoteOpenPreparationCache(namespace);
    if (!namespace) {
      preparedRequestsRef.current.clear();
      prepareTokensRef.current.clear();
      setWarmPreparedNotes([]);
      return;
    }
    for (const [path, request] of preparedRequestsRef.current) {
      const isClassified = request.allowClassified === true;
      if (namespace === "classified" && isClassified) {
        preparedRequestsRef.current.delete(path);
        prepareTokensRef.current.delete(path);
      }
      if (namespace === "normal" && !isClassified) {
        preparedRequestsRef.current.delete(path);
        prepareTokensRef.current.delete(path);
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
    warmNotePath,
    warmPreparedNotes,
  };
}
