import type { Editor } from "@tiptap/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Dispatch, SetStateAction, ReactNode } from "react";

import { ArtifactWorkspaceView } from "@/components/layout/ArtifactWorkspaceView";
import { DocumentOpenLoadingSurface } from "@/components/layout/DocumentOpenLoadingSurface";
import { EditorFindReplaceBar } from "@/components/editor/EditorFindReplaceBar";
import { EditorOutline } from "@/components/editor/EditorOutline";
import { MediaWorkspaceView } from "@/components/layout/MediaWorkspaceView";
import { TipTapEditor } from "@/components/editor/TipTapEditor";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { WelcomeEmpty } from "@/components/layout/WelcomeEmpty";
import { IrisContextMenu } from "@/components/ui/iris-context-menu";
import type { IrisContextMenuGroup } from "@/components/ui/iris-context-menu";
import { useHomeRecentNotes } from "@/hooks/useHomeRecentNotes";
import { documentOpenEnd } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import type { ArtifactTab } from "@/types/assistant-artifact";
import type { MediaTab } from "@/hooks/useMediaTabs";
import type { HomePendingOpen } from "@/lib/home-open-transition";
import type {
  DocumentOpenPriority,
  NoteOpenSource,
  PreparedNoteOpen,
} from "@/lib/document-open-runtime";
import { DOCUMENT_OPEN_BUDGETS } from "@/lib/document-open-runtime";
import type { PendingNoteOpen } from "@/hooks/useTabManager";
import type { EditorHtmlCacheNamespace } from "@/lib/editor-html-cache";
import type { FileListItem } from "@/types/ipc";

interface EditorMenuPort {
  menu: {
    open: boolean;
    x: number;
    y: number;
  };
  groups: IrisContextMenuGroup[];
  handleContextMenu: (event: React.MouseEvent) => void;
  close: () => void;
}

interface EditorSurfaceSnapshot {
  activeFileLocked: boolean;
  activeNoteIsClassified: boolean;
  cacheNamespace: EditorHtmlCacheNamespace;
  editorBodyMarkdown: string;
  editorContentTick: number;
  editorPreparedHtml: string | null;
  editorTitleSlot: ReactNode;
  path: string;
  title: string;
}

interface EditorSurfaceRecord {
  contentReady: boolean;
  editor: Editor | null;
  identityKey: string;
  lastActivatedAt: number;
  ready: boolean;
  snapshot: EditorSurfaceSnapshot;
}

interface DocumentLoadingGate {
  identityKey: string | null;
  shownAt: number | null;
  visible: boolean;
}

const DOCUMENT_OPEN_LOADING_WATCHDOG_MS = 5000;
const READY_SURFACE_RETAIN_LIMIT = 8;

interface AppEditorWorkspaceProps {
  activeFileLocked: boolean;
  activeArtifactTab: ArtifactTab | null;
  activeMediaTab: MediaTab | null;
  activeNoteIsClassified: boolean;
  activePath: string | null;
  editorBodyMarkdown: string;
  editorContentTick: number;
  editorContextMenu: EditorMenuPort;
  editorInstance: Editor | null;
  editorPreparedHtml?: string | null;
  editorTitleSlot: ReactNode;
  editorZoom: number;
  findReplaceMode: "find" | "replace";
  findReplaceOpen: boolean;
  handleDirty: () => void;
  handleEditorReady: (editor: Editor | null) => void;
  handleLockToggle: (locked: boolean) => Promise<void>;
  handleNewNoteLeavingHome: () => void | Promise<void>;
  getNoteContent: () => string;
  homeActive: boolean;
  inlineAi: {
    retry: (editor: Editor) => Promise<void>;
    dismiss: (editor: Editor) => void;
    finish: () => void;
  };
  onOutlineOpenChange: (open: boolean) => void;
  onOpenAiManagement: () => void;
  onOpenQuickOpen: () => void;
  onOpenSearch: () => void;
  openNoteLeavingHome: (
    path: string,
    titleHint?: string,
    options?: {
      priority?: DocumentOpenPriority;
      source?: NoteOpenSource;
    },
  ) => void | Promise<void>;
  onPrepareNote?: (file: FileListItem, source?: NoteOpenSource) => void;
  onPrepareNotePath?: (
    path: string,
    titleHint?: string,
    source?: NoteOpenSource,
  ) => void;
  outlineOpen: boolean;
  pendingOpen: HomePendingOpen | null;
  pendingNoteOpen: PendingNoteOpen | null;
  onPendingOpenSettled?: (pending: HomePendingOpen) => boolean;
  commitPendingNoteOpen: (path: string, sequence: number) => boolean;
  runEditorActionById: (actionId: string) => void;
  setFindReplaceMode: Dispatch<SetStateAction<"find" | "replace">>;
  setFindReplaceOpen: (open: boolean) => void;
  updateEditorStats: (stats: {
    characterCount: number;
    readingMinutes: number;
  }) => void;
  onPatchApplied?: (newContent: string) => void;
  onVaultRefresh?: () => void;
  vaultIndexEpoch: number;
  vaultPath: string | null;
  warmPreparedNotes?: readonly PreparedNoteOpen[] | null;
  openNotePaths?: readonly string[];
  zen: boolean;
}

