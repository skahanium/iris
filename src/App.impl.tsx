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
import { preloadAssistantPanel } from "@/lib/preload-assistant-panel";
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
import { useCjkPunctuationSettings } from "@/hooks/useCjkPunctuationSettings";
import { useFeedWorkspaceMode } from "@/hooks/useFeedWorkspaceMode";
import { useFeedSaveAsNote } from "@/hooks/useFeedSaveAsNote";
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
import { WorkspaceNavigator } from "@/components/file/WorkspaceNavigator";
import { FeedWorkspace } from "@/components/feed/FeedWorkspace";
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
  return localStorage.getItem("iris-outline-open") !== "false";
}
function saveOutlineOpen(open: boolean): void {
  localStorage.setItem("iris-outline-open", open ? "true" : "false");
}
type FindReplaceMode = "find" | "replace";
type DiscardNote = (path: string, markdown: string) => Promise<void>;
type SuppressShellUi = (path: string) => void;

type ConflictStateValue = ConflictState | null;

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

function scheduleAssistantPanelPreload(): () => void {
  const handle = window.requestAnimationFrame(() => {
    preloadAssistantPanel();
  });
  return () => window.cancelAnimationFrame(handle);
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
  const [splashVisible, setStartupSplashVisible] = useState(isTauriRuntime);
  const [aiStatus, setAiStatus] = useState("AI 空闲");
  const [conflictState, setConflictState] = useState<ConflictStateValue>(null);
  const {
    editorStats,
    updateEditorStats,
    resetEditorStats,
    resetSessionCharDelta,
    applySessionCharDelta,
    setActiveEditorSession,
    clearSessionCharDelta,
  } = useEditorStats();
  const [workspaceEmpty, setWorkspaceEmpty] = useState(true);
  const [zen, setZen] = useState(false);
  useZenExitKeyboard({ zen, setZen });
  const { workspaceMode, handleWorkspaceModeChange, returnToDocuments } =
    useFeedWorkspaceMode(zen, setZen, setAiStatus);
  const [outlineOpen, setOutlineOpen] = useState(loadOutlineOpen);
  const [findReplaceOpen, setFindReplaceOpen] = useState(false);
  const [findReplaceMode, setFindReplaceMode] =
    useState<FindReplaceMode>("find");
  const [classifiedOpen, setClassifiedOpen] = useState(false);
  const [assistantVisible, setAssistantVisible] = useState(true);
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
  useEffect(() => scheduleAssistantPanelPreload(), []);
  useEffect(() => {
    return vaultPath ? scheduleManagementCenterPreload() : undefined;
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
  const discardPristineNoteRef = useRef<DiscardNote>(async () => undefined);
  const clearSuppressShellUiRef = useRef<() => void>(() => undefined);
  const beginSuppressShellUiRef = useRef<SuppressShellUi>(() => undefined);
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
    isPathClosing,
  } = useTabManager({
    onStatusChange: setAiStatus,
    onVaultIndexBump: bumpVaultIndex,
    persistBeforeLeave: (path, options) =>
      persistBeforeLeaveRef.current(path, options),
    discardPristineNote: (path, content) =>
      discardPristineNoteRef.current(path, content),
    getLiveMarkdown: () => getLiveMarkdownForTabsRef.current(),
    beginSuppressShellUi: (path) => beginSuppressShellUiRef.current(path),
    clearSuppressShellUi: () => clearSuppressShellUiRef.current(),
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
      const sessionId = tabsRef.current.find(
        (tab) => tab.path === path,
      )?.documentSessionId;
      return closeTab(path).then((result) => {
        if (result.closed && sessionId) {
          clearSessionCharDelta(sessionId);
        }
        return result;
      });
    },
    [
      activePathRef,
      clearSessionCharDelta,
      closeTab,
      rejectDepartureInteraction,
      tabs.length,
    ],
  );
  const guardedOpenNote = useCallback(
    async (...args: Parameters<typeof openNote>): Promise<void> => {
      if (rejectDepartureInteraction()) return;
      await openNote(...args);
    },
    [openNote, rejectDepartureInteraction],
  );
  const handleFeedSaveAsNote = useFeedSaveAsNote(
    guardedOpenNote,
    returnToDocuments,
  );
  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;
  const openNotePaths = useMemo(() => tabs.map((tab) => tab.path), [tabs]);
  const activeDocumentSessionId = useMemo(
    () => tabs.find((t) => t.path === activePath)?.documentSessionId,
    [activePath, tabs],
  );
  const updateInstallBarrierRef = useRef<() => Promise<void>>(
    async () => undefined,
  );
  useWorkspaceSessionSnapshot({ activePath, tabs, vaultPath });
  const openClassifiedPaths = useMemo(
    () => tabs.filter((t) => isClassifiedVaultPath(t.path)).map((t) => t.path),
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
    if (classifiedOpen) void refreshClassifiedStatus();
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
    enterWorkspaceEmpty,
    warmPreparedNotes,
  } = usePreparedWorkspaceTransitions<
    NonNullable<Parameters<typeof openNote>[2]>
  >({
    activateTab: guardedActivateTab,
    cancelPendingDocumentOpen: cancelOpenTransaction,
    classifiedVaultStatus,
    handleNewNote: guardedHandleNewNote,
    openNote: guardedOpenNote,
    setWorkspaceEmpty,
    tabs,
    vaultPath,
    workspaceEmpty,
  });
  const currentNoteIsClassified = isClassifiedVaultPath(activePath ?? "");
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
    setWorkspaceEmpty,
    enterWorkspaceEmpty,
    tabs,
  });
  const documentForegroundActive =
    !workspaceEmpty && Boolean(activePath) && !activeMediaTab;

  useEffect(() => {
    if (workspaceEmpty || activeMediaTab || !activePath) return;
    if (tabs.some((tab) => tab.path === activePath)) {
      setWorkspaceEmpty(false);
    }
  }, [workspaceEmpty, activePath, activeMediaTab, setWorkspaceEmpty, tabs]);

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
    setTitleFocused,
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
  const cjkPunctuationSettings = useCjkPunctuationSettings();

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
    clearSuppressShellUi,
    beginSuppressShellUi,
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
  clearSuppressShellUiRef.current = clearSuppressShellUi;
  beginSuppressShellUiRef.current = beginSuppressShellUi;
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
    handleBeforeFileLock,
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
    // 原子移动期间应用自身 watcher 事件被抑制；只退役旧路径缓存（活动 Tab 已重绑）。
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
      if (isPathClosing(sourcePath)) return;
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
      isPathClosing,
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
    if (activePath) void reportForegroundActivity();
  }, [activePath, reportForegroundActivity]);

  const {
    canRedo,
    canUndo,
    editorInstance,
    handleEditorReady: handleUndoRedoEditorReady,
    scheduleUndoRedoStateRefresh,
  } = useEditorUndoRedoState({ activePath, editorRef });

  const activeDocumentDirty = Boolean(
    tabs.find((tab) => tab.path === activePath)?.dirty,
  );
  const {
    aiPanelOpen,
    assistantChrome,
    consumeEditorSelectionReference,
    dismissEditorSelectionReference,
    editorSelectionCandidate,
    setAiPanelOpen,
    setAssistantChrome,
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
    editor: editorInstance,
    documentKey: activeDocumentSessionId ?? activePath,
    documentDirty: activeDocumentDirty,
    assistantVisible,
    selectionEnabled:
      Boolean(activePath) && !activeMediaTab && !activeNoteIsClassified,
    isDocumentDirty: () => dirtyRef.current,
    setAiStatus,
  });

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
    if (!documentForegroundActive) {
      resetEditorStats();
    }
  }, [documentForegroundActive, resetEditorStats]);

  useEffect(() => {
    setActiveEditorSession(activeDocumentSessionId ?? null);
  }, [activeDocumentSessionId, setActiveEditorSession]);

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
        onFocusChange={setTitleFocused}
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
      setTitleFocused,
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
    documentForegroundActive &&
    activePath &&
    displayTitleForChrome(activePath, noteTitle);
  const statusBarShowsMediaChrome = Boolean(activeMediaTab && !workspaceEmpty);
  const statusBarShowsNoteChrome = documentForegroundActive;
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

  if (splashVisible || !vaultPath) {
    return (
      <AppPreVaultGate
        loading={loading}
        startupSplashVisible={splashVisible}
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

  const navigatorBridge = {
    activePath: activeWorkspacePath,
    onOpenDocument: guardedOpenNote,
    onPrepareNote: prepareVisibleNote,
    fileLifecycle: {
      handleBeforeFilePathChange,
      handleFilePathChanged,
      handleFilePathChangeFailed,
      handleBeforeFileDelete,
      handleFileDeleted,
      handleBeforeFileLock,
    },
  };

  return (
    <DesktopFrame>
      <AppShell
        aiPanelOpen={aiPanelOpen}
        onAiPanelOpenChange={setAiPanelOpen}
        onAssistantVisibilityChange={setAssistantVisible}
        zen={zen}
        workspaceMode={workspaceMode}
        onWorkspaceModeChange={handleWorkspaceModeChange}
        feedWorkspace={<FeedWorkspace onSaveAsNote={handleFeedSaveAsNote} />}
        navigator={<WorkspaceNavigator {...navigatorBridge} />}
        tabBar={
          <TabBar
            tabs={workspaceTabs}
            activePath={activeWorkspacePath}
            onSelect={(path) => {
              if (workspaceMode === "feeds") returnToDocuments();
              void handleActivateWorkspaceTab(path);
            }}
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
            workspaceEmpty={workspaceEmpty}
            inlineAi={inlineAi}
            isMutationBlocked={isEditorMutationBlocked}
            persistenceBarrierActive={isPersistenceBarrierActive}
            onOutlineOpenChange={(open) => {
              setOutlineOpen(open);
              saveOutlineOpen(open);
            }}
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
            resetSessionCharDelta={resetSessionCharDelta}
            applySessionCharDelta={applySessionCharDelta}
            vaultIndexEpoch={vaultIndexEpoch}
            vaultPath={vaultPath}
            warmPreparedNotes={warmPreparedNotes}
            openNotePaths={openNotePaths}
            onOpenSearch={() => overlays.setSearchOpen(true)}
            zen={zen}
            cjkPunctuationEnabled={cjkPunctuationSettings.cjkPunctuationEnabled}
          />
        }
        aiPanel={
          <AppAiPanelSlot
            aiDomain={aiDomain}
            classifiedPath={classifiedPath}
            consumeEditorSelectionReference={consumeEditorSelectionReference}
            dismissEditorSelectionReference={dismissEditorSelectionReference}
            editorSelectionCandidate={editorSelectionCandidate}
            editorInteractionLocked={isEditorPersistenceBlocked}
            runtimeDocumentCandidates={assistantRuntimeDocumentCandidates}
            handleInsertToEditor={handleAssistantInsertToEditor}
            webSearch={webSearch}
            onOpenWebVerificationSettings={() =>
              overlays.openManagementCenter("ai", "web-search")
            }
            onChromeChange={setAssistantChrome}
          />
        }
        statusBar={
          <AppStatusBarSlot
            activePath={statusBarShowsNoteChrome ? activePath : null}
            activeDocumentTitle={
              statusBarShowsMediaChrome
                ? activeMediaTab!.title
                : activeDocumentTitle || null
            }
            persistenceStatus={
              statusBarShowsNoteChrome ? saveStatus : undefined
            }
            characterCount={
              statusBarShowsNoteChrome ? editorStats.characterCount : 0
            }
            readingMinutes={
              statusBarShowsNoteChrome ? editorStats.readingMinutes : 0
            }
            sessionCharsAdded={
              statusBarShowsNoteChrome ? editorStats.sessionCharsAdded : 0
            }
            sessionCharsRemoved={
              statusBarShowsNoteChrome ? editorStats.sessionCharsRemoved : 0
            }
            aiStatus={aiStatus}
            assistantChrome={assistantChrome}
            editorZoom={statusBarShowsNoteChrome ? editorZoom : undefined}
            onEditorZoomIn={statusBarShowsNoteChrome ? zoomIn : undefined}
            onEditorZoomOut={statusBarShowsNoteChrome ? zoomOut : undefined}
            onEditorZoomReset={statusBarShowsNoteChrome ? resetZoom : undefined}
            onEditorZoomChange={statusBarShowsNoteChrome ? setZoom : undefined}
            onUndo={handleUndo}
            onRedo={handleRedo}
            canUndo={
              statusBarShowsNoteChrome && canUndo && !isEditorPersistenceBlocked
            }
            canRedo={
              statusBarShowsNoteChrome && canRedo && !isEditorPersistenceBlocked
            }
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
            onBeforeFileLock={handleBeforeFileLock}
            onFileLockChanged={setFileLocked}
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
            cjkPunctuationSettings={cjkPunctuationSettings}
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
