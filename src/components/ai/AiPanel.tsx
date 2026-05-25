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
  BING_SEARCH_CREDENTIAL_SERVICE,
  llmCredentialService,
} from "@/lib/credentials";
import {
  credentialDelete,
  credentialHas,
  credentialSet,
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
  const [llmKeyInput, setLlmKeyInput] = useState("");
  const [bingKeyInput, setBingKeyInput] = useState("");
  const [bingKeyConfigured, setBingKeyConfigured] = useState(false);
  const [relatedNotes, setRelatedNotes] = useState<SemanticHit[]>([]);
  const [contextStatus, setContextStatus] = useState<string | null>(null);
  const streamBuf = useRef("");
  const requestIdRef = useRef<string | null>(null);

  const refreshBingKeyStatus = useCallback(async () => {
    const has = await credentialHas(BING_SEARCH_CREDENTIAL_SERVICE);
    setBingKeyConfigured(has);
  }, []);

  useEffect(() => {
    void refreshBingKeyStatus();
  }, [refreshBingKeyStatus]);

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

  const saveLlmApiKey = useCallback(async () => {
    if (!llmKeyInput.trim()) return;
    await credentialSet(llmCredentialService(provider), llmKeyInput.trim());
    setLlmKeyInput("");
  }, [llmKeyInput, provider]);

  const saveBingApiKey = useCallback(async () => {
    if (!bingKeyInput.trim()) return;
    await credentialSet(BING_SEARCH_CREDENTIAL_SERVICE, bingKeyInput.trim());
    setBingKeyInput("");
    await refreshBingKeyStatus();
  }, [bingKeyInput, refreshBingKeyStatus]);

  const clearBingApiKey = useCallback(async () => {
    await credentialDelete(BING_SEARCH_CREDENTIAL_SERVICE);
    setBingKeyInput("");
    await refreshBingKeyStatus();
  }, [refreshBingKeyStatus]);

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
        <div>
          <label className="mb-1 block text-xs text-muted-foreground">
            LLM API Key（系统凭据）
          </label>
          <Input
            type="password"
            placeholder="按当前提供商保存"
            value={llmKeyInput}
            onChange={(e) => setLlmKeyInput(e.target.value)}
            onBlur={() => void saveLlmApiKey()}
          />
        </div>

        <div>
          <label className="mb-1 block text-xs text-muted-foreground">
            Bing 搜索 API Key（系统凭据 · iris/bing-search）
          </label>
          <Input
            type="password"
            placeholder="Azure Bing Web Search v7 订阅密钥"
            value={bingKeyInput}
            onChange={(e) => setBingKeyInput(e.target.value)}
            onBlur={() => void saveBingApiKey()}
          />
          <p className="mt-1 text-xs text-muted-foreground">
            {bingKeyConfigured
              ? "已配置 Bing：联网搜索优先走 Bing API。"
              : "未配置 Bing：联网搜索使用 DuckDuckGo（无需 Key）。"}
          </p>
          {bingKeyConfigured && (
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="mt-1 h-7 px-2 text-xs"
              onClick={() => void clearBingApiKey()}
            >
              清除 Bing Key
            </Button>
          )}
        </div>

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
        <div className="m-2 rounded border border-border bg-card p-2 text-xs">
          <div className="text-muted-foreground">引用自 {quote.filePath}</div>
          <p className="mt-1 line-clamp-4">{quote.text}</p>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="mt-1"
            onClick={onClearQuote}
          >
            移除引用
          </Button>
        </div>
      )}

      {contextStatus && (
        <p className="border-b border-border px-3 py-1.5 text-xs text-muted-foreground">
          {contextStatus}
        </p>
      )}

      {relatedNotes.length > 0 && (
        <div className="mx-2 mb-2 rounded border border-border/60 bg-card/50 p-2 text-xs">
          <div className="font-medium text-muted-foreground">本次关联笔记</div>
          <ul className="mt-1 space-y-1">
            {relatedNotes.map((h) => (
              <li key={`${h.path}-${h.chunk_id}`}>
                <span className="text-primary">{h.title}</span>
                <span className="text-muted-foreground"> · {h.path}</span>
                <p className="line-clamp-2 text-foreground/80">{h.snippet}</p>
              </li>
            ))}
          </ul>
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