function displayTitleFromPath(path: string): string {
  return path.split(/[\\/]/).pop()?.replace(/\.md$/i, "") || path;
}

function snapshotSafePath(path: string): string {
  return path;
}

function homePendingMatchesPath(
  pending: HomePendingOpen | null,
  path: string,
  sequence?: number,
): pending is HomePendingOpen {
  if (!pending || pending.error) return false;
  if (pending.kind === "new-note") {
    return sequence === undefined || pending.sequence === sequence;
  }
  return pending.path === path;
}

function surfaceIdentity(snapshot: EditorSurfaceSnapshot): string {
  return snapshot.path;
}

function pendingOpenIdentity(pending: HomePendingOpen): string {
  const path = pending.kind === "new-note" ? "new-note" : pending.path;
  return `pending:${pending.kind}:${path}:${pending.sequence}`;
}

function endDocumentOpenToken(token?: string): void {
  if (!token) return;
  void documentOpenEnd(token).catch(() => undefined);
}

function retainedSurfaceRecords(
  records: EditorSurfaceRecord[],
  context: { activePath: string | null; pendingPath: string | null },
): EditorSurfaceRecord[] {
  const requiredPaths = new Set(
    [context.activePath, context.pendingPath].filter(
      (path): path is string => typeof path === "string" && path.length > 0,
    ),
  );
  const required = records.filter((record) =>
    requiredPaths.has(record.snapshot.path),
  );
  const cleanReady = records
    .filter(
      (record) =>
        !requiredPaths.has(record.snapshot.path) && record.ready === true,
    )
    .sort((a, b) => b.lastActivatedAt - a.lastActivatedAt)
    .slice(0, READY_SURFACE_RETAIN_LIMIT);
  const retained = new Set([...required, ...cleanReady]);
  return records.filter((record) => retained.has(record));
}

