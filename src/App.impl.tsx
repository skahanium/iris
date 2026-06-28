import type { Editor } from "@tiptap/react";
import { Moon, Sun } from "lucide-react";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { DocumentTitleField } from "@/components/editor/DocumentTitleField";
import { AppAiPanelSlot } from "@/components/layout/AppAiPanelSlot";
import { AppEditorWorkspace } from "@/components/layout/AppEditorWorkspace";
import { AppOverlays } from "@/components/layout/AppOverlays";
import { AppShell } from "@/components/layout/AppShell";
import { AppStatusBarSlot } from "@/components/layout/AppStatusBarSlot";
import { DesktopFrame } from "@/components/layout/DesktopFrame";
import { PreVaultDesktopFrame } from "@/components/layout/PreVaultDesktopFrame";
import { StartupSplash } from "@/components/layout/StartupSplash";
import { TabBar } from "@/components/layout/TabBar";
import { Button } from "@/components/ui/button";
import { useAppKeyboard } from "@/hooks/useAppKeyboard";
import { useAiSidecarBridge } from "@/hooks/useAiSidecarBridge";
import { useAutoVersionSettings } from "@/hooks/useAutoVersionSettings";
import {
  useCurrentFileChangeListener,
  type ConflictState,
} from "@/hooks/useCurrentFileChangeListener";
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
import { useNavigatorFileLifecycle } from "@/hooks/useNavigatorFileLifecycle";
import { useFileConflictResolution } from "@/hooks/useFileConflictResolution";
import { useEditorZoom } from "@/hooks/useEditorZoom";
import { useEditorStats } from "@/hooks/useEditorStats";
import { useEditorUndoRedoState } from "@/hooks/useEditorUndoRedoState";
import { useInlineAi } from "@/hooks/useInlineAi";
import { useConnectivityStatus } from "@/hooks/useConnectivityStatus";
import { useLlmProvider } from "@/hooks/useLlmProvider";
import { useOverlayManager } from "@/hooks/useOverlayManager";
import { useArtifactTabs } from "@/hooks/useArtifactTabs";
import { usePreparedWorkspaceTransitions } from "@/hooks/usePreparedWorkspaceTransitions";
import { usePreparedNoteInvalidationCallbacks } from "@/hooks/usePreparedNoteInvalidationCallbacks";
import { useWorkspaceAssistantRouting } from "@/hooks/useWorkspaceAssistantRouting";
import { useWorkspaceSessionSnapshot } from "@/hooks/useWorkspaceSessionSnapshot";
import { useWorkspaceTabRouting } from "@/hooks/useWorkspaceTabRouting";
import { useTabManager } from "@/hooks/useTabManager";
import { useTheme } from "@/hooks/useTheme";
import { useZenExitKeyboard } from "@/hooks/useZenExitKeyboard";
import { useMacOSWindowChromeSync } from "@/hooks/useMacOSWindowChromeSync";
import { useVault } from "@/hooks/useVault";
import { displayTitleForChrome } from "@/lib/note-display";
import { isClassifiedVaultPath } from "@/lib/classified-path";
import {
  fileSetLock,
  listenClassifiedFileTaken,
  listenVersionSaveComplete,
} from "@/lib/ipc";
import { formatVersionSaveStatus } from "@/lib/version-save-status";
import { isTauriRuntime } from "@/lib/tauri-runtime";

function loadOutlineOpen(): boolean {
  try {
    return localStorage.getItem("iris-outline-open") !== "false";
  } catch {
    return true;
  }
}

function saveOutlineOpen(open: boolean): void {
  try {
    localStorage.setItem("iris-outline-open", open ? "true" : "false");
  } catch {
    return;
  }
}

