import type { Editor } from "@tiptap/react";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { DocumentTitleField } from "@/components/editor/DocumentTitleField";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { AppAiPanelSlot } from "@/components/layout/AppAiPanelSlot";
import { AppEditorWorkspace } from "@/components/layout/AppEditorWorkspace";
import { AppOverlays } from "@/components/layout/AppOverlays";
import { preloadManagementCenter } from "@/lib/preload-overlays";
import { AppShell } from "@/components/layout/AppShell";
import { AppStatusBarSlot } from "@/components/layout/AppStatusBarSlot";
import { DesktopFrame } from "@/components/layout/DesktopFrame";
import {
  AppPreVaultGate,
  BrowserRuntimeNotice,
} from "@/components/layout/AppPreVaultScreens";
import { TabBar } from "@/components/layout/TabBar";
import { useAppKeyboard } from "@/hooks/useAppKeyboard";
import { useAiSidecarBridge } from "@/hooks/useAiSidecarBridge";
import { useAutoVersionSettings } from "@/hooks/useAutoVersionSettings";
import { useFollowSystemProxy } from "@/hooks/useFollowSystemProxy";
import { useAppUpdateController } from "@/hooks/useAppUpdate";
import { useEmbeddingScheduler } from "@/hooks/useEmbeddingScheduler";
import type { ConflictState } from "@/hooks/useCurrentFileChangeListener";
import { useExternalDocumentLifecycle } from "@/hooks/useExternalDocumentLifecycle";
import { useAppShortcuts } from "@/hooks/useAppShortcuts";
import { useAppEditorActions } from "@/hooks/useAppEditorActions";
import {
  useAppPersistenceLifecycle,
  type PersistBeforeLeave,
  type PersistenceBlocker,
} from "@/hooks/useAppPersistenceLifecycle";
import { useClassifiedVaultSession } from "@/hooks/useClassifiedVaultSession";
import { useEditorContextMenu } from "@/hooks/useEditorContextMenu";
import { useAutoVaultIndex } from "@/hooks/useAutoVaultIndex";
import { useOpenNote } from "@/hooks/useOpenNote";
import { useNavigatorFileLifecycle } from "@/hooks/useNavigatorFileLifecycle";
import { useNoteLifecycleIntentActions } from "@/hooks/useNoteLifecycleIntentActions";
import { useFileConflictResolution } from "@/hooks/useFileConflictResolution";
import { useEditorZoom } from "@/hooks/useEditorZoom";
import { useEditorStats } from "@/hooks/useEditorStats";
import { useEditorUndoRedoState } from "@/hooks/useEditorUndoRedoState";
import { useInlineAi } from "@/hooks/useInlineAi";
import { useConnectivityStatus } from "@/hooks/useConnectivityStatus";
import { useOverlayManager } from "@/hooks/useOverlayManager";
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
import { listenClassifiedFileTaken } from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";
import type { DocumentPersistenceMoveResult } from "@/lib/document-persistence-coordinator";

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
interface IdlePreloadScheduler {
  requestIdleCallback?: (callback: () => void) => number;
  cancelIdleCallback?: (handle: number) => void;
  setTimeout: Window["setTimeout"];
  clearTimeout: Window["clearTimeout"];
}
function scheduleManagementCenterPreload(): () => void {
  const scheduler = window as unknown as IdlePreloadScheduler;
  if (scheduler.requestIdleCallback) {
    const handle = scheduler.requestIdleCallback(() =>
      preloadManagementCenter(),
    );
    return () => scheduler.cancelIdleCallback?.(handle);
  }
  const handle = scheduler.setTimeout(() => preloadManagementCenter(), 0);
  return () => scheduler.clearTimeout(handle);
}
function App() {
  useMacOSWindowChromeSync();
  const {
    vaultPath,
    loading,
    pickVault,
    refresh: retryVaultLoad,
    error: vaultError,
  } = useVault();
  const { theme, setTheme } = useTheme();
  const [startupSplashVisible, setStartupSplashVisible] =
    useState(isTauriRuntime);
  const [aiStatus, setAiStatus] = useState("AI 空闲");
  const [conflictState, setConflictState] = useState<ConflictState | null>(
    null,
  );
  const { editorStats, updateEditorStats, resetEditorStats } = useEditorStats();
  // With no restored tab the workspace is Home, not an editor fallback.
  // Keeping this true also lets Home immediately read the recovered catalog.
  const [homeActive, setHomeActive] = useState(true);
  const [zen, setZen] = useState(false);
  useZenExitKeyboard({ zen, setZen });
  const [outlineOpen, setOutlineOpen] = useState(loadOutlineOpen);
  const [findReplaceOpen, setFindReplaceOpen] = useState(false);
  const [findReplaceMode, setFindReplaceMode] = useState<"find" | "replace">(
    "find",
  );
  const [classifiedOpen, setClassifiedOpen] = useState(false);
  const [persistenceBlocker, setPersistenceBlocker] =
    useState<PersistenceBlocker | null>(null);
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
  const { status: connectivityStatus } = useConnectivityStatus();
  useEffect(() => {
    if (!vaultPath) return undefined;
    return scheduleManagementCenterPreload();
  }, [vaultPath]);
  const bumpVaultIndex = useCallback(
    () => setVaultIndexEpoch((n) => n + 1),
    [],
  );
  const dirtyRef = useRef(false);
  const autoSnapshotGenerationRef = useRef(0);
  const departureInteractionLockedRef = useRef(false);
  const persistBeforeLeaveRef = useRef<PersistBeforeLeave>(async () => null);
  const getLiveMarkdownForTabsRef = useRef<() => string>(() => "");
  const discardPristineNoteRef = useRef<
    (path: string, markdown: string) => Promise<void>
  >(async () => undefined);
  const {
    tabs,
    activePath,
    markdown,
    editorContentTick,
    persistenceContentTick,
    activePathRef,
    markdownRef,
    frontmatterYamlRef,
    openNote,
    activateTab,
    closeTab,
    cancelOpenTransaction,
    discardOpenTab,
    handleNewNote,
    markDirty,
    markClean,
    promoteTab,
    updateTabTitle,
    replaceOpenTabPath,
    syncTabMarkdownCache,
    invalidateDocumentRuntimeState,
    getTabMarkdownCached,
    setMarkdown,
    activeFileLocked,
    setFileLocked,
    pendingNoteOpen,
    commitPendingNoteOpen,
  } = useTabManager({
    onStatusChange: setAiStatus,
    onVaultIndexBump: bumpVaultIndex,
    persistBeforeLeave: (path, options) =>
      persistBeforeLeaveRef.current(path, options),
    discardPristineNote: (path, content) =>
      discardPristineNoteRef.current(path, content),
    getLiveMarkdown: () => getLiveMarkdownForTabsRef.current(),
  });
  const rejectDepartureInteraction = useCallback(() => {
    if (!departureInteractionLockedRef.current) return false;
    setAiStatus("文档正在保存，暂不能切换或新建笔记");
    return true;
  }, []);
  const guardedHandleNewNote = useCallback(
    async (...args: Parameters<typeof handleNewNote>): Promise<void> => {
      if (rejectDepartureInteraction()) return;
      await handleNewNote(...args);
    },
    [handleNewNote, rejectDepartureInteraction],
  );
  const guardedActivateTab = useCallback(
    async (...args: Parameters<typeof activateTab>): Promise<void> => {
      if (rejectDepartureInteraction()) return;
      await activateTab(...args);
    },
    [activateTab, rejectDepartureInteraction],
  );
  const guardedCloseTab = useCallback(
    (path: string) => {
      if (rejectDepartureInteraction()) {
        return Promise.resolve({
          closed: false,
          discardedPristine: false,
          nextActivePath: activePathRef.current,
          remainingNoteCount: tabs.length,
        });
      }
      return closeTab(path);
    },
    [activePathRef, closeTab, rejectDepartureInteraction, tabs],
  );
  const guardedOpenNote = useCallback(
    async (...args: Parameters<typeof openNote>): Promise<void> => {
      if (rejectDepartureInteraction()) return;
      await openNote(...args);
    },
    [openNote, rejectDepartureInteraction],
  );
  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;
  const openNotePaths = useMemo(() => tabs.map((tab) => tab.path), [tabs]);
  const activeDocumentSessionId = useMemo(
    () => tabs.find((tab) => tab.path === activePath)?.documentSessionId,
    [activePath, tabs],
  );
  const updateInstallBarrierRef = useRef<() => Promise<void>>(
    async () => undefined,
  );
  useWorkspaceSessionSnapshot({ activePath, tabs, vaultPath });
  const {
    aiPanelOpen,
    assistantChrome,
    consumeEditorSelectionReference,
    editorSelectionReference,
    setAiPanelOpen,
    setWebSearch,
    setWebSearchProviderId,
    sendSelectionToAi,
    toggleWebSearch,
    refreshWebSearchProviders,
    webSearchAvailability,
    webSearchEnabled: webSearch,
    webSearchProviderId,
    webSearchProviders,
  } = useAiSidecarBridge({
    editorRef,
    isDocumentDirty: () => dirtyRef.current,
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
    handleActivateWorkspaceTab: handleActivateNoteTab,
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
    activateTab: guardedActivateTab,
    cancelPendingDocumentOpen: cancelOpenTransaction,
    classifiedVaultStatus,
    handleNewNote: guardedHandleNewNote,
    openNote: guardedOpenNote,
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
    activePath,
    closeTab: guardedCloseTab,
    currentNoteIsClassified,
    handleActivateNoteTab,
    handleNewNoteLeavingHome,
    openNoteLeavingHome,
    setHomeActive,
    showHome,
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
  const abortInlineAiForPersistenceRef = useRef<() => void>(() => undefined);
  const pathRenamePersistenceRef = useRef({
    rename: async (
      _oldPath: string,
      _newPath: string,
      _markdown: string,
      _move: () => Promise<DocumentPersistenceMoveResult>,
    ) => "",
  });
  const committedPathRenameRef = useRef<
    (oldPath: string, newPath: string) => void
  >((_oldPath, _newPath) => undefined);
  const inlineAiDomain =
    activeNoteIsClassified &&
    classifiedUnlocked &&
    !activeMediaTab &&
    activePath
      ? "classified"
      : "normal";
  const {
    noteTitle,
    editorBodyMarkdown,
    getLiveMarkdown,
    applySavedMarkdown,
    onTitleChange,
    onTitleBlur,
    onTitleCancel,
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
    renamePersistedPath: (path, newPath, markdownSnapshot, move) =>
      pathRenamePersistenceRef.current.rename(
        path,
        newPath,
        markdownSnapshot,
        move,
      ),
    updateTabTitle,
    replaceOpenTabPath,
    onPathRenamed: (oldPath, newPath) =>
      committedPathRenameRef.current(oldPath, newPath),
    onPathRenameError: () =>
      setAiStatus("标题未改名：文件名同步失败，仍保留原文件名"),
  });
  getLiveMarkdownRef.current = getLiveMarkdown;
  getLiveMarkdownForTabsRef.current = getLiveMarkdown;
  const autoVersionSettings = useAutoVersionSettings();
  const followSystemProxySettings = useFollowSystemProxy();

  const {
    notifyDirty,
    flushWhenEditorReady,
    restoreCurrentVersion,
    discardPristineNote,
    cancelPendingSave,
    awaitSaveInFlight,
    resetVersionIdle,
    handleLockToggle,
    handleSaveNote,
    versionSnapshotScheduler,
    flushAllOpenTabs,
    renamePath,
    beginPathMigration,
    completePathMigration,
    abortPathMigration,
    saveStatus,
    hasDirtyDocuments,
    isPersistenceBarrierActive,
    releasePersistenceBarrier,
  } = useAppPersistenceLifecycle({
    activeFileLocked,
    activePath,
    activePathRef,
    applySavedMarkdown,
    autoSnapshotGenerationRef,
    autoVersionEnabled: autoVersionSettings.autoVersionEnabled,
    autoVersionIdleMinutes: autoVersionSettings.autoVersionIdleMinutes,
    dirtyRef,
    persistenceContentTick,
    editorRef,
    editorReadyRef: editorReadyForPersistenceRef,
    getLiveMarkdownRef,
    getTabMarkdownCached,
    markClean,
    markdown,
    onPersistenceBarrierRelease: () => {
      departureInteractionLockedRef.current = false;
    },
    onPersistenceBarrierStart: () => {
      departureInteractionLockedRef.current = true;
      editorRef.current?.setEditable(false);
      abortInlineAiForPersistenceRef.current();
    },
    onPersistenceBlocked: setPersistenceBlocker,
    persistBeforeLeaveRef,
    setAiStatus,
    setFileLocked,
    setMarkdown,
    syncTabMarkdownCache,
    tabsRef,
  });
  discardPristineNoteRef.current = discardPristineNote;
  updateInstallBarrierRef.current = flushAllOpenTabs;
  const isEditorPersistenceBlocked =
    activeFileLocked || isPersistenceBarrierActive;
  const isEditorMutationBlocked = useCallback(
    () => activeFileLocked || departureInteractionLockedRef.current,
    [activeFileLocked],
  );
  const inlineAi = useInlineAi({
    domain: inlineAiDomain,
    isDocumentDirty: () => dirtyRef.current,
    isMutationBlocked: isEditorMutationBlocked,
    onStatus: setAiStatus,
  });
  abortInlineAiForPersistenceRef.current = inlineAi.abortAndDetach;
  const {
    loading: embeddingStatusLoading,
    reportForegroundActivity,
    setPaused: setEmbeddingPaused,
    start: startEmbeddingRebuild,
    status: embeddingStatus,
  } = useEmbeddingScheduler({ hasDirtyDocuments });

  const appUpdateController = useAppUpdateController({
    beforeInstall: () => updateInstallBarrierRef.current(),
    enabled: Boolean(vaultPath),
    hasDirtyDocuments,
    releaseAfterInstallFailure: releasePersistenceBarrier,
    onStatus: setAiStatus,
  });

  pathRenamePersistenceRef.current = {
    rename: renamePath,
  };

  const {
    handleBeforeFilePathChange,
    handleFilePathChanged,
    handleFilePathChangeFailed,
    handleBeforeFileDelete,
    handleFileDeleted,
  } = useNavigatorFileLifecycle({
    abortPathMigration,
    beginPathMigration,
    bumpVaultIndex,
    completePathMigration,
    discardOpenTab,
    persistBeforeLeaveRef,
    replaceOpenTabPath,
    tabsRef,
  });

  const {
    handleApplicationPathRenamed,
    handlePreparedFileDeleted,
    handlePreparedFilePathChanged,
    invalidateActivePreparedNote,
  } = usePreparedNoteInvalidationCallbacks({
    activePathRef,
    handleFileDeleted,
    handleFilePathChanged,
    invalidatePreparedNote,
    invalidateDocumentRuntimeState,
  });
  committedPathRenameRef.current = (oldPath, _newPath) => {
    // The application's own watcher events are deliberately suppressed during
    // an atomic move. Retire only the old-path warm/runtime caches here; the
    // active tab and its session-keyed editor have already been rebound.
    handleApplicationPathRenamed(oldPath);
    bumpVaultIndex();
  };

  useExternalDocumentLifecycle({
    activePathRef,
    awaitSaveInFlight,
    bumpVaultIndex,
    cancelPendingSave,
    discardOpenTab,
    getLiveMarkdownRef,
    invalidatePreparedNote,
    promoteTab,
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

  const {
    finalizeCurrentWithPromotion,
    handleLockToggleWithPromotion,
    handleSaveNoteWithPromotion,
    restoreCurrentVersionWithPromotion,
  } = useNoteLifecycleIntentActions({
    activePathRef,
    bumpVaultIndex,
    flushWhenEditorReady,
    handleLockToggle,
    handleSaveNote,
    promoteTab,
    restoreCurrentVersion,
  });

  const applyMarkdownToEditor = useCallback(
    (content: string) => {
      markdownRef.current = content;
      loadBodyIntoEditor(content);
      setMarkdown(content);
    },
    [loadBodyIntoEditor, markdownRef, setMarkdown],
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
    flushWhenEditorReady,
    invalidatePreparedNote,
    isMutationBlocked: isEditorMutationBlocked,
    markClean,
    openNoteLeavingHome,
    setConflictState,
    syncTabMarkdownCache,
  });

  const openFindReplace = useCallback((mode: "find" | "replace") => {
    setFindReplaceMode(mode);
    setFindReplaceOpen(true);
  }, []);

  const handleDirty = useCallback(
    (sourcePath: string) => {
      if (sourcePath !== activePathRef.current) return;
      if (isEditorPersistenceBlocked) return;
      const captured = notifyDirty(sourcePath);
      if (!captured) return;
      if (!dirtyRef.current) {
        dirtyRef.current = true;
        markDirty();
        invalidateActivePreparedNote();
      }
      void reportForegroundActivity();
      resetVersionIdle();
    },
    [
      activePathRef,
      isEditorPersistenceBlocked,
      invalidateActivePreparedNote,
      markDirty,
      notifyDirty,
      reportForegroundActivity,
      resetVersionIdle,
    ],
  );

  const handleTitleChange = useCallback(
    (raw: string) => {
      if (isEditorPersistenceBlocked) return;
      onTitleChange(raw);
    },
    [isEditorPersistenceBlocked, onTitleChange],
  );

  const { rescanVaultManually } = useAutoVaultIndex(vaultPath, loading, {
    onStatus: setAiStatus,
    onIndexed: bumpVaultIndex,
  });

  useEffect(() => {
    if (!activePath) return;
    void reportForegroundActivity();
  }, [activePath, reportForegroundActivity]);

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

  const handleTitleBlur = useCallback(
    (committedTitle: string) => {
      onTitleBlur(committedTitle);
      void reportForegroundActivity();
    },
    [onTitleBlur, reportForegroundActivity],
  );

  const editorTitleSlot = useMemo(
    () => (
      <DocumentTitleField
        value={noteTitle}
        resetKey={activeDocumentSessionId ?? activePath ?? ""}
        onChange={handleTitleChange}
        onBlur={handleTitleBlur}
        onCancel={onTitleCancel}
        editorRef={editorRef}
        readOnly={isEditorPersistenceBlocked}
      />
    ),
    [
      activeDocumentSessionId,
      activePath,
      noteTitle,
      handleTitleChange,
      handleTitleBlur,
      onTitleCancel,
      editorRef,
      isEditorPersistenceBlocked,
    ],
  );

  const { handleInsertToEditor, handleRedo, handleUndo, runEditorActionById } =
    useAppEditorActions({
      activeNoteIsClassified,
      activePathRef,
      editorRef,
      getLiveMarkdown,
      inlineAi,
      isMutationBlocked: isEditorMutationBlocked,
      scheduleUndoRedoStateRefresh,
      sendSelectionToAi,
      setAiStatus,
    });

  const editorContextMenu = useEditorContextMenu(
    editorInstance,
    Boolean(activePath),
    () => setAiStatus("选区 AI：请使用右键菜单"),
    isEditorPersistenceBlocked,
    {
      aiDomain: activeNoteIsClassified ? "classified" : "normal",
      classifiedUnlocked,
    },
  );

  const { appShortcutItems, handleAppShortcut } = useAppShortcuts({
    activePath,
    activePathRef,
    closeTab: guardedCloseTab,
    handleNewNote: guardedHandleNewNote,
    handleSaveNote: handleSaveNoteWithPromotion,
    handleVaultRescan: rescanVaultManually,
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
  const {
    aiDomain,
    assistantRuntimeDocumentCandidates,
    classifiedPath,
    handleAssistantInsertToEditor,
  } = useWorkspaceAssistantRouting({
    activeMediaTab,
    activeNoteIsClassified,
    activePath,
    classifiedUnlocked,
    handleInsertToEditor,
    setAiStatus,
    tabs,
  });
  if (!isTauriRuntime()) {
    return <BrowserRuntimeNotice />;
  }

  if (startupSplashVisible || !vaultPath) {
    return (
      <AppPreVaultGate
        loading={loading}
        startupSplashVisible={startupSplashVisible}
        vaultError={vaultError}
        vaultPath={vaultPath}
        theme={theme}
        onExited={() => setStartupSplashVisible(false)}
        onPickVault={() => void pickVault()}
        onRetryVaultLoad={() => void retryVaultLoad()}
        onThemeChange={(nextTheme) => void setTheme(nextTheme)}
      />
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
            activeMediaTab={activeMediaTab}
            activeNoteIsClassified={activeNoteIsClassified}
            activeDocumentSessionId={activeDocumentSessionId}
            activePath={activePath}
            committedSourceMarkdown={markdown}
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
            handleLockToggle={handleLockToggleWithPromotion}
            handleNewNoteLeavingHome={handleNewWorkspaceNote}
            homeActive={homeActive}
            inlineAi={inlineAi}
            isMutationBlocked={isEditorMutationBlocked}
            persistenceBarrierActive={isPersistenceBarrierActive}
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
            onBeforeFileDelete={handleBeforeFileDelete}
            outlineOpen={outlineOpen}
            pendingOpen={pendingOpen}
            pendingNoteOpen={pendingNoteOpen}
            onPendingOpenSettled={clearPendingOpenFromWorkspace}
            commitPendingNoteOpen={commitPendingNoteOpen}
            runEditorActionById={runEditorActionById}
            setFindReplaceMode={setFindReplaceMode}
            setFindReplaceOpen={setFindReplaceOpen}
            updateEditorStats={updateEditorStats}
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
            classifiedPath={classifiedPath}
            consumeEditorSelectionReference={consumeEditorSelectionReference}
            editorSelectionReference={editorSelectionReference}
            editorInteractionLocked={isEditorPersistenceBlocked}
            runtimeDocumentCandidates={assistantRuntimeDocumentCandidates}
            handleInsertToEditor={handleAssistantInsertToEditor}
            webSearch={webSearch}
            webSearchProviderName={
              webSearchAvailability.effectiveProvider?.name ?? null
            }
            onOpenWebVerificationSettings={() =>
              overlays.openManagementCenter("ai", "web-search")
            }
          />
        }
        statusBar={
          <AppStatusBarSlot
            activePath={activeMediaTab ? null : activePath}
            activeDocumentTitle={
              activeMediaTab ? activeMediaTab.title : activeDocumentTitle
            }
            persistenceStatus={saveStatus}
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
            canUndo={canUndo && !isEditorPersistenceBlocked}
            canRedo={canRedo && !isEditorPersistenceBlocked}
            webSearch={webSearch}
            webSearchAvailability={webSearchAvailability}
            onWebSearchChange={setWebSearch}
            theme={theme}
            onThemeChange={(nextTheme) => void setTheme(nextTheme)}
            connectivity={connectivityStatus}
            appUpdate={appUpdateController.statusBar}
            onOpenConnectivitySettings={() =>
              overlays.openManagementCenter("ai")
            }
            onOpenManagementCenter={() =>
              overlays.openManagementCenter("overview")
            }
            onOpenUpdateCenter={() => overlays.openManagementCenter("overview")}
            onOpenGraph={() => overlays.openOverlay("graph")}
            onOpenKnowledgeRelations={() =>
              overlays.openOverlay("knowledgeRelations")
            }
          />
        }
        overlays={
          <AppOverlays
            activePath={activePath}
            restoreVersion={restoreCurrentVersionWithPromotion}
            bumpVaultIndex={bumpVaultIndex}
            classifiedIdleDeadline={classifiedIdleDeadline}
            classifiedOpen={classifiedOpen}
            classifiedVaultStatus={classifiedVaultStatus}
            classifiedWaiting={classifiedWaiting}
            connectivityStatus={connectivityStatus}
            conflictState={conflictState}
            embeddingStatus={embeddingStatus}
            embeddingStatusLoading={embeddingStatusLoading}
            getCurrentContent={() => getLiveMarkdownRef.current()}
            onBeforeFinalizeCurrent={finalizeCurrentWithPromotion}
            handleConflictAcceptExternal={handleConflictAcceptExternal}
            handleConflictKeepLocal={handleConflictKeepLocal}
            handleConflictManualEdit={handleConflictManualEdit}
            markdown={markdown}
            onBeforeFilePathChange={handleBeforeFilePathChange}
            onFilePathChanged={handlePreparedFilePathChanged}
            onFilePathChangeFailed={handleFilePathChangeFailed}
            onBeforeFileDelete={handleBeforeFileDelete}
            onFileDeleted={handlePreparedFileDeleted}
            onClassifiedUnlocked={onClassifiedUnlocked}
            onIndexDegraded={() => setAiStatus("已保存但索引待修复")}
            onOpenDocumentRecovery={() =>
              overlays.openOverlay("documentRecovery")
            }
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
            webSearchAvailability={webSearchAvailability}
            webSearchProviderId={webSearchProviderId}
            webSearchProviders={webSearchProviders}
            setWebSearchProviderId={setWebSearchProviderId}
            refreshWebSearchProviders={refreshWebSearchProviders}
            openKnowledgeRelations={() =>
              overlays.openOverlay("knowledgeRelations")
            }
            onSetEmbeddingPaused={setEmbeddingPaused}
            onStartEmbeddingRebuild={startEmbeddingRebuild}
            openVersion={() => overlays.openOverlay("version")}
            rescanVault={rescanVaultManually}
            autoVersionSettings={autoVersionSettings}
            followSystemProxySettings={followSystemProxySettings}
            tabs={tabs}
            touchClassifiedActivity={touchClassifiedActivity}
            versionSnapshotScheduler={versionSnapshotScheduler}
            webSearch={webSearch}
            appUpdateController={appUpdateController}
          />
        }
      />
      <ConfirmDialog
        open={persistenceBlocker !== null}
        title="保存失败"
        message="存在尚未确认落盘的 Markdown，不能关闭应用。"
        description="请重试保存，或返回编辑后检查内容。"
        confirmLabel="重试"
        cancelLabel="返回编辑"
        variant="destructive"
        onConfirm={() => {
          const blocker = persistenceBlocker;
          if (!blocker) return;
          void blocker.retry();
        }}
        onCancel={() => setPersistenceBlocker(null)}
      />
    </DesktopFrame>
  );
}
App.displayName = "App";

export default App;
