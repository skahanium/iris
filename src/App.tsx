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
import { htmlToMarkdown, markdownToHtml } from "@/lib/markdown";
import { fileRead, listenFileChanged } from "@/lib/ipc";
import { isModKey } from "@/lib/utils";

function App() {
  const { vaultPath, loading, pickVault } = useVault();
  const { theme, setTheme } = useTheme();
  const [tabs, setTabs] = useState<TabItem[]>([]);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [markdown, setMarkdown] = useState("");
  const htmlRef = useRef("");
  const activePathRef = useRef<string | null>(null);
  const markdownRef = useRef("");
  const skipNextDirtyRef = useRef(false);
  const [editor, setEditor] = useState<Editor | null>(null);
  const overlays = useOverlayManager();
  const [aiPanelOpen, setAiPanelOpen] = useState(true);
  const [conflictOpen, setConflictOpen] = useState(false);
  const [conflictPath, setConflictPath] = useState("");
  const [conflictExternal, setConflictExternal] = useState("");
  const [quote, setQuote] = useState<ContextQuote | null>(null);
  const [aiStatus, setAiStatus] = useState("AI 空闲");
  const [reloadPrompt, setReloadPrompt] = useState<string | null>(null);
  const { provider: llmProvider, setProvider: setLlmProvider } =
    useLlmProvider();
  const inlineAi = useInlineAi({
    provider: llmProvider,
    onStatus: setAiStatus,
  });

  activePathRef.current = activePath;
  markdownRef.current = markdown;

  const { scheduleSave } = useEditorSave(activePath, () => {
    setTabs((t) =>
      t.map((tab) =>
        tab.path === activePath ? { ...tab, dirty: false } : tab,
      ),
    );
  });

  const openFile = useCallback(async (path: string) => {
    const content = await fileRead(path);
    setMarkdown(content);
    htmlRef.current = "";
    skipNextDirtyRef.current = true;
    setActivePath(path);
    setTabs((prev) => {
      if (prev.some((t) => t.path === path)) return prev;
      const title = path.replace(/\.md$/, "").split("/").pop() ?? path;
      return [...prev, { path, title, dirty: false }];
    });
    setReloadPrompt(null);
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

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listenFileChanged((ev) => {
      const path = activePathRef.current;
      if (!path || ev.path !== path) return;
      if (ev.event_type === "modify") {
        void fileRead(ev.path).then((extContent) => {
          if (extContent !== markdownRef.current) {
            setConflictPath(ev.path);
            setConflictExternal(extContent);
            setConflictOpen(true);
          }
        });
      } else {
        setReloadPrompt(`外部已修改 ${ev.path}，是否重新加载？`);
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  const applyMarkdownToEditor = useCallback(
    (content: string) => {
      setMarkdown(content);
      if (editor) {
        editor.commands.setContent(markdownToHtml(content), false);
      }
    },
    [editor],
  );

  const handleHtmlUpdate = useCallback(
    (html: string) => {
      htmlRef.current = html;
      const md = htmlToMarkdown(html);
      setMarkdown(md);

      if (!activePath) return;

      if (skipNextDirtyRef.current) {
        skipNextDirtyRef.current = false;
        return;
      }

      scheduleSave(md);
      setTabs((t) =>
        t.map((tab) =>
          tab.path === activePath ? { ...tab, dirty: true } : tab,
        ),
      );
    },
    [activePath, scheduleSave],
  );

  const runInlineAi = useCallback(
    (action: string) => {
      if (!editor) return;
      void inlineAi.run(editor, action);
    },
    [editor, inlineAi],
  );

  const handleSlashCommand = useCallback(
    (command: string) => {
      if (!editor) return;
      void inlineAi.runSlash(editor, command, markdown);
    },
    [editor, inlineAi, markdown],
  );

  const sendSelectionToAi = useCallback(() => {
    if (!editor || !activePath) return;
    const { from, to } = editor.state.selection;
    const text = editor.state.doc.textBetween(from, to, "\n");
    if (!text) return;
    setQuote({ filePath: activePath, text });
  }, [editor, activePath]);

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
          {reloadPrompt && (
            <div className="flex items-center gap-2 border-b border-primary/25 bg-editor-border/40 px-4 py-2 font-sans text-sm text-editor-ink">
              <span>{reloadPrompt}</span>
              <Button
                type="button"
                size="sm"
                onClick={() => activePath && void openFile(activePath)}
              >
                重新加载
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() => setReloadPrompt(null)}
              >
                忽略
              </Button>
            </div>
          )}
          {activePath ? (
            <TipTapEditor
              key={activePath}
              initialMarkdown={markdown}
              onUpdateHtml={handleHtmlUpdate}
              onSlashCommand={handleSlashCommand}
              onEditorReady={setEditor}
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
            editor={editor}
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
            open={conflictOpen}
            localContent={markdown}
            externalContent={conflictExternal}
            filePath={conflictPath}
            onKeepLocal={() => setConflictOpen(false)}
            onAcceptExternal={() => {
              applyMarkdownToEditor(conflictExternal);
              setConflictOpen(false);
            }}
            onManualEdit={() => setConflictOpen(false)}
          />
        </>
      }
    />
  );
}

export default App;
