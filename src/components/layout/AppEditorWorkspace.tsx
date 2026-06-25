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
import {
  EDITOR_HTML_CACHE_FORMAT_VERSION,
  editorHtmlDigest,
} from "@/lib/editor-html-cache";
import { cn } from "@/lib/utils";
import type { ArtifactTab } from "@/types/assistant-artifact";
import type { MediaTab } from "@/hooks/useMediaTabs";
import type { HomePendingOpen } from "@/lib/home-open-transition";
import type { PreparedNoteOpen } from "@/lib/document-open-runtime";
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
  editor: Editor | null;
  identityKey: string;
  ready: boolean;
  snapshot: EditorSurfaceSnapshot;
}

interface DocumentLoadingGate {
  identityKey: string | null;
  shownAt: number | null;
  visible: boolean;
}

const DOCUMENT_OPEN_LOADING_DELAY_MS = 100;
const DOCUMENT_OPEN_LOADING_MIN_MS = 800;

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
  ) => void | Promise<void>;
  onPrepareNote?: (file: FileListItem) => void;
  onPrepareNotePath?: (path: string, titleHint?: string) => void;
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
  return [
    snapshot.path,
    EDITOR_HTML_CACHE_FORMAT_VERSION,
    editorHtmlDigest(snapshot.editorBodyMarkdown),
  ].join("\0");
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

  const currentEditorSurface = useMemo<EditorSurfaceSnapshot | null>(() => {
    if (
      !effectiveNotePath ||
      (homeActive && !pendingNoteOpen) ||
      (!pendingNoteOpen && pendingOpen && !pendingOpen.error && !homeActive) ||
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
    homeActive,
    pendingNoteOpen,
    pendingOpen,
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
  const [documentLoadingGate, setDocumentLoadingGate] =
    useState<DocumentLoadingGate>({
      identityKey: null,
      shownAt: null,
      visible: false,
    });
  const documentLoadingGateRef = useRef(documentLoadingGate);
  const loadingDelayTimerRef = useRef<number | null>(null);
  const loadingReleaseTimerRef = useRef<number | null>(null);

  const syncDocumentLoadingGate = useCallback((next: DocumentLoadingGate) => {
    documentLoadingGateRef.current = next;
    setDocumentLoadingGate(next);
  }, []);

  const clearLoadingDelayTimer = useCallback(() => {
    if (loadingDelayTimerRef.current === null) return;
    window.clearTimeout(loadingDelayTimerRef.current);
    loadingDelayTimerRef.current = null;
  }, []);

  const clearLoadingReleaseTimer = useCallback(() => {
    if (loadingReleaseTimerRef.current === null) return;
    window.clearTimeout(loadingReleaseTimerRef.current);
    loadingReleaseTimerRef.current = null;
  }, []);

  useEffect(() => {
    if (!currentEditorSurface) return;
    const identityKey = surfaceIdentity(currentEditorSurface);
    setSurfaceRecords((previous) => {
      const existing = previous.find(
        (record) => record.snapshot.path === currentEditorSurface.path,
      );
      if (!existing) {
        return [
          ...previous,
          {
            editor: null,
            identityKey,
            ready: false,
            snapshot: currentEditorSurface,
          },
        ];
      }
      return previous.map((record) => {
        if (record.snapshot.path !== currentEditorSurface.path) return record;
        if (record.identityKey !== identityKey) {
          return {
            editor: null,
            identityKey,
            ready: false,
            snapshot: currentEditorSurface,
          };
        }
        return {
          ...record,
          snapshot: currentEditorSurface,
        };
      });
    });
  }, [currentEditorSurface]);

  useEffect(() => {
    const allowed = new Set(openNotePaths);
    setSurfaceRecords((previous) => {
      const next = previous.filter(
        (record) =>
          allowed.has(record.snapshot.path) ||
          record.snapshot.path === effectiveNotePathRef.current,
      );
      return next.length === previous.length ? previous : next;
    });
  }, [openNotePaths]);

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
      homePendingMatchesPath(pendingOpen, currentEditorSurface.path)),
  );
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
    currentEditorSurface &&
    documentLoadingGate.identityKey === currentSurfaceIdentity &&
    documentLoadingGate.visible,
  );

  useEffect(() => {
    clearLoadingDelayTimer();
    clearLoadingReleaseTimer();

    const pendingForCurrentSurface =
      pendingOpen &&
      currentEditorSurface &&
      homePendingMatchesPath(pendingOpen, currentEditorSurface.path)
        ? pendingOpen
        : null;

    if (!currentSurfaceIdentity || activeSurfaceReadyRef.current) {
      syncDocumentLoadingGate({
        identityKey: null,
        shownAt: null,
        visible: false,
      });
      return;
    }

    if (pendingForCurrentSurface) {
      syncDocumentLoadingGate({
        identityKey: currentSurfaceIdentity,
        shownAt: pendingForCurrentSurface.startedAt,
        visible: true,
      });
      return;
    }

    syncDocumentLoadingGate({
      identityKey: currentSurfaceIdentity,
      shownAt: null,
      visible: false,
    });
    loadingDelayTimerRef.current = window.setTimeout(() => {
      loadingDelayTimerRef.current = null;
      if (
        documentLoadingGateRef.current.identityKey !== currentSurfaceIdentity
      ) {
        return;
      }
      syncDocumentLoadingGate({
        identityKey: currentSurfaceIdentity,
        shownAt: Date.now(),
        visible: true,
      });
    }, DOCUMENT_OPEN_LOADING_DELAY_MS);

    return () => {
      clearLoadingDelayTimer();
      clearLoadingReleaseTimer();
    };
  }, [
    clearLoadingDelayTimer,
    clearLoadingReleaseTimer,
    currentEditorSurface,
    currentSurfaceIdentity,
    pendingOpen,
    syncDocumentLoadingGate,
  ]);

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
        previous.map((record) =>
          record.snapshot.path === path ? { ...record, editor } : record,
        ),
      );
      if (!editor && path === activePathRef.current) {
        handleEditorReady(null);
      }
    },
    [handleEditorReady],
  );

  const releaseSurfaceFirstFrame = useCallback(
    (path: string, identityKey: string, editor: Editor) => {
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
    [syncDocumentLoadingGate],
  );

  const handleSurfaceFirstFrameReady = useCallback(
    (path: string, identityKey: string, editor: Editor) => {
      clearLoadingDelayTimer();
      clearLoadingReleaseTimer();
      setSurfaceRecords((previous) =>
        previous.map((record) =>
          record.snapshot.path === path
            ? { ...record, editor, ready: true }
            : record,
        ),
      );

      const gate = documentLoadingGateRef.current;
      const pendingStartedAt =
        pendingOpenRef.current &&
        homePendingMatchesPath(pendingOpenRef.current, path) &&
        typeof pendingOpenRef.current.startedAt === "number"
          ? pendingOpenRef.current.startedAt
          : null;
      const shownAt =
        gate.identityKey === identityKey &&
        gate.visible &&
        typeof gate.shownAt === "number"
          ? gate.shownAt
          : pendingStartedAt;
      if (shownAt !== null) {
        const remaining = Math.max(
          DOCUMENT_OPEN_LOADING_MIN_MS - (Date.now() - shownAt),
          0,
        );
        if (remaining > 0) {
          loadingReleaseTimerRef.current = window.setTimeout(() => {
            loadingReleaseTimerRef.current = null;
            releaseSurfaceFirstFrame(path, identityKey, editor);
          }, remaining);
          return;
        }
      }

      releaseSurfaceFirstFrame(path, identityKey, editor);
    },
    [
      clearLoadingDelayTimer,
      clearLoadingReleaseTimer,
      releaseSurfaceFirstFrame,
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
              onOpenWikiLink={(title) => openNoteLeavingHome(title + ".md")}
              onPrepareWikiLink={(title) =>
                onPrepareNotePath?.(title + ".md", title)
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
      {(showDocumentLoading || pendingOpenLoading) && loadingPath ? (
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
          onOpenNote={openNoteLeavingHome}
          onPrepareNote={onPrepareNotePath}
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
            onOpen={openNoteLeavingHome}
            onNew={handleNewNoteLeavingHome}
            onQuickOpen={onOpenQuickOpen}
            onSearch={onOpenSearch}
            onOpenAiManagement={onOpenAiManagement}
            onPrepare={onPrepareNote}
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
