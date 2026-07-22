import { useEffect, useRef, type Dispatch, type SetStateAction } from "react";

import type { ChatLine } from "../AiMessageList";
import { projectAssistantProcessEvents } from "@/lib/assistant-process";
import type { AssistantRunEventState } from "@/lib/assistant-run-events";
import type { ClassifiedRunResultRequest } from "@/types/ai";

export interface AssistantRunTranscriptOptions {
  run: AssistantRunEventState | null;
  messages: readonly ChatLine[];
  setMessages: Dispatch<SetStateAction<ChatLine[]>>;
  setStreaming: (streaming: boolean) => void;
  setActivityHint: (hint: string | null) => void;
  setError: (message: string | null) => void;
  classifiedContextRef?: string | null;
  takeClassifiedResult?: (
    request: ClassifiedRunResultRequest,
  ) => Promise<string>;
}

/** Projects persisted Run events into the local, presentation-only transcript. */
export function useAssistantRunTranscript({
  run,
  messages,
  setMessages,
  setStreaming,
  setActivityHint,
  setError,
  classifiedContextRef,
  takeClassifiedResult,
}: AssistantRunTranscriptOptions) {
  const appliedEventRef = useRef<string | null>(null);

  useEffect(() => {
    if (!run || run.lastSeq === 0) return;
    if (
      !messages.some(
        (message) =>
          message.role === "assistant" && message.runId === run.runId,
      )
    ) {
      return;
    }
    const event = run.events.at(-1);
    if (!event) return;
    const key = `${run.runId}:${run.lastSeq}:${run.transientRevision}`;
    if (appliedEventRef.current === key) return;
    appliedEventRef.current = key;

    setMessages((previous) => {
      const index = previous.findIndex(
        (message) =>
          message.role === "assistant" && message.runId === run.runId,
      );
      if (index < 0) return previous;
      const processItems = projectAssistantProcessEvents(
        run.events,
        run.reasoningSummaries,
      );
      const current = previous[index];
      if (
        current?.content === run.content &&
        sameProcessItems(current.processItems, processItems)
      ) {
        return previous;
      }
      return previous.map((message, messageIndex) =>
        messageIndex === index
          ? { ...message, content: run.content, processItems }
          : message,
      );
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
        if (
          run.events.at(-1)?.payload.kind === "completed" &&
          classifiedContextRef &&
          takeClassifiedResult
        ) {
          void takeClassifiedResult({
            runId: run.runId,
            contextRef: classifiedContextRef,
          })
            .then((content) => {
              setMessages((previous) => {
                return previous.map((message) =>
                  message.role === "assistant" && message.runId === run.runId
                    ? { ...message, content }
                    : message,
                );
              });
            })
            .catch(() => {
              setError("涉密回答已失效；请重新附带当前文档后重试。");
            });
        }
        return;
      case "failed":
        setStreaming(false);
        setActivityHint(null);
        setMessages((previous) =>
          previous.filter(
            (message) =>
              !(
                message.role === "assistant" &&
                message.runId === run.runId &&
                !message.content.trim()
              ),
          ),
        );
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
          const index = previous.findIndex(
            (message) =>
              message.role === "assistant" && message.runId === run.runId,
          );
          const target = previous[index];
          if (!target || target.content.trim()) {
            return [
              ...previous,
              { role: "system", content: "本次回答已取消。" },
            ];
          }
          return previous.map((message, messageIndex) =>
            messageIndex === index
              ? { role: "system", content: "本次回答已取消。" }
              : message,
          );
        });
        return;
      default:
        return;
    }
  }, [
    classifiedContextRef,
    messages,
    run,
    setActivityHint,
    setError,
    setMessages,
    setStreaming,
    takeClassifiedResult,
  ]);
}

function sameProcessItems(
  left: ChatLine["processItems"],
  right: ChatLine["processItems"],
): boolean {
  if (left === right) return true;
  if (!left || !right || left.length !== right.length) return false;
  return left.every(
    (item, index) =>
      item.id === right[index]?.id &&
      item.label === right[index]?.label &&
      item.status === right[index]?.status &&
      item.durationMs === right[index]?.durationMs,
  );
}
