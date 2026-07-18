import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type RefObject,
  type SetStateAction,
} from "react";

import { useToast } from "@/components/ui/use-toast";
import {
  compactChatLinesForState,
  getAiPayloadStore,
  releaseChatLinePayloadRefs,
  restoreChatLineContent,
  restoreChatLinesForPersistence,
} from "@/lib/ai-payload-store";
import { assistantSessionRetract } from "@/lib/ipc";
import type {
  AssistantRunAccepted,
  AssistantSessionRef,
  DisplayMention,
} from "@/types/ai";

import type { ChatLine, ImageAttachment } from "../AiMessageList";

const MAX_CONVERSATION_UI_MESSAGES = 240;

// Keep user-facing toast literals ASCII-only in source: a legacy WebView code-page decode
// must not turn UTF-8 string literals into mojibake before React receives them.
const COPY_SELECTED_SUCCESS_TOAST = "\u5df2\u590d\u5236\u9009\u4e2d\u6d88\u606f";
const COPY_SELECTED_FAILURE_TOAST = "\u590d\u5236\u5931\u8d25";
const RETRACT_SYNC_FAILURE_TOAST =
  "\u64a4\u56de\u672a\u540c\u6b65\u5230\u4f1a\u8bdd\u8bb0\u5f55";

interface BubbleSelectionPort {
  selected: Set<number>;
  clear: () => void;
}

interface UseAssistantConversationParams {
  bubbleSelection: BubbleSelectionPort;
  clearContextReferences: () => void;
  clearTaskSurfaces: () => void;
  onInsertToEditor?: (content: string) => void;
  setInput: Dispatch<SetStateAction<string>>;
  setStreaming: Dispatch<SetStateAction<boolean>>;
  textareaRef: RefObject<HTMLTextAreaElement | null>;
}

function selectedMessages(
  messages: ChatLine[],
  selected: Set<number>,
): ChatLine[] {
  return Array.from(selected)
    .sort((left, right) => left - right)
    .map((index) => messages[index])
    .filter((message): message is ChatLine => message != null);
}

function exportContent(message: ChatLine): string {
  const content = restoreChatLineContent(message);
  return message.role === "user" ? `> ${content}` : content;
}

