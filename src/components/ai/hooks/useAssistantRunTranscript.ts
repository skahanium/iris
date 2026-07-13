import { useEffect, useRef, type Dispatch, type SetStateAction } from "react";

import type { ChatLine } from "../AiMessageList";
import type { AssistantRunEventState } from "@/lib/assistant-run-events";

export interface AssistantRunTranscriptOptions {
  run: AssistantRunEventState | null;
  setMessages: Dispatch<SetStateAction<ChatLine[]>>;
  setStreaming: (streaming: boolean) => void;
  setActivityHint: (hint: string | null) => void;
  setError: (message: string | null) => void;
}

/** Projects persisted Run events into the local, presentation-only transcript. */
export function useAssistantRunTranscript({
  run,
  setMessages,
  setStreaming,
  setActivityHint,
  setError,
}: AssistantRunTranscriptOptions) {
  const appliedEventRef = useRef<string | null>(null);

  useEffect(() => {
    if (!run || run.lastSeq === 0) return;
    const event = run.events.at(-1);
    if (!event) return;
    const key = `${run.runId}:${run.lastSeq}`;
    if (appliedEventRef.current === key) return;
    appliedEventRef.current = key;

    setMessages((previous) => {
      const last = previous.at(-1);
      if (!last || last.role !== "assistant") return previous;
      if (last.content === run.content) return previous;
      return [...previous.slice(0, -1), { ...last, content: run.content }];
    });

    setActivityHint(run.stage);
    switch (run.state) {
      case "accepted":
      case "preparing":
      case "running":
      case "verifying":
        setStreaming(true);
        return;
      case "awaiting_confirmation":
      case "paused":
        setStreaming(false);
        return;
      case "completed":
        setStreaming(false);
        setActivityHint(null);
        return;
      case "failed":
        setStreaming(false);
        setActivityHint(null);
        setError(
          event.payload.kind === "failed"
            ? event.payload.message
            : "本次运行未能完成。",
        );
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
  }, [run, setActivityHint, setError, setMessages, setStreaming]);
}
