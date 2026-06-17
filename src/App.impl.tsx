import type { Editor } from "@tiptap/react";
import { Moon, Sun } from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";

import { DocumentTitleField } from "@/components/editor/DocumentTitleField";
import { AppAiPanelSlot } from "@/components/layout/AppAiPanelSlot";
import { AppEditorWorkspace } from "@/components/layout/AppEditorWorkspace";
import { AppOverlays } from "@/components/layout/AppOverlays";
import { AppShell } from "@/components/layout/AppShell";
import { AppStatusBarSlot } from "@/components/layout/AppStatusBarSlot";
import { DesktopFrame } from "@/components/layout/DesktopFrame";
import { MinimalWindowChrome } from "@/components/layout/MinimalWindowChrome";
import { TabBar } from "@/components/layout/TabBar";
import { Button } from "@/components/ui/button";
import { useAppKeyboard } from "@/hooks/useAppKeyboard";
import { useAiSidecarBridge } from "@/hooks/useAiSidecarBridge";
import { useAutoVersionSettings } from "@/hooks/useAutoVersionSettings";
import { useAppShortcuts } from "@/hooks/useAppShortcuts";
import { useAppEditorActions } from "@/hooks/useAppEditorActions";
import {
  useAppPersistenceLifecycle,
  type PersistBeforeLeave,
} from "@/hooks/useAppPersistenceLifecycle";
import { useClassifiedVaultSession } from "@/hooks/useClassifiedVaultSession";
import { useEditorContextMenu } from "@/hooks/useEditorContextMenu";
import { useAutoVaultIndex } from "@/hooks/useAutoVaultIndex";
import { useOpenNote } from "@/hooks/useOpenNote";
import { useEditorZoom } from "@/hooks/useEditorZoom";
import { useEditorStats } from "@/hooks/useEditorStats";
import { useInlineAi } from "@/hooks/useInlineAi";
import { useConnectivityStatus } from "@/hooks/useConnectivityStatus";
import { useLlmProvider } from "@/hooks/useLlmProvider";
import { useOverlayManager } from "@/hooks/useOverlayManager";
import { useTabManager } from "@/hooks/useTabManager";
import { useTheme } from "@/hooks/useTheme";
import { useZenExitKeyboard } from "@/hooks/useZenExitKeyboard";
import { useMacOSWindowChromeSync } from "@/hooks/useMacOSWindowChromeSync";
import { useVault } from "@/hooks/useVault";
import { displayTitleForChrome } from "@/lib/note-display";
import { isClassifiedVaultPath } from "@/lib/classified-path";
import {
  fileRead,
  fileSetLock,
  listenClassifiedFileTaken,
  listenFileChanged,
  listenVersionSaveComplete,
} from "@/lib/ipc";
import { formatVersionSaveStatus } from "@/lib/version-save-status";
import { isTauriRuntime } from "@/lib/tauri-runtime";

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
  const [aiStatus, setAiStatus] = useState("AI 空闲");
  const [conflictState, setConflictState] = useState<{
    open: boolean;
    localContent: string;
    externalContent: string;
    filePath: string;
  } | null>(null);
  const { editorStats, updateEditorStats, resetEditorStats } = useEditorStats();
  const [homeActive, setHomeActive] = useState(false);
  const [zen, setZen] = useState(false);
  useZenExitKeyboard({ zen, setZen });
  const [outlineOpen, setOutlineOpen] = useState(loadOutlineOpen);
  const [findReplaceOpen, setFindReplaceOpen] = useState(false);
  const [findReplaceMode, setFindReplaceMode] = useState<"find" | "replace">(
    "find",
  );
  const [classifiedOpen, setClassifiedOpen] = useState(false);
  const [vaultIndexEpoch, setVaultIndexEpoch] = useState(0);
  const {
    zoom: editorZoom,
    setZoom,
    zoomIn,
    zoomOut,
    resetZoom,
  } = useEditorZoom();
  const editorRef = useRef<Editor | null>(null);
  const [editorInstance, setEditorInstance] = useState<Editor | null>(null);
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const undoRedoStateRef = useRef({ canUndo: false, canRedo: false });
  const editorTransactionCleanupRef = useRef<(() => void) | null>(null);
  const undoRedoRafRef = useRef<number | null>(null);
  const overlays = useOverlayManager();
  const { provider: llmProvider } = useLlmProvider();
  const { status: connectivityStatus } = useConnectivityStatus();

  const bumpVaultIndex = useCallback(
    () => setVaultIndexEpoch((n) => n + 1),
    [],
  );

  const showHome = useCallback(() => {
    setHomeActive(true);
  }, []);

  const leaveHome = useCallback(() => {
    setHomeActive(false);
  }, []);

  const dirtyRef = useRef(false);
  const autoSnapshotGenerationRef = useRef(0);

  const persistBeforeLeaveRef = useRef<PersistBeforeLeave>(async () => null);

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
    activeFileLocked,
    setFileLocked,
  } = useTabManager({
    onStatusChange: setAiStatus,
    onVaultIndexBump: bumpVaultIndex,
    persistBeforeLeave: (path) => persistBeforeLeaveRef.current(path),
  });

  const {
    aiPanelOpen,
    assistantChrome,
    prefillMessage: assistantPrefill,
    selectionQuote,
    setAiPanelOpen,
    setAssistantChrome,
    setWebSearch,
    sendSelectionToAi,
    toggleWebSearch,
    webSearchEnabled: webSearch,
  } = useAiSidecarBridge({
    activePathRef,
    editorRef,
    setAiStatus,
  });

  const openNoteLeavingHome = useCallback(
    (
      path: string,
      titleHint?: string,
      options?: Parameters<typeof openNote>[2],
    ) => {
      leaveHome();
      void openNote(path, titleHint, options);
    },
    [leaveHome, openNote],
  );

  const handleActivateTab = useCallback(
    (path: string) => {
      leaveHome();
      activateTab(path);
    },
    [leaveHome, activateTab],
  );

  const handleNewNoteLeavingHome = useCallback(() => {
    leaveHome();
    void handleNewNote();
  }, [leaveHome, handleNewNote]);

  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;

  /** Resync dirty flag only when switching documents (not on every tab metadata update). */
  useEffect(() => {
    if (!activePath) {
      dirtyRef.current = false;
      return;
    }
    const tab = tabsRef.current.find((t) => t.path === activePath);
    dirtyRef.current = tab?.dirty ?? false;
  }, [activePath]);

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
    editorContentTick,
    activePathRef,
    markdownRef,
    frontmatterYamlRef,
    editorRef,
    dirtyRef,
    updateTabTitle,
    replaceOpenTabPath,
  });

  getLiveMarkdownRef.current = getLiveMarkdown;

  const autoVersionSettings = useAutoVersionSettings();

  const {
    notifyDirty,
    flushSave,
    resetVersionIdle,
    handleSaveNote,
    versionSnapshotScheduler,
  } = useAppPersistenceLifecycle({
    activeFileLocked,
    activePath,
    activePathRef,
    applySavedMarkdown,
    autoSnapshotGenerationRef,
    autoVersionEnabled: autoVersionSettings.autoVersionEnabled,
    autoVersionIdleMinutes: autoVersionSettings.autoVersionIdleMinutes,
    dirtyRef,
    editorRef,
    getLiveMarkdownRef,
    getTabMarkdownCached,
    markClean,
    noteTitle,
    persistBeforeLeaveRef,
    schedulePathSync,
    setAiStatus,
    setMarkdown,
    syncTabMarkdownCache,
    tabsRef,
  });

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
        .then(({ content: externalContent }) => {
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

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let unlisten: (() => void) | undefined;
    void listenClassifiedFileTaken((event) => {
      const path = event.path;
      if (tabsRef.current.some((tab) => tab.path === path)) {
        void closeTab(path);
      }
      bumpVaultIndex();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [closeTab, bumpVaultIndex]);

  const openClassifiedPaths = useMemo(
    () =>
      tabs.filter((tab) => isClassifiedVaultPath(tab.path)).map((t) => t.path),
    [tabs],
  );

  const {
    status: classifiedVaultStatus,
    waiting: classifiedWaiting,
    idleDeadline: classifiedIdleDeadline,
    refreshStatus: refreshClassifiedStatus,
    touchActivity: touchClassifiedActivity,
    requestLock: requestClassifiedLock,
    onUnlocked: onClassifiedUnlocked,
    setWaiting: setClassifiedWaiting,
  } = useClassifiedVaultSession({
    enabled: Boolean(vaultPath) && isTauriRuntime(),
    openClassifiedPaths,
  });

  useEffect(() => {
    if (classifiedOpen) {
      void refreshClassifiedStatus();
    }
  }, [classifiedOpen, refreshClassifiedStatus]);

  const activeNoteIsClassified = Boolean(
    activePath && isClassifiedVaultPath(activePath),
  );

  const handleLockToggle = useCallback(
    async (locked: boolean) => {
      const path = activePathRef.current;
      if (!path || isClassifiedVaultPath(path)) return;
      setFileLocked(path, locked);
      try {
        await fileSetLock(path, locked);
      } catch (err: unknown) {
        setFileLocked(path, !locked);
        const msg = err instanceof Error ? err.message : String(err);
        setAiStatus(`锁定状态保存失败：${msg}`);
      }
    },
    [activePathRef, setFileLocked],
  );

  const handleConflictKeepLocal = useCallback(() => {
    setConflictState(null);
    // Re-save local content to overwrite external changes
    void flushSave();
  }, [flushSave]);

  const handleConflictAcceptExternal = useCallback(() => {
    if (!conflictState) return;
    setConflictState(null);
    // Re-open the note from disk to load external content
    openNoteLeavingHome(conflictState.filePath);
  }, [conflictState, openNoteLeavingHome]);

  const handleConflictManualEdit = useCallback(() => {
    setConflictState(null);
  }, []);

  const openFindReplace = useCallback((mode: "find" | "replace") => {
    setFindReplaceMode(mode);
    setFindReplaceOpen(true);
  }, []);

  const handleDirty = useCallback(() => {
    if (activeFileLocked) return;
    if (!dirtyRef.current) {
      dirtyRef.current = true;
      markDirty();
    }
    notifyDirty();
    resetVersionIdle();
  }, [activeFileLocked, notifyDirty, resetVersionIdle, markDirty]);

  const handleTitleChange = useCallback(
    (raw: string) => {
      if (activeFileLocked) return;
      onTitleChange(raw);
      if (!dirtyRef.current) {
        dirtyRef.current = true;
        markDirty();
      }
      notifyDirty();
      resetVersionIdle();
    },
    [activeFileLocked, markDirty, notifyDirty, onTitleChange, resetVersionIdle],
  );

  const { rescanVault } = useAutoVaultIndex(vaultPath, loading, {
    onStatus: setAiStatus,
    onIndexed: bumpVaultIndex,
  });

  const handleVaultRescan = useCallback(() => {
    void rescanVault("manual");
  }, [rescanVault]);

  const handleOpenConnectivitySettings = useCallback(
    () => overlays.openManagementCenter("ai"),
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

  const cancelUndoRedoStateRefresh = useCallback(() => {
    if (undoRedoRafRef.current === null) return;
    cancelAnimationFrame(undoRedoRafRef.current);
    undoRedoRafRef.current = null;
  }, []);

  const scheduleUndoRedoStateRefresh = useCallback(
    (ed: Editor | null = editorRef.current) => {
      if (undoRedoRafRef.current !== null) {
        cancelAnimationFrame(undoRedoRafRef.current);
      }
      undoRedoRafRef.current = requestAnimationFrame(() => {
        undoRedoRafRef.current = null;
        const currentEditor = ed && !ed.isDestroyed ? ed : editorRef.current;
        updateUndoRedoState(
          currentEditor && !currentEditor.isDestroyed ? currentEditor : null,
        );
      });
    },
    [updateUndoRedoState],
  );

  const clearEditorTransactionListener = useCallback(() => {
    editorTransactionCleanupRef.current?.();
    editorTransactionCleanupRef.current = null;
  }, []);

  useEffect(() => {
    if (!activePath) {
      clearEditorTransactionListener();
      cancelUndoRedoStateRefresh();
      setEditorInstance(null);
      updateUndoRedoState(null);
    }
  }, [
    activePath,
    cancelUndoRedoStateRefresh,
    clearEditorTransactionListener,
    updateUndoRedoState,
  ]);

  useEffect(() => {
    return () => {
      cancelUndoRedoStateRefresh();
    };
  }, [cancelUndoRedoStateRefresh]);

  useEffect(() => {
    if (!activePath) {
      resetEditorStats();
    }
  }, [activePath, resetEditorStats]);

  const handleEditorReady = useCallback(
    (ed: Editor | null) => {
      clearEditorTransactionListener();
      editorRef.current = ed;
      if (!ed) {
        cancelUndoRedoStateRefresh();
        setEditorInstance(null);
        updateUndoRedoState(null);
        return;
      }

      setEditorInstance(ed);
      updateUndoRedoState(ed);

      const handleTransaction = ({
        editor: currentEditor,
      }: {
        editor: Editor;
      }) => {
        scheduleUndoRedoStateRefresh(currentEditor);
      };

      ed.on("transaction", handleTransaction);
      editorTransactionCleanupRef.current = () => {
        ed.off("transaction", handleTransaction);
      };
    },
    [
      cancelUndoRedoStateRefresh,
      clearEditorTransactionListener,
      scheduleUndoRedoStateRefresh,
      updateUndoRedoState,
    ],
  );

  const editorTitleSlot = useMemo(
    () => (
      <DocumentTitleField
        value={noteTitle}
        onChange={handleTitleChange}
        onBlur={onTitleBlur}
        editorRef={editorRef}
        readOnly={activeFileLocked}
      />
    ),
    [noteTitle, handleTitleChange, onTitleBlur, editorRef, activeFileLocked],
  );

  const {
    getParagraphText,
    getWritingContext,
    handleInsertToEditor,
    handleRedo,
    handleUndo,
    runEditorActionById,
  } = useAppEditorActions({
    activeNoteIsClassified,
    activePathRef,
    editorRef,
    getLiveMarkdown,
    inlineAi,
    scheduleUndoRedoStateRefresh,
    sendSelectionToAi,
    setAiStatus,
  });

  const editorContextMenu = useEditorContextMenu(
    editorInstance,
    Boolean(activePath),
    () => setAiStatus("选区 AI：请使用右键菜单"),
    activeFileLocked,
  );

  const { appShortcutItems, handleAppShortcut } = useAppShortcuts({
    activePath,
    activePathRef,
    closeTab,
    handleNewNote,
    handleSaveNote,
    handleVaultRescan,
    openFindReplace,
    overlays,
    resetZoom,
    saveOutlineOpen,
    sendSelectionToAi,
    setAiPanelOpen,
    setClassifiedOpen,
    setOutlineOpen,
    setTheme,
    setZen,
    theme,
    toggleWebSearch,
    vaultPath,
    zoomIn,
    zoomOut,
  });

  useAppKeyboard({
    items: appShortcutItems,
    vaultPath,
    activePathRef,
    onAction: handleAppShortcut,
  });

  const activeDocumentTitle = useMemo(() => {
    if (!activePath) return null;
    return displayTitleForChrome(activePath, noteTitle);
  }, [activePath, noteTitle]);
  const assistantNotePath = activeNoteIsClassified ? null : activePath;
  const assistantDocumentTitle = activeNoteIsClassified
    ? null
    : activeDocumentTitle;

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
            isHomeActive={homeActive}
            onHome={showHome}
            onSelect={handleActivateTab}
            onClose={closeTab}
            onNew={handleNewNoteLeavingHome}
          />
        }
        editor={
          <AppEditorWorkspace
            activeFileLocked={activeFileLocked}
            activeNoteIsClassified={activeNoteIsClassified}
            activePath={activePath}
            editorBodyMarkdown={editorBodyMarkdown}
            editorContentTick={editorContentTick}
            editorContextMenu={editorContextMenu}
            editorInstance={editorInstance}
            editorTitleSlot={editorTitleSlot}
            editorZoom={editorZoom}
            findReplaceMode={findReplaceMode}
            findReplaceOpen={findReplaceOpen}
            handleDirty={handleDirty}
            handleEditorReady={handleEditorReady}
            handleLockToggle={handleLockToggle}
            handleNewNoteLeavingHome={handleNewNoteLeavingHome}
            homeActive={homeActive}
            inlineAi={inlineAi}
            onOutlineOpenChange={(open) => {
              setOutlineOpen(open);
              saveOutlineOpen(open);
            }}
            onOpenAiManagement={() => overlays.openManagementCenter("ai")}
            onOpenQuickOpen={() => overlays.openOverlay("quickOpen")}
            onOpenSearch={() => overlays.openOverlay("search")}
            openNoteLeavingHome={openNoteLeavingHome}
            outlineOpen={outlineOpen}
            runEditorActionById={runEditorActionById}
            setFindReplaceMode={setFindReplaceMode}
            setFindReplaceOpen={setFindReplaceOpen}
            updateEditorStats={updateEditorStats}
            vaultIndexEpoch={vaultIndexEpoch}
            vaultPath={vaultPath}
            zen={zen}
          />
        }
        aiPanel={
          <AppAiPanelSlot
            activeDocumentTitle={activeDocumentTitle}
            activeNoteIsClassified={activeNoteIsClassified}
            activePathRef={activePathRef}
            assistantDocumentTitle={assistantDocumentTitle}
            assistantNotePath={assistantNotePath}
            assistantPrefill={assistantPrefill}
            bumpVaultIndex={bumpVaultIndex}
            dirtyRef={dirtyRef}
            getLiveMarkdown={getLiveMarkdown}
            getParagraphText={getParagraphText}
            getWritingContext={getWritingContext}
            handleInsertToEditor={handleInsertToEditor}
            markClean={markClean}
            markdownRef={markdownRef}
            selectionQuote={selectionQuote}
            setAiStatus={setAiStatus}
            setAssistantChrome={setAssistantChrome}
            syncTabMarkdownCache={syncTabMarkdownCache}
            webSearch={webSearch}
            applyMarkdownToEditor={applyMarkdownToEditor}
          />
        }
        statusBar={
          <AppStatusBarSlot
            activePath={activePath}
            activeDocumentTitle={activeDocumentTitle}
            unsaved={tabs.find((t) => t.path === activePath)?.dirty ?? false}
            characterCount={editorStats.characterCount}
            readingMinutes={editorStats.readingMinutes}
            aiStatus={aiStatus}
            assistantChrome={assistantChrome}
            editorZoom={editorZoom}
            onEditorZoomIn={zoomIn}
            onEditorZoomOut={zoomOut}
            onEditorZoomReset={resetZoom}
            onEditorZoomChange={setZoom}
            onUndo={handleUndo}
            onRedo={handleRedo}
            canUndo={canUndo}
            canRedo={canRedo}
            webSearch={webSearch}
            onWebSearchChange={setWebSearch}
            theme={theme}
            onThemeChange={(nextTheme) => void setTheme(nextTheme)}
            connectivity={connectivityStatus}
            onOpenConnectivitySettings={handleOpenConnectivitySettings}
            onOpenManagementCenter={() =>
              overlays.openManagementCenter("overview")
            }
            onOpenGraph={() => overlays.openOverlay("graph")}
          />
        }
        overlays={
          <AppOverlays
            activePath={activePath}
            applyMarkdownToEditor={applyMarkdownToEditor}
            bumpVaultIndex={bumpVaultIndex}
            classifiedIdleDeadline={classifiedIdleDeadline}
            classifiedOpen={classifiedOpen}
            classifiedVaultStatus={classifiedVaultStatus}
            classifiedWaiting={classifiedWaiting}
            conflictState={conflictState}
            getCurrentContent={() => getLiveMarkdownRef.current()}
            handleConflictAcceptExternal={handleConflictAcceptExternal}
            handleConflictKeepLocal={handleConflictKeepLocal}
            handleConflictManualEdit={handleConflictManualEdit}
            markdown={markdown}
            onClassifiedUnlocked={onClassifiedUnlocked}
            openClassifiedPaths={openClassifiedPaths}
            openNoteLeavingHome={openNoteLeavingHome}
            overlays={overlays}
            refreshClassifiedStatus={refreshClassifiedStatus}
            requestClassifiedLock={requestClassifiedLock}
            setClassifiedOpen={setClassifiedOpen}
            setClassifiedWaiting={setClassifiedWaiting}
            setWebSearch={setWebSearch}
            openKnowledgeRelations={() =>
              overlays.openOverlay("knowledgeRelations")
            }
            openVersion={() => overlays.openOverlay("version")}
            rescanVault={handleVaultRescan}
            autoVersionSettings={autoVersionSettings}
            tabs={tabs}
            touchClassifiedActivity={touchClassifiedActivity}
            versionSnapshotScheduler={versionSnapshotScheduler}
            webSearch={webSearch}
          />
        }
      />
    </DesktopFrame>
  );
}

export default App;