/** Presentation-only conversation state backed by opaque unified session references. */
export function useAssistantConversation({
  bubbleSelection,
  clearContextReferences,
  clearTaskSurfaces,
  onInsertToEditor,
  setInput,
  setStreaming,
  textareaRef,
}: UseAssistantConversationParams) {
  const payloadStoreRef = useRef(getAiPayloadStore());
  const [messages, setMessagesState] = useState<ChatLine[]>([]);
  const [runSession, setRunSession] = useState<AssistantSessionRef | null>(
    null,
  );
  const toast = useToast();
  const messagesRef = useRef(messages);
  messagesRef.current = messages;

  const setMessages: Dispatch<SetStateAction<ChatLine[]>> = useCallback(
    (action) => {
      setMessagesState((previous) => {
        const next = typeof action === "function" ? action(previous) : action;
        const bounded =
          next.length > MAX_CONVERSATION_UI_MESSAGES
            ? next.slice(-MAX_CONVERSATION_UI_MESSAGES)
            : next;
        return compactChatLinesForState(
          payloadStoreRef.current,
          bounded,
          previous,
        );
      });
    },
    [],
  );

  useEffect(() => {
    const store = payloadStoreRef.current;
    return () => releaseChatLinePayloadRefs(store, messagesRef.current);
  }, []);

  const handleNewChat = useCallback(() => {
    clearTaskSurfaces();
    clearContextReferences();
    bubbleSelection.clear();
    setMessages([]);
    setRunSession(null);
    setInput("");
    setStreaming(false);
  }, [
    bubbleSelection,
    clearContextReferences,
    clearTaskSurfaces,
    setInput,
    setMessages,
    setStreaming,
  ]);

  const handleRetract = useCallback(
    async (index: number) => {
      const target = messagesRef.current[index];
      if (!target) return;
      if (runSession && target.seq != null) {
        try {
          await assistantSessionRetract({
            session: runSession,
            fromSeq: target.seq,
          });
        } catch {
          toast(RETRACT_SYNC_FAILURE_TOAST, { tone: "error" });
          return;
        }
      }
      setMessages((previous) => previous.slice(0, index));
      bubbleSelection.clear();
    },
    [bubbleSelection, runSession, setMessages, toast],
  );

  const commitAcceptedTurn = useCallback(
    (
      rawMessage: string,
      accepted: AssistantRunAccepted,
      images?: ImageAttachment[],
      displayMentions?: DisplayMention[],
    ) => {
      const user: ChatLine = {
        role: "user",
        content: rawMessage,
        clientRequestId: accepted.clientRequestId,
        runId: accepted.runId,
        turnId: accepted.turnId,
      };
      if (displayMentions?.length) user.displayMentions = displayMentions;
      if (images?.length) user.images = images;
      const assistant: ChatLine = {
        role: "assistant",
        content: "",
        clientRequestId: accepted.clientRequestId,
        runId: accepted.runId,
        turnId: accepted.turnId,
      };
      setMessages((previous) => {
        if (
          previous.some(
            (message) =>
              message.clientRequestId === accepted.clientRequestId ||
              message.runId === accepted.runId,
          )
        ) {
          return previous;
        }
        return [...previous, user, assistant];
      });
    },
    [setMessages],
  );

  const appendAcceptedRetry = useCallback(
    (accepted: AssistantRunAccepted) => {
      setMessages((previous) => {
        if (previous.some((message) => message.runId === accepted.runId)) {
          return previous;
        }
        return [
          ...previous,
          {
            role: "assistant",
            content: "",
            clientRequestId: accepted.clientRequestId,
            runId: accepted.runId,
            turnId: accepted.turnId,
          },
        ];
      });
    },
    [setMessages],
  );

  const handleQuoteToInput = useCallback(
    (text: string) => {
      const quote = text
        .split("\n")
        .map((line) => `> ${line}`)
        .join("\n");
      setInput((previous) =>
        previous.trim() ? `${previous.trim()}\n\n${quote}\n\n` : `${quote}\n\n`,
      );
      textareaRef.current?.focus();
    },
    [setInput, textareaRef],
  );

  const handleLoadSession = useCallback(
    (session: AssistantSessionRef, loaded: ChatLine[]) => {
      setRunSession(session);
      setMessages(
        restoreChatLinesForPersistence(loaded, payloadStoreRef.current),
      );
      bubbleSelection.clear();
      clearTaskSurfaces();
      clearContextReferences();
      setStreaming(false);
    },
    [
      bubbleSelection,
      clearContextReferences,
      clearTaskSurfaces,
      setMessages,
      setStreaming,
    ],
  );

  const handleCopySelected = useCallback(async () => {
    const content = selectedMessages(
      messagesRef.current,
      bubbleSelection.selected,
    )
      .map(exportContent)
      .join("\n\n");
    if (!content) return;
    try {
      await navigator.clipboard.writeText(content);
      toast(COPY_SELECTED_SUCCESS_TOAST, { tone: "success" });
    } catch {
      toast(COPY_SELECTED_FAILURE_TOAST, { tone: "error" });
    }
    bubbleSelection.clear();
  }, [bubbleSelection, toast]);

  const handleExportSelected = useCallback(() => {
    const content = selectedMessages(
      messagesRef.current,
      bubbleSelection.selected,
    )
      .map((message) => `## ${message.role}\n\n${exportContent(message)}`)
      .join("\n\n---\n\n");
    if (!content) return;
    const blob = new Blob([content], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `iris-export-${new Date().toISOString().slice(0, 10)}.md`;
    anchor.click();
    URL.revokeObjectURL(url);
    bubbleSelection.clear();
  }, [bubbleSelection]);

  const handleInsertToEditor = useCallback(() => {
    if (!onInsertToEditor) return;
    const content = selectedMessages(
      messagesRef.current,
      bubbleSelection.selected,
    )
      .map(exportContent)
      .join("\n\n");
    if (!content) return;
    onInsertToEditor(content);
    bubbleSelection.clear();
  }, [bubbleSelection, onInsertToEditor]);

  return {
    appendAcceptedRetry,
    commitAcceptedTurn,
    handleCopySelected,
    handleExportSelected,
    handleInsertToEditor,
    handleLoadSession,
    handleNewChat,
    handleQuoteToInput,
    handleRetract,
    messages,
    runSession,
    setMessages,
    setRunSession,
  };
}
