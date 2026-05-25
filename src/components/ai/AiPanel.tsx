import { listen } from "@tauri-apps/api/event";
import { Send, Sparkles } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  buildAiSystemPrompt,
  filterRelatedSemanticHits,
  RELATED_NOTES_FETCH_LIMIT,
  type ContextQuote,
} from "@/lib/ai-context";
import {
  llmGenerate,
  listenLlmToken,
  searchSemantic,
} from "@/lib/ipc";
import type { ChatMessage, LlmTokenEvent, SemanticHit } from "@/types/ipc";

export type { ContextQuote };

interface AiPanelProps {
  notePath: string | null;
  noteContent: string;
  quote: ContextQuote | null;
  onClearQuote: () => void;
  provider: string;
  onProviderChange: (provider: string) => void;
}

interface ChatLine {
  role: "user" | "assistant";
  content: string;
}

export function AiPanel({
  notePath,
  noteContent,
  quote,
  onClearQuote,
  provider,
  onProviderChange,
}: AiPanelProps) {
  const [messages, setMessages] = useState<ChatLine[]>([]);
  const [input, setInput] = useState("");
  const [webSearch, setWebSearch] = useState(false);
  const [streaming, setStreaming] = useState(false);
  const [relatedNotes, setRelatedNotes] = useState<SemanticHit[]>([]);
  const [contextStatus, setContextStatus] = useState<string | null>(null);
  const [quoteOnceOnly, setQuoteOnceOnly] = useState(false);
  const [quoteExpanded, setQuoteExpanded] = useState(false);
  const streamBuf = useRef("");
  const requestIdRef = useRef<string | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listenLlmToken((payload) => {
      const ev = payload as LlmTokenEvent;
      if (requestIdRef.current && ev.request_id !== requestIdRef.current) return;
      streamBuf.current += ev.token;
      setMessages((prev) => {
        const copy = [...prev];
        const last = copy[copy.length - 1];
        if (last?.role === "assistant") {
          copy[copy.length - 1] = { role: "assistant", content: streamBuf.current };
        } else {
          copy.push({ role: "assistant", content: streamBuf.current });
        }
        return copy;
      });
    }).then((fn) => {
      unlisten = fn;
    });
    const doneUn = listen("llm:done", () => {
      setStreaming(false);
      streamBuf.current = "";
    });
    return () => {
      unlisten?.();
      void doneUn.then((fn) => fn());
    };
  }, []);

  const send = useCallback(async () => {
    if (!input.trim() || streaming) return;
    const userMsg = input.trim();
    setInput("");
    setMessages((m) => [...m, { role: "user", content: userMsg }]);
    setStreaming(true);
    streamBuf.current = "";

    let relatedHits: SemanticHit[] = [];
    try {
      const raw = await searchSemantic(userMsg, RELATED_NOTES_FETCH_LIMIT);
      relatedHits = filterRelatedSemanticHits(raw, notePath);
      setRelatedNotes(relatedHits);
      setContextStatus(
        relatedHits.length > 0
          ? `已注入 ${relatedHits.length} 条关联笔记`
          : "未找到关联笔记，仅使用当前笔记",
      );
    } catch {
      setRelatedNotes([]);
      setContextStatus("关联笔记检索失败，仅使用当前笔记");
    }

    const system = buildAiSystemPrompt({
      notePath,
      noteContent,
      quote,
      relatedHits,
    });

    const chatMessages: ChatMessage[] = [
      ...messages.map((m) => ({ role: m.role, content: m.content })),
      { role: "user", content: userMsg },
    ];

    try {
      const rid = await llmGenerate({
        provider,
        messages: chatMessages,
        system,
        stream: true,
        web_search: webSearch,
      });
      requestIdRef.current = rid;
      setMessages((m) => [...m, { role: "assistant", content: "" }]);
    } catch (e) {
      setStreaming(false);
      setMessages((m) => [
        ...m,
        {
          role: "assistant",
          content: `错误: ${e instanceof Error ? e.message : String(e)}`,
        },
      ]);
    }
  }, [input, streaming, messages, notePath, noteContent, quote, provider, webSearch]);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-border px-3 py-2">
        <Sparkles className="h-4 w-4 text-primary" />
        <span className="text-sm font-medium">AI</span>
        <select
          className="ml-auto rounded border border-border bg-card px-2 py-0.5 text-xs"
          value={provider}
          onChange={(e) => onProviderChange(e.target.value)}
        >
          <option value="openai">OpenAI</option>
          <option value="anthropic">Claude</option>
          <option value="ollama">Ollama</option>
          <option value="custom">自定义</option>
        </select>
      </div>

      <div className="space-y-3 border-b border-border px-3 py-2">
        <label className="flex items-center gap-2 text-xs text-muted-foreground">
          <input
            type="checkbox"
            checked={webSearch}
            onChange={(e) => setWebSearch(e.target.checked)}
          />
          联网搜索
        </label>
      </div>

      {quote && (
        <div className="m-2 rounded border border-primary/30 bg-editor-paper/10 p-2.5 text-xs">
          <div className="mb-1 flex items-center justify-between">
            <span className="font-medium text-muted-foreground">
              引用自 {quote.filePath}
              {quote.heading ? ` / ${quote.heading}` : ""}
            </span>
            <label className="flex items-center gap-1 text-muted-foreground/70">
              <input
                type="checkbox"
                checked={quoteOnceOnly}
                onChange={(e) => setQuoteOnceOnly(e.target.checked)}
                className="h-3 w-3"
              />
              仅此次
            </label>
          </div>
          <p
            className={
              quoteExpanded
                ? "font-editor leading-relaxed text-foreground/90"
                : "line-clamp-5 font-editor leading-relaxed text-foreground/90"
            }
          >
            {quote.text}
          </p>
          {quote.text.length > 200 && (
            <button
              type="button"
              className="mt-1 text-primary/70 hover:text-primary"
              onClick={() => setQuoteExpanded(!quoteExpanded)}
            >
              {quoteExpanded ? "收起" : "展开"}
            </button>
          )}
          <div className="mt-1.5 flex gap-2">
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="h-7 px-2 text-xs"
              onClick={() => {
                onClearQuote();
                setQuoteExpanded(false);
                setQuoteOnceOnly(false);
              }}
            >
              移除引用
            </Button>
          </div>
        </div>
      )}

      {contextStatus && (
        <p className="border-b border-border px-3 py-1.5 text-xs text-muted-foreground">
          {contextStatus}
        </p>
      )}

      {relatedNotes.length > 0 && (
        <div className="mx-2 mb-2 space-y-1">
          <p className="px-1 text-xs text-muted-foreground">关联笔记</p>
          <div className="flex flex-wrap gap-1.5">
            {relatedNotes.map((h) => (
              <span
                key={`${h.path}-${h.chunk_id}`}
                className="inline-flex items-center rounded-full border border-primary/20 bg-editor-paper/10 px-2.5 py-0.5 text-xs text-primary"
                title={h.snippet}
              >
                {h.title}
                <span className="ml-1 text-muted-foreground/60">
                  {Math.round(h.score * 100)}%
                </span>
              </span>
            ))}
          </div>
        </div>
      )}

      <ScrollArea className="flex-1 px-3 py-2">
        <div className="space-y-3 text-sm">
          {messages.map((m, i) => (
            <div
              key={`${i}-${m.role}`}
              className={
                m.role === "user"
                  ? "rounded bg-muted/50 p-2"
                  : "rounded border border-border/50 p-2"
              }
            >
              {m.content || (streaming && m.role === "assistant" ? "…" : "")}
            </div>
          ))}
        </div>
      </ScrollArea>

      <div className="flex gap-2 border-t border-border p-3">
        <Input
          value={input}
          placeholder="向 AI 提问…"
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void send();
            }
          }}
        />
        <Button type="button" size="icon" disabled={streaming} onClick={() => void send()}>
          <Send className="h-4 w-4" />
        </Button>
      </div>
    </div>
  );
}
