import { useEffect, useRef, type Dispatch, type SetStateAction } from "react";

import type { ChatLine } from "../AiMessageList";
import {
  collapseRepeatedWebSearchProcessItems,
  isInternalRuntimeTool,
  projectAssistantProcessEvents,
  type AssistantProcessItem,
} from "@/lib/assistant-process";
import { deriveRunOutputting } from "@/lib/assistant-run-activity";
import type { AssistantRunEventState } from "@/lib/assistant-run-events";
import { ensureTerminalAnswerComplete } from "@/lib/ensure-answer-complete-process";
import type {
  AssistantPresentationItem,
  AssistantPresentationState,
} from "@/lib/assistant-presentation";
import { sanitizeAssistantVisibleText } from "@/lib/assistant-visible-text";
import { assistantSessionLoad } from "@/lib/ipc";
import { toolDisplayName } from "@/lib/tool-display-names";
import type {
  AssistantSessionRef,
  ClassifiedRunResultRequest,
} from "@/types/ai";

export interface AssistantConversationProjectionOptions {
  run: AssistantRunEventState | null;
  presentation?: AssistantPresentationState | null;
  session?: AssistantSessionRef | null;
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

/**
 * The only Run/presentation-to-conversation writer. A projection key makes a
 * durable event and its live presentation counterpart idempotent, while the
 * single assistant row is the only row replaced for ordinary updates.
 */
export function useAssistantConversationProjection({
  run,
  presentation,
  session,
  messages,
  setMessages,
  setStreaming,
  setActivityHint,
  setError,
  classifiedContextRef,
  takeClassifiedResult,
}: AssistantConversationProjectionOptions) {
  const appliedProjectionRef = useRef<string | null>(null);
  const hydratedCitationRunsRef = useRef(new Set<string>());

  useEffect(() => {
    if (
      !run ||
      run.state !== "completed" ||
      !session ||
      session.domain !== "normal" ||
      hydratedCitationRunsRef.current.has(run.runId)
    ) {
      return;
    }
    hydratedCitationRunsRef.current.add(run.runId);
    void assistantSessionLoad({ session, limit: 48 })
      .then((loaded) => {
        const persisted = loaded.find(
          (message) =>
            message.role === "assistant" &&
            message.runId === run.runId &&
            Boolean(message.webCitations?.length),
        );
        if (!persisted?.webCitations?.length) return;
        setMessages((previous) =>
          patchRunMessage(previous, run.runId, {
            webCitations: persisted.webCitations,
            citationBinding: persisted.citationBinding,
            sourceSummary: persisted.sourceSummary,
          }),
        );
      })
      .catch(() => hydratedCitationRunsRef.current.delete(run.runId));
  }, [run, session, setMessages]);

  useEffect(() => {
    if (!run) return;
    const outputting = deriveRunOutputting(run, presentation);
    setStreaming(outputting);
    if (outputting) {
      setActivityHint(run.stage);
      return;
    }
    if (
      ["completed", "failed", "cancelled"].includes(run.state ?? "") ||
      (presentation?.runId === run.runId && presentation.answerComplete)
    ) {
      if (run.state !== "failed" && run.state !== "cancelled") {
        setActivityHint(null);
      }
    }
  }, [presentation, run, setActivityHint, setStreaming]);

  useEffect(() => {
    if (!run) return;
    const presentationSeq =
      presentation?.runId === run.runId ? presentation.lastSeq : 0;
    const hasLivePresentation = presentationSeq > 0;
    if (run.lastSeq === 0 && !hasLivePresentation) return;
    if (!messages.some((message) => message.runId === run.runId)) return;

    const projectionKey = `${run.runId}:${run.lastSeq}:${run.transientRevision}:${presentationSeq}`;
    if (appliedProjectionRef.current === projectionKey) return;
    appliedProjectionRef.current = projectionKey;

    const terminal = ["completed", "failed", "cancelled"].includes(
      run.state ?? "",
    );
    const presentationReady =
      presentation?.runId === run.runId && presentation.resyncFromSeq === null;
    const presentationOwnsContent =
      presentationReady && (!terminal || presentation.answerComplete);
    const presentationOwnsProcess = presentationReady && !terminal;
    const processItems = ensureTerminalAnswerComplete(
      presentationOwnsProcess
        ? collapseRepeatedWebSearchProcessItems(
            (presentation?.processItems ?? [])
              .filter(
                (item) =>
                  item.kind !== "tool" || !isInternalRuntimeTool(item.label),
              )
              .map(toProcessItem),
          )
        : projectAssistantProcessEvents(run.events, run.reasoningSummaries),
      run.state,
    );

    setMessages((previous) => {
      const current = previous.find(
        (message) =>
          message.role === "assistant" && message.runId === run.runId,
      );
      if (!current) return previous;
      const content = presentationOwnsContent
        ? sanitizeAssistantVisibleText(presentation?.answer ?? "") ||
          current.content
        : run.content.trim()
          ? run.content
          : current.content;
      const presentationStreaming = presentationOwnsContent && !terminal;
      if (
        current.content === content &&
        current.presentationStreaming === presentationStreaming &&
        sameProcessItems(current.processItems, processItems)
      ) {
        return previous;
      }
      return patchRunMessage(previous, run.runId, {
        content,
        processItems,
        presentationStreaming,
      });
    });

    setActivityHint(run.stage);
    if (run.state === "awaiting_confirmation" || run.state === "paused") {
      setStreaming(false);
      return;
    }
    if (run.state === "completed") {
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
          .then((content) =>
            setMessages((previous) =>
              patchRunMessage(previous, run.runId, { content }),
            ),
          )
          .catch(() => setError("涉密回答已失效；请重新附带当前文档后重试。"));
      }
      return;
    }
    if (run.state === "failed") {
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
      const event = run.events.at(-1);
      if (event) setError(userVisibleRunFailure(run, event));
      return;
    }
    if (run.state === "cancelled") {
      setStreaming(false);
      setActivityHint(null);
      setMessages((previous) => appendCancellationNotice(previous, run.runId));
    }
  }, [
    classifiedContextRef,
    messages,
    presentation,
    run,
    setActivityHint,
    setError,
    setMessages,
    setStreaming,
    takeClassifiedResult,
  ]);
}

