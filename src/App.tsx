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
  type ReactNode,
} from "react";

import {
  UnifiedAssistantPanel,
  type AssistantSelectionQuote,
} from "@/components/ai/UnifiedAssistantPanel";
import { SkillsPanel } from "@/components/ai/SkillsPanel";
import type { WritingEditorContext } from "@/types/ai";
import {
  EMPTY_ASSISTANT_CHROME,
  type AssistantChromeSnapshot,
} from "@/types/assistant-chrome";
import { DocumentTitleField } from "@/components/editor/DocumentTitleField";
import { EditorOutline } from "@/components/editor/EditorOutline";
import { TipTapEditor } from "@/components/editor/TipTapEditor";
import { IrisContextMenu } from "@/components/ui/iris-context-menu";
import { BacklinksPanel } from "@/components/file/BacklinksPanel";
import { ConflictDialog } from "@/components/file/ConflictDialog";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { VaultNavigator } from "@/components/file/VaultNavigator";
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
import { useEditorContextMenu } from "@/hooks/useEditorContextMenu";
import { useAutoVaultIndex } from "@/hooks/useAutoVaultIndex";
import { useEditorSave } from "@/hooks/useEditorSave";
import { useOpenNote } from "@/hooks/useOpenNote";
import { useEditorZoom } from "@/hooks/useEditorZoom";
import { useInlineAi } from "@/hooks/useInlineAi";
import { useConnectivityStatus } from "@/hooks/useConnectivityStatus";
import { useLlmProvider } from "@/hooks/useLlmProvider";
import { useOverlayManager } from "@/hooks/useOverlayManager";
import { useTabManager } from "@/hooks/useTabManager";
import { useVersionIdle } from "@/hooks/useVersionIdle";
import { useTheme } from "@/hooks/useTheme";
import { useMacOSWindowChromeSync } from "@/hooks/useMacOSWindowChromeSync";
import { useVault } from "@/hooks/useVault";
import {
  buildCommandPaletteItems,
  recordCommandUsage,
  type CommandPaletteItem,
} from "@/lib/command-palette";
import { runEditorAction } from "@/lib/editor-action-executor";
import { insertAssistantMarkdownAtCursor } from "@/lib/editor-insert";
import {
  displayTitleForChrome,
  resolveNoteDisplayTitle,
} from "@/lib/note-display";
import {
  fileRead,
  fileWrite,
  listenFileChanged,
  listenVersionSaveComplete,
  settingsGet,
  settingsSet,
  versionSaveManual,
} from "@/lib/ipc";
import { isNoteSubstantivelyEmpty } from "@/lib/note-substance";
import { formatVersionSaveStatus } from "@/lib/version-save-status";
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

const OUTLINE_OPEN_KEY = "iris-outline-open";

function loadOutlineOpen(): boolean {
  try {
    return localStorage.getItem(OUTLINE_OPEN_KEY) !== "false";
  } catch (e) {
    console.warn("[App] localStorage read failed:", e);
    return true;
  }
}

function saveOutlineOpen(open: boolean): void {
  try {
    localStorage.setItem(OUTLINE_OPEN_KEY, open ? "true" : "false");
  } catch (e) {
    console.warn("[App] localStorage write failed:", e);
  }
}

function PreVaultDesktopFrame({ children }: { children: ReactNode }) {
  return (
    <DesktopFrame>
      <MinimalWindowChrome />
      {children}
    </DesktopFrame>
  );
}

