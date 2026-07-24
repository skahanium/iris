/**
 * Converts an ephemeral Run presentation stream into per-message visual state.
 * It intentionally never writes the durable Run state or session history.
 *
 * Presentation events already carry the authoritative live order. This hook
 * mirrors them immediately — it must not invent a second timeline.
 */
import { useEffect, useRef, type Dispatch, type SetStateAction } from "react";

import type { ChatLine } from "../AiMessageList";
import {
  isInternalRuntimeTool,
  type AssistantProcessItem,
} from "@/lib/assistant-process";
import type {
  AssistantPresentationItem,
  AssistantPresentationState,
} from "@/lib/assistant-presentation";
import type { AssistantRunEventState } from "@/lib/assistant-run-events";
import { toolDisplayName } from "@/lib/tool-display-names";
import { sanitizeAssistantVisibleText } from "@/lib/assistant-visible-text";

export interface AssistantPresentationPlaybackOptions {
  presentation: AssistantPresentationState | null;
  /** Authoritative Run facts, used only to stop an unsafe visual playback. */
  run?: AssistantRunEventState | null;
  setMessages: Dispatch<SetStateAction<ChatLine[]>>;
}

export function useAssistantPresentationPlayback({
  presentation,
  run,
  setMessages,
}: AssistantPresentationPlaybackOptions): void {
  const stoppedRunsRef = useRef(new Set<string>());

  useEffect(() => {
    if (!presentation) return;
    if (mustFallBackToDurableFacts(presentation, run ?? null)) {
      stoppedRunsRef.current.add(presentation.runId);
      return;
    }
    if (stoppedRunsRef.current.has(presentation.runId)) {
      stoppedRunsRef.current.delete(presentation.runId);
    }
    const content = sanitizeAssistantVisibleText(presentation.answer);
    const processItems = presentation.processItems
      .filter(
        (item) => item.kind !== "tool" || !isInternalRuntimeTool(item.label),
      )
      .map(toProcessItem);
    const presentationStreaming = !presentation.answerComplete;
    setMessages((previous) => {
      let changed = false;
      const next = previous.map((message) => {
        if (
          message.role !== "assistant" ||
          message.runId !== presentation.runId
        ) {
          return message;
        }
        if (
          message.content === content &&
          sameProcessItems(message.processItems, processItems) &&
          message.presentationStreaming === presentationStreaming
        ) {
          return message;
        }
        changed = true;
        return {
          ...message,
          content,
          processItems,
          presentationStreaming,
        };
      });
      return changed ? next : previous;
    });
  }, [presentation, run, setMessages]);
}

function mustFallBackToDurableFacts(
  presentation: AssistantPresentationState,
  run: AssistantRunEventState | null,
): boolean {
  if (presentation.resyncFromSeq !== null) return true;
  return (
    run?.runId === presentation.runId &&
    ["completed", "failed", "cancelled"].includes(run.state ?? "") &&
    !presentation.answerComplete
  );
}

function toProcessItem(item: AssistantPresentationItem): AssistantProcessItem {
  return {
    id: item.id,
    kind: item.kind,
    label:
      item.kind === "tool"
        ? toolDisplayName(item.label.replaceAll(".", "_"))
        : item.label,
    status: item.status,
    createdAt: item.elapsedMs,
    ...(typeof item.durationMs === "number"
      ? { durationMs: item.durationMs }
      : {}),
  };
}

function sameProcessItems(
  left: ChatLine["processItems"],
  right: AssistantProcessItem[],
): boolean {
  if (!left || left.length !== right.length) return false;
  return left.every(
    (item, index) =>
      item.id === right[index]?.id &&
      item.label === right[index]?.label &&
      item.status === right[index]?.status &&
      item.durationMs === right[index]?.durationMs,
  );
}