function patchRunMessage(
  messages: ChatLine[],
  runId: string,
  patch: Partial<ChatLine>,
): ChatLine[] {
  const index = messages.findIndex(
    (message) => message.role === "assistant" && message.runId === runId,
  );
  if (index < 0) return messages;
  const next = messages.slice();
  next[index] = { ...messages[index]!, ...patch };
  return next;
}

function appendCancellationNotice(
  messages: ChatLine[],
  runId: string,
): ChatLine[] {
  const index = messages.findIndex(
    (message) => message.role === "assistant" && message.runId === runId,
  );
  const target = messages[index];
  if (!target) {
    return [
      ...messages,
      { role: "system", content: "本次回答已停止。发送继续可接着生成。" },
    ];
  }
  if (!target.content.trim()) {
    const next = messages.slice();
    next[index] = { role: "system", content: "本次回答已取消。" };
    return next;
  }
  if (
    messages.some(
      (message, messageIndex) =>
        messageIndex > index &&
        message.role === "system" &&
        message.content.includes("发送继续"),
    )
  ) {
    return messages;
  }
  return [
    ...messages,
    { role: "system", content: "本次回答已停止。发送继续可接着生成。" },
  ];
}

function userVisibleRunFailure(
  run: AssistantRunEventState,
  event: NonNullable<AssistantRunEventState["events"]>[number],
): string {
  if (
    event.payload.kind === "failed" &&
    event.payload.code === "agent_run_provider_unavailable" &&
    run.webSearched
  ) {
    return "联网检索已完成，但模型服务暂时不可用。请稍后重试或在设置中更换模型。";
  }
  return event.payload.kind === "failed"
    ? event.payload.message
    : "本次运行未能完成。";
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
  if (left === right) return true;
  if (!left || left.length !== right.length) return false;
  return left.every(
    (item, index) =>
      item.id === right[index]?.id &&
      item.label === right[index]?.label &&
      item.status === right[index]?.status &&
      item.durationMs === right[index]?.durationMs,
  );
}
