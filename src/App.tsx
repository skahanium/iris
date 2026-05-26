import type { Editor } from "@tiptap/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { AiPanel, type ContextQuote } from "@/components/ai/AiPanel";
import { EditorOutline } from "@/components/editor/EditorOutline";
import { TipTapEditor } from "@/components/editor/TipTapEditor";
import { FloatingToolbar } from "@/components/editor/FloatingToolbar";
import { BacklinksPanel } from "@/components/file/BacklinksPanel";
import { ConflictDialog } from "@/components/file/ConflictDialog";
import { FileSheet } from "@/components/file/FileSheet";
import { GraphView } from "@/components/graph/GraphView";
import { QuickOpen } from "@/components/file/QuickOpen";
import { SearchPanel } from "@/components/file/SearchPanel";
import { SettingsPanel } from "@/components/settings/SettingsPanel";
import { TagView } from "@/components/tag/TagView";
import { VersionTimeline } from "@/components/version/VersionTimeline";
import { AppShell } from "@/components/layout/AppShell";
import { StatusBar } from "@/components/layout/StatusBar";
import { TabBar, type TabItem } from "@/components/layout/TabBar";
import { WelcomeEmpty } from "@/components/layout/WelcomeEmpty";
import { resolveDocumentTitle } from "@/lib/document-title";
import { createDefaultNote } from "@/lib/note-create";
import { Moon, PanelRight, Sun } from "lucide-react";

