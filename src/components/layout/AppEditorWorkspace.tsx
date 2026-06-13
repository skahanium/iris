import type { Editor } from "@tiptap/react";
import type { Dispatch, SetStateAction, ReactNode } from "react";

import { EditorFindReplaceBar } from "@/components/editor/EditorFindReplaceBar";
import { EditorOutline } from "@/components/editor/EditorOutline";
import { TipTapEditor } from "@/components/editor/TipTapEditor";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { WelcomeEmpty } from "@/components/layout/WelcomeEmpty";
import { IrisContextMenu } from "@/components/ui/iris-context-menu";
import type { IrisContextMenuGroup } from "@/components/ui/iris-context-menu";
import { EDITOR_HTML_CACHE_FORMAT_VERSION } from "@/lib/editor-html-cache";
import { cn } from "@/lib/utils";

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
  homeActive: boolean;
  inlineAi: {
    retry: (editor: Editor) => Promise<void>;
    dismiss: (editor: Editor) => void;
    finish: () => void;
  };
  onOutlineOpenChange: (open: boolean) => void;
  onOpenAiSystemCenter: () => void;
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
  vaultIndexEpoch: number;
  vaultPath: string | null;
  zen: boolean;
}

export function AppEditorWorkspace({
  activeFileLocked,
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
  homeActive,
  inlineAi,
  onOutlineOpenChange,
  onOpenAiSystemCenter,
  onOpenQuickOpen,
  onOpenSearch,
  openNoteLeavingHome,
  outlineOpen,
  runEditorActionById,
  setFindReplaceMode,
  setFindReplaceOpen,
  updateEditorStats,
  vaultIndexEpoch,
  vaultPath,
  zen,
}: AppEditorWorkspaceProps) {
  return (
    <div
      data-testid="editor-shell"
      className={cn(
        "relative flex min-h-0 flex-1 flex-col",
        outlineOpen && activePath && "iris-editor-outline-open",
      )}
    >
      {activePath && !homeActive ? (
        <ErrorBoundary scope="编辑器">
          <TipTapEditor
            key={`${activePath}:${EDITOR_HTML_CACHE_FORMAT_VERSION}`}
            initialBodyMarkdown={editorBodyMarkdown}
            contentCacheKey={activePath}
            reingestKey={editorContentTick}
            reloadContentTick={editorContentTick}
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
          onAiSystemCenter={onOpenAiSystemCenter}
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
        open={findReplaceOpen && Boolean(activePath)}
        onClose={() => setFindReplaceOpen(false)}
        onModeChange={setFindReplaceMode}
      />
    </div>
  );
}
