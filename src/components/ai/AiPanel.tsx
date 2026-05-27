import { marked } from "marked";
import { Send, Layers, Settings2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { sanitizeHtml } from "@/lib/sanitize";
import {
  contextAssemble,
  aiSendMessage,
  toolConfirm as toolConfirmIpc,
  listenLlmToken,
  listenLlmDone,
  listenLlmError,
} from "@/lib/ipc";
import type { AiScene, AssembledContext, ContextPacket } from "@/types/ai";
import type { LlmTokenEvent } from "@/types/ipc";

import { ContextPacketList } from "./ContextPacketCard";
import {
  ToolConfirmDialog,
  type ToolConfirmRequest,
} from "./ToolConfirmDialog";

// ─── Backward Compatibility Exports ──────────────────────

/** @deprecated Use ContextPacket instead */
export interface ContextQuote {
  filePath: string;
  heading?: string;
  text: string;
}

// ─── Scene Options ───────────────────────────────────────

const SCENE_OPTIONS: { value: AiScene; label: string; description: string }[] = [
  {
    value: "knowledge_lookup",
    label: "知识管家",
    description: "查找、解释、引用知识库材料",
  },
  {
    value: "exemplar_learning",
    label: "学习伴侣",
    description: "分析范文结构和表达方式",
  },
  {
    value: "drafting_assist",
    label: "写作伴侣",
    description: "文稿创作辅助",
  },
  {
    value: "research_synthesis",
    label: "研究助理",
    description: "多材料论证和证据分析",
  },
];

// ─── Types ───────────────────────────────────────────────

interface AiPanelProps {
  notePath: string | null;
  noteContent: string;
  onInsertText?: (text: string) => void;
  onReplaceSelection?: (text: string) => void;
}

interface ChatLine {
  role: "user" | "assistant" | "system";
  content: string;
  toolCalls?: Array<{ name: string; status: string }>;
}

// ─── Assistant Message Renderer ──────────────────────────

function AssistantMessage({ content }: { content: string }) {
  const html = useMemo(() => {
    const raw = marked.parse(content || "", { async: false }) as string;
    return sanitizeHtml(raw);
  }, [content]);

  return (
    <div
      className="ai-msg text-sm leading-relaxed [&_code]:rounded [&_code]:bg-muted/60 [&_code]:px-1 [&_p]:mb-2 [&_ul]:mb-2 [&_ul]:list-disc [&_ul]:pl-5"
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

// ─── Main Component ──────────────────────────────────────

export function AiPanel({
  notePath,
  noteContent: _noteContent,
  onInsertText: _onInsertText,
  onReplaceSelection: _onReplaceSelection,
}: AiPanelProps) {
  // State
  const [scene, setScene] = useState<AiScene>("knowledge_lookup");
  const [messages, setMessages] = useState<ChatLine[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [sessionId, setSessionId] = useState<number | null>(null);
  const [packets, setPackets] = useState<ContextPacket[]>([]);
  const [selectedPacketIds, setSelectedPacketIds] = useState<string[]>([]);
  const [contextStatus, setContextStatus] = useState<string | null>(null);
  const [showPackets, setShowPackets] = useState(false);
  const [toolConfirmRequest, setToolConfirmRequest] =
    useState<ToolConfirmRequest | null>(null);

  // Refs
  const streamBuf = useRef("");
  const requestIdRef = useRef<string | null>(null);

  // Listen for streaming events
  useEffect(() => {
    let unlistenToken: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;
    let cleanup: (() => void) | undefined;

    const tokenPromise = listenLlmToken((ev: LlmTokenEvent) => {
      if (requestIdRef.current && ev.request_id !== requestIdRef.current) return;
      streamBuf.current += ev.token;
      setMessages((prev) => {
        const copy = [...prev];
        const last = copy[copy.length - 1];
        if (last?.role === "assistant") {
          copy[copy.length - 1] = { ...last, content: streamBuf.current };
        } else {
          copy.push({ role: "assistant", content: streamBuf.current });
        }
        return copy;
      });
    }).then((fn) => {
      unlistenToken = fn;
    });

    const donePromise = listenLlmDone(() => {
      setStreaming(false);
      streamBuf.current = "";
    }).then((fn) => {
      unlistenDone = fn;
    });

    const errorPromise = listenLlmError((ev) => {
      setStreaming(false);
      streamBuf.current = "";
      setMessages((prev) => [
        ...prev,
        {
          role: "system",
          content: `错误: ${ev.error ?? "未知错误"}`,
        },
      ]);
    }).then((fn) => {
      unlistenError = fn;
    });

    void Promise.all([tokenPromise, donePromise, errorPromise]).then(() => {
      cleanup = () => {
        unlistenToken?.();
        unlistenDone?.();
        unlistenError?.();
      };
    });

    return () => {
      cleanup?.();
    };
  }, []);

  // Listen for tool confirmation requests
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    import("@tauri-apps/api/event").then(({ listen }) => {
      listen<ToolConfirmRequest>("ai:tool_confirm_request", (event) => {
        setToolConfirmRequest(event.payload);
      }).then((fn) => {
        unlisten = fn;
      });
    });

    return () => {
      unlisten?.();
    };
  }, []);

  // Assemble context when scene or query changes
  const assembleContext = useCallback(
    async (query: string) => {
      try {
        const result: AssembledContext = await contextAssemble({
          scene,
          note_path: notePath,
          note_content_hash: null,
          query,
          session_id: sessionId,
        });
        setPackets(result.packets);
        setContextStatus(
          `已加载 ${result.packets.length} 条证据 | ` +
            `${result.context_status.regulations_loaded} 条法规 | ` +
            `${result.context_status.anchors_loaded} 个锚点`
        );
        return result;
      } catch (e) {
        setContextStatus(`上下文组装失败: ${e instanceof Error ? e.message : String(e)}`);
        return null;
      }
    },
    [scene, notePath, sessionId]
  );

  // Send message
  const send = useCallback(async () => {
    if (!input.trim() || streaming) return;
    const userMsg = input.trim();
    setInput("");
    setMessages((m) => [...m, { role: "user", content: userMsg }]);
    setStreaming(true);
    streamBuf.current = "";

    // Assemble context first
    const context = await assembleContext(userMsg);
    if (!context) {
      setStreaming(false);
      return;
    }

    try {
      const result = await aiSendMessage({
        scene,
        session_id: sessionId,
        message: userMsg,
        selected_packet_ids: selectedPacketIds.length > 0 ? selectedPacketIds : undefined,
      });

      requestIdRef.current = result.request_id;
      if (result.session_id) {
        setSessionId(result.session_id);
      }

      // Add assistant message placeholder
      setMessages((m) => [
        ...m,
        {
          role: "assistant",
          content: result.content ?? "",
          toolCalls: result.tool_calls?.map((tc: { function: { name: string } }) => ({
            name: tc.function.name,
            status: "pending",
          })),
        },
      ]);

      // If response is complete (non-streaming fallback)
      if (result.status === "completed" && result.content) {
        setStreaming(false);
      }
    } catch (e) {
      setStreaming(false);
      setMessages((m) => [
        ...m,
        {
          role: "system",
          content: `错误: ${e instanceof Error ? e.message : String(e)}`,
        },
      ]);
    }
  }, [input, streaming, scene, sessionId, selectedPacketIds, assembleContext]);

  // Handle tool confirmation
  const handleToolConfirm = useCallback(
    async (
      requestId: string,
      toolCallId: string,
      decision: "approve" | "reject" | "modify",
      modifiedArgs?: unknown
    ) => {
      try {
        await toolConfirmIpc({
          request_id: requestId,
          tool_call_id: toolCallId,
          decision,
          modified_args: modifiedArgs,
        });

        // If approved, handle tool result
        if (decision === "approve") {
          // Tool execution will be handled by the backend
          // and result will be emitted as ai:tool_result event
        }
      } catch (e) {
        setMessages((m) => [
          ...m,
          {
            role: "system",
            content: `工具确认失败: ${e instanceof Error ? e.message : String(e)}`,
          },
        ]);
      }
    },
    []
  );

  // Toggle packet selection
  const togglePacketSelection = useCallback((id: string) => {
    setSelectedPacketIds((prev) =>
      prev.includes(id) ? prev.filter((pid) => pid !== id) : [...prev, id]
    );
  }, []);

  return (
    <div className="flex h-full flex-col">
      {/* Header with scene selector */}
      <div className="flex items-center gap-2 border-b border-border p-3">
        <Select value={scene} onValueChange={(v: string) => setScene(v as AiScene)}>
          <SelectTrigger className="w-[180px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {SCENE_OPTIONS.map((opt) => (
              <SelectItem key={opt.value} value={opt.value}>
                <div className="flex flex-col">
                  <span>{opt.label}</span>
                  <span className="text-[10px] text-muted-foreground">
                    {opt.description}
                  </span>
                </div>
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <div className="flex-1" />

        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8"
          onClick={() => setShowPackets(!showPackets)}
        >
          <Layers className="h-4 w-4" />
        </Button>
      </div>

      {/* Context status bar */}
      {contextStatus && (
        <div className="border-b border-border px-3 py-1.5 text-xs text-muted-foreground bg-muted/30">
          {contextStatus}
        </div>
      )}

      {/* Evidence packets panel */}
      {showPackets && (
        <div className="border-b border-border max-h-[300px] overflow-auto p-3">
          <ContextPacketList
            packets={packets}
            selectedIds={selectedPacketIds}
            onSelect={togglePacketSelection}
            compact
          />
        </div>
      )}

      {/* Chat messages */}
      <ScrollArea className="flex-1 px-3 py-2">
        <div className="space-y-3 text-sm">
          {messages.map((m, i) => (
            <div
              key={`${i}-${m.role}`}
              className={
                m.role === "user"
                  ? "ai-msg-user"
                  : m.role === "system"
                    ? "ai-msg-system text-xs text-muted-foreground italic"
                    : "ai-msg-assistant"
              }
            >
              {m.role === "assistant" ? (
                m.content ? (
                  <>
                    <AssistantMessage content={m.content} />
                    {m.toolCalls && m.toolCalls.length > 0 && (
                      <div className="mt-2 flex flex-wrap gap-1">
                        {m.toolCalls.map((tc, j) => (
                          <span
                            key={j}
                            className="inline-flex items-center rounded bg-primary/10 px-2 py-0.5 text-xs"
                          >
                            <Settings2 className="h-3 w-3 mr-1" />
                            {tc.name}
                          </span>
                        ))}
                      </div>
                    )}
                  </>
                ) : streaming ? (
                  "…"
                ) : null
              ) : (
                m.content
              )}
            </div>
          ))}
        </div>
      </ScrollArea>

      {/* Input area */}
      <div className="flex gap-2 border-t border-border p-3">
        <Input
          value={input}
          placeholder={`向${SCENE_OPTIONS.find((o) => o.value === scene)?.label ?? "AI"}提问…`}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void send();
            }
          }}
          disabled={streaming}
        />
        <Button
          type="button"
          size="icon"
          disabled={streaming || !input.trim()}
          onClick={() => void send()}
        >
          <Send className="h-4 w-4" />
        </Button>
      </div>

      {/* Tool confirmation dialog */}
      <ToolConfirmDialog
        request={toolConfirmRequest}
        onConfirm={handleToolConfirm}
        onClose={() => setToolConfirmRequest(null)}
      />
    </div>
  );
}
