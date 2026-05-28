import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";

import { AiComposer } from "@/components/ui/ai-composer";
import {
  contextAssemble,
  aiSendMessage,
  corpusList,
  fileList,
  toolConfirm as toolConfirmIpc,
  listenLlmToken,
  listenLlmDone,
  listenLlmError,
  llmAbort,
} from "@/lib/ipc";
import {
  buildMentionCandidates,
  findActiveMentionQuery,
  insertMentionToken,
  parseMentionTokens,
  stripMentionTokensForDisplay,
  tokensToContextScope,
  type MentionCandidate,
  type MentionToken,
} from "@/lib/ai-context-scope";
import { SCENE_META } from "@/lib/ai/scene-types";
import { invokeErrorMessage } from "@/lib/credentials";
import { setActiveAiScene } from "@/hooks/useConnectivityStatus";
import { useListboxKeyboard } from "@/hooks/useListboxKeyboard";
import type {
  AiScene,
  AssembledContext,
  ContextPacket,
  ExecutionPlan,
} from "@/types/ai";
import type { FileListItem, LlmTokenEvent } from "@/types/ipc";

import { AiMentionPopover } from "./AiMentionPopover";
import { AiMessageList, type ChatLine } from "./AiMessageList";
import { ContextPacketDrawer } from "./ContextPacketDrawer";
import { ContextScopeChips } from "./ContextScopeChips";
import { ContextStatusBar } from "./ContextStatusBar";
import { ExecutionPlanPreview } from "./ExecutionPlanPreview";
import { AiPanelHeader } from "./AiPanelHeader";
import {
  ToolConfirmDialog,
  type ToolConfirmRequest,
} from "./ToolConfirmDialog";

interface AiPanelProps {
  notePath: string | null;
  noteDisplayTitle: string | null;
  noteContent: string;
  onInsertText?: (text: string) => void;
  onReplaceSelection?: (text: string) => void;
}

