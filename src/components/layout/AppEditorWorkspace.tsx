import type { Editor } from "@tiptap/react";
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
  openNoteLeavingHome: (path: string) => void;
  outlineOpen: boolean;
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
  outlineOpen,
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
      ) : activePath && !homeActive ? (
        <ErrorBoundary scope="编辑器">
          <TipTapEditor
            key={`${activePath}:${EDITOR_HTML_CACHE_FORMAT_VERSION}`}
            initialBodyMarkdown={editorBodyMarkdown}
            contentCacheKey={activePath}
            vaultPath={vaultPath}
            reingestKey={editorContentTick}
            zen={zen}
            zoom={editorZoom}
            titleSlot={editorTitleSlot}
            locked={activeFileLocked}
            setLocked={
              activeNoteIsClassified
                ? undefined
                : (locked) => void handleLockToggle(locked)
            }
            onDirty={handleDirty}
            onSlashCommand={runEditorActionById}
            onBodyContextMenu={editorContextMenu.handleContextMenu}
            onEditorReady={handleEditorReady}
            onBodyStatsChange={updateEditorStats}
            onInlineAiRetry={(ed) => void inlineAi.retry(ed)}
            onInlineAiDismiss={(ed) => inlineAi.dismiss(ed)}
            onInlineAiAccept={() => inlineAi.finish()}
            onOpenWikiLink={(title) => openNoteLeavingHome(`${title}.md`)}
          />
          <EditorOutline
            editor={editorInstance}
            open={outlineOpen}
            notePath={activePath}
            onOpenNote={openNoteLeavingHome}
            locked={activeFileLocked}
            zen={zen}
            onOpenChange={onOutlineOpenChange}
          />
        </ErrorBoundary>
      ) : (
        <WelcomeEmpty
          vaultKey={`${vaultPath ?? ""}:${vaultIndexEpoch}`}
          onOpen={openNoteLeavingHome}
          onNew={handleNewNoteLeavingHome}
          onQuickOpen={onOpenQuickOpen}
          onSearch={onOpenSearch}
          onOpenAiManagement={onOpenAiManagement}
        />
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
