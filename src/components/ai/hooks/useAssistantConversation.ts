import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type MutableRefObject,
  type RefObject,
  type SetStateAction,
} from "react";

import { useToast } from "@/components/ui/use-toast";
import {
  parseMentionTokens,
  stripMentionTokensForDisplay,
} from "@/lib/ai-context-scope";
import {
  citationRecordsFromContextPackets,
  replaceAiCitationsForDocument,
} from "@/lib/ai/evidence-citations";
import {
  compactChatLinesForState,
  getAiPayloadStore,
  releaseChatLinePayloadRefs,
  restoreChatLineContent,
  restoreChatLinesForPersistence,
} from "@/lib/ai-payload-store";
import { mergeContextPackets } from "@/lib/ai/merge-context-packets";
import {
  classifiedAiThreadLoad,
  classifiedAiThreadSave,
  llmAbort,
  sessionRetract,
} from "@/lib/ipc";
import type {
  AssistantActionState,
  AssistantIntent,
  ContextPacket,
  TokenUsage,
} from "@/types/ai";
import type { ClassifiedAiThread, ClassifiedAiMessage } from "@/types/ipc";

import type { ChatLine } from "../AiMessageList";

import {
  buildActionState,
  buildTaskSummary,
} from "../unified-assistant-panel-utils";

const MAX_CONVERSATION_UI_MESSAGES = 240;

interface BubbleSelectionPort {
  selected: Set<number>;
  clear: () => void;
}

interface UseAssistantConversationParams {
  actionIntent: AssistantIntent;
  aiDomain?: "normal" | "classified";
  bubbleSelection: BubbleSelectionPort;
  clearCitationMiss: () => void;
  clearContextReferences: () => void;
  clearTaskSurfaces: () => void;
  documentPath?: string;
  forceNewSessionRef: MutableRefObject<boolean>;
  onInsertToEditor?: (content: string) => void;
  requestIdRef: MutableRefObject<string | null>;
  setActionState: Dispatch<SetStateAction<AssistantActionState>>;
  setActivityHint: Dispatch<SetStateAction<string | null>>;
  setHarnessRequestId: Dispatch<SetStateAction<string | null>>;
  setInput: Dispatch<SetStateAction<string>>;
  setPackets: Dispatch<SetStateAction<ContextPacket[]>>;
  setSelectedPacketIds: Dispatch<SetStateAction<string[]>>;
  setStreaming: Dispatch<SetStateAction<boolean>>;
  streamBufRef: MutableRefObject<string>;
  textareaRef: RefObject<HTMLTextAreaElement | null>;
  streaming?: boolean;
  deps?: {
    abortLlm?: (requestId: string) => Promise<unknown>;
    retractSession?: (sessionId: number, seq: number) => Promise<unknown>;
  };
}

function selectedMessages(
  messages: ChatLine[],
  selected: Set<number>,
): ChatLine[] {
  return Array.from(selected)
    .sort((a, b) => a - b)
    .map((index) => messages[index])
    .filter((message): message is ChatLine => message != null);
}

interface DocumentContentResult {
  content: string;
  missing: string[];
}

function documentContentForMessage(message: ChatLine): DocumentContentResult {
  if (message.role === "user") {
    return { content: `> ${restoreChatLineContent(message)}`, missing: [] };
  }
  if (message.role !== "assistant") {
    return { content: restoreChatLineContent(message), missing: [] };
  }
  const result = replaceAiCitationsForDocument(
    restoreChatLineContent(message),
    citationRecordsFromContextPackets(message.evidencePackets),
  );
  return { content: result.markdown, missing: result.missing };
}

function documentContentForMessages(
  messages: ChatLine[],
): DocumentContentResult {
  const converted = messages.map(documentContentForMessage);
  return {
    content: converted.map((item) => item.content).join("\n\n"),
    missing: Array.from(new Set(converted.flatMap((item) => item.missing))),
  };
}

function warnMissingCitations(
  missing: string[],
  setActivityHint: Dispatch<SetStateAction<string | null>>,
) {
  if (missing.length > 0) {
    setActivityHint(`有引用未找到：${missing.join("、")}`);
  }
}

