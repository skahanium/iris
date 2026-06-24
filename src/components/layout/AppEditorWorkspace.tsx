import type { Editor } from "@tiptap/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Dispatch, SetStateAction, ReactNode } from "react";

import { ArtifactWorkspaceView } from "@/components/layout/ArtifactWorkspaceView";
import { EditorFindReplaceBar } from "@/components/editor/EditorFindReplaceBar";
import { EditorOutline } from "@/components/editor/EditorOutline";
import { TipTapEditor } from "@/components/editor/TipTapEditor";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { WelcomeEmpty } from "@/components/layout/WelcomeEmpty";
import { IrisContextMenu } from "@/components/ui/iris-context-menu";
import type { IrisContextMenuGroup } from "@/components/ui/iris-context-menu";
import { EDITOR_HTML_CACHE_FORMAT_VERSION } from "@/lib/editor-html-cache";
import { cn } from "@/lib/utils";
import type { ArtifactTab } from "@/types/assistant-artifact";
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

type EditorSurfaceVisibility = "visible" | "staging" | "warm";

interface EditorSurfaceSnapshot {
  activeFileLocked: boolean;
  activeNoteIsClassified: boolean;
  editorBodyMarkdown: string;
  cacheNamespace: EditorHtmlCacheNamespace;
  editorContentTick: number;
  editorTitleSlot: ReactNode;
  path: string;
  pendingSequence?: number;
}

