import { useEffect, useRef, type Dispatch, type SetStateAction } from "react";

import type { ChatLine } from "../AiMessageList";
import type { AssistantRunEvent } from "@/types/ai";

export interface AssistantRunTranscriptOptions {
  event: AssistantRunEvent | null;
  setMessages: Dispatch<SetStateAction<ChatLine[]>>;
  setStreaming: (streaming: boolean) => void;
  setActivityHint: (hint: string | null) => void;
  setError: (message: string | null) => void;
}

/** Projects persisted Run events into the local, presentation-only transcript. */
export function useAssistantRunTranscript({
  event,
  setMessages,
  setStreaming,
  setActivityHint,
  setError,
}: AssistantRunTranscriptOptions) {
  const appliedEventRef = useRef<string | null>(null);

  useEffect(() => {
    if (!event) return;
    const key = `${event.runId}:${event.seq}`;
    if (appliedEventRef.current === key) return;
    appliedEventRef.current = key;

    switch (event.type) {
      case "stage_changed":
        setActivityHint(event.payload.stage);
        return;
      case "content_delta":
        setMessages((previous) => {
          const last = previous.at(-1);
          if (!last || last.role !== "assistant") return previous;
          return [
            ...previous.slice(0, -1),
            { ...last, content: `${last.content}${event.payload.delta}` },
          ];
        });
        return;
      case "completed":
        setStreaming(false);
        setActivityHint(null);
        return;
      case "failed":
        setStreaming(false);
        setActivityHint(null);
        setError(event.payload.message);
        return;
      case "cancelled":
        setStreaming(false);
        setActivityHint(null);
        setMessages((previous) => {
          const last = previous.at(-1);
          if (!last || last.role !== "assistant" || last.content.trim()) {
            return [
              ...previous,
              { role: "system", content: "本次回答已取消。" },
            ];
          }
          return [
            ...previous.slice(0, -1),
            { role: "system", content: "本次回答已取消。" },
          ];
        });
        return;
      default:
        return;
    }
  }, [event, setActivityHint, setError, setMessages, setStreaming]);
}