import { Button } from "@/components/ui/button";
import { useEditorSave } from "@/hooks/useEditorSave";
import { useEditorZoom } from "@/hooks/useEditorZoom";
import { useVersionIdle } from "@/hooks/useVersionIdle";
import { useInlineAi } from "@/hooks/useInlineAi";
import { useLlmProvider } from "@/hooks/useLlmProvider";
import { useOverlayManager } from "@/hooks/useOverlayManager";
import { useTheme, useVault } from "@/hooks/useVault";
import { titleFromFields, splitFrontmatter } from "@/lib/frontmatter";
import {
  editorHtmlToMarkdown,
  extractFrontmatterYaml,
  markdownToEditorHtml,
} from "@/lib/markdown";
import { fileRead, versionSaveManual } from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";
import { debounce, isModKey } from "@/lib/utils";

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
  const [tabs, setTabs] = useState<TabItem[]>([]);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [markdown, setMarkdown] = useState("");
  const activePathRef = useRef<string | null>(null);
  const markdownRef = useRef("");
  const editorRef = useRef<Editor | null>(null);
  const [editorInstance, setEditorInstance] = useState<Editor | null>(null);
  const overlays = useOverlayManager();
  const [aiPanelOpen, setAiPanelOpen] = useState(true);
  const [quote, setQuote] = useState<ContextQuote | null>(null);
  const [aiStatus, setAiStatus] = useState("AI 空闲");
  const [zen, setZen] = useState(false);
  const [outlineOpen, setOutlineOpen] = useState(loadOutlineOpen);
  const { zoom: editorZoom, zoomIn, zoomOut, resetZoom } = useEditorZoom();
  const frontmatterYamlRef = useRef<string | null>(null);
  const editorUpdateCleanupRef = useRef<(() => void) | null>(null);
  const { provider: llmProvider, setProvider: setLlmProvider } =
    useLlmProvider();
  const inlineAi = useInlineAi({
    provider: llmProvider,
    onStatus: setAiStatus,
  });

  activePathRef.current = activePath;
  markdownRef.current = markdown;

  const dirtyRef = useRef(false);

  const serializeEditorHtml = useCallback(
    (html: string) => editorHtmlToMarkdown(html, frontmatterYamlRef.current),
    [],
  );

  const { notifyDirty, flushSave } = useEditorSave(
    activePath,
    editorRef,
    (md) => {
      markdownRef.current = md;
      setMarkdown(md);
      frontmatterYamlRef.current = extractFrontmatterYaml(md);
      dirtyRef.current = false;
      setTabs((t) =>
        t.map((tab) =>
          tab.path === activePath ? { ...tab, dirty: false } : tab,
        ),
      );
    },
    serializeEditorHtml,
  );

  const getEditorMarkdown = useCallback(() => {
    const ed = editorRef.current;
    if (ed) {
      return editorHtmlToMarkdown(ed.getHTML(), frontmatterYamlRef.current);
    }
    return markdownRef.current;
  }, []);

  const { onActivity: resetVersionIdle } = useVersionIdle(
    activePath,
    getEditorMarkdown,
  );

  const handleSaveVersion = useCallback(async () => {
    const path = activePathRef.current;
    if (!path) return;
    await flushSave();
    await versionSaveManual(path, getEditorMarkdown());
  }, [flushSave, getEditorMarkdown]);

  const handleDirty = useCallback(() => {
    if (!dirtyRef.current) {
      dirtyRef.current = true;
      setTabs((t) =>
        t.map((tab) =>
          tab.path === activePathRef.current ? { ...tab, dirty: true } : tab,
        ),
      );
    }
    notifyDirty();
    resetVersionIdle();
  }, [notifyDirty, resetVersionIdle]);

  const titleDebounce = useMemo(
    () =>
      debounce((newTitle: string) => {
        const path = activePathRef.current;
        if (!path) return;
        setTabs((prev) =>
          prev.map((t) =>
            t.path === path ? { ...t, title: newTitle || "无标题" } : t,
          ),
        );
      }, 300),
    [],
  );

  const openFile = useCallback(async (path: string, titleHint?: string) => {
    const content = await fileRead(path);
    frontmatterYamlRef.current = extractFrontmatterYaml(content);
    const fromFile = titleFromFields(splitFrontmatter(content).fields);
    const title =
      fromFile ||
      (titleHint?.trim() ?? "") ||
      (await resolveDocumentTitle(path, titleHint));
    setMarkdown(content);
    markdownRef.current = content;
    dirtyRef.current = false;
    setActivePath(path);
    setTabs((prev) => {
      if (prev.some((t) => t.path === path)) {
        return prev.map((t) => (t.path === path ? { ...t, title } : t));
      }
      return [...prev, { path, title, dirty: false }];
    });
  }, []);

  const closeTab = useCallback(
    (path: string) => {
      setTabs((prev) => {
        const idx = prev.findIndex((t) => t.path === path);
        const next = prev.filter((t) => t.path !== path);
        if (activePathRef.current === path) {
          if (next.length === 0) {
            setActivePath(null);
            setMarkdown("");
          } else {
            const newIdx = Math.min(Math.max(0, idx), next.length - 1);
            const newPath = next[newIdx]!.path;
            void openFile(newPath);
          }
        }
        return next;
      });
    },
    [openFile],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (
        isModKey(e) &&
        !e.shiftKey &&
        (e.key === "s" || e.key === "S") &&
        activePathRef.current
      ) {
        e.preventDefault();
        void handleSaveVersion();
      }
      if (isModKey(e) && e.key === "p") {
        e.preventDefault();
        overlays.openOverlay("quickOpen");
      }
      if (isModKey(e) && e.shiftKey && (e.key === "E" || e.key === "e")) {
        e.preventDefault();
        overlays.openOverlay("fileSheet");
      }
      if (isModKey(e) && e.shiftKey && (e.key === "F" || e.key === "f")) {
        e.preventDefault();
        overlays.openOverlay("search");
      }
      if (isModKey(e) && e.shiftKey && (e.key === "A" || e.key === "a")) {
        e.preventDefault();
        setAiPanelOpen((open) => !open);
      }
      if (
        isModKey(e) &&
        e.shiftKey &&
        (e.key === "V" || e.key === "v") &&
        activePathRef.current
      ) {
        e.preventDefault();
        overlays.toggleOverlay("version");
      }
      if (isModKey(e) && e.key === "w" && activePathRef.current) {
        e.preventDefault();
        closeTab(activePathRef.current);
      }
      if (isModKey(e) && e.key === ",") {
        e.preventDefault();
        overlays.toggleOverlay("settings");
      }
      if (isModKey(e) && e.shiftKey && (e.key === "B" || e.key === "b")) {
        e.preventDefault();
        overlays.toggleOverlay("backlinks");
      }
      if (isModKey(e) && e.shiftKey && (e.key === "G" || e.key === "g")) {
        e.preventDefault();
        overlays.toggleOverlay("graph");
      }
      if (isModKey(e) && e.shiftKey && (e.key === "T" || e.key === "t")) {
        e.preventDefault();
        overlays.toggleOverlay("tags");
      }
      if (isModKey(e) && e.key === ".") {
        e.preventDefault();
        setZen((z) => !z);
      }
      if (
        isModKey(e) &&
        e.shiftKey &&
        (e.key === "O" || e.key === "o") &&
        activePathRef.current
      ) {
        e.preventDefault();
        setOutlineOpen((open) => {
          const next = !open;
          saveOutlineOpen(next);
          return next;
        });
      }
      if (isModKey(e) && !e.shiftKey && (e.key === "=" || e.key === "+")) {
        e.preventDefault();
        zoomIn();
      }
      if (isModKey(e) && !e.shiftKey && e.key === "-") {
        e.preventDefault();
        zoomOut();
      }
      if (isModKey(e) && !e.shiftKey && e.key === "0") {
        e.preventDefault();
        resetZoom();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [overlays, closeTab, handleSaveVersion, zoomIn, zoomOut, resetZoom]);

  useEffect(() => {
    if (!zen) return;
    const onEsc = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setZen(false);
        editorRef.current?.commands.focus();
      }
    };
    window.addEventListener("keydown", onEsc);
    return () => window.removeEventListener("keydown", onEsc);
  }, [zen]);

  useEffect(
    () => () => {
      editorUpdateCleanupRef.current?.();
      titleDebounce.cancel();
    },
    [titleDebounce],
  );

  const applyMarkdownToEditor = useCallback((content: string) => {
    frontmatterYamlRef.current = extractFrontmatterYaml(content);
    setMarkdown(content);
    markdownRef.current = content;
    const stem = activePathRef.current
      ? pathStem(activePathRef.current)
      : "";
    if (editorRef.current) {
      editorRef.current.commands.setContent(
        markdownToEditorHtml(content, stem),
        false,
      );
    }
  }, []);

  useEffect(() => {
    if (!activePath) setEditorInstance(null);
  }, [activePath]);

  const handleEditorReady = useCallback(
    (ed: Editor) => {
      editorRef.current = ed;
      setEditorInstance(ed);
      editorUpdateCleanupRef.current?.();
      let lastSyncedTitle = "";
      const onTitleUpdate = () => {
        const first = ed.state.doc.firstChild;
        if (first?.type.name !== "noteTitle") return;
        const next = first.textContent.trim();
        if (next === lastSyncedTitle) return;
        lastSyncedTitle = next;
        titleDebounce(next);
      };
      ed.on("update", onTitleUpdate);
      editorUpdateCleanupRef.current = () => {
        ed.off("update", onTitleUpdate);
      };
      onTitleUpdate();
    },
    [titleDebounce],
  );

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
    [inlineAi],
  );

  const sendSelectionToAi = useCallback(() => {
    const ed = editorRef.current;
    const path = activePathRef.current;
    if (!ed || !path) return;
    const { from, to } = ed.state.selection;
    const text = ed.state.doc.textBetween(from, to, "\n");
    if (!text) return;
    setQuote({ filePath: path, text });
  }, []);

  const handleNewNote = useCallback(async () => {
    const created = await createDefaultNote();
    await openFile(created.path, created.title);
  }, [openFile]);

  const activeDocumentTitle =
    tabs.find((t) => t.path === activePath)?.title ?? null;

  const chromeActions = (
    <>
      <Button
        type="button"
        size="sm"
        variant="outline"
        className="h-8 gap-1.5 px-2.5"
        onClick={() => setAiPanelOpen((o) => !o)}
        aria-pressed={aiPanelOpen}
        aria-label={aiPanelOpen ? "收起 AI 侧栏" : "展开 AI 侧栏"}
      >
        <PanelRight className="h-3.5 w-3.5" />
        <span className="hidden sm:inline">
          {aiPanelOpen ? "收起 AI" : "AI"}
        </span>
      </Button>
      <Button
        type="button"
        size="sm"
        variant="outline"
        className="h-8 gap-1.5 px-2.5"
        onClick={() => void setTheme(theme === "dark" ? "light" : "dark")}
        aria-label={theme === "dark" ? "切换为亮色" : "切换为暗色"}
      >
        {theme === "dark" ? (
          <Sun className="h-3.5 w-3.5" />
        ) : (
          <Moon className="h-3.5 w-3.5" />
        )}
        <span className="hidden sm:inline">
          {theme === "dark" ? "亮色" : "暗色"}
        </span>
      </Button>
    </>
  );

  if (!isTauriRuntime()) {
    return (
      <div className="flex h-dvh flex-col items-center justify-center gap-4 bg-background px-6 text-center">
        <h1 className="font-editor text-xl font-semibold text-foreground">
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
      <div className="flex h-dvh items-center justify-center bg-background text-muted-foreground">
        加载中…
      </div>
    );
  }

  if (!vaultPath) {
    return (
      <div className="flex h-dvh flex-col items-center justify-center gap-6 bg-background px-6">
        <div className="text-center">
          <h1 className="font-editor text-3xl font-semibold tracking-tight text-foreground">
            Iris
          </h1>
          <p className="mt-2 text-sm text-muted-foreground">
            纸墨笔记 · 本地优先
          </p>
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
    );
  }

  return (
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
          chromeActions={chromeActions}
        />
      }
      editor={
        <div className="relative flex min-h-0 flex-1 flex-col">
          {activePath ? (
            <>
              <TipTapEditor
                key={activePath}
                initialMarkdown={markdown}
                titleFallback={pathStem(activePath)}
                zen={zen}
                zoom={editorZoom}
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
            </>
          ) : (
            <WelcomeEmpty
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
        <AiPanel
          notePath={activePath}
          noteContent={markdown}
          quote={quote}
          onClearQuote={() => setQuote(null)}
          provider={llmProvider}
          onProviderChange={setLlmProvider}
        />
      }
      statusBar={
        <StatusBar
          path={activePath}
          documentTitle={activeDocumentTitle}
          markdown={markdown}
          wordCount={splitFrontmatter(markdown).body
            .replace(/\s+/g, "")
            .length}
          aiStatus={aiStatus}
          editorZoom={editorZoom}
          onEditorZoomIn={zoomIn}
          onEditorZoomOut={zoomOut}
          onEditorZoomReset={resetZoom}
        />
      }
      overlays={
        <>
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
          <SearchPanel
            open={overlays.searchOpen}
            onClose={() => overlays.closeOverlay("search")}
            onOpen={(p) => void openFile(p)}
          />
          <SettingsPanel
            open={overlays.settingsOpen}
            onClose={() => overlays.closeOverlay("settings")}
            provider={llmProvider}
            theme={theme}
            onThemeChange={(t) => void setTheme(t)}
          />
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
          <GraphView
            open={overlays.graphOpen}
            onClose={() => overlays.closeOverlay("graph")}
            onOpenNote={(p) => void openFile(p)}
          />
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
  );
}

export default App;
