import type { Editor } from "@tiptap/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Dispatch, SetStateAction, ReactNode } from "react";

import { ArtifactWorkspaceView } from "@/components/layout/ArtifactWorkspaceView";
import { EditorFindReplaceBar } from "@/components/editor/EditorFindReplaceBar";
import { EditorOutline } from "@/components/editor/EditorOutline";
import { MediaWorkspaceView } from "@/components/layout/MediaWorkspaceView";
import { TipTapEditor } from "@/components/editor/TipTapEditor";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { WelcomeEmpty } from "@/components/layout/WelcomeEmpty";
import { IrisContextMenu } from "@/components/ui/iris-context-menu";
import type { IrisContextMenuGroup } from "@/components/ui/iris-context-menu";
import { useHomeRecentNotes } from "@/hooks/useHomeRecentNotes";
import { EDITOR_HTML_CACHE_FORMAT_VERSION } from "@/lib/editor-html-cache";
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
  editorBodyMarkdown: string;
  cacheNamespace: EditorHtmlCacheNamespace;
  editorContentTick: number;
  editorTitleSlot: ReactNode;
  path: string;
}

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
  zen: boolean;
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
  runEditorActionById,
  setFindReplaceMode,
  setFindReplaceOpen,
  updateEditorStats,
  onPatchApplied,
  onVaultRefresh,
  vaultIndexEpoch,
  vaultPath,
  zen,
}: AppEditorWorkspaceProps) {
  const { recentNotes, refreshRecent } = useHomeRecentNotes({
    onPrepare: onPrepareNote,
    vaultIndexEpoch,
    vaultPath,
  });

  const currentEditorSurface = useMemo<EditorSurfaceSnapshot | null>(() => {
    if (!activePath || homeActive || activeArtifactTab || activeMediaTab) {
      return null;
    }
    return {
      activeFileLocked,
      activeNoteIsClassified,
      cacheNamespace: activeNoteIsClassified ? "classified" : "normal",
      editorBodyMarkdown,
      editorContentTick,
      editorTitleSlot,
      path: activePath,
    };
  }, [
    activeArtifactTab,
    activeFileLocked,
    activeMediaTab,
    activeNoteIsClassified,
    activePath,
    editorBodyMarkdown,
    editorContentTick,
    editorTitleSlot,
    homeActive,
  ]);

  const visibleSurface = currentEditorSurface;
  const visibleEditorPathRef = useRef(visibleSurface?.path ?? null);
  visibleEditorPathRef.current = visibleSurface?.path ?? null;
  const surfaceHydrationKey = visibleSurface
    ? visibleSurface.path + "\0" + visibleSurface.editorContentTick
    : null;
  const [hydratedSurfaceKey, setHydratedSurfaceKey] = useState<string | null>(
    null,
  );

  useEffect(() => {
    setHydratedSurfaceKey(null);
    if (!surfaceHydrationKey) return;
    const timer = window.setTimeout(() => {
      setHydratedSurfaceKey(surfaceHydrationKey);
    }, 0);
    return () => window.clearTimeout(timer);
  }, [surfaceHydrationKey]);

  const visibleEditorHydrated =
    surfaceHydrationKey !== null && hydratedSurfaceKey === surfaceHydrationKey;

  const handleVisibleEditorReady = useCallback(
    (path: string, editor: Editor | null) => {
      if (path !== visibleEditorPathRef.current) return;
      handleEditorReady(editor);
    },
    [handleEditorReady],
  );

  const renderEditorSurface = useCallback(
    (snapshot: EditorSurfaceSnapshot) => {
      return (
        <div
          key={snapshot.path}
          data-editor-visibility="visible"
          className="flex min-h-0 flex-1 flex-col"
        >
          <ErrorBoundary scope="editor">
            <TipTapEditor
              key={snapshot.path + ":" + EDITOR_HTML_CACHE_FORMAT_VERSION}
              initialBodyMarkdown={snapshot.editorBodyMarkdown}
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
                handleVisibleEditorReady(snapshot.path, editor);
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
            <EditorOutline
              editor={editorInstance}
              open={outlineOpen}
              notePath={snapshot.path}
              onOpenNote={openNoteLeavingHome}
              onPrepareNote={onPrepareNotePath}
              locked={snapshot.activeFileLocked}
              zen={zen}
              onOpenChange={onOutlineOpenChange}
            />
          </ErrorBoundary>
        </div>
      );
    },
    [
      editorContextMenu.handleContextMenu,
      editorInstance,
      editorZoom,
      handleDirty,
      handleLockToggle,
      handleVisibleEditorReady,
      inlineAi,
      onOutlineOpenChange,
      onPrepareNotePath,
      openNoteLeavingHome,
      outlineOpen,
      runEditorActionById,
      updateEditorStats,
      vaultPath,
      zen,
    ],
  );

  const renderReadablePreview = useCallback(
    (snapshot: EditorSurfaceSnapshot) => (
      <div
        key={snapshot.path}
        data-testid="readable-note-preview"
        data-path={snapshot.path}
        className="iris-editor flex min-h-0 flex-1 flex-col"
      >
        <div className="iris-editor-zoom-scroll min-h-0 flex-1 overflow-y-auto overflow-x-hidden">
          <div className="iris-editor-canvas">
            <pre className="iris-markdown-content whitespace-pre-wrap font-sans text-base leading-7 text-foreground">
              {snapshot.editorBodyMarkdown}
            </pre>
          </div>
        </div>
      </div>
    ),
    [],
  );

  const renderVisibleSurface = useCallback(
    (snapshot: EditorSurfaceSnapshot) =>
      visibleEditorHydrated
        ? renderEditorSurface(snapshot)
        : renderReadablePreview(snapshot),
    [renderEditorSurface, renderReadablePreview, visibleEditorHydrated],
  );

  return (
    <div
      data-testid="editor-shell"
      className={cn(
        "relative flex min-h-0 flex-1 flex-col",
        outlineOpen && !zen && activePath && "iris-editor-outline-open",
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
      ) : visibleSurface ? (
        renderVisibleSurface(visibleSurface)
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