export function useAssistantConversation({
  actionIntent,
  aiDomain = "normal",
  bubbleSelection,
  clearCitationMiss,
  clearContextReferences,
  clearTaskSurfaces,
  documentPath,
  forceNewSessionRef,
  onInsertToEditor,
  requestIdRef,
  setActionState,
  setActivityHint,
  setHarnessRequestId,
  setInput,
  setPackets,
  setSelectedPacketIds,
  setStreaming,
  streamBufRef,
  textareaRef,
  streaming = false,
  deps,
}: UseAssistantConversationParams) {
  const payloadStoreRef = useRef(getAiPayloadStore());
  const [messages, setMessagesState] = useState<ChatLine[]>([]);
  const setMessages: Dispatch<SetStateAction<ChatLine[]>> = useCallback(
    (action) => {
      setMessagesState((prev) => {
        const next = typeof action === "function" ? action(prev) : action;
        const boundedNext =
          next.length > MAX_CONVERSATION_UI_MESSAGES
            ? next.slice(-MAX_CONVERSATION_UI_MESSAGES)
            : next;
        return compactChatLinesForState(
          payloadStoreRef.current,
          boundedNext,
          prev,
        );
      });
    },
    [],
  );
  const [sessionId, setSessionId] = useState<number | null>(null);
  const [classifiedThreadId, setClassifiedThreadId] = useState<string | null>(
    null,
  );
  const [sessionTokenUsage, setSessionTokenUsage] = useState<TokenUsage | null>(
    null,
  );
  const toast = useToast();
  const messagesRef = useRef(messages);
  messagesRef.current = messages;
  const sessionIdRef = useRef(sessionId);
  sessionIdRef.current = sessionId;

  useEffect(() => {
    const store = payloadStoreRef.current;
    return () => {
      releaseChatLinePayloadRefs(store, messagesRef.current);
    };
  }, []);
  const classifiedThreadIdRef = useRef(classifiedThreadId);
  classifiedThreadIdRef.current = classifiedThreadId;

  const abortLlm = deps?.abortLlm ?? llmAbort;
  const retractSession = deps?.retractSession ?? sessionRetract;

  const handleNewChat = useCallback(() => {
    clearTaskSurfaces();
    clearContextReferences();
    clearCitationMiss();
    setPackets([]);
    setSelectedPacketIds([]);
    setMessages([]);
    setSessionId(null);
    setClassifiedThreadId(null);
    setSessionTokenUsage(null);
    setInput("");
    setActivityHint(null);
    setStreaming(false);
    streamBufRef.current = "";
    requestIdRef.current = null;
    setHarnessRequestId(null);
    forceNewSessionRef.current = true;
    setActionState(buildActionState("chat", "idle"));
  }, [
    clearCitationMiss,
    clearContextReferences,
    clearTaskSurfaces,
    forceNewSessionRef,
    requestIdRef,
    setActionState,
    setActivityHint,
    setHarnessRequestId,
    setInput,
    setPackets,
    setSelectedPacketIds,
    setMessages,
    setStreaming,
    streamBufRef,
  ]);

  const handleRetract = useCallback(
    async (index: number) => {
      const target = messagesRef.current[index];
      if (!target) return;
      if (streaming && requestIdRef.current) {
        try {
          await abortLlm(requestIdRef.current);
        } catch {
          /* ignore */
        }
        setStreaming(false);
      }

      if (aiDomain === "classified") {
        // In classified domain, update the encrypted thread
        const threadId = classifiedThreadIdRef.current;
        if (threadId) {
          try {
            const thread = await classifiedAiThreadLoad(threadId);
            const updatedMessages = thread.messages.slice(0, index);
            await classifiedAiThreadSave({
              ...thread,
              messages: updatedMessages,
              updatedAt: new Date().toISOString(),
            });
          } catch (err) {
            console.warn("[retract] classified thread update failed:", err);
          }
        }
      } else {
        const sid = sessionIdRef.current;
        const seq = target.seq;
        if (sid && seq) {
          try {
            await retractSession(sid, seq);
          } catch (err) {
            console.warn("[retract] backend failed:", err);
          }
        }
      }

      setMessages((prev) => prev.slice(0, index));
    },
    [
      abortLlm,
      aiDomain,
      classifiedThreadIdRef,
      requestIdRef,
      retractSession,
      setMessages,
      setStreaming,
      streaming,
    ],
  );

  const handleInsertToEditor = useCallback(() => {
    if (!onInsertToEditor) return;
    const result = documentContentForMessages(
      selectedMessages(messagesRef.current, bubbleSelection.selected),
    );
    if (result.content) {
      onInsertToEditor(result.content);
      warnMissingCitations(result.missing, setActivityHint);
      bubbleSelection.clear();
    }
  }, [bubbleSelection, onInsertToEditor, setActivityHint]);

  const handleCopySelected = useCallback(async () => {
    const result = documentContentForMessages(
      selectedMessages(messagesRef.current, bubbleSelection.selected),
    );
    if (result.content) {
      try {
        await navigator.clipboard.writeText(result.content);
        toast("已复制选中消息", { tone: "success" });
        warnMissingCitations(result.missing, setActivityHint);
      } catch {
        toast("复制失败", { tone: "error" });
      }
      bubbleSelection.clear();
    }
  }, [bubbleSelection, setActivityHint, toast]);

  const handleExportSelected = useCallback(() => {
    const lines = selectedMessages(
      messagesRef.current,
      bubbleSelection.selected,
    ).map((message) => {
      if (message.role === "user")
        return `## 用户\n\n${restoreChatLineContent(message)}`;
      if (message.role === "assistant") {
        return `## 助手\n\n${restoreChatLineContent(message)}`;
      }
      return `## ${message.role}\n\n${restoreChatLineContent(message)}`;
    });
    if (lines.length === 0) return;
    const md = lines.join("\n\n---\n\n");
    const blob = new Blob([md], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `iris-export-${new Date().toISOString().slice(0, 10)}.md`;
    a.click();
    URL.revokeObjectURL(url);
    bubbleSelection.clear();
  }, [bubbleSelection]);

  const appendUserMessage = useCallback(
    (
      rawMessage: string,
      imgs?: import("../AiMessageList").ImageAttachment[],
    ) => {
      const display = stripMentionTokensForDisplay(rawMessage);
      const mentions = parseMentionTokens(rawMessage);
      const nextMessage: ChatLine = {
        role: "user",
        content: display,
      };

      if (mentions.length > 0) {
        nextMessage.mentions = mentions;
      }

      if (imgs?.length) {
        nextMessage.images = imgs;
      }

      setMessages((prev) => [...prev, nextMessage]);
    },
    [setMessages],
  );

  const ensureAssistantStreamSlot = useCallback(() => {
    setMessages((prev) => {
      const last = prev[prev.length - 1];
      if (last?.role === "assistant") return prev;
      return [...prev, { role: "assistant", content: "" }];
    });
  }, [setMessages]);

  const appendAssistantSummary = useCallback(
    (intent: AssistantIntent, count?: number) => {
      setMessages((prev) => [
        ...prev,
        {
          role: "assistant",
          content: buildTaskSummary(intent, count),
        },
      ]);
    },
    [setMessages],
  );

  const handleQuoteToInput = useCallback(
    (text: string) => {
      const quoted = text
        .split("\n")
        .map((line) => `> ${line}`)
        .join("\n");
      setInput((prev) =>
        prev.trim() ? `${prev.trim()}\n\n${quoted}\n\n` : `${quoted}\n\n`,
      );
      textareaRef.current?.focus();
    },
    [setInput, textareaRef],
  );

  const handleLoadSession = useCallback(
    (
      id: number | string,
      loaded: ChatLine[],
      ledgerPackets?: ContextPacket[],
    ) => {
      const messagePackets = loaded.flatMap(
        (message) => message.evidencePackets ?? [],
      );
      const loadedPackets = mergeContextPackets(messagePackets, ledgerPackets);

      if (aiDomain === "classified") {
        setClassifiedThreadId(id as string);
        setSessionId(null);
      } else {
        setSessionId(id as number);
        setClassifiedThreadId(null);
      }

      setMessages(
        restoreChatLinesForPersistence(loaded, payloadStoreRef.current),
      );
      setPackets(loadedPackets);
      setSelectedPacketIds([]);
      forceNewSessionRef.current = false;
      clearTaskSurfaces();
      clearContextReferences();
      clearCitationMiss();
      setActionState(buildActionState(actionIntent, "idle"));
    },
    [
      actionIntent,
      aiDomain,
      clearCitationMiss,
      clearContextReferences,
      clearTaskSurfaces,
      forceNewSessionRef,
      setActionState,
      setMessages,
      setPackets,
      setSelectedPacketIds,
    ],
  );

  const saveClassifiedThread = useCallback(
    async (messagesToSave: ChatLine[]) => {
      if (aiDomain !== "classified" || !documentPath) return;

      const now = new Date().toISOString();
      const threadId = classifiedThreadIdRef.current ?? crypto.randomUUID();

      const classifiedMessages: ClassifiedAiMessage[] = messagesToSave.map(
        (msg, idx) => ({
          seq: idx + 1,
          role: msg.role,
          content: restoreChatLineContent(msg),
          contentParts: msg.images?.length
            ? msg.images.map((img) => ({
                type: "image_url",
                image_url: {
                  url: `data:${img.mimeType};base64,${img.dataBase64}`,
                  detail: "auto",
                },
              }))
            : undefined,
          createdAt: now,
        }),
      );

      const thread: ClassifiedAiThread = {
        version: 1,
        threadId,
        documentPath,
        title: null,
        createdAt: now,
        updatedAt: now,
        messages: classifiedMessages,
        evidencePackets: [],
        tokenUsage: null,
      };

      await classifiedAiThreadSave(thread);
      if (!classifiedThreadIdRef.current) {
        setClassifiedThreadId(threadId);
      }
    },
    [aiDomain, classifiedThreadIdRef, documentPath],
  );

  return {
    appendAssistantSummary,
    appendUserMessage,
    classifiedThreadId,
    ensureAssistantStreamSlot,
    handleCopySelected,
    handleExportSelected,
    handleInsertToEditor,
    handleLoadSession,
    handleNewChat,
    handleQuoteToInput,
    handleRetract,
    messages,
    messagesRef,
    saveClassifiedThread,
    sessionId,
    sessionIdRef,
    sessionTokenUsage,
    setMessages,
    setSessionId,
    setSessionTokenUsage,
  };
}