function App() {
  useMacOSWindowChromeSync();

  const { vaultPath, loading, pickVault, error: vaultError } = useVault();
  const { theme, setTheme } = useTheme();
  const [aiPanelOpen, setAiPanelOpen] = useState(true);
  const [webSearch, setWebSearchState] = useState(false);

  useEffect(() => {
    void settingsGet<boolean>("web_search_enabled").then((enabled) => {
      if (enabled === true) {
        setWebSearchState(true);
      }
    });
  }, []);

  const setWebSearch = useCallback((enabled: boolean) => {
    setWebSearchState(enabled);
    void settingsSet("web_search_enabled", enabled);
  }, []);

  const toggleWebSearch = useCallback(() => {
    setWebSearchState((prev) => {
      const next = !prev;
      void settingsSet("web_search_enabled", next);
      return next;
    });
  }, []);
  const [selectionQuote, setSelectionQuote] =
    useState<AssistantSelectionQuote | null>(null);
  const [assistantPrefill, setAssistantPrefill] = useState<string | null>(null);
  const [aiStatus, setAiStatus] = useState("AI 空闲");
  const [assistantChrome, setAssistantChrome] =
    useState<AssistantChromeSnapshot>(EMPTY_ASSISTANT_CHROME);
  const [conflictState, setConflictState] = useState<{
    open: boolean;
    localContent: string;
    externalContent: string;
    filePath: string;
  } | null>(null);
  const [editorStats, setEditorStats] = useState({
    characterCount: 0,
    readingMinutes: 1,
  });
  const editorStatsRef = useRef({ characterCount: 0, readingMinutes: 1 });
  const editorStatsTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const updateEditorStats = useCallback(
    (stats: { characterCount: number; readingMinutes: number }) => {
      editorStatsRef.current = stats;
      if (editorStatsTimerRef.current) return;
      editorStatsTimerRef.current = setTimeout(() => {
        editorStatsTimerRef.current = null;
        setEditorStats({ ...editorStatsRef.current });
      }, 2000);
    },
    [],
  );
  const [keyboardLeaderPending, setKeyboardLeaderPending] = useState(false);
  const [zen, setZen] = useState(false);
  const [outlineOpen, setOutlineOpen] = useState(loadOutlineOpen);
  const [vaultIndexEpoch, setVaultIndexEpoch] = useState(0);
  const { zoom: editorZoom, zoomIn, zoomOut, resetZoom } = useEditorZoom();
  const editorRef = useRef<Editor | null>(null);
  const [editorInstance, setEditorInstance] = useState<Editor | null>(null);
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const undoRedoStateRef = useRef({ canUndo: false, canRedo: false });
  const editorTransactionCleanupRef = useRef<(() => void) | null>(null);
  const overlays = useOverlayManager();
  const { provider: llmProvider } = useLlmProvider();
  const { status: connectivityStatus } = useConnectivityStatus();

  const bumpVaultIndex = useCallback(
    () => setVaultIndexEpoch((n) => n + 1),
    [],
  );

  const dirtyRef = useRef(false);
  const persistBeforeLeaveRef = useRef<
    (path: string) => Promise<string | null>
  >(async () => null);

  const {
    tabs,
    activePath,
    markdown,
    editorContentTick,
    activePathRef,
    markdownRef,
    frontmatterYamlRef,
    openNote,
    activateTab,
    closeTab,
    handleNewNote,
    markDirty,
    markClean,
    updateTabTitle,
    replaceOpenTabPath,
    syncTabMarkdownCache,
    getTabMarkdownCached,
    setMarkdown,
  } = useTabManager({
    onStatusChange: setAiStatus,
    onVaultIndexBump: bumpVaultIndex,
    persistBeforeLeave: (path) => persistBeforeLeaveRef.current(path),
  });

  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;

  const inlineAi = useInlineAi({
    provider: llmProvider,
    onStatus: setAiStatus,
  });

  const getLiveMarkdownRef = useRef(() => markdownRef.current);

  const {
    noteTitle,
    editorBodyMarkdown,
    getLiveMarkdown,
    applySavedMarkdown,
    onTitleChange,
    onTitleBlur,
    schedulePathSync,
    loadBodyIntoEditor,
  } = useOpenNote({
    activePath,
    markdown,
    editorContentTick,
    activePathRef,
    markdownRef,
    frontmatterYamlRef,
    editorRef,
    updateTabTitle,
    replaceOpenTabPath,
  });

  getLiveMarkdownRef.current = getLiveMarkdown;

  const { notifyDirty, flushSave, flushSaveForPath } = useEditorSave(
    activePath,
    () => getLiveMarkdownRef.current(),
    (md) => {
      applySavedMarkdown(md);
      dirtyRef.current = false;
      const path = activePathRef.current;
      if (path) {
        syncTabMarkdownCache(path, md);
        markdownRef.current = md;
        markClean(path, resolveNoteDisplayTitle({ path, title: noteTitle }));
        if (noteTitle.trim() === "") {
          schedulePathSync(path, noteTitle);
        }
      }
    },
  );

  persistBeforeLeaveRef.current = async (path: string) => {
    const tab = tabsRef.current.find((t) => t.path === path);
    if (path === activePathRef.current) {
      if (!dirtyRef.current) {
        return getLiveMarkdownRef.current();
      }
      const snapshot = getLiveMarkdownRef.current();
      const md = await flushSaveForPath(path, () => snapshot);
      if (md) {
        dirtyRef.current = false;
        markdownRef.current = md;
        syncTabMarkdownCache(path, md);
        markClean(path, resolveNoteDisplayTitle({ path, title: noteTitle }));
      }
      return md;
    }
    if (!tab?.dirty) {
      return getTabMarkdownCached(path) ?? null;
    }
    const cached = getTabMarkdownCached(path);
    if (!cached || isNoteSubstantivelyEmpty(cached)) {
      return null;
    }
    await fileWrite(path, cached);
    markClean(path, tab.title);
    return cached;
  };

  const { onActivity: resetVersionIdle } = useVersionIdle(
    activePath,
    () => markdownRef.current,
  );

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let unlisten: (() => void) | undefined;
    void listenVersionSaveComplete((payload) => {
      if (payload.path !== activePathRef.current) return;
      setAiStatus(formatVersionSaveStatus(payload));
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [activePathRef]);

  // Listen for external file changes and show conflict dialog
  useEffect(() => {
    if (!isTauriRuntime()) return;
    let unlisten: (() => void) | undefined;
    void listenFileChanged((event) => {
      const currentPath = activePathRef.current;
      if (!currentPath || event.path !== currentPath) return;
      if (event.event_type === "removed") return;
      // External change detected on the currently open file
      void fileRead(event.path)
        .then((externalContent) => {
          const localContent = getLiveMarkdownRef.current();
          // Only show conflict if content actually differs
          if (externalContent !== localContent) {
            setConflictState({
              open: true,
              localContent,
              externalContent,
              filePath: event.path,
            });
          }
        })
        .catch((err: unknown) => {
          console.warn("[App] failed to read external file for conflict:", err);
        });
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [activePathRef, getLiveMarkdownRef]);

  const handleConflictKeepLocal = useCallback(() => {
    setConflictState(null);
    // Re-save local content to overwrite external changes
    void flushSave();
  }, [flushSave]);

  const handleConflictAcceptExternal = useCallback(() => {
    if (!conflictState) return;
    setConflictState(null);
    // Re-open the note from disk to load external content
    void openNote(conflictState.filePath);
  }, [conflictState, openNote]);

  const handleConflictManualEdit = useCallback(() => {
    setConflictState(null);
  }, []);

  const handleSaveNote = useCallback(async () => {
    await flushSave();
  }, [flushSave]);

  const handleSaveVersion = useCallback(async () => {
    const path = activePathRef.current;
    if (!path) return;
    const md = await flushSave();
    if (!md) return;
    setAiStatus("正在后台创建版本快照…");
    void versionSaveManual(path, md).catch((err: unknown) => {
      const msg = err instanceof Error ? err.message : String(err);
      setAiStatus(`版本快照提交失败：${msg}`);
    });
  }, [flushSave, activePathRef]);

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
      onTitleChange(raw);
      if (!dirtyRef.current) {
        dirtyRef.current = true;
        markDirty();
      }
      notifyDirty();
      resetVersionIdle();
    },
    [markDirty, notifyDirty, onTitleChange, resetVersionIdle],
  );

  const getWritingContext = useCallback((): WritingEditorContext | null => {
    const ed = editorRef.current;
    const path = activePathRef.current;
    if (!ed || !path) return null;
    const { from, to } = ed.state.selection;
    const selection =
      from !== to ? ed.state.doc.textBetween(from, to, "\n") : "";
    return {
      selection,
      cursorContext: getLiveMarkdown(),
    };
  }, [activePathRef, getLiveMarkdown]);

  const getParagraphText = useCallback((): string | null => {
    const ed = editorRef.current;
    if (!ed) return null;
    const { from, to } = ed.state.selection;
    if (from !== to) {
      return ed.state.doc.textBetween(from, to, "\n");
    }
    const $from = ed.state.doc.resolve(from);
    const start = $from.start($from.depth);
    const end = $from.end($from.depth);
    return ed.state.doc.textBetween(start, end, "\n");
  }, []);

  const { rescanVault } = useAutoVaultIndex(vaultPath, loading, {
    onStatus: setAiStatus,
    onIndexed: bumpVaultIndex,
  });

  const handleVaultRescan = useCallback(() => {
    void rescanVault("manual");
  }, [rescanVault]);

  const handleOpenConnectivitySettings = useCallback(
    () => overlays.openOverlay("settings"),
    [overlays],
  );

  const applyMarkdownToEditor = useCallback(
    (content: string) => {
      markdownRef.current = content;
      loadBodyIntoEditor(content);
      setMarkdown(content);
    },
    [loadBodyIntoEditor, markdownRef, setMarkdown],
  );

  const updateUndoRedoState = useCallback((ed: Editor | null) => {
    const next = ed
      ? {
          canUndo: ed.can().undo(),
          canRedo: ed.can().redo(),
        }
      : { canUndo: false, canRedo: false };
    const prev = undoRedoStateRef.current;
    undoRedoStateRef.current = next;
    if (prev.canUndo !== next.canUndo) setCanUndo(next.canUndo);
    if (prev.canRedo !== next.canRedo) setCanRedo(next.canRedo);
  }, []);

  const clearEditorTransactionListener = useCallback(() => {
    editorTransactionCleanupRef.current?.();
    editorTransactionCleanupRef.current = null;
  }, []);

  useEffect(() => {
    if (!activePath) {
      clearEditorTransactionListener();
      setEditorInstance(null);
      updateUndoRedoState(null);
    }
  }, [activePath, clearEditorTransactionListener, updateUndoRedoState]);

  useEffect(() => {
    if (!activePath) {
      editorStatsRef.current = { characterCount: 0, readingMinutes: 1 };
      setEditorStats({ characterCount: 0, readingMinutes: 1 });
    }
  }, [activePath]);

  const handleEditorReady = useCallback(
    (ed: Editor | null) => {
      clearEditorTransactionListener();
      editorRef.current = ed;
      if (!ed) {
        setEditorInstance(null);
        updateUndoRedoState(null);
        return;
      }

      setEditorInstance(ed);
      updateUndoRedoState(ed);

      const handleTransaction = ({
        editor: currentEditor,
        transaction,
      }: {
        editor: Editor;
        transaction: { docChanged: boolean };
      }) => {
        if (!transaction.docChanged) return;
        updateUndoRedoState(currentEditor);
      };

      ed.on("transaction", handleTransaction);
      editorTransactionCleanupRef.current = () => {
        ed.off("transaction", handleTransaction);
      };
    },
    [clearEditorTransactionListener, updateUndoRedoState],
  );

  const editorTitleSlot = useMemo(
    () => (
      <DocumentTitleField
        value={noteTitle}
        onChange={handleTitleChange}
        onBlur={onTitleBlur}
        editorRef={editorRef}
      />
    ),
    [noteTitle, handleTitleChange, onTitleBlur, editorRef],
  );

  const runInlineAi = useCallback(
    (action: string) => {
      const ed = editorRef.current;
      if (!ed) return;
      void inlineAi.run(ed, action);
    },
    [inlineAi],
  );

  const handleSlashCommand = useCallback(
    (command: string) => {
      if (!editorRef.current) return;
      void inlineAi.runSlash(editorRef.current, command, getLiveMarkdown());
    },
    [getLiveMarkdown, inlineAi],
  );

  const sendSelectionToAi = useCallback(
    (options?: { prefill?: string }) => {
      const ed = editorRef.current;
      const path = activePathRef.current;
      if (!ed || !path) return;
      const { from, to } = ed.state.selection;
      const text = ed.state.doc.textBetween(from, to, "\n");
      if (!text) {
        setAiStatus("请先在编辑器中选中文本");
        return;
      }
      setSelectionQuote({ filePath: path, text });
      setAssistantPrefill(options?.prefill ?? null);
      setAiPanelOpen(true);
    },
    [activePathRef],
  );

  const editorActionHandlers = useMemo(
    () => ({
      onInlineAi: (action: string) => runInlineAi(action),
      onSlashCommand: (command: string) => handleSlashCommand(command),
      onSendToAi: (options?: { prefill?: string }) =>
        sendSelectionToAi(options),
      onStatus: (message: string) => setAiStatus(message),
    }),
    [handleSlashCommand, runInlineAi, sendSelectionToAi],
  );

  const runEditorActionById = useCallback(
    (actionId: string) => {
      void runEditorAction(actionId, editorRef.current, editorActionHandlers);
    },
    [editorActionHandlers],
  );

  const handleInsertToEditor = useCallback((content: string) => {
    const ed = editorRef.current;
    if (!ed) return;
    insertAssistantMarkdownAtCursor(ed, content);
  }, []);

  const handleUndo = useCallback(() => {
    editorRef.current?.commands.undo();
  }, []);

  const handleRedo = useCallback(() => {
    editorRef.current?.commands.redo();
  }, []);

  const editorContextMenu = useEditorContextMenu(
    editorInstance,
    Boolean(activePath),
    () => setAiStatus("选区 AI：请使用右键菜单"),
  );

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
      recordCommandUsage(item.id);
      overlays.closeOverlay("commandPalette");
      switch (action.type) {
        case "openOverlay":
          overlays.openOverlay(action.overlay);
          break;
        case "newNote":
          void handleNewNote();
          break;
        case "saveNote":
          void handleSaveNote();
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
          toggleWebSearch();
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
        case "noop":
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
      handleSaveNote,
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
      toggleWebSearch,
    ],
  );

  useAppKeyboard({
    items: commandPaletteItems,
    vaultPath,
    activePathRef,
    onAction: handleCommandPaletteSelect,
    onLeaderPendingChange: setKeyboardLeaderPending,
  });

  const activeDocumentTitle = useMemo(() => {
    if (!activePath) return null;
    return displayTitleForChrome(activePath, noteTitle);
  }, [activePath, noteTitle]);

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
      <PreVaultDesktopFrame>
        <div className="flex min-h-0 flex-1 items-center justify-center bg-background text-muted-foreground">
          加载中…
        </div>
      </PreVaultDesktopFrame>
    );
  }

  if (!vaultPath) {
    return (
      <PreVaultDesktopFrame>
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
          {vaultError ? (
            <p
              className="max-w-md text-center text-sm text-destructive"
              role="alert"
            >
              {vaultError}
            </p>
          ) : null}
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
      </PreVaultDesktopFrame>
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
            onSelect={(p) => activateTab(p)}
            onClose={closeTab}
            onNew={() => void handleNewNote()}
          />
        }
        editor={
          <div
            data-testid="editor-shell"
            className={cn(
              "relative flex min-h-0 flex-1 flex-col",
              outlineOpen && activePath && "iris-editor-outline-open",
            )}
          >
            {activePath ? (
              <ErrorBoundary scope="编辑器">
                <TipTapEditor
                  key={activePath}
                  initialBodyMarkdown={editorBodyMarkdown}
                  contentCacheKey={activePath}
                  reingestKey={editorContentTick}
                  zen={zen}
                  zoom={editorZoom}
                  titleSlot={editorTitleSlot}
                  onDirty={handleDirty}
                  onSlashCommand={runEditorActionById}
                  onBodyContextMenu={editorContextMenu.handleContextMenu}
                  onEditorReady={handleEditorReady}
                  onBodyStatsChange={updateEditorStats}
                  onInlineAiRetry={(ed) => void inlineAi.retry(ed)}
                  onOpenWikiLink={(title) => void openNote(`${title}.md`)}
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
                onOpen={(p) => void openNote(p)}
                onNew={() => void handleNewNote()}
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
          </div>
        }
        aiPanel={
          <ErrorBoundary scope="AI面板">
            <UnifiedAssistantPanel
              notePath={activePath}
              noteDisplayTitle={activeDocumentTitle}
              noteContent={markdown}
              webSearch={webSearch}
              getWritingContext={getWritingContext}
              getParagraphText={getParagraphText}
              selectionQuote={selectionQuote}
              prefillMessage={assistantPrefill}
              onChromeChange={setAssistantChrome}
              onVaultRefresh={bumpVaultIndex}
              onInsertToEditor={handleInsertToEditor}
              onPatchApplied={(newContent: string) => {
                applyMarkdownToEditor(newContent);
                markdownRef.current = newContent;
                const path = activePathRef.current;
                if (path) {
                  markClean(
                    path,
                    resolveNoteDisplayTitle({
                      path,
                      title: activeDocumentTitle ?? undefined,
                    }),
                  );
                }
              }}
            />
          </ErrorBoundary>
        }
        statusBar={
          <StatusBar
            path={activePath}
            documentTitle={activeDocumentTitle}
            unsaved={tabs.find((t) => t.path === activePath)?.dirty ?? false}
            characterCount={editorStats.characterCount}
            readingMinutes={editorStats.readingMinutes}
            aiStatus={aiStatus}
            assistantChrome={assistantChrome}
            keyboardLeaderPending={keyboardLeaderPending}
            editorZoom={editorZoom}
            onEditorZoomIn={zoomIn}
            onEditorZoomOut={zoomOut}
            onEditorZoomReset={resetZoom}
            onUndo={handleUndo}
            onRedo={handleRedo}
            canUndo={canUndo}
            canRedo={canRedo}
            webSearch={webSearch}
            onWebSearchChange={setWebSearch}
            connectivity={connectivityStatus}
            onOpenConnectivitySettings={handleOpenConnectivitySettings}
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
              onSelect={(p) => void openNote(p)}
            />
            <VaultNavigator
              open={overlays.fileSheet}
              onClose={() => overlays.closeOverlay("fileSheet")}
              onOpen={(p) => void openNote(p)}
            />
            <RecycleBinSheet
              open={overlays.recycleBinOpen}
              onClose={() => overlays.closeOverlay("recycleBin")}
              onRestored={(p) => void openNote(p)}
              onIndexChange={bumpVaultIndex}
            />
            <SearchPanel
              open={overlays.searchOpen}
              onClose={() => overlays.closeOverlay("search")}
              onOpen={(p) => void openNote(p)}
            />
            <Suspense fallback={<LazyFallback />}>
              <SettingsPanel
                open={overlays.settingsOpen}
                onClose={() => overlays.closeOverlay("settings")}
                theme={theme}
                onThemeChange={(t) => void setTheme(t)}
              />
            </Suspense>
            <SkillsPanel
              open={overlays.skillsOpen}
              onClose={() => overlays.closeOverlay("skills")}
            />
            <BacklinksPanel
              open={overlays.backlinksOpen}
              onClose={() => overlays.closeOverlay("backlinks")}
              notePath={activePath}
              onOpen={(p) => void openNote(p)}
            />
            <TagView
              open={overlays.tagViewOpen}
              onClose={() => overlays.closeOverlay("tags")}
              onOpen={(p) => void openNote(p)}
            />
            <Suspense fallback={<LazyFallback />}>
              <VersionTimeline
                open={overlays.versionOpen}
                onClose={() => overlays.closeOverlay("version")}
                notePath={activePath}
                currentContent={markdown}
                getCurrentContent={() => getLiveMarkdownRef.current()}
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
                  onOpenNote={(p) => void openNote(p)}
                />
              </Suspense>
            </ErrorBoundary>
            <ConflictDialog
              open={conflictState?.open ?? false}
              localContent={conflictState?.localContent ?? ""}
              externalContent={conflictState?.externalContent ?? ""}
              filePath={conflictState?.filePath ?? ""}
              onKeepLocal={handleConflictKeepLocal}
              onAcceptExternal={handleConflictAcceptExternal}
              onManualEdit={handleConflictManualEdit}
            />
          </>
        }
      />
    </DesktopFrame>
  );
}

export default App;
