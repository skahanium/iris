import {
  useCallback,
  useRef,
  useState,
  type Dispatch,
  type MutableRefObject,
  type RefObject,
  type SetStateAction,
} from "react";

import { stripMentionTokensForDisplay } from "@/lib/ai-context-scope";
import { llmAbort, sessionRetract } from "@/lib/ipc";
import type {
  AssistantActionState,
  AssistantIntent,
  ContextPacket,
  TokenUsage,
} from "@/types/ai";

import type { ChatLine } from "../AiMessageList";
import {
  buildActionState,
  buildTaskSummary,
} from "../unified-assistant-panel-utils";

interface BubbleSelectionPort {
  selected: Set<number>;
  clear: () => void;
}

interface UseAssistantConversationParams {
  actionIntent: AssistantIntent;
  bubbleSelection: BubbleSelectionPort;
  clearCitationMiss: () => void;
  clearTaskSurfaces: () => void;
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

export function useAssistantConversation({
  actionIntent,
  bubbleSelection,
  clearCitationMiss,
  clearTaskSurfaces,
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
  const [messages, setMessages] = useState<ChatLine[]>([]);
  const [sessionId, setSessionId] = useState<number | null>(null);
  const [sessionTokenUsage, setSessionTokenUsage] = useState<TokenUsage | null>(
    null,
  );
  const messagesRef = useRef(messages);
  messagesRef.current = messages;
  const sessionIdRef = useRef(sessionId);
  sessionIdRef.current = sessionId;

  const abortLlm = deps?.abortLlm ?? llmAbort;
  const retractSession = deps?.retractSession ?? sessionRetract;

  const handleNewChat = useCallback(() => {
    clearTaskSurfaces();
    clearCitationMiss();
    setPackets([]);
    setSelectedPacketIds([]);
    setMessages([]);
    setSessionId(null);
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
    clearTaskSurfaces,
    forceNewSessionRef,
    requestIdRef,
    setActionState,
    setActivityHint,
    setHarnessRequestId,
    setInput,
    setPackets,
    setSelectedPacketIds,
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
      const sid = sessionIdRef.current;
      const seq = target.seq;
      if (sid && seq) {
        try {
          await retractSession(sid, seq);
        } catch (err) {
          console.warn("[retract] backend failed:", err);
        }
      }
      setMessages((prev) => prev.slice(0, index));
    },
    [abortLlm, requestIdRef, retractSession, setStreaming, streaming],
  );

  const handleInsertToEditor = useCallback(() => {
    if (!onInsertToEditor) return;
    const content = selectedMessages(
      messagesRef.current,
      bubbleSelection.selected,
    )
      .map((message) => {
        if (message.role === "user") return `> ${message.content}`;
        return message.content;
      })
      .join("\n\n");
    if (content) {
      onInsertToEditor(content);
      bubbleSelection.clear();
    }
  }, [bubbleSelection, onInsertToEditor]);

  const handleCopySelected = useCallback(async () => {
    const content = selectedMessages(
      messagesRef.current,
      bubbleSelection.selected,
    )
      .map((message) => message.content)
      .join("\n\n");
    if (content) {
      try {
        await navigator.clipboard.writeText(content);
      } catch {
        /* ignore */
      }
      bubbleSelection.clear();
    }
  }, [bubbleSelection]);

  const handleExportSelected = useCallback(() => {
    const lines = selectedMessages(
      messagesRef.current,
      bubbleSelection.selected,
    ).map((message) => {
      if (message.role === "user") return `## 用户\n\n${message.content}`;
      if (message.role === "assistant") {
        return `## 助手\n\n${message.content}`;
      }
      return `## ${message.role}\n\n${message.content}`;
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
    (rawMessage: string, imgs?: import("../AiMessageList").ImageAttachment[]) => {
      const display = stripMentionTokensForDisplay(rawMessage);
      setMessages((prev) => [
        ...prev,
        {
          role: "user",
          content: imgs?.length ? `[图片] ${display}` : display,
          images: imgs?.length ? imgs : undefined,
        },
      ]);
    },
    [],
  );

  const ensureAssistantStreamSlot = useCallback(() => {
    setMessages((prev) => {
      const last = prev[prev.length - 1];
      if (last?.role === "assistant") return prev;
      return [...prev, { role: "assistant", content: "" }];
    });
  }, []);

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
    [],
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
    (id: number, loaded: ChatLine[]) => {
      setSessionId(id);
      setMessages(loaded);
      forceNewSessionRef.current = false;
      clearTaskSurfaces();
      clearCitationMiss();
      setActionState(buildActionState(actionIntent, "idle"));
    },
    [
      actionIntent,
      clearCitationMiss,
      clearTaskSurfaces,
      forceNewSessionRef,
      setActionState,
    ],
  );

  return {
    appendAssistantSummary,
    appendUserMessage,
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
    sessionId,
    sessionIdRef,
    sessionTokenUsage,
    setMessages,
    setSessionId,
    setSessionTokenUsage,
  };
}