export function AppEditorWorkspace({
  activeFileLocked,
  activeArtifactTab,
  activeMediaTab,
  activeNoteIsClassified,
  activePath,
  editorBodyMarkdown,
  editorContentTick,
  editorContextMenu,
  editorInstance,
  editorPreparedHtml = null,
  editorTitleSlot,
  editorZoom,
  findReplaceMode,
  findReplaceOpen,
  handleDirty,
  handleEditorReady,
  handleLockToggle,
  handleNewNoteLeavingHome,
  getNoteContent,
  homeActive,
  inlineAi,
  onOutlineOpenChange,
  onOpenAiManagement,
  onOpenQuickOpen,
  onOpenSearch,
  openNoteLeavingHome,
  onPrepareNote,
  onPrepareNotePath,
  outlineOpen,
  pendingOpen,
  pendingNoteOpen,
  onPendingOpenSettled,
  commitPendingNoteOpen,
  runEditorActionById,
  setFindReplaceMode,
  setFindReplaceOpen,
  updateEditorStats,
  onPatchApplied,
  onVaultRefresh,
  vaultIndexEpoch,
  vaultPath,
  warmPreparedNotes,
  openNotePaths = activePath ? [activePath] : [],
  zen,
}: AppEditorWorkspaceProps) {
  const { recentNotes, refreshRecent } = useHomeRecentNotes({
    onPrepare: onPrepareNote,
    vaultIndexEpoch,
    vaultPath,
  });

  const effectiveNotePath = pendingNoteOpen?.path ?? activePath;
  const effectiveBodyMarkdown =
    pendingNoteOpen?.bodyMarkdown ?? editorBodyMarkdown;
  const effectivePreparedHtml =
    pendingNoteOpen?.preparedEditorHtml ?? editorPreparedHtml;
  const effectiveLocked = pendingNoteOpen?.isLocked ?? activeFileLocked;
  const effectiveNamespace =
    pendingNoteOpen?.namespace ??
    (activeNoteIsClassified ? "classified" : "normal");
  const effectiveTitle = pendingNoteOpen?.title;
  const hideCurrentSurfaceForPendingOpen = Boolean(
    !pendingNoteOpen &&
    pendingOpen &&
    pendingOpen.kind !== "new-note" &&
    !pendingOpen.error &&
    !homeActive,
  );

  const currentEditorSurface = useMemo<EditorSurfaceSnapshot | null>(() => {
    if (
      !effectiveNotePath ||
      (homeActive && !pendingNoteOpen) ||
      hideCurrentSurfaceForPendingOpen ||
      activeArtifactTab ||
      activeMediaTab
    ) {
      return null;
    }
    const prepared =
      effectivePreparedHtml ??
      warmPreparedNotes?.find((note) => note.path === effectiveNotePath)
        ?.preparedEditorHtml ??
      null;
    return {
      activeFileLocked: effectiveLocked,
      activeNoteIsClassified: effectiveNamespace === "classified",
      cacheNamespace: effectiveNamespace,
      editorBodyMarkdown: effectiveBodyMarkdown,
      editorContentTick,
      editorPreparedHtml: prepared,
      editorTitleSlot: pendingNoteOpen ? null : editorTitleSlot,
      path: effectiveNotePath,
      title: effectiveTitle ?? displayTitleFromPath(effectiveNotePath),
    };
  }, [
    activeArtifactTab,
    activeMediaTab,
    editorContentTick,
    editorTitleSlot,
    effectiveBodyMarkdown,
    effectiveLocked,
    effectiveNamespace,
    effectiveNotePath,
    effectivePreparedHtml,
    effectiveTitle,
    hideCurrentSurfaceForPendingOpen,
    homeActive,
    pendingNoteOpen,
    warmPreparedNotes,
  ]);

  const activePathRef = useRef(activePath);
  activePathRef.current = activePath;
  const effectiveNotePathRef = useRef(effectiveNotePath);
  effectiveNotePathRef.current = effectiveNotePath;
  const pendingNoteOpenRef = useRef(pendingNoteOpen);
  pendingNoteOpenRef.current = pendingNoteOpen;
  const pendingOpenRef = useRef(pendingOpen);
  pendingOpenRef.current = pendingOpen;
  const commitPendingNoteOpenRef = useRef(commitPendingNoteOpen);
  commitPendingNoteOpenRef.current = commitPendingNoteOpen;
  const handleEditorReadyRef = useRef(handleEditorReady);
  handleEditorReadyRef.current = handleEditorReady;
  const onPendingOpenSettledRef = useRef(onPendingOpenSettled);
  onPendingOpenSettledRef.current = onPendingOpenSettled;
  const [surfaceRecords, setSurfaceRecords] = useState<EditorSurfaceRecord[]>(
    [],
  );
  const surfaceActivationSeqRef = useRef(0);
  const [documentLoadingGate, setDocumentLoadingGate] =
    useState<DocumentLoadingGate>({
      identityKey: null,
      shownAt: null,
      visible: false,
    });

  const retainCurrentSurfaceRecords = useCallback(
    (records: EditorSurfaceRecord[]) =>
      retainedSurfaceRecords(records, {
        activePath: effectiveNotePathRef.current ?? activePathRef.current,
        pendingPath:
          pendingNoteOpenRef.current?.path ??
          pendingOpenRef.current?.path ??
          null,
      }),
    [],
  );
  const documentLoadingGateRef = useRef(documentLoadingGate);
  const loadingWatchdogTimerRef = useRef<number | null>(null);
  const loadingVisibilityTimerRef = useRef<number | null>(null);

  const syncDocumentLoadingGate = useCallback((next: DocumentLoadingGate) => {
    documentLoadingGateRef.current = next;
    setDocumentLoadingGate(next);
  }, []);

  const clearLoadingWatchdogTimer = useCallback(() => {
    if (loadingWatchdogTimerRef.current === null) return;
    window.clearTimeout(loadingWatchdogTimerRef.current);
    loadingWatchdogTimerRef.current = null;
  }, []);

  const clearLoadingVisibilityTimer = useCallback(() => {
    if (loadingVisibilityTimerRef.current === null) return;
    window.clearTimeout(loadingVisibilityTimerRef.current);
    loadingVisibilityTimerRef.current = null;
  }, []);

  const scheduleDocumentLoadingVisibility = useCallback(
    (identityKey: string, startedAt: number) => {
      clearLoadingVisibilityTimer();
      const elapsed = Math.max(0, performance.now() - startedAt);
      const remaining = DOCUMENT_OPEN_BUDGETS.coldLoadingVisibleMs - elapsed;
      const hiddenGate = {
        identityKey,
        shownAt: startedAt,
        visible: false,
      };

      if (remaining <= 0) {
        syncDocumentLoadingGate({
          ...hiddenGate,
          visible: true,
        });
        return;
      }

      syncDocumentLoadingGate(hiddenGate);
      loadingVisibilityTimerRef.current = window.setTimeout(() => {
        loadingVisibilityTimerRef.current = null;
        if (documentLoadingGateRef.current.identityKey !== identityKey) return;
        syncDocumentLoadingGate({
          identityKey,
          shownAt: startedAt,
          visible: true,
        });
      }, remaining);
    },
    [clearLoadingVisibilityTimer, syncDocumentLoadingGate],
  );

  useEffect(() => {
    if (!currentEditorSurface) return;
    const identityKey = surfaceIdentity(currentEditorSurface);
    const lastActivatedAt = ++surfaceActivationSeqRef.current;
    setSurfaceRecords((previous) => {
      const existing = previous.find(
        (record) => record.snapshot.path === currentEditorSurface.path,
      );
      if (!existing) {
        return retainCurrentSurfaceRecords([
          ...previous,
          {
            contentReady: false,
            editor: null,
            identityKey,
            lastActivatedAt,
            ready: false,
            snapshot: currentEditorSurface,
          },
        ]);
      }
      return retainCurrentSurfaceRecords(
        previous.map((record) => {
          if (record.snapshot.path !== currentEditorSurface.path) return record;
          const contentChanged =
            record.snapshot.editorContentTick !==
              currentEditorSurface.editorContentTick ||
            record.snapshot.editorBodyMarkdown !==
              currentEditorSurface.editorBodyMarkdown ||
            record.snapshot.editorPreparedHtml !==
              currentEditorSurface.editorPreparedHtml;
          return {
            ...record,
            contentReady: contentChanged ? false : record.contentReady,
            ready: contentChanged ? false : record.ready,
            lastActivatedAt,
            snapshot: currentEditorSurface,
          };
        }),
      );
    });
  }, [currentEditorSurface, retainCurrentSurfaceRecords]);

  useEffect(() => {
    const allowed = new Set(openNotePaths);
    setSurfaceRecords((previous) => {
      const next = previous.filter(
        (record) =>
          allowed.has(record.snapshot.path) ||
          record.snapshot.path === effectiveNotePathRef.current,
      );
      const retained = retainCurrentSurfaceRecords(next);
      return retained.length === previous.length &&
        retained.every((record, index) => record === previous[index])
        ? previous
        : retained;
    });
  }, [openNotePaths, retainCurrentSurfaceRecords]);

  const activeSurfaceRecord = surfaceRecords.find(
    (record) => record.snapshot.path === effectiveNotePath,
  );
  const activeEditorReady = Boolean(activeSurfaceRecord?.ready);
  const currentSurfaceIdentity = currentEditorSurface
    ? surfaceIdentity(currentEditorSurface)
    : null;
  const activeSurfaceReadyRef = useRef(false);
  activeSurfaceReadyRef.current = Boolean(
    activeSurfaceRecord?.identityKey === currentSurfaceIdentity &&
    activeSurfaceRecord.ready,
  );
  const pendingOpenLoading = Boolean(
    !homeActive &&
    !activeArtifactTab &&
    !activeMediaTab &&
    pendingOpen &&
    !pendingOpen.error &&
    (!currentEditorSurface ||
      homePendingMatchesPath(pendingOpen, currentEditorSurface.path)) &&
    !activeSurfaceRecord?.ready,
  );
  const pendingOpenLoadingIdentity =
    pendingOpenLoading && pendingOpen ? pendingOpenIdentity(pendingOpen) : null;
  const loadingPath =
    currentEditorSurface?.path ?? pendingNoteOpen?.path ?? pendingOpen?.path;
  const loadingTitle =
    currentEditorSurface?.title ??
    pendingNoteOpen?.title ??
    pendingOpen?.title ??
    (loadingPath ? displayTitleFromPath(loadingPath) : null);
  const showDocumentLoading = Boolean(
    !activeArtifactTab &&
    !activeMediaTab &&
    !homeActive &&
    (currentEditorSurface || pendingOpenLoadingIdentity) &&
    documentLoadingGate.identityKey ===
      (currentSurfaceIdentity ?? pendingOpenLoadingIdentity) &&
    documentLoadingGate.visible,
  );

  useEffect(() => {
    if (!activePath) {
      handleEditorReady(null);
      return;
    }
    const record = surfaceRecords.find(
      (item) => item.snapshot.path === effectiveNotePath,
    );
    if (record?.ready && record.editor) {
      handleEditorReady(record.editor);
    }
  }, [activePath, effectiveNotePath, handleEditorReady, surfaceRecords]);

  const handleSurfaceEditorReady = useCallback(
    (path: string, editor: Editor | null) => {
      setSurfaceRecords((previous) =>
        retainCurrentSurfaceRecords(
          previous.map((record) =>
            record.snapshot.path === path ? { ...record, editor } : record,
          ),
        ),
      );
      if (!editor && path === activePathRef.current) {
        handleEditorReady(null);
      }
    },
    [handleEditorReady, retainCurrentSurfaceRecords],
  );

  const handleSurfaceContentReady = useCallback(
    (path: string, editor: Editor) => {
      setSurfaceRecords((previous) =>
        retainCurrentSurfaceRecords(
          previous.map((record) =>
            record.snapshot.path === path
              ? { ...record, contentReady: true, editor }
              : record,
          ),
        ),
      );
    },
    [retainCurrentSurfaceRecords],
  );

  const releaseSurfaceFirstFrame = useCallback(
    (path: string, identityKey: string, editor: Editor) => {
      clearLoadingVisibilityTimer();
      if (documentLoadingGateRef.current.identityKey === identityKey) {
        syncDocumentLoadingGate({
          identityKey: null,
          shownAt: null,
          visible: false,
        });
      }

      const pending = pendingNoteOpenRef.current;
      if (pending?.path === path) {
        const committed = commitPendingNoteOpenRef.current(
          snapshotSafePath(path),
          pending.sequence,
        );
        const homePending = pendingOpenRef.current;
        endDocumentOpenToken(pending.documentOpenToken);
        if (
          committed &&
          homePending &&
          !homePending.error &&
          homePendingMatchesPath(homePending, path, pending.sequence)
        ) {
          onPendingOpenSettledRef.current?.(homePending);
        }
        return;
      }
      if (path === activePathRef.current) {
        handleEditorReadyRef.current(editor);
      }
    },
    [clearLoadingVisibilityTimer, syncDocumentLoadingGate],
  );

  useEffect(() => {
    clearLoadingWatchdogTimer();
    clearLoadingVisibilityTimer();

    const pendingForCurrentSurface =
      pendingOpen &&
      currentEditorSurface &&
      homePendingMatchesPath(pendingOpen, currentEditorSurface.path)
        ? pendingOpen
        : null;
    const pendingOnlyIdentity =
      !currentSurfaceIdentity && pendingOpenLoadingIdentity
        ? pendingOpenLoadingIdentity
        : null;

    if (!currentSurfaceIdentity && !pendingOnlyIdentity) {
      syncDocumentLoadingGate({
        identityKey: null,
        shownAt: null,
        visible: false,
      });
      return;
    }

    if (!currentSurfaceIdentity && pendingOnlyIdentity && pendingOpen) {
      scheduleDocumentLoadingVisibility(
        pendingOnlyIdentity,
        pendingOpen.startedAt,
      );
      return;
    }

    if (!currentSurfaceIdentity || activeSurfaceReadyRef.current) {
      syncDocumentLoadingGate({
        identityKey: null,
        shownAt: null,
        visible: false,
      });
      return;
    }

    loadingWatchdogTimerRef.current = window.setTimeout(() => {
      loadingWatchdogTimerRef.current = null;
      setSurfaceRecords((previous) => {
        const record = previous.find(
          (item) => item.identityKey === currentSurfaceIdentity,
        );
        if (!record?.editor || !record.contentReady || record.ready) {
          return previous;
        }

        releaseSurfaceFirstFrame(
          record.snapshot.path,
          record.identityKey,
          record.editor,
        );
        return retainCurrentSurfaceRecords(
          previous.map((item) =>
            item.identityKey === currentSurfaceIdentity
              ? { ...item, ready: true }
              : item,
          ),
        );
      });
    }, DOCUMENT_OPEN_LOADING_WATCHDOG_MS);

    if (pendingForCurrentSurface) {
      scheduleDocumentLoadingVisibility(
        currentSurfaceIdentity,
        pendingForCurrentSurface.startedAt,
      );
      return;
    }

    scheduleDocumentLoadingVisibility(
      currentSurfaceIdentity,
      performance.now(),
    );

    return () => {
      clearLoadingWatchdogTimer();
      clearLoadingVisibilityTimer();
    };
  }, [
    clearLoadingWatchdogTimer,
    clearLoadingVisibilityTimer,
    currentEditorSurface,
    currentSurfaceIdentity,
    pendingOpen,
    pendingOpenLoadingIdentity,
    releaseSurfaceFirstFrame,
    retainCurrentSurfaceRecords,
    scheduleDocumentLoadingVisibility,
    syncDocumentLoadingGate,
  ]);

  const handleSurfaceFirstFrameReady = useCallback(
    (path: string, identityKey: string, editor: Editor) => {
      clearLoadingWatchdogTimer();
      setSurfaceRecords((previous) =>
        retainCurrentSurfaceRecords(
          previous.map((record) =>
            record.snapshot.path === path
              ? { ...record, contentReady: true, editor, ready: true }
              : record,
          ),
        ),
      );

      releaseSurfaceFirstFrame(path, identityKey, editor);
    },
    [
      clearLoadingWatchdogTimer,
      releaseSurfaceFirstFrame,
      retainCurrentSurfaceRecords,
    ],
  );

  const renderEditorSurface = useCallback(
    (record: EditorSurfaceRecord) => {
      const snapshot = record.snapshot;
      const isVisible = snapshot.path === effectiveNotePath && record.ready;
      const visibility = isVisible
        ? "visible"
        : snapshot.path === effectiveNotePath
          ? "staging"
          : "hidden";
      return (
        <div
          key={record.identityKey}
          data-path={snapshot.path}
          data-editor-visibility={visibility}
          aria-hidden={!isVisible}
          className={cn(
            "absolute inset-0 flex min-h-0 flex-1 flex-col",
            !isVisible && "pointer-events-none opacity-0",
          )}
        >
          <ErrorBoundary scope="editor">
            <TipTapEditor
              initialBodyMarkdown={snapshot.editorBodyMarkdown}
              initialEditorHtml={snapshot.editorPreparedHtml}
              contentCacheKey={snapshot.path}
              contentCacheNamespace={snapshot.cacheNamespace}
              vaultPath={vaultPath}
              reingestKey={snapshot.editorContentTick}
              zen={zen}
              zoom={editorZoom}
              mediaLoading="visible"
              titleSlot={snapshot.editorTitleSlot}
              locked={snapshot.activeFileLocked}
              setLocked={
                !snapshot.activeNoteIsClassified
                  ? (locked) => void handleLockToggle(locked)
                  : undefined
              }
              onDirty={handleDirty}
              onSlashCommand={runEditorActionById}
              onBodyContextMenu={editorContextMenu.handleContextMenu}
              onEditorReady={(editor) => {
                handleSurfaceEditorReady(snapshot.path, editor);
              }}
              onContentReady={(editor) => {
                handleSurfaceContentReady(snapshot.path, editor);
              }}
              onFirstFrameReady={(editor) => {
                handleSurfaceFirstFrameReady(
                  snapshot.path,
                  record.identityKey,
                  editor,
                );
              }}
              onBodyStatsChange={updateEditorStats}
              onInlineAiRetry={(ed) => void inlineAi.retry(ed)}
              onInlineAiDismiss={(ed) => inlineAi.dismiss(ed)}
              onInlineAiAccept={() => inlineAi.finish()}
              onOpenWikiLink={(title) =>
                openNoteLeavingHome(title + ".md", title, {
                  priority: "foreground",
                  source: "link",
                })
              }
              onPrepareWikiLink={(title) =>
                onPrepareNotePath?.(title + ".md", title, "link")
              }
            />
          </ErrorBoundary>
        </div>
      );
    },
    [
      effectiveNotePath,
      editorContextMenu.handleContextMenu,
      editorZoom,
      handleDirty,
      handleLockToggle,
      handleSurfaceContentReady,
      handleSurfaceEditorReady,
      handleSurfaceFirstFrameReady,
      inlineAi,
      onPrepareNotePath,
      openNoteLeavingHome,
      runEditorActionById,
      updateEditorStats,
      vaultPath,
      zen,
    ],
  );

  const renderEditorStack = () => (
    <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
      {surfaceRecords.map(renderEditorSurface)}
      {showDocumentLoading && loadingPath ? (
        <DocumentOpenLoadingSurface
          path={loadingPath}
          title={loadingTitle}
          zen={zen}
        />
      ) : null}
      {effectiveNotePath && activeEditorReady ? (
        <EditorOutline
          editor={editorInstance}
          open={outlineOpen}
          notePath={effectiveNotePath}
          onOpenNote={(path: string) =>
            openNoteLeavingHome(path, undefined, {
              priority: "foreground",
              source: "outline",
            })
          }
          onPrepareNote={(path, titleHint) =>
            onPrepareNotePath?.(path, titleHint, "outline")
          }
          locked={currentEditorSurface?.activeFileLocked ?? activeFileLocked}
          zen={zen}
          onOpenChange={onOutlineOpenChange}
        />
      ) : null}
    </div>
  );

  return (
    <div
      data-testid="editor-shell"
      className={cn(
        "relative flex min-h-0 flex-1 flex-col",
        outlineOpen && !zen && effectiveNotePath && "iris-editor-outline-open",
      )}
    >
      {activeArtifactTab && !homeActive ? (
        <ArtifactWorkspaceView
          tab={activeArtifactTab}
          getNoteContent={getNoteContent}
          onPatchApplied={onPatchApplied}
          onVaultRefresh={onVaultRefresh}
        />
      ) : activeMediaTab && !homeActive ? (
        <MediaWorkspaceView tab={activeMediaTab} />
      ) : currentEditorSurface || pendingOpenLoading ? (
        renderEditorStack()
      ) : (
        <>
          <WelcomeEmpty
            onOpen={(path, titleHint, source) =>
              openNoteLeavingHome(path, titleHint, {
                priority: "foreground",
                source,
              })
            }
            onNew={handleNewNoteLeavingHome}
            onQuickOpen={onOpenQuickOpen}
            onSearch={onOpenSearch}
            onOpenAiManagement={onOpenAiManagement}
            onPrepare={(file, source) => onPrepareNote?.(file, source)}
            onRefreshRecent={refreshRecent}
            pendingOpen={pendingOpen}
            recentNotes={recentNotes}
          />
        </>
      )}
      <IrisContextMenu
        open={editorContextMenu.menu.open}
        x={editorContextMenu.menu.x}
        y={editorContextMenu.menu.y}
        groups={editorContextMenu.groups}
        onSelect={runEditorActionById}
        onClose={editorContextMenu.close}
      />
      <EditorFindReplaceBar
        editor={editorInstance}
        mode={findReplaceMode}
        open={findReplaceOpen && Boolean(activePath) && !activeArtifactTab}
        onClose={() => setFindReplaceOpen(false)}
        onModeChange={setFindReplaceMode}
      />
    </div>
  );
}
