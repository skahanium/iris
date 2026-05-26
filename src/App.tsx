import type { Editor } from "@tiptap/react";
import { useCallback, useEffect, useRef, useState } from "react";

import { AiPanel, type ContextQuote } from "@/components/ai/AiPanel";
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
import { createDefaultNote } from "@/lib/note-create";
import { Button } from "@/components/ui/button";
import { useEditorSave } from "@/hooks/useEditorSave";
import { useInlineAi } from "@/hooks/useInlineAi";
import { useLlmProvider } from "@/hooks/useLlmProvider";
import { useOverlayManager } from "@/hooks/useOverlayManager";
import { useTheme, useVault } from "@/hooks/useVault";
import { markdownToHtml } from "@/lib/markdown";
import { fileRead } from "@/lib/ipc";
import { isModKey } from "@/lib/utils";

function App() {
  const { vaultPath, loading, pickVault } = useVault();
  const { theme, setTheme } = useTheme();
  const [tabs, setTabs] = useState<TabItem[]>([]);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [markdown, setMarkdown] = useState("");
  const activePathRef = useRef<string | null>(null);
  const markdownRef = useRef("");
  const editorRef = useRef<Editor | null>(null);
  const overlays = useOverlayManager();
  const [aiPanelOpen, setAiPanelOpen] = useState(true);
  const [quote, setQuote] = useState<ContextQuote | null>(null);
  const [aiStatus, setAiStatus] = useState("AI 空闲");
  const { provider: llmProvider, setProvider: setLlmProvider } =
    useLlmProvider();
  const inlineAi = useInlineAi({
    provider: llmProvider,
    onStatus: setAiStatus,
  });

  activePathRef.current = activePath;
  markdownRef.current = markdown;

  const dirtyRef = useRef(false);

  const { notifyDirty } = useEditorSave(
    activePath,
    editorRef,
    (md) => {
      markdownRef.current = md;
      setMarkdown(md);
      dirtyRef.current = false;
      setTabs((t) =>
        t.map((tab) =>
          tab.path === activePath ? { ...tab, dirty: false } : tab,
        ),
      );
    },
  );

  const handleDirty = useCallback(() => {
    if (!dirtyRef.current) {
      dirtyRef.current = true;
      setTabs((t) =>
        t.map((tab) =>
          tab.path === activePathRef.current
            ? { ...tab, dirty: true }
            : tab,
        ),
      );
    }
    notifyDirty();
  }, [notifyDirty]);

  const openFile = useCallback(async (path: string) => {
    const content = await fileRead(path);
    setMarkdown(content);
    markdownRef.current = content;
    dirtyRef.current = false;
    setActivePath(path);
    setTabs((prev) => {
      if (prev.some((t) => t.path === path)) return prev;
      const title = path.replace(/\.md$/, "").split("/").pop() ?? path;
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
      if (isModKey(e) && e.key === "p") {
        e.preventDefault();
        overlays.setQuickOpen(true);
      }
      if (isModKey(e) && e.shiftKey && (e.key === "E" || e.key === "e")) {
        e.preventDefault();
        overlays.openSidePanel("fileSheet");
      }
      if (isModKey(e) && e.shiftKey && (e.key === "F" || e.key === "f")) {
        e.preventDefault();
        overlays.openSidePanel("search");
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
        overlays.toggleSidePanel("version");
      }
      if (isModKey(e) && e.key === "w" && activePathRef.current) {
        e.preventDefault();
        closeTab(activePathRef.current);
      }
      if (isModKey(e) && e.key === ",") {
        e.preventDefault();
        overlays.toggleSidePanel("settings");
      }
      if (isModKey(e) && e.shiftKey && (e.key === "B" || e.key === "b")) {
        e.preventDefault();
        overlays.toggleSidePanel("backlinks");
      }
      if (isModKey(e) && e.shiftKey && (e.key === "G" || e.key === "g")) {
        e.preventDefault();
        overlays.toggleSidePanel("graph");
      }
      if (isModKey(e) && e.shiftKey && (e.key === "T" || e.key === "t")) {
        e.preventDefault();
        overlays.toggleSidePanel("tags");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [overlays, closeTab]);

  const applyMarkdownToEditor = useCallback(
    (content: string) => {
      setMarkdown(content);
      markdownRef.current = content;
      if (editorRef.current) {
        editorRef.current.commands.setContent(markdownToHtml(content), false);
      }
    },
    [],
  );

  const handleEditorReady = useCallback((ed: Editor) => {
    editorRef.current = ed;
  }, []);

  const runInlineAi = useCallback(
    (action: string) => {
      if (!editorRef.current) return;
      void inlineAi.run(editorRef.current, action);
    },
    [inlineAi],
  );

  const handleSlashCommand = useCallback(
    (command: string) => {
      if (!editorRef.current) return;
      void inlineAi.runSlash(
        editorRef.current,
        command,
        markdownRef.current,
      );
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
    const name = await createDefaultNote();
    await openFile(name);
  }, [openFile]);

  if (loading) {
    return (
      <div className="flex h-screen items-center justify-center">加载中…</div>
    );
  }

  if (!vaultPath) {
    return (
      <div className="flex h-screen flex-col items-center justify-center gap-4">
        <h1 className="text-2xl font-semibold">Iris</h1>
        <p className="text-muted-foreground">选择笔记目录以开始</p>
        <Button type="button" onClick={() => void pickVault()}>
          选择笔记目录
        </Button>
      </div>
    );
  }

  return (
    <AppShell
      aiPanelOpen={aiPanelOpen}
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
        <div className="relative flex min-h-0 flex-1 flex-col">
          {activePath ? (
            <TipTapEditor
              key={activePath}
              initialMarkdown={markdown}
              onDirty={handleDirty}
              onSlashCommand={handleSlashCommand}
              onEditorReady={handleEditorReady}
              onInlineAiRetry={(ed) => void inlineAi.retry(ed)}
              onOpenWikiLink={(title) => void openFile(`${title}.md`)}
            />
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
          wordCount={markdown.split(/\s+/).filter(Boolean).length}
          aiStatus={aiStatus}
        />
      }
      overlays={
        <>
          <div
            className={
              aiPanelOpen
                ? "fixed right-[292px] top-2 z-30 flex gap-1"
                : "fixed right-3 top-2 z-30 flex gap-1"
            }
          >
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => setAiPanelOpen((o) => !o)}
            >
              {aiPanelOpen ? "收起 AI" : "AI"}
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
            >
              {theme === "dark" ? "亮色" : "暗色"}
            </Button>
          </div>
          <QuickOpen
            open={overlays.quickOpen}
            onClose={() => overlays.setQuickOpen(false)}
            onSelect={(p) => void openFile(p)}
          />
          <FileSheet
            open={overlays.fileSheet}
            onClose={() => overlays.setFileSheet(false)}
            onOpen={(p) => void openFile(p)}
          />
          <SearchPanel
            open={overlays.searchOpen}
            onClose={() => overlays.setSearchOpen(false)}
            onOpen={(p) => void openFile(p)}
          />
          <SettingsPanel
            open={overlays.settingsOpen}
            onClose={() => overlays.setSettingsOpen(false)}
            provider={llmProvider}
          />
          <BacklinksPanel
            open={overlays.backlinksOpen}
            onClose={() => overlays.setBacklinksOpen(false)}
            notePath={activePath}
            onOpen={(p) => void openFile(p)}
          />
          <TagView
            open={overlays.tagViewOpen}
            onClose={() => overlays.setTagViewOpen(false)}
            onOpen={(p) => void openFile(p)}
          />
          <VersionTimeline
            open={overlays.versionOpen}
            onClose={() => overlays.setVersionOpen(false)}
            notePath={activePath}
            currentContent={markdown}
            onRestore={applyMarkdownToEditor}
          />
          <GraphView
            open={overlays.graphOpen}
            onClose={() => overlays.setGraphOpen(false)}
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
