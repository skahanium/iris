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
import { ClassifiedPanel } from "@/components/classified/ClassifiedPanel";
import { DocumentTitleField } from "@/components/editor/DocumentTitleField";
import { EditorFindReplaceBar } from "@/components/editor/EditorFindReplaceBar";
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
import { useClassifiedVaultSession } from "@/hooks/useClassifiedVaultSession";
import { useEditorContextMenu } from "@/hooks/useEditorContextMenu";
import { useAutoVaultIndex } from "@/hooks/useAutoVaultIndex";
import { useEditorSave } from "@/hooks/useEditorSave";
import { useOpenNote } from "@/hooks/useOpenNote";
import { useTauriCloseSave } from "@/hooks/useTauriCloseSave";
import { useEditorZoom } from "@/hooks/useEditorZoom";
import { useEditorStats } from "@/hooks/useEditorStats";
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
import { isClassifiedVaultPath } from "@/lib/classified-path";
import {
  fileRead,
  fileSetLock,
  fileWrite,
  listenClassifiedFileTaken,
  listenFileChanged,
  listenVersionSaveComplete,
  settingsGet,
  settingsSet,
  versionSaveIdle,
  versionSaveManual,
} from "@/lib/ipc";
import { setCachedEditorHtml } from "@/lib/editor-html-cache";
import { waitForEditorRef } from "@/lib/wait-for-editor";
import { isNoteSubstantivelyEmpty } from "@/lib/note-substance";
import { formatVersionSaveStatus } from "@/lib/version-save-status";
import { isTauriRuntime } from "@/lib/tauri-runtime";
import type { AutoSnapshotLeaveReason } from "@/lib/version-auto-snapshot-policy";
import { createLeaveSnapshotEnqueuer } from "@/lib/version-leave-snapshot";
import {
  persistActiveTabBeforeLeave,
  persistInactiveDirtyTabBeforeLeave,
} from "@/lib/persist-before-leave";
import {
  createVersionSnapshotScheduler,
  type LastSavedSnapshot,
} from "@/lib/version-snapshot-scheduler";
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
const AiSystemCenterPanel = lazy(() =>
  import("@/components/settings/AiSystemCenterPanel").then((m) => ({
    default: m.AiSystemCenterPanel,
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

interface PersistBeforeLeaveOptions {
  reason?: AutoSnapshotLeaveReason;
}

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
  const { editorStats, updateEditorStats, resetEditorStats } = useEditorStats();
  const [keyboardLeaderPending, setKeyboardLeaderPending] = useState(false);
  const [homeActive, setHomeActive] = useState(false);
  const [zen, setZen] = useState(false);
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

  const persistBeforeLeaveRef = useRef<
    (
      path: string,
      options?: PersistBeforeLeaveOptions,
    ) => Promise<string | null>
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
    activeFileLocked,
    setFileLocked,
  } = useTabManager({
    onStatusChange: setAiStatus,
    onVaultIndexBump: bumpVaultIndex,
    persistBeforeLeave: (path) => persistBeforeLeaveRef.current(path),
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
    updateTabTitle,
    replaceOpenTabPath,
  });

  getLiveMarkdownRef.current = getLiveMarkdown;

  const { notifyDirty, flushSave, flushSaveForPath, getLastSavedSnapshot } =
    useEditorSave(
      activePath,
      () => getLiveMarkdownRef.current(),
      (md) => {
        applySavedMarkdown(md);
        dirtyRef.current = false;
        const path = activePathRef.current;
        if (path) {
          setMarkdown(md);
          syncTabMarkdownCache(path, md);
          markClean(path, resolveNoteDisplayTitle({ path, title: noteTitle }));
          if (noteTitle.trim() === "") {
            schedulePathSync(path, noteTitle);
          }
        }
      },
    );

  const versionSnapshotScheduler = useMemo(
    () =>
      createVersionSnapshotScheduler({
        versionSaveIdle,
        onError: (err) => {
          const msg = err instanceof Error ? err.message : String(err);
          setAiStatus(`自动版本备份提交失败：${msg}`);
        },
      }),
    [],
  );

  const enqueueIdleSnapshot = useCallback(
    (snapshot: LastSavedSnapshot) => {
      const result = versionSnapshotScheduler.enqueueIdle(snapshot);
      if (result.accepted) {
        void result.done;
      }
    },
    [versionSnapshotScheduler],
  );

  const enqueueLeaveSnapshot = useMemo(
    () =>
      createLeaveSnapshotEnqueuer({
        enqueueIdleSnapshot,
        nextDirtyGeneration: () => {
          autoSnapshotGenerationRef.current += 1;
          return autoSnapshotGenerationRef.current;
        },
      }),
    [enqueueIdleSnapshot],
  );

  persistBeforeLeaveRef.current = async (
    path: string,
    options: PersistBeforeLeaveOptions = {},
  ) => {
    const reason = options.reason ?? "tab_leave";
    const tab = tabsRef.current.find((t) => t.path === path);
    if (path === activePathRef.current) {
      await waitForEditorRef(editorRef);
      const md = await persistActiveTabBeforeLeave({
        path,
        reason,
        getMarkdown: () => getLiveMarkdownRef.current(),
        flushSaveForPath,
        getLastSavedSnapshot,
        enqueueIdleSnapshot,
      });
      if (md) {
        dirtyRef.current = false;
        setMarkdown(md);
        syncTabMarkdownCache(path, md);
        const ed = editorRef.current;
        if (ed && !ed.isDestroyed) {
          setCachedEditorHtml(path, ed.getHTML());
        }
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
    await persistInactiveDirtyTabBeforeLeave({
      path,
      reason,
      cachedMarkdown: cached,
      writeFile: async (targetPath, content) => {
        await fileWrite(targetPath, content);
      },
      enqueueLeaveSnapshot,
    });
    markClean(path, tab.title);
    return cached;
  };

  const { onActivity: resetVersionIdle, clearTimer: clearVersionIdleTimer } =
    useVersionIdle(activePath, getLastSavedSnapshot, enqueueIdleSnapshot);

  const flushAllOpenTabs = useCallback(async () => {
    const paths = tabsRef.current.map((tab) => tab.path);
    versionSnapshotScheduler.setAppClosing(true);
    clearVersionIdleTimer();
    try {
      for (const path of paths) {
        await persistBeforeLeaveRef.current(path, { reason: "app_close" });
      }
    } finally {
      versionSnapshotScheduler.setAppClosing(false);
    }
  }, [clearVersionIdleTimer, versionSnapshotScheduler]);

  useTauriCloseSave({
    flushBeforeClose: flushAllOpenTabs,
    onError: (message) => {
      setAiStatus(`关闭前保存失败：${message}`);
    },
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
    onLocked: () => setAiStatus("涉密保险库已锁定"),
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

  const handleSaveNote = useCallback(async () => {
    if (activeFileLocked) {
      setAiStatus("笔记已锁定，无法保存");
      return;
    }
    await flushSave();
  }, [activeFileLocked, flushSave]);

  const handleSaveVersion = useCallback(async () => {
    const path = activePathRef.current;
    if (!path) return;
    const md = await flushSave();
    if (!md) return;
    setAiStatus("正在后台创建版本快照…");
    versionSnapshotScheduler.markHighPriorityStart(path);
    void versionSaveManual(path, md)
      .catch((err: unknown) => {
        const msg = err instanceof Error ? err.message : String(err);
        setAiStatus(`版本快照提交失败：${msg}`);
      })
      .finally(() => {
        versionSnapshotScheduler.markHighPriorityEnd(path);
      });
  }, [flushSave, activePathRef, versionSnapshotScheduler]);

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

  const getWritingContext = useCallback((): WritingEditorContext | null => {
    const ed = editorRef.current;
    const path = activePathRef.current;
    if (!ed || !path) return null;
    if (isClassifiedVaultPath(path)) return null;
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
    const path = activePathRef.current;
    if (!ed || !path) return null;
    if (isClassifiedVaultPath(path)) return null;
    const { from, to } = ed.state.selection;
    if (from !== to) {
      return ed.state.doc.textBetween(from, to, "\n");
    }
    const $from = ed.state.doc.resolve(from);
    const start = $from.start($from.depth);
    const end = $from.end($from.depth);
    return ed.state.doc.textBetween(start, end, "\n");
  }, [activePathRef]);

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

  const runInlineAi = useCallback(
    (action: string) => {
      if (activeNoteIsClassified) {
        setAiStatus("涉密笔记不能发送到 AI");
        return;
      }
      const ed = editorRef.current;
      if (!ed) return;
      void inlineAi.run(ed, action);
    },
    [activeNoteIsClassified, inlineAi],
  );

  const handleSlashCommand = useCallback(
    (command: string) => {
      if (activeNoteIsClassified) {
        setAiStatus("涉密笔记不能发送到 AI");
        return;
      }
      if (!editorRef.current) return;
      void inlineAi.runSlash(editorRef.current, command, getLiveMarkdown());
    },
    [activeNoteIsClassified, getLiveMarkdown, inlineAi],
  );

  const sendSelectionToAi = useCallback(
    (options?: { prefill?: string }) => {
      const ed = editorRef.current;
      const path = activePathRef.current;
      if (!ed || !path) return;
      if (isClassifiedVaultPath(path)) {
        setAiStatus("涉密笔记不能发送到 AI");
        return;
      }
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

  const handleInsertToEditor = useCallback(
    (content: string) => {
      const ed = editorRef.current;
      const path = activePathRef.current;
      if (!ed || !path) return;
      if (isClassifiedVaultPath(path)) {
        setAiStatus("涉密笔记不能接收 AI 插入");
        return;
      }
      insertAssistantMarkdownAtCursor(ed, content);
    },
    [activePathRef],
  );

  const handleUndo = useCallback(() => {
    const ed = editorRef.current;
    if (!ed) return;
    ed.commands.undo();
    scheduleUndoRedoStateRefresh(ed);
  }, [scheduleUndoRedoStateRefresh]);

  const handleRedo = useCallback(() => {
    const ed = editorRef.current;
    if (!ed) return;
    ed.commands.redo();
    scheduleUndoRedoStateRefresh(ed);
  }, [scheduleUndoRedoStateRefresh]);

  const editorContextMenu = useEditorContextMenu(
    editorInstance,
    Boolean(activePath),
    () => setAiStatus("选区 AI：请使用右键菜单"),
    activeFileLocked,
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
        case "openClassifiedPanel":
          setClassifiedOpen(true);
          break;
        case "openFindReplace":
          openFindReplace(action.mode);
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
      openFindReplace,
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
                  key={activePath}
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
                  onOpenChange={(open) => {
                    setOutlineOpen(open);
                    saveOutlineOpen(open);
                  }}
                />
              </ErrorBoundary>
            ) : (
              <WelcomeEmpty
                vaultKey={`${vaultPath ?? ""}:${vaultIndexEpoch}`}
                onOpen={openNoteLeavingHome}
                onNew={handleNewNoteLeavingHome}
                onQuickOpen={() => overlays.openOverlay("quickOpen")}
                onSearch={() => overlays.openOverlay("search")}
                onAiSystemCenter={() => overlays.openOverlay("aiSystemCenter")}
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
        }
        aiPanel={
          <ErrorBoundary scope="AI面板">
            <UnifiedAssistantPanel
              notePath={assistantNotePath}
              noteDisplayTitle={assistantDocumentTitle}
              getNoteContent={getLiveMarkdown}
              webSearch={webSearch}
              getWritingContext={getWritingContext}
              getParagraphText={getParagraphText}
              selectionQuote={activeNoteIsClassified ? null : selectionQuote}
              prefillMessage={assistantPrefill}
              onChromeChange={setAssistantChrome}
              onVaultRefresh={bumpVaultIndex}
              onInsertToEditor={handleInsertToEditor}
              onPatchApplied={(newContent: string) => {
                if (activeNoteIsClassified) {
                  setAiStatus("涉密笔记不能接收 AI 改写");
                  return;
                }
                applyMarkdownToEditor(newContent);
                markdownRef.current = newContent;
                dirtyRef.current = false;
                const path = activePathRef.current;
                if (path) {
                  syncTabMarkdownCache(path, newContent);
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
              onSelect={openNoteLeavingHome}
            />
            <VaultNavigator
              open={overlays.fileSheet}
              onClose={() => overlays.closeOverlay("fileSheet")}
              onOpen={openNoteLeavingHome}
            />
            <RecycleBinSheet
              open={overlays.recycleBinOpen}
              onClose={() => overlays.closeOverlay("recycleBin")}
              onRestored={openNoteLeavingHome}
              onIndexChange={bumpVaultIndex}
            />
            <SearchPanel
              open={overlays.searchOpen}
              onClose={() => overlays.closeOverlay("search")}
              onOpen={openNoteLeavingHome}
            />
            <Suspense fallback={<LazyFallback />}>
              <SettingsPanel
                open={overlays.settingsOpen}
                onClose={() => overlays.closeOverlay("settings")}
                theme={theme}
                onThemeChange={(t) => void setTheme(t)}
                webSearch={webSearch}
                onWebSearchChange={setWebSearch}
              />
            </Suspense>
            <Suspense fallback={<LazyFallback />}>
              <AiSystemCenterPanel
                open={overlays.aiSystemCenterOpen}
                onClose={() => overlays.closeOverlay("aiSystemCenter")}
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
              onOpen={openNoteLeavingHome}
            />
            <TagView
              open={overlays.tagViewOpen}
              onClose={() => overlays.closeOverlay("tags")}
              onOpen={openNoteLeavingHome}
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
                onHighPriorityStart={(path) =>
                  versionSnapshotScheduler.markHighPriorityStart(path)
                }
                onHighPriorityEnd={(path) =>
                  versionSnapshotScheduler.markHighPriorityEnd(path)
                }
              />
            </Suspense>
            <ErrorBoundary scope="知识图谱">
              <Suspense fallback={<LazyFallback />}>
                <GraphView
                  open={overlays.graphOpen}
                  onClose={() => overlays.closeOverlay("graph")}
                  onOpenNote={openNoteLeavingHome}
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
            <ClassifiedPanel
              open={classifiedOpen}
              onClose={() => setClassifiedOpen(false)}
              status={classifiedVaultStatus}
              waiting={classifiedWaiting}
              idleDeadline={classifiedIdleDeadline}
              openClassifiedPaths={openClassifiedPaths}
              onOpenFile={(path) =>
                openNoteLeavingHome(path, undefined, { allowClassified: true })
              }
              onUnlockSuccess={() => void onClassifiedUnlocked()}
              onRequestLock={() => requestClassifiedLock()}
              onActivity={touchClassifiedActivity}
              onRefreshStatus={refreshClassifiedStatus}
              onEnterWaiting={() => setClassifiedWaiting(true)}
            />
          </>
        }
      />
    </DesktopFrame>
  );
}

export default App;
