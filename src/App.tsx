import type { Editor } from "@tiptap/react";
import { Moon, Sun } from "lucide-react";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { AiPanel } from "@/components/ai/AiPanel";
import type { ContextQuote } from "@/components/ai/ContextPacketCard";
import { EditorOutline } from "@/components/editor/EditorOutline";
import { FloatingToolbar } from "@/components/editor/FloatingToolbar";
import { TipTapEditor } from "@/components/editor/TipTapEditor";
import { BacklinksPanel } from "@/components/file/BacklinksPanel";
import { ConflictDialog } from "@/components/file/ConflictDialog";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { FileSheet } from "@/components/file/FileSheet";
import { QuickOpen } from "@/components/file/QuickOpen";
import { RecycleBinSheet } from "@/components/file/RecycleBinSheet";
import { SearchPanel } from "@/components/file/SearchPanel";
import { CommandPalette } from "@/components/layout/CommandPalette";
import { AppShell } from "@/components/layout/AppShell";
import { DesktopFrame } from "@/components/layout/DesktopFrame";
import { MinimalWindowChrome } from "@/components/layout/MinimalWindowChrome";
import { StatusBar } from "@/components/layout/StatusBar";
import { TabBar } from "@/components/layout/TabBar";
import { WelcomeEmpty } from "@/components/layout/WelcomeEmpty";
import { TagView } from "@/components/tag/TagView";
import { Button } from "@/components/ui/button";
import { useAppKeyboard } from "@/hooks/useAppKeyboard";
import { useAutoVaultIndex } from "@/hooks/useAutoVaultIndex";
import { useEditorSave } from "@/hooks/useEditorSave";
import { useEditorZoom } from "@/hooks/useEditorZoom";
import { useInlineAi } from "@/hooks/useInlineAi";
import { useLlmProvider } from "@/hooks/useLlmProvider";
import { useOverlayManager } from "@/hooks/useOverlayManager";
import { useTabManager } from "@/hooks/useTabManager";
import { useVersionIdle } from "@/hooks/useVersionIdle";
import { useTheme } from "@/hooks/useTheme";
import { useVault } from "@/hooks/useVault";
import {
  buildCommandPaletteItems,
  type CommandPaletteItem,
} from "@/lib/command-palette";
import { displayTitleFromMarkdown } from "@/lib/document-title";
import { resolveNoteDisplayTitle } from "@/lib/note-display";
import { splitFrontmatter } from "@/lib/frontmatter";
import {
  editorHtmlToMarkdown,
  extractFrontmatterYaml,
  markdownToEditorHtml,
} from "@/lib/markdown";
import { versionSaveManual } from "@/lib/ipc";
import { noteTitleFromEditor } from "@/lib/note-title";
import { isTauriRuntime } from "@/lib/tauri-runtime";
import { cn } from "@/lib/utils";

const GraphView = lazy(() =>
  import("@/components/graph/GraphView").then((m) => ({
    default: m.GraphView,
  })),
);
const SettingsPanel = lazy(() =>
  import("@/components/settings/SettingsPanel").then((m) => ({
    default: m.SettingsPanel,
  })),
);
const VersionTimeline = lazy(() =>
  import("@/components/version/VersionTimeline").then((m) => ({
    default: m.VersionTimeline,
  })),
);

const LazyFallback = () => (
  <div className="flex items-center justify-center p-8 text-sm text-muted-foreground">
    加载中…
  </div>
);

function pathStem(path: string): string {
  return path.replace(/\.md$/i, "").split("/").pop() ?? path;
}

const OUTLINE_OPEN_KEY = "iris-outline-open";

function loadOutlineOpen(): boolean {
  try {
    return localStorage.getItem(OUTLINE_OPEN_KEY) !== "false";
  } catch {
    return true;
  }
}

function saveOutlineOpen(open: boolean): void {
  try {
    localStorage.setItem(OUTLINE_OPEN_KEY, open ? "true" : "false");
  } catch {
    /* ignore */
  }
}