export function AiPanel({
  notePath,
  noteDisplayTitle,
  noteContent: _noteContent,
  onInsertText: _onInsertText,
  onReplaceSelection: _onReplaceSelection,
}: AiPanelProps) {
  const [scene, setSceneState] = useState<AiScene>("knowledge_lookup");
  const setScene = useCallback((next: AiScene) => {
    setSceneState(next);
    setActiveAiScene(next);
  }, []);
  useEffect(() => {
    setActiveAiScene(scene);
  }, [scene]);
  const [messages, setMessages] = useState<ChatLine[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [sessionId, setSessionId] = useState<number | null>(null);
  const [packets, setPackets] = useState<ContextPacket[]>([]);
  const [selectedPacketIds, setSelectedPacketIds] = useState<string[]>([]);
  const [contextStatusData, setContextStatusData] = useState<
    import("@/types/ai").ContextStatus | null
  >(null);
  const [packetsOpen, setPacketsOpen] = useState(false);
  const [toolConfirmRequest, setToolConfirmRequest] =
    useState<ToolConfirmRequest | null>(null);
  const [executionPlan, setExecutionPlan] = useState<ExecutionPlan | null>(
    null,
  );

  const streamBuf = useRef("");
  const requestIdRef = useRef<string | null>(null);
  /** 侧栏 `send()` 进行中，用于过滤其它 LLM 流与 `llm:done` 竞态 */
  const panelSendActiveRef = useRef(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const [vaultFiles, setVaultFiles] = useState<FileListItem[]>([]);
  const [mentionOpen, setMentionOpen] = useState(false);
  const [mentionStart, setMentionStart] = useState(0);
  const [mentionQuery, setMentionQuery] = useState("");
  const [corpusNames, setCorpusNames] = useState<string[]>([]);

  const mentionTokens = useMemo(() => parseMentionTokens(input), [input]);
  const contextScope = useMemo(
    () => tokensToContextScope(mentionTokens),
    [mentionTokens],
  );
  const mentionCandidates = useMemo(
    () => buildMentionCandidates(vaultFiles, mentionQuery),
    [vaultFiles, mentionQuery],
  );

  useEffect(() => {
    void fileList()
      .then(setVaultFiles)
      .catch(() => setVaultFiles([]));
    void corpusList()
      .then((items) => setCorpusNames(items.map((c) => c.name)))
      .catch(() => setCorpusNames([]));
  }, []);

  const syncMentionFromInput = useCallback(() => {
    const ta = textareaRef.current;
    if (!ta) {
      setMentionOpen(false);
      return;
    }
    const active = findActiveMentionQuery(input, ta.selectionStart);
    if (active) {
      setMentionOpen(true);
      setMentionStart(active.start);
      setMentionQuery(active.query);
    } else {
      setMentionOpen(false);
    }
  }, [input]);

  useEffect(() => {
    syncMentionFromInput();
  }, [input, syncMentionFromInput]);

  const selectMention = useCallback(
    (candidate: MentionCandidate) => {
      const ta = textareaRef.current;
      const cursor = ta?.selectionStart ?? input.length;
      const { text, cursor: nextCursor } = insertMentionToken(
        input,
        cursor,
        mentionStart,
        candidate,
      );
      setInput(text);
      setMentionOpen(false);
      requestAnimationFrame(() => {
        const el = textareaRef.current;
        if (!el) return;
        el.focus();
        el.setSelectionRange(nextCursor, nextCursor);
      });
    },
    [input, mentionStart],
  );

  const removeMentionToken = useCallback((token: MentionToken) => {
    setInput((prev) => prev.replace(token.raw, "").replace(/\s{2,}/g, " "));
  }, []);

  const {
    highlight: mentionHighlight,
    handleKeyDown: handleMentionKeyDown,
    setHighlight: setMentionHighlight,
    navDeltaRef: mentionNavDeltaRef,
  } = useListboxKeyboard({
    length: mentionCandidates.length,
    enabled: mentionOpen && mentionCandidates.length > 0,
    wrap: false,
    resetKey: `${mentionQuery}:${mentionCandidates.length}`,
    onActivate: (index) => {
      const item = mentionCandidates[index];
      if (item) selectMention(item);
    },
  });

  const handleComposerKeyDown = useCallback(
    (e: KeyboardEvent<HTMLTextAreaElement>) => {
      if (mentionOpen) {
        if (e.key === "Escape") {
          e.preventDefault();
          setMentionOpen(false);
          return;
        }
        if (handleMentionKeyDown(e)) return;
      }
    },
    [mentionOpen, handleMentionKeyDown],
  );

  useEffect(() => {
    let unlistenToken: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;
    let cleanup: (() => void) | undefined;

    const tokenPromise = listenLlmToken((ev: LlmTokenEvent) => {
      if (!panelSendActiveRef.current) return;
      if (!requestIdRef.current) {
        requestIdRef.current = ev.request_id;
      } else if (ev.request_id !== requestIdRef.current) {
        return;
      }
      streamBuf.current += ev.token;
      const snapshot = streamBuf.current;
      setMessages((prev) => {
        const copy = [...prev];
        const last = copy[copy.length - 1];
        if (last?.role === "assistant") {
          copy[copy.length - 1] = { ...last, content: snapshot };
        } else {
          copy.push({ role: "assistant", content: snapshot });
        }
        return copy;
      });
    }).then((fn) => {
      unlistenToken = fn;
    });

    const donePromise = listenLlmDone((ev) => {
      if (!panelSendActiveRef.current) return;
      if (
        requestIdRef.current &&
        ev.request_id &&
        ev.request_id !== requestIdRef.current
      ) {
        return;
      }
      setStreaming(false);
    }).then((fn) => {
      unlistenDone = fn;
    });

    const errorPromise = listenLlmError((ev) => {
      if (!panelSendActiveRef.current) return;
      if (
        requestIdRef.current &&
        ev.request_id &&
        ev.request_id !== requestIdRef.current
      ) {
        return;
      }
      panelSendActiveRef.current = false;
      setStreaming(false);
      streamBuf.current = "";
      requestIdRef.current = null;
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

  const assembleContext = useCallback(
    async (query: string): Promise<AssembledContext> => {
      const result = await contextAssemble({
        scene,
        note_path: notePath,
        note_content_hash: null,
        query,
        session_id: sessionId,
        context_scope: contextScope,
      });
      setPackets(result.packets);
      setContextStatusData(result.context_status);
      if (result.packets.length > 0) {
        setPacketsOpen(true);
      }
      return result;
    },
    [scene, notePath, sessionId, contextScope],
  );

  const send = useCallback(async () => {
    if (!input.trim() || streaming) return;
    const rawMsg = input.trim();
    const userMsg = stripMentionTokensForDisplay(rawMsg);
    setInput("");
    setMessages((m) => [...m, { role: "user", content: userMsg }]);
    setStreaming(true);
    streamBuf.current = "";
    requestIdRef.current = null;
    panelSendActiveRef.current = true;

    try {
      await assembleContext(rawMsg);

      const result = await aiSendMessage({
        scene,
        session_id: sessionId,
        message: rawMsg,
        note_path: notePath,
        context_scope: contextScope,
        selected_packet_ids:
          selectedPacketIds.length > 0 ? selectedPacketIds : undefined,
      });

      requestIdRef.current = result.request_id;
      if (result.session_id) {
        setSessionId(result.session_id);
      }

      const toolCalls = result.tool_calls?.map(
        (tc: { function: { name: string }; id: string }) => ({
          id: tc.id,
          name: tc.function.name,
          status: "pending" as const,
        }),
      );
      const finalContent = result.content?.trim() || streamBuf.current || "";

      setMessages((m) => {
        const copy = [...m];
        const last = copy[copy.length - 1];
        if (last?.role === "assistant") {
          copy[copy.length - 1] = {
            ...last,
            content: finalContent,
            toolCalls,
          };
        } else {
          copy.push({
            role: "assistant",
            content: finalContent,
            toolCalls,
          });
        }
        return copy;
      });

      setStreaming(false);
    } catch (e) {
      setStreaming(false);
      setContextStatusData(null);
      setMessages((m) => [
        ...m,
        {
          role: "system",
          content: `错误: ${invokeErrorMessage(e)}`,
        },
      ]);
    } finally {
      panelSendActiveRef.current = false;
      requestIdRef.current = null;
      streamBuf.current = "";
    }
  }, [
    input,
    streaming,
    scene,
    sessionId,
    notePath,
    contextScope,
    selectedPacketIds,
    assembleContext,
  ]);

  const stopStreaming = useCallback(() => {
    const id = requestIdRef.current;
    if (id) {
      void llmAbort(id);
    }
    panelSendActiveRef.current = false;
    setStreaming(false);
    streamBuf.current = "";
    requestIdRef.current = null;
  }, []);

  const handleToolConfirm = useCallback(
    async (
      requestId: string,
      toolCallId: string,
      decision: "approve" | "reject" | "modify",
      modifiedArgs?: unknown,
    ) => {
      try {
        await toolConfirmIpc({
          request_id: requestId,
          tool_call_id: toolCallId,
          decision,
          modified_args: modifiedArgs,
        });
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
    [],
  );

  const togglePacketSelection = useCallback((id: string) => {
    setSelectedPacketIds((prev) =>
      prev.includes(id) ? prev.filter((pid) => pid !== id) : [...prev, id],
    );
  }, []);

  const handlePlanApprove = useCallback(() => {
    setExecutionPlan(null);
    void send();
  }, [send]);

  const handlePlanModify = useCallback(() => {
    setExecutionPlan(null);
  }, []);

  const sceneLabel = SCENE_META[scene].label;

  return (
    <div className="flex h-full flex-col bg-panel">
      <AiPanelHeader scene={scene} onSceneChange={setScene} />

      <ContextStatusBar
        scene={scene}
        contextStatus={contextStatusData}
        noteDisplayTitle={noteDisplayTitle}
        totalPackets={packets.length}
        corpusNames={corpusNames}
      />

      <ContextPacketDrawer
        open={packetsOpen}
        onOpenChange={setPacketsOpen}
        packets={packets}
        selectedIds={selectedPacketIds}
        onSelect={togglePacketSelection}
      />

      {executionPlan && (
        <div className="px-4 py-2">
          <ExecutionPlanPreview
            plan={executionPlan}
            onApprove={handlePlanApprove}
            onModify={handlePlanModify}
          />
        </div>
      )}

      <AiMessageList messages={messages} streaming={streaming} />

      <ContextScopeChips tokens={mentionTokens} onRemove={removeMentionToken} />

      <AiComposer
        value={input}
        streaming={streaming}
        disabled={streaming}
        placeholder={`向${sceneLabel}提问… 输入 @ 指定范围`}
        textareaRef={textareaRef}
        onComposerKeyDown={handleComposerKeyDown}
        onSelect={syncMentionFromInput}
        onChange={setInput}
        onSubmit={() => void send()}
        onStop={stopStreaming}
        mentionPopover={
          <AiMentionPopover
            open={mentionOpen}
            query={mentionQuery}
            candidates={mentionCandidates}
            highlight={mentionHighlight}
            onHighlight={setMentionHighlight}
            navDeltaRef={mentionNavDeltaRef}
            onSelect={selectMention}
          />
        }
      />

      <ToolConfirmDialog
        request={toolConfirmRequest}
        onConfirm={handleToolConfirm}
        onClose={() => setToolConfirmRequest(null)}
      />
    </div>
  );
}