interface AppEditorWorkspaceProps {
  activeFileLocked: boolean;
  activeArtifactTab: ArtifactTab | null;
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
  handleNewNoteLeavingHome: () => void;
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
  pendingNoteOpen,
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
  zen,
}: AppEditorWorkspaceProps) {
  const currentEditorSurface = useMemo<EditorSurfaceSnapshot | null>(() => {
    if (!activePath || homeActive || activeArtifactTab) return null;
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
    activeNoteIsClassified,
    activePath,
    editorBodyMarkdown,
    editorContentTick,
    editorTitleSlot,
    homeActive,
  ]);

  const [visibleEditorSurface, setVisibleEditorSurface] =
    useState<EditorSurfaceSnapshot | null>(currentEditorSurface);
  const visibleEditorPathRef = useRef(visibleEditorSurface?.path ?? null);
  visibleEditorPathRef.current = visibleEditorSurface?.path ?? null;

  useEffect(() => {
    if (!currentEditorSurface) {
      setVisibleEditorSurface(null);
      return;
    }
    setVisibleEditorSurface((previous) => {
      if (!previous || previous.path === currentEditorSurface.path) {
        return currentEditorSurface;
      }
      return previous;
    });
  }, [currentEditorSurface]);

  const visibleSurface = visibleEditorSurface ?? currentEditorSurface;
  const pendingEditorSurface = useMemo<EditorSurfaceSnapshot | null>(() => {
    if (!pendingNoteOpen || activeArtifactTab) return null;
    return {
      activeFileLocked: pendingNoteOpen.isLocked,
      activeNoteIsClassified: pendingNoteOpen.namespace === "classified",
      cacheNamespace: pendingNoteOpen.namespace,
      editorBodyMarkdown: pendingNoteOpen.bodyMarkdown,
      editorContentTick: 0,
      editorTitleSlot: null,
      path: pendingNoteOpen.path,
      pendingSequence: pendingNoteOpen.sequence,
    };
  }, [activeArtifactTab, pendingNoteOpen]);

  const fallbackStagingSurface =
    currentEditorSurface &&
    visibleSurface &&
    currentEditorSurface.path !== visibleSurface.path
      ? currentEditorSurface
      : null;
  const stagingSurface =
    pendingEditorSurface && pendingEditorSurface.path !== visibleSurface?.path
      ? pendingEditorSurface
      : fallbackStagingSurface;

  const warmSurfaces = useMemo<EditorSurfaceSnapshot[]>(() => {
    const blockedPaths = new Set(
      [visibleSurface?.path, stagingSurface?.path].filter(
        (path): path is string => Boolean(path),
      ),
    );
    const allowClassifiedWarm = visibleSurface?.activeNoteIsClassified === true;
    return (warmPreparedNotes ?? [])
      .filter((note) => !blockedPaths.has(note.path))
      .filter((note) => note.namespace !== "classified" || allowClassifiedWarm)
      .slice(0, 2)
      .map((note) => ({
        activeFileLocked: note.isLocked,
        activeNoteIsClassified: note.namespace === "classified",
        cacheNamespace: note.namespace,
        editorBodyMarkdown: note.bodyMarkdown,
        editorContentTick: 0,
        editorTitleSlot: null,
        path: note.path,
      }));
  }, [
    stagingSurface?.path,
    visibleSurface?.activeNoteIsClassified,
    visibleSurface?.path,
    warmPreparedNotes,
  ]);

  const handleVisibleEditorReady = useCallback(
    (path: string, editor: Editor | null) => {
      if (path !== visibleEditorPathRef.current) return;
      handleEditorReady(editor);
    },
    [handleEditorReady],
  );

  const handleStagingEditorReady = useCallback(
    (snapshot: EditorSurfaceSnapshot, editor: Editor | null) => {
      if (!editor) return;
      if (snapshot.pendingSequence !== undefined) {
        if (!commitPendingNoteOpen(snapshot.path, snapshot.pendingSequence)) {
          return;
        }
      } else if (activePath !== snapshot.path) {
        return;
      }
      setVisibleEditorSurface(snapshot);
      handleEditorReady(editor);
    },
    [activePath, commitPendingNoteOpen, handleEditorReady],
  );

  const renderEditorSurface = useCallback(
    (snapshot: EditorSurfaceSnapshot, visibility: EditorSurfaceVisibility) => {
      const isVisible = visibility === "visible";
      return (
        <div
          key={snapshot.path}
          data-editor-visibility={visibility}
          aria-hidden={isVisible ? undefined : true}
          className={cn(
            isVisible
              ? "flex min-h-0 flex-1 flex-col"
              : "pointer-events-none absolute inset-0 min-h-0 overflow-hidden opacity-0",
          )}
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
              titleSlot={isVisible ? snapshot.editorTitleSlot : null}
              locked={isVisible ? snapshot.activeFileLocked : true}
              setLocked={
                isVisible && !snapshot.activeNoteIsClassified
                  ? (locked) => void handleLockToggle(locked)
                  : undefined
              }
              onDirty={isVisible ? handleDirty : undefined}
              onSlashCommand={isVisible ? runEditorActionById : undefined}
              onBodyContextMenu={
                isVisible ? editorContextMenu.handleContextMenu : undefined
              }
              onEditorReady={(editor) => {
                if (isVisible) {
                  handleVisibleEditorReady(snapshot.path, editor);
                  return;
                }
                if (visibility === "staging") {
                  handleStagingEditorReady(snapshot, editor);
                }
              }}
              onBodyStatsChange={isVisible ? updateEditorStats : undefined}
              onInlineAiRetry={
                isVisible ? (ed) => void inlineAi.retry(ed) : undefined
              }
              onInlineAiDismiss={
                isVisible ? (ed) => inlineAi.dismiss(ed) : undefined
              }
              onInlineAiAccept={isVisible ? () => inlineAi.finish() : undefined}
              onOpenWikiLink={
                isVisible
                  ? (title) => openNoteLeavingHome(title + ".md")
                  : undefined
              }
              onPrepareWikiLink={
                isVisible
                  ? (title) => onPrepareNotePath?.(title + ".md", title)
                  : undefined
              }
            />
            {isVisible ? (
              <EditorOutline
                editor={editorInstance}
                open={outlineOpen}
                notePath={snapshot.path}
                onOpenNote={openNoteLeavingHome}
                onPrepareNote={onPrepareNotePath}
                locked={isVisible ? snapshot.activeFileLocked : true}
                zen={zen}
                onOpenChange={onOutlineOpenChange}
              />
            ) : null}
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
      handleStagingEditorReady,
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

  const editorSurfaceEntries: Array<{
    snapshot: EditorSurfaceSnapshot;
    visibility: EditorSurfaceVisibility;
  }> = [];
  if (visibleSurface) {
    editorSurfaceEntries.push({
      snapshot: visibleSurface,
      visibility: "visible",
    });
  }
  if (stagingSurface) {
    editorSurfaceEntries.push({
      snapshot: stagingSurface,
      visibility: "staging",
    });
  }
  warmSurfaces.forEach((snapshot) => {
    editorSurfaceEntries.push({ snapshot, visibility: "warm" });
  });

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
      ) : visibleSurface ? (
        <>
          {editorSurfaceEntries.map(({ snapshot, visibility }) =>
            renderEditorSurface(snapshot, visibility),
          )}
        </>
      ) : (
        <>
          {editorSurfaceEntries.map(({ snapshot, visibility }) =>
            renderEditorSurface(snapshot, visibility),
          )}
          <WelcomeEmpty
            vaultKey={(vaultPath ?? "") + ":" + vaultIndexEpoch}
            onOpen={openNoteLeavingHome}
            onNew={handleNewNoteLeavingHome}
            onQuickOpen={onOpenQuickOpen}
            onSearch={onOpenSearch}
            onOpenAiManagement={onOpenAiManagement}
            onPrepare={onPrepareNote}
            pendingOpen={pendingOpen}
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