function App() {
  const { vaultPath, loading, pickVault } = useVault();
  const { theme, setTheme } = useTheme();
  const [aiPanelOpen, setAiPanelOpen] = useState(true);
  const [webSearch, setWebSearch] = useState(false);
  const [, setQuote] = useState<ContextQuote | null>(null);
  const [aiStatus, setAiStatus] = useState("AI 空闲");
  const [zen, setZen] = useState(false);
  const [outlineOpen, setOutlineOpen] = useState(loadOutlineOpen);
  const [vaultIndexEpoch, setVaultIndexEpoch] = useState(0);
  const { zoom: editorZoom, zoomIn, zoomOut, resetZoom } = useEditorZoom();
  const editorRef = useRef<Editor | null>(null);
  const [editorInstance, setEditorInstance] = useState<Editor | null>(null);
  const overlays = useOverlayManager();
  const { provider: llmProvider } = useLlmProvider();

  const bumpVaultIndex = useCallback(
    () => setVaultIndexEpoch((n) => n + 1),
    [],
  );

  const {
    tabs,
    activePath,
    markdown,
    activePathRef,
    markdownRef,
    frontmatterYamlRef,
    openFile,
    closeTab,
    handleNewNote,
    markDirty,
    markClean,
    getEditorMarkdown,
  } = useTabManager({
    onStatusChange: setAiStatus,
    onVaultIndexBump: bumpVaultIndex,
  });

  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;

  const inlineAi = useInlineAi({
    provider: llmProvider,
    onStatus: setAiStatus,
  });

  const dirtyRef = useRef(false);

  const serializeEditorHtml = useCallback(
    (html: string) => editorHtmlToMarkdown(html, frontmatterYamlRef.current),
    [frontmatterYamlRef],
  );

  const { notifyDirty, flushSave } = useEditorSave(
    activePath,
    editorRef,
    (md) => {
      markdownRef.current = md;
      frontmatterYamlRef.current = extractFrontmatterYaml(md);
      dirtyRef.current = false;
      const savedTitle = displayTitleFromMarkdown(md, "");
      markClean(
        activePath ?? "",
        resolveNoteDisplayTitle({
          path: activePath ?? "",
          title: savedTitle,
        }),
      );
    },
    serializeEditorHtml,
  );

  const { onActivity: resetVersionIdle } = useVersionIdle(
    activePath,
    getEditorMarkdown,
  );

  const handleSaveVersion = useCallback(async () => {
    const path = activePathRef.current;
    if (!path) return;
    await flushSave();
    await versionSaveManual(path, getEditorMarkdown());
  }, [flushSave, getEditorMarkdown, activePathRef]);

  const handleDirty = useCallback(() => {
    if (!dirtyRef.current) {
      dirtyRef.current = true;
      markDirty();
    }
    notifyDirty();
    resetVersionIdle();
  }, [notifyDirty, resetVersionIdle, markDirty]);

  const handleTitleChange = useCallback(
    (raw: string) => {
      const path = activePathRef.current;
      if (!path) return;
      const title = resolveNoteDisplayTitle({
        path,
        title:
          raw.trim() || tabsRef.current.find((t) => t.path === path)?.title,
      });
      markClean(path, title);
    },
    [activePathRef, markClean],
  );

  const { rescanVault } = useAutoVaultIndex(vaultPath, loading, {
    onStatus: setAiStatus,
    onIndexed: bumpVaultIndex,
  });

  const handleVaultRescan = useCallback(() => {
    void rescanVault("manual");
  }, [rescanVault]);

  useAppKeyboard({
    overlays,
    activePathRef,
    onSaveVersion: () => void handleSaveVersion(),
    onCloseTab: closeTab,
    onToggleAiPanel: () => setAiPanelOpen((open) => !open),
    onToggleZen: () => setZen((z) => !z),
    onToggleOutline: () => {
      setOutlineOpen((open) => {
        const next = !open;
        saveOutlineOpen(next);
        return next;
      });
    },
    onToggleWebSearch: () => setWebSearch((on) => !on),
    onRescanVault: handleVaultRescan,
    zoomIn,
    zoomOut,
    resetZoom,
    vaultPath,
  });

  const applyMarkdownToEditor = useCallback(
    (content: string) => {
      frontmatterYamlRef.current = extractFrontmatterYaml(content);
      markdownRef.current = content;
      const stem = activePathRef.current ? pathStem(activePathRef.current) : "";
      if (editorRef.current) {
        editorRef.current.commands.setContent(
          markdownToEditorHtml(content, stem),
          false,
        );
      }
    },
    [frontmatterYamlRef, markdownRef, activePathRef],
  );

  useEffect(() => {
    if (!activePath) setEditorInstance(null);
  }, [activePath]);

  const handleEditorReady = useCallback((ed: Editor) => {
    editorRef.current = ed;
    setEditorInstance(ed);
  }, []);

  const runInlineAi = useCallback(
    (action: string) => {
      const ed = editorRef.current;
      if (!ed || ed.isActive("noteTitle")) return;
      void inlineAi.run(ed, action);
    },
    [inlineAi],
  );

  const handleSlashCommand = useCallback(
    (command: string) => {
      if (!editorRef.current) return;
      void inlineAi.runSlash(editorRef.current, command, markdownRef.current);
    },
    [inlineAi, markdownRef],
  );

  const sendSelectionToAi = useCallback(() => {
    const ed = editorRef.current;
    const path = activePathRef.current;
    if (!ed || !path) return;
    const { from, to } = ed.state.selection;
    const text = ed.state.doc.textBetween(from, to, "\n");
    if (!text) return;
    setQuote({ filePath: path, text });
  }, [activePathRef]);

  const commandPaletteItems = useMemo(
    () =>
      buildCommandPaletteItems({
        hasVault: Boolean(vaultPath),
        hasActiveNote: Boolean(activePath),
      }),
    [vaultPath, activePath],
  );

  const handleCommandPaletteSelect = useCallback(
    (item: CommandPaletteItem) => {
      const action = item.action;
      if (
        action.type === "openOverlay" &&
        action.overlay === "commandPalette"
      ) {
        return;
      }
      overlays.closeOverlay("commandPalette");
      switch (action.type) {
        case "openOverlay":
          overlays.openOverlay(action.overlay);
          break;
        case "newNote":
          void handleNewNote();
          break;
        case "saveVersion":
          void handleSaveVersion();
          break;
        case "closeTab":
          if (activePathRef.current) closeTab(activePathRef.current);
          break;
        case "toggleAiPanel":
          setAiPanelOpen((open) => !open);
          break;
        case "toggleZen":
          setZen((z) => !z);
          break;
        case "toggleOutline":
          setOutlineOpen((open) => {
            const next = !open;
            saveOutlineOpen(next);
            return next;
          });
          break;
        case "toggleTheme":
          void setTheme(theme === "dark" ? "light" : "dark");
          break;
        case "toggleWebSearch":
          setWebSearch((on) => !on);
          break;
        case "rescanVault":
          void handleVaultRescan();
          break;
        case "zoomIn":
          zoomIn();
          break;
        case "zoomOut":
          zoomOut();
          break;
        case "zoomReset":
          resetZoom();
          break;
        case "sendSelectionToAi":
          sendSelectionToAi();
          break;
        default: {
          const _exhaustive: never = action;
          return _exhaustive;
        }
      }
    },
    [
      overlays,
      handleNewNote,
      handleSaveVersion,
      closeTab,
      activePathRef,
      theme,
      setTheme,
      handleVaultRescan,
      zoomIn,
      zoomOut,
      resetZoom,
      sendSelectionToAi,
    ],
  );

  const activeDocumentTitle = useMemo(() => {
    if (!activePath) return null;
    const tabTitle = tabs.find((t) => t.path === activePath)?.title;
    if (editorInstance && editorRef.current) {
      const live = noteTitleFromEditor(editorRef.current).trim();
      if (live) {
        return resolveNoteDisplayTitle({
          path: activePath,
          title: live,
        });
      }
    }
    return resolveNoteDisplayTitle({
      path: activePath,
      title: tabTitle,
    });
  }, [activePath, tabs, editorInstance]);

  const editorTitleFallback = useMemo(() => {
    if (!activePath) return "无标题1";
    return resolveNoteDisplayTitle({
      path: activePath,
      title: tabs.find((t) => t.path === activePath)?.title,
    });
  }, [activePath, tabs]);

  if (!isTauriRuntime()) {
    return (
      <div className="flex h-dvh flex-col items-center justify-center gap-4 bg-background px-6 text-center">
        <h1 className="text-xl font-semibold text-foreground">
          请在 Iris 桌面窗口中使用
        </h1>
        <p className="max-w-md text-sm leading-relaxed text-muted-foreground">
          <code className="rounded bg-muted px-1 py-0.5 text-xs">
            http://127.0.0.1:1420
          </code>{" "}
          只是 Vite 前端热更新地址，浏览器里没有 Rust 后端，无法读写笔记目录。
        </p>
        <p className="max-w-md text-sm text-muted-foreground">
          方式 B 需要两个终端：一个 <code className="text-xs">npm run dev</code>
          ，另一个启动 <code className="text-xs">npx tauri dev …</code>
          ，使用弹出的{" "}
          <strong className="font-medium text-foreground">Iris</strong>{" "}
          窗口操作。
        </p>
      </div>
    );
  }

  if (loading) {
    return (
      <DesktopFrame>
        <MinimalWindowChrome />
        <div className="flex min-h-0 flex-1 items-center justify-center bg-background text-muted-foreground">
          加载中…
        </div>
      </DesktopFrame>
    );
  }

  if (!vaultPath) {
    return (
      <DesktopFrame>
        <MinimalWindowChrome />
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-6 bg-background px-6">
          <div className="text-center">
            <h1 className="text-3xl font-semibold tracking-tight text-foreground">
              Iris
            </h1>
            <p className="mt-2 text-sm text-muted-foreground">本地优先笔记</p>
          </div>
          <Button type="button" onClick={() => void pickVault()}>
            选择笔记目录
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="gap-1.5"
            onClick={() => void setTheme(theme === "dark" ? "light" : "dark")}
          >
            {theme === "dark" ? (
              <Sun className="h-3.5 w-3.5" />
            ) : (
              <Moon className="h-3.5 w-3.5" />
            )}
            {theme === "dark" ? "亮色模式" : "暗色模式"}
          </Button>
        </div>
      </DesktopFrame>
    );
  }

  return (
    <DesktopFrame>
      <AppShell
        aiPanelOpen={aiPanelOpen}
        zen={zen}
        tabBar={
          <TabBar
            tabs={tabs}
            activePath={activePath}
            onSelect={(p) => void openFile(p)}
            onClose={closeTab}
            onNew={() => void handleNewNote()}
          />
        }
        editor={
          <div
            className={cn(
              "relative flex min-h-0 flex-1 flex-col",
              outlineOpen && activePath && "iris-editor-outline-open",
            )}
          >
            {activePath ? (
              <ErrorBoundary scope="编辑器">
                <TipTapEditor
                  key={activePath}
                  initialMarkdown={markdown}
                  titleFallback={editorTitleFallback}
                  zen={zen}
                  zoom={editorZoom}
                  onTitleChange={handleTitleChange}
                  onDirty={handleDirty}
                  onSlashCommand={handleSlashCommand}
                  onEditorReady={handleEditorReady}
                  onInlineAiRetry={(ed) => void inlineAi.retry(ed)}
                  onOpenWikiLink={(title) => void openFile(`${title}.md`)}
                />
                <EditorOutline
                  editor={editorInstance}
                  open={outlineOpen}
                  zen={zen}
                  onOpenChange={(open) => {
                    setOutlineOpen(open);
                    saveOutlineOpen(open);
                  }}
                />
              </ErrorBoundary>
            ) : (
              <WelcomeEmpty
                vaultKey={`${vaultPath ?? ""}:${vaultIndexEpoch}`}
                onOpen={(p) => void openFile(p)}
                onNew={() => void handleNewNote()}
              />
            )}
            <FloatingToolbar
              editor={editorRef.current}
              onInlineAi={(a) => void runInlineAi(a)}
              onSendToAi={sendSelectionToAi}
            />
          </div>
        }
        aiPanel={
          <ErrorBoundary scope="AI面板">
            <AiPanel
              notePath={activePath}
              noteDisplayTitle={activeDocumentTitle}
              noteContent={markdown}
            />
          </ErrorBoundary>
        }
        statusBar={
          <StatusBar
            path={activePath}
            documentTitle={activeDocumentTitle}
            unsaved={tabs.find((t) => t.path === activePath)?.dirty ?? false}
            markdown={markdown}
            wordCount={
              splitFrontmatter(markdown).body.replace(/\s+/g, "").length
            }
            aiStatus={aiStatus}
            editorZoom={editorZoom}
            onEditorZoomIn={zoomIn}
            onEditorZoomOut={zoomOut}
            onEditorZoomReset={resetZoom}
            webSearch={webSearch}
            onWebSearchChange={setWebSearch}
          />
        }
        overlays={
          <>
            <CommandPalette
              open={overlays.commandPaletteOpen}
              items={commandPaletteItems}
              onClose={() => overlays.closeOverlay("commandPalette")}
              onSelect={handleCommandPaletteSelect}
            />
            <QuickOpen
              open={overlays.quickOpen}
              onClose={() => overlays.closeOverlay("quickOpen")}
              onSelect={(p) => void openFile(p)}
            />
            <FileSheet
              open={overlays.fileSheet}
              onClose={() => overlays.closeOverlay("fileSheet")}
              onOpen={(p) => void openFile(p)}
            />
            <RecycleBinSheet
              open={overlays.recycleBinOpen}
              onClose={() => overlays.closeOverlay("recycleBin")}
              onRestored={(p) => void openFile(p)}
              onIndexChange={bumpVaultIndex}
            />
            <SearchPanel
              open={overlays.searchOpen}
              onClose={() => overlays.closeOverlay("search")}
              onOpen={(p) => void openFile(p)}
            />
            <Suspense fallback={<LazyFallback />}>
              <SettingsPanel
                open={overlays.settingsOpen}
                onClose={() => overlays.closeOverlay("settings")}
                provider={llmProvider}
                theme={theme}
                onThemeChange={(t) => void setTheme(t)}
              />
            </Suspense>
            <BacklinksPanel
              open={overlays.backlinksOpen}
              onClose={() => overlays.closeOverlay("backlinks")}
              notePath={activePath}
              onOpen={(p) => void openFile(p)}
            />
            <TagView
              open={overlays.tagViewOpen}
              onClose={() => overlays.closeOverlay("tags")}
              onOpen={(p) => void openFile(p)}
            />
            <Suspense fallback={<LazyFallback />}>
              <VersionTimeline
                open={overlays.versionOpen}
                onClose={() => overlays.closeOverlay("version")}
                notePath={activePath}
                currentContent={markdown}
                hasUnsavedEdits={
                  tabs.find((t) => t.path === activePath)?.dirty ?? false
                }
                onRestore={applyMarkdownToEditor}
              />
            </Suspense>
            <ErrorBoundary scope="知识图谱">
              <Suspense fallback={<LazyFallback />}>
                <GraphView
                  open={overlays.graphOpen}
                  onClose={() => overlays.closeOverlay("graph")}
                  onOpenNote={(p) => void openFile(p)}
                />
              </Suspense>
            </ErrorBoundary>
            <ConflictDialog
              open={false}
              localContent=""
              externalContent=""
              filePath=""
              onKeepLocal={() => {}}
              onAcceptExternal={() => {}}
              onManualEdit={() => {}}
            />
          </>
        }
      />
    </DesktopFrame>
  );
}

export default App;
