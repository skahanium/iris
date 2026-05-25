import type { Editor } from "@tiptap/react";
import { useCallback, useEffect, useRef, useState } from "react";

import { AiPanel, type ContextQuote } from "@/components/ai/AiPanel";
import { TipTapEditor } from "@/components/editor/TipTapEditor";
import { FloatingToolbar } from "@/components/editor/FloatingToolbar";
import { FileSheet } from "@/components/file/FileSheet";
import { QuickOpen } from "@/components/file/QuickOpen";
import { SearchPanel } from "@/components/file/SearchPanel";
import { AppShell } from "@/components/layout/AppShell";
import { StatusBar } from "@/components/layout/StatusBar";
import { TabBar, type TabItem } from "@/components/layout/TabBar";
import { Button } from "@/components/ui/button";
import { useEditorSave } from "@/hooks/useEditorSave";
import { useInlineAi } from "@/hooks/useInlineAi";
import { useLlmProvider } from "@/hooks/useLlmProvider";
import { useTheme, useVault } from "@/hooks/useVault";
import { htmlToMarkdown } from "@/lib/markdown";
import { fileCreate, fileRead, listenFileChanged } from "@/lib/ipc";
import type { FileChangedEvent } from "@/types/ipc";

function App() {
  const { vaultPath, loading, pickVault } = useVault();
  const { theme, setTheme } = useTheme();
  const [tabs, setTabs] = useState<TabItem[]>([]);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [markdown, setMarkdown] = useState("");
  const htmlRef = useRef("");
  const [editor, setEditor] = useState<Editor | null>(null);
  const [quickOpen, setQuickOpen] = useState(false);
  const [fileSheet, setFileSheet] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [aiPanelOpen, setAiPanelOpen] = useState(true);
  const [quote, setQuote] = useState<ContextQuote | null>(null);
  const [aiStatus, setAiStatus] = useState("AI 空闲");
  const [reloadPrompt, setReloadPrompt] = useState<string | null>(null);
  const { provider: llmProvider, setProvider: setLlmProvider } = useLlmProvider();
  const inlineAi = useInlineAi({ provider: llmProvider, onStatus: setAiStatus });

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
    setActivePath(path);
    setTabs((prev) => {
      if (prev.some((t) => t.path === path)) return prev;
      const title = path.replace(/\.md$/, "").split("/").pop() ?? path;
      return [...prev, { path, title, dirty: false }];
    });
    setReloadPrompt(null);
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key === "p") {
        e.preventDefault();
        setQuickOpen(true);
      }
      if (e.ctrlKey && e.shiftKey && (e.key === "E" || e.key === "e")) {
        e.preventDefault();
        setFileSheet(true);
      }
      if (e.ctrlKey && e.shiftKey && (e.key === "F" || e.key === "f")) {
        e.preventDefault();
        setSearchOpen(true);
      }
      if (e.ctrlKey && e.shiftKey && (e.key === "A" || e.key === "a")) {
        e.preventDefault();
        setAiPanelOpen((open) => !open);
      }
      if (e.ctrlKey && e.key === "w" && activePath) {
        e.preventDefault();
        setTabs((t) => t.filter((x) => x.path !== activePath));
        setActivePath(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [activePath]);

  useEffect(() => {
    void listenFileChanged((payload) => {
      const ev = payload as FileChangedEvent;
      if (ev.path === activePath) {
        setReloadPrompt(`外部已修改 ${ev.path}，是否重新加载？`);
      }
    });
  }, [activePath]);

  const handleHtmlUpdate = useCallback(
    (html: string) => {
      htmlRef.current = html;
      const md = htmlToMarkdown(html);
      setMarkdown(md);
      if (activePath) {
        scheduleSave(md);
        setTabs((t) =>
          t.map((tab) =>
            tab.path === activePath ? { ...tab, dirty: true } : tab,
          ),
        );
      }
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
          onClose={(p) => {
            setTabs((t) => t.filter((x) => x.path !== p));
            if (activePath === p) setActivePath(null);
          }}
          onNew={async () => {
            const name = `note-${Date.now()}.md`;
            await fileCreate(name);
            await openFile(name);
          }}
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
            />
          ) : (
            <div className="flex flex-1 flex-col items-center justify-center gap-2 font-sans text-editor-muted">
              <p className="font-editor text-lg text-editor-ink/80">铺开纸面，开始写</p>
              <p className="text-sm">
                Ctrl+P 打开 · Ctrl+Shift+E 文件 · Ctrl+Shift+A AI 侧栏
              </p>
            </div>
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
            open={quickOpen}
            onClose={() => setQuickOpen(false)}
            onSelect={(p) => void openFile(p)}
          />
          <FileSheet
            open={fileSheet}
            onClose={() => setFileSheet(false)}
            onOpen={(p) => void openFile(p)}
          />
          <SearchPanel
            open={searchOpen}
            onClose={() => setSearchOpen(false)}
            onOpen={(p) => void openFile(p)}
          />
        </>
      }
    />
  );
}

export default App;