function App() {
  useMacOSWindowChromeSync();

  const { vaultPath, loading, pickVault, error: vaultError } = useVault();
  const { theme, setTheme } = useTheme();
  const [startupSplashVisible, setStartupSplashVisible] =
    useState(isTauriRuntime);
  const [aiStatus, setAiStatus] = useState("AI 空闲");
  const [conflictState, setConflictState] = useState<ConflictState | null>(
    null,
  );
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
  const editorReadyForPersistenceRef = useRef(false);
  const overlays = useOverlayManager();
  const { provider: llmProvider } = useLlmProvider();
  const { status: connectivityStatus } = useConnectivityStatus();

  const bumpVaultIndex = useCallback(
    () => setVaultIndexEpoch((n) => n + 1),
    [],
  );

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
    discardOpenTab,
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
    pendingNoteOpen,
    commitPendingNoteOpen,
  } = useTabManager({
    onStatusChange: setAiStatus,
    onVaultIndexBump: bumpVaultIndex,
    persistBeforeLeave: (path) => persistBeforeLeaveRef.current(path),
  });
  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;
  const openNotePaths = useMemo(() => tabs.map((tab) => tab.path), [tabs]);

  useWorkspaceSessionSnapshot({ activePath, tabs, vaultPath });
  const {
    activateArtifact,
    activeArtifactTab,
    artifactTabs,
    closeArtifact,
    closeAllEvidenceArtifacts,
    closeEvidenceArtifactsForSession,
    openArtifact,
    setActiveArtifactId,
  } = useArtifactTabs();

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
    getNoteContent: () => markdownRef.current,
    setAiStatus,
  });

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

  const classifiedUnlocked = classifiedVaultStatus === "unlocked";

  useEffect(() => {
    if (classifiedOpen) {
      void refreshClassifiedStatus();
    }
  }, [classifiedOpen, refreshClassifiedStatus]);

  const {
    clearPendingOpenFromWorkspace,
    handleActivateWorkspaceTab: handleActivateNoteOrArtifactTab,
    handleNewNoteLeavingHome,
    invalidatePreparedNote,
    openNoteLeavingHome,
    pendingOpen,
    prepareVisibleNote,
    prepareNotePath,
    prepareClassifiedNotePath,
    showHome,
    warmPreparedNotes,
  } = usePreparedWorkspaceTransitions<
    NonNullable<Parameters<typeof openNote>[2]>
  >({
    activePathRef,
    activateArtifact,
    activateTab,
    classifiedVaultStatus,
    handleNewNote,
    openNote,
    setActiveArtifactId,
    setHomeActive,
    tabs,
    vaultPath,
  });

  const currentNoteIsClassified = Boolean(
    activePath && isClassifiedVaultPath(activePath),
  );
  const {
    activeMediaTab,
    activeNoteIsClassified,
    activeWorkspacePath,
    handleActivateWorkspaceTab,
    handleCloseWorkspaceTab,
    handleNewWorkspaceNote,
    openWorkspacePathLeavingHome,
    workspaceTabs,
  } = useWorkspaceTabRouting<NonNullable<Parameters<typeof openNote>[2]>>({
    activeArtifactTab,
    activePath,
    artifactTabs,
    closeArtifact,
    closeTab,
    currentNoteIsClassified,
    handleActivateNoteOrArtifactTab,
    handleNewNoteLeavingHome,
    openNoteLeavingHome,
    setActiveArtifactId,
    setHomeActive,
    tabs,
  });

  useEffect(() => {
    if (!activePath) {
      dirtyRef.current = false;
      return;
    }
    const tab = tabsRef.current.find((t) => t.path === activePath);
    dirtyRef.current = tab?.dirty ?? false;
  }, [activePath]);

  const getLiveMarkdownRef = useRef(() => markdownRef.current);
  const inlineAiDomain =
    activeNoteIsClassified &&
    classifiedUnlocked &&
    !activeArtifactTab &&
    !activeMediaTab &&
    activePath
      ? "classified"
      : "normal";
  const inlineAi = useInlineAi({
    provider: llmProvider,
    domain: inlineAiDomain,
    notePath: inlineAiDomain === "classified" ? activePath : null,
    getNoteContent: () => getLiveMarkdownRef.current(),
    onStatus: setAiStatus,
  });

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
    editorReadyRef: editorReadyForPersistenceRef,
    dirtyRef,
    updateTabTitle,
    replaceOpenTabPath,
  });

  getLiveMarkdownRef.current = getLiveMarkdown;

  const autoVersionSettings = useAutoVersionSettings();

  const {
    notifyDirty,
    flushSave,
    cancelPendingSave,
    awaitSaveInFlight,
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
    editorReadyRef: editorReadyForPersistenceRef,
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

  const {
    handleBeforeFilePathChange,
    handleFilePathChanged,
    handleBeforeFileDelete,
    handleFileDeleted,
  } = useNavigatorFileLifecycle({
    activePathRef,
    awaitSaveInFlight,
    bumpVaultIndex,
    cancelPendingSave,
    discardOpenTab,
    getTabMarkdownCached,
    markClean,
    markdownRef,
    persistBeforeLeaveRef,
    replaceOpenTabPath,
    tabsRef,
  });

  const {
    handlePreparedFileDeleted,
    handlePreparedFilePathChanged,
    invalidateActivePreparedNote,
  } = usePreparedNoteInvalidationCallbacks({
    activePathRef,
    handleFileDeleted,
    handleFilePathChanged,
    invalidatePreparedNote,
  });

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenVersionSaveComplete((payload) => {
      if (disposed) return;
      if (payload.path !== activePathRef.current) return;
      setAiStatus(formatVersionSaveStatus(payload));
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [activePathRef]);

  useCurrentFileChangeListener({
    activePathRef,
    awaitSaveInFlight,
    bumpVaultIndex,
    cancelPendingSave,
    discardOpenTab,
    getLiveMarkdownRef,
    onFileChanged: invalidatePreparedNote,
    setConflictState,
  });

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenClassifiedFileTaken((event) => {
      if (disposed) return;
      const path = event.path;
      invalidatePreparedNote(path);
      if (tabsRef.current.some((tab) => tab.path === path)) {
        void closeTab(path);
      }
      bumpVaultIndex();
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [closeTab, bumpVaultIndex, invalidatePreparedNote]);

  const applyMarkdownToEditor = useCallback(
    (content: string) => {
      markdownRef.current = content;
      loadBodyIntoEditor(content);
      setMarkdown(content);
    },
    [loadBodyIntoEditor, markdownRef, setMarkdown],
  );

  const handleLockToggle = useCallback(
    async (locked: boolean) => {
      const path = activePathRef.current;
      if (!path || isClassifiedVaultPath(path)) return;
      try {
        if (locked) {
          await flushSave();
        }
        setFileLocked(path, locked);
        await fileSetLock(path, locked);
        invalidatePreparedNote(path);
      } catch (err: unknown) {
        setFileLocked(path, !locked);
        const msg = err instanceof Error ? err.message : String(err);
        setAiStatus(`锁定状态保存失败：${msg}`);
      }
    },
    [activePathRef, flushSave, invalidatePreparedNote, setFileLocked],
  );

  const {
    handleConflictAcceptExternal,
    handleConflictKeepLocal,
    handleConflictManualEdit,
  } = useFileConflictResolution({
    activePathRef,
    applyMarkdownToEditor,
    conflictState,
    dirtyRef,
    flushSave,
    invalidatePreparedNote,
    markClean,
    openNoteLeavingHome,
    setConflictState,
    syncTabMarkdownCache,
  });

  const openFindReplace = useCallback((mode: "find" | "replace") => {
    setFindReplaceMode(mode);
    setFindReplaceOpen(true);
  }, []);

  const handleDirty = useCallback(() => {
    if (activeFileLocked) return;
    if (!dirtyRef.current) {
      dirtyRef.current = true;
      markDirty();
      invalidateActivePreparedNote();
    }
    notifyDirty();
    resetVersionIdle();
  }, [
    activeFileLocked,
    invalidateActivePreparedNote,
    markDirty,
    notifyDirty,
    resetVersionIdle,
  ]);

  const handleTitleChange = useCallback(
    (raw: string) => {
      if (activeFileLocked) return;
      onTitleChange(raw);
      if (!dirtyRef.current) {
        dirtyRef.current = true;
        markDirty();
        invalidateActivePreparedNote();
      }
      notifyDirty();
      resetVersionIdle();
    },
    [
      activeFileLocked,
      invalidateActivePreparedNote,
      markDirty,
      notifyDirty,
      onTitleChange,
      resetVersionIdle,
    ],
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

  const {
    canRedo,
    canUndo,
    editorInstance,
    handleEditorReady: handleUndoRedoEditorReady,
    scheduleUndoRedoStateRefresh,
  } = useEditorUndoRedoState({ activePath, editorRef });

  const handleEditorReady = useCallback(
    (editor: Editor | null) => {
      editorReadyForPersistenceRef.current = editor != null;
      handleUndoRedoEditorReady(editor);
    },
    [handleUndoRedoEditorReady],
  );

  useLayoutEffect(() => {
    editorReadyForPersistenceRef.current = false;
  }, [activePath, editorContentTick]);

  useEffect(() => {
    if (!activePath) {
      resetEditorStats();
    }
  }, [activePath, resetEditorStats]);

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
    {
      aiDomain: activeNoteIsClassified ? "classified" : "normal",
      classifiedUnlocked,
    },
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

  const activeDocumentTitle =
    activePath && displayTitleForChrome(activePath, noteTitle);
  const assistantNotePathWithoutMedia =
    activeArtifactTab || activeNoteIsClassified ? null : activePath;
  const getLiveMarkdownForNoteSurface = useCallback(
    () =>
      activeArtifactTab || activeNoteIsClassified ? "" : getLiveMarkdown(),
    [activeArtifactTab, activeNoteIsClassified, getLiveMarkdown],
  );
  const getWritingContextForNoteSurface = useCallback(
    () => (activeArtifactTab ? null : getWritingContext()),
    [activeArtifactTab, getWritingContext],
  );
  const handleInsertToNoteSurface = useCallback(
    (content: string) => {
      handleInsertToEditor(content);
    },
    [handleInsertToEditor],
  );
  const {
    aiDomain,
    assistantNotePath,
    assistantSelectionQuote,
    classifiedPath,
    getAssistantLiveMarkdown,
    getAssistantParagraphText,
    getAssistantWritingContext,
    handleAssistantInsertToEditor,
  } = useWorkspaceAssistantRouting({
    activeArtifactTab,
    activeMediaTab,
    activeNoteIsClassified,
    activePath,
    assistantNotePathWithoutMedia,
    classifiedUnlocked,
    getLiveMarkdown: getLiveMarkdownForNoteSurface,
    getParagraphText,
    getWritingContext: getWritingContextForNoteSurface,
    handleInsertToEditor: handleInsertToNoteSurface,
    selectionQuote,
    setAiStatus,
  });
  const handlePatchApplied = useCallback(
    (newContent: string) => {
      applyMarkdownToEditor(newContent);
      markdownRef.current = newContent;
      dirtyRef.current = false;
      const path = activePathRef.current;
      if (path) {
        invalidatePreparedNote(path);
        syncTabMarkdownCache(path, newContent);
        markClean(path, activeDocumentTitle ?? noteTitle);
      }
    },
    [
      activeDocumentTitle,
      activePathRef,
      applyMarkdownToEditor,
      invalidatePreparedNote,
      markClean,
      markdownRef,
      noteTitle,
      syncTabMarkdownCache,
    ],
  );

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
          这里只是 Vite 前端热更新地址，浏览器里没有 Rust
          后端，无法读写笔记目录。
        </p>
        <p className="max-w-md text-sm text-muted-foreground">
          方式 B 需要两个终端：一个 <code className="text-xs">npm run dev</code>
          ，另一个启动 <code className="text-xs">npx tauri dev</code>
          ，请使用弹出的{" "}
          <strong className="font-medium text-foreground">Iris</strong>{" "}
          窗口操作。
        </p>
      </div>
    );
  }

  if (startupSplashVisible) {
    return (
      <PreVaultDesktopFrame>
        <StartupSplash
          ready={!loading}
          onExited={() => setStartupSplashVisible(false)}
        />
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
            tabs={workspaceTabs}
            activePath={activeWorkspacePath}
            isHomeActive={homeActive}
            onHome={showHome}
            onSelect={handleActivateWorkspaceTab}
            onClose={handleCloseWorkspaceTab}
            onNew={handleNewWorkspaceNote}
          />
        }
        editor={
          <AppEditorWorkspace
            activeFileLocked={activeFileLocked}
            activeArtifactTab={activeArtifactTab}
            activeMediaTab={activeMediaTab}
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
            handleNewNoteLeavingHome={handleNewWorkspaceNote}
            getNoteContent={getLiveMarkdown}
            homeActive={homeActive}
            inlineAi={inlineAi}
            onOutlineOpenChange={(open) => {
              setOutlineOpen(open);
              saveOutlineOpen(open);
            }}
            onOpenAiManagement={() => overlays.openManagementCenter("ai")}
            onOpenQuickOpen={() => overlays.openOverlay("quickOpen")}
            onOpenSearch={() => overlays.openOverlay("search")}
            openNoteLeavingHome={openWorkspacePathLeavingHome}
            onPrepareNotePath={prepareNotePath}
            onPrepareNote={prepareVisibleNote}
            outlineOpen={outlineOpen}
            pendingOpen={pendingOpen}
            pendingNoteOpen={pendingNoteOpen}
            onPendingOpenSettled={clearPendingOpenFromWorkspace}
            commitPendingNoteOpen={commitPendingNoteOpen}
            runEditorActionById={runEditorActionById}
            setFindReplaceMode={setFindReplaceMode}
            setFindReplaceOpen={setFindReplaceOpen}
            updateEditorStats={updateEditorStats}
            onPatchApplied={handlePatchApplied}
            onVaultRefresh={bumpVaultIndex}
            vaultIndexEpoch={vaultIndexEpoch}
            vaultPath={vaultPath}
            warmPreparedNotes={warmPreparedNotes}
            openNotePaths={openNotePaths}
            zen={zen}
          />
        }
        aiPanel={
          <AppAiPanelSlot
            aiDomain={aiDomain}
            assistantNotePath={assistantNotePath}
            assistantPrefill={assistantPrefill}
            bumpVaultIndex={bumpVaultIndex}
            classifiedPath={classifiedPath}
            getLiveMarkdown={getAssistantLiveMarkdown}
            getParagraphText={getAssistantParagraphText}
            getWritingContext={getAssistantWritingContext}
            handleInsertToEditor={handleAssistantInsertToEditor}
            onOpenArtifact={openArtifact}
            openNoteLeavingHome={openWorkspacePathLeavingHome}
            onPrepareNotePath={prepareNotePath}
            onSessionDeleted={closeEvidenceArtifactsForSession}
            onSessionsCleared={closeAllEvidenceArtifacts}
            onPatchApplied={handlePatchApplied}
            selectionQuote={assistantSelectionQuote}
            setAssistantChrome={setAssistantChrome}
            webSearch={webSearch}
          />
        }
        statusBar={
          <AppStatusBarSlot
            activePath={activeArtifactTab || activeMediaTab ? null : activePath}
            activeDocumentTitle={
              activeArtifactTab
                ? activeArtifactTab.title
                : activeMediaTab
                  ? activeMediaTab.title
                  : activeDocumentTitle
            }
            unsaved={
              activeArtifactTab || activeMediaTab
                ? false
                : (tabs.find((t) => t.path === activePath)?.dirty ?? false)
            }
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
            onBeforeFilePathChange={handleBeforeFilePathChange}
            onFilePathChanged={handlePreparedFilePathChanged}
            onBeforeFileDelete={handleBeforeFileDelete}
            onFileDeleted={handlePreparedFileDeleted}
            onClassifiedUnlocked={onClassifiedUnlocked}
            openClassifiedPaths={openClassifiedPaths}
            openNoteLeavingHome={openWorkspacePathLeavingHome}
            onPrepareNote={prepareVisibleNote}
            onPrepareNotePath={prepareNotePath}
            onPrepareClassifiedNotePath={prepareClassifiedNotePath}
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

App.displayName = "App";

export default App;
