import { useEffect, useRef, type Dispatch, type SetStateAction } from "react";

import type { ChatLine } from "../AiMessageList";
import {
  collapseRepeatedWebSearchProcessItems,
  isInternalRuntimeTool,
  projectAssistantProcessEvents,
  type AssistantProcessItem,
} from "@/lib/assistant-process";
import { deriveRunOutputting } from "@/lib/assistant-run-activity";
import {
  type AssistantRunEventState,
  userVisibleRunFailureMessage,
} from "@/lib/assistant-run-events";
import { ensureTerminalAnswerComplete } from "@/lib/ensure-answer-complete-process";
import type {
  AssistantPresentationItem,
  AssistantPresentationState,
} from "@/lib/assistant-presentation";
import { sanitizeAssistantVisibleText } from "@/lib/assistant-visible-text";
import { restoreChatLineContent } from "@/lib/ai-payload-store";
import { assistantSessionLoad } from "@/lib/ipc";
import { toolDisplayName } from "@/lib/tool-display-names";
import type {
  AssistantSessionRef,
  ClassifiedRunResultRequest,
} from "@/types/ai";
import type { AssistantAnswerReveal } from "./useAssistantAnswerReveal";

export interface AssistantConversationProjectionOptions {
  run: AssistantRunEventState | null;
  presentation?: AssistantPresentationState | null;
  /** Legacy caller compatibility; provisional playback never owns the body. */
  presentationReveal?: AssistantAnswerReveal;
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
  const hydratedCompletedRunsRef = useRef(new Set<string>());
  const classifiedResultRef = useRef<{
    key: string;
    result: Promise<string>;
  } | null>(null);

  useEffect(() => {
    if (
      !run ||
      run.state !== "completed" ||
      !session ||
      session.domain !== "normal" ||
      hydratedCompletedRunsRef.current.has(run.runId)
    ) {
      return;
    }
    hydratedCompletedRunsRef.current.add(run.runId);
    void assistantSessionLoad({ session, limit: 48 })
      .then((loaded) => {
        const persisted = loaded.find(
          (message) =>
            message.role === "assistant" && message.runId === run.runId,
        );
        if (!persisted) return;
        setMessages((previous) => {
          const current = previous.find(
            (message) =>
              message.role === "assistant" && message.runId === run.runId,
          );
          const processItems = persisted.processEvents?.length
            ? ensureTerminalAnswerComplete(
                projectAssistantProcessEvents(persisted.processEvents),
                "completed",
              )
            : current?.processItems;
          const loadedContent = sanitizeAssistantVisibleText(persisted.content);
          const mayAdvanceContent =
            Boolean(loadedContent) && !current?.answerPresentation?.complete;
          return upsertRunMessage(previous, run.runId, {
            ...(mayAdvanceContent
              ? {
                  content: loadedContent,
                  contentRef: undefined,
                  presentationStreaming: false,
                  answerPresentation: {
                    runId: run.runId,
                    resetEpoch: 0,
                    complete: true,
                  },
                }
              : {}),
            turnId: persisted.turnId,
            turnState: persisted.turnState,
            seq: persisted.seq,
            created_at: persisted.createdAt,
            processItems,
            webCitations: persisted.webCitations,
            citationBinding: persisted.citationBinding,
            sourceSummary: persisted.sourceSummary,
          });
        });
      })
      .catch(() => hydratedCompletedRunsRef.current.delete(run.runId));
  }, [run, session, setMessages]);

  useEffect(() => {
    if (!classifiedContextRef) {
      classifiedResultRef.current = null;
      return;
    }
    if (run?.state !== "completed" || !takeClassifiedResult) return;
    let disposed = false;
    const runId = run.runId;
    const key = `${classifiedContextRef}:${runId}`;
    if (classifiedResultRef.current?.key !== key) {
      classifiedResultRef.current = {
        key,
        result: takeClassifiedResult({
          runId,
          contextRef: classifiedContextRef,
        }),
      };
    }
    void classifiedResultRef.current.result
      .then((body) => {
        if (disposed) return;
        const content = sanitizeAssistantVisibleText(body);
        if (!content.trim()) throw new Error("classified_result_empty");
        setMessages((previous) => {
          const current = previous.find(
            (message) =>
              message.role === "assistant" && message.runId === runId,
          );
          if (current?.answerPresentation?.complete) return previous;
          return upsertRunMessage(previous, runId, {
            content,
            contentRef: undefined,
            presentationStreaming: false,
            processItems: ensureTerminalAnswerComplete(
              current?.processItems,
              "completed",
            ),
            answerPresentation: { runId, resetEpoch: 0, complete: true },
          });
        });
      })
      .catch(() => {
        if (!disposed) setError("涉密回答已失效；请重新附带当前文档后重试。");
      });
    return () => {
      disposed = true;
    };
  }, [
    run?.runId,
    run?.state,
    classifiedContextRef,
    takeClassifiedResult,
    setMessages,
    setError,
  ]);

  useEffect(() => {
    if (!run) return;
    // The Run acceptance event can arrive before the conversation projection
    // has appended its user/assistant placeholder. Do not mark the whole
    // transcript streaming in that gap: the list would otherwise treat the
    // previous assistant answer as the current streaming bubble.
    const hasRunMessage = messages.some(
      (message) => message.runId === run.runId,
    );
    const outputting = hasRunMessage && deriveRunOutputting(run, presentation);
    setStreaming(outputting);
    if (outputting) {
      setActivityHint(run.stage);
      return;
    }
    if (["completed", "failed", "cancelled"].includes(run.state ?? "")) {
      if (run.state !== "failed" && run.state !== "cancelled") {
        setActivityHint(null);
      }
    }
  }, [messages, presentation, run, setActivityHint, setStreaming]);

  useEffect(() => {
    if (!run) return;
    const presentationSeq =
      presentation?.runId === run.runId ? presentation.lastSeq : 0;
    const hasLivePresentation = presentationSeq > 0;
    if (run.lastSeq === 0 && !hasLivePresentation) return;
    if (!messages.some((message) => message.runId === run.runId)) return;

    const projectionKey = `${run.runId}:${run.lastSeq}:${run.transientRevision}:${presentationSeq}:${run.state}:${run.resyncFromSeq}`;
    if (appliedProjectionRef.current === projectionKey) return;
    appliedProjectionRef.current = projectionKey;

    const terminal = ["completed", "failed", "cancelled"].includes(
      run.state ?? "",
    );
    const presentationReady =
      presentation?.runId === run.runId && presentation.resyncFromSeq === null;
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

    const currentMessage = messages.find(
      (message) => message.role === "assistant" && message.runId === run.runId,
    );
    // A completed, contiguous durable snapshot is the only normal-domain target.
    // Presentation deltas/reset are retained for wire compatibility, never publication.
    const stopped = run.state === "cancelled" || run.state === "failed";
    const durable = sanitizeAssistantVisibleText(run.content);
    const ready =
      run.state === "completed" &&
      run.resyncFromSeq === null &&
      !classifiedContextRef &&
      Boolean(durable.trim());
    const frozen = currentMessage?.answerPresentation?.complete;
    const content = frozen
      ? restoreChatLineContent(currentMessage)
      : ready
        ? durable
        : "";
    const answerPresentation = {
      runId: run.runId,
      resetEpoch: 0,
      complete: Boolean(frozen || ready),
      stopped: !frozen && stopped,
      settled: currentMessage?.answerPresentation?.settled,
    };
    const presentationStreaming = !stopped && !answerPresentation.complete;
    // Completion without a body (e.g. an event gap) is still waiting for publication.
    const readyProcessItems = answerPresentation.complete
      ? processItems
      : processItems.filter((item) => item.id !== "stage:answer-complete");

    setMessages((previous) => {
      const current = previous.find(
        (message) =>
          message.role === "assistant" && message.runId === run.runId,
      );
      if (
        current &&
        current.content === content &&
        current.contentRef === undefined &&
        current.presentationStreaming === presentationStreaming &&
        current.answerPresentation?.resetEpoch ===
          answerPresentation?.resetEpoch &&
        current.answerPresentation?.complete === answerPresentation?.complete &&
        current.answerPresentation?.stopped === answerPresentation?.stopped &&
        sameProcessItems(current.processItems, readyProcessItems)
      ) {
        return previous;
      }
      return upsertRunMessage(previous, run.runId, {
        content: current?.answerPresentation?.complete
          ? restoreChatLineContent(current)
          : content,
        contentRef: undefined,
        processItems: readyProcessItems,
        presentationStreaming,
        answerPresentation: current?.answerPresentation?.complete
          ? current.answerPresentation
          : answerPresentation,
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
      return;
    }
    if (run.state === "failed" && !frozen) {
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
    if (run.state === "cancelled" && !frozen) {
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

function upsertRunMessage(
  messages: ChatLine[],
  runId: string,
  patch: Partial<ChatLine>,
): ChatLine[] {
  const index = messages.findIndex(
    (message) => message.role === "assistant" && message.runId === runId,
  );
  if (index >= 0) {
    const next = messages.slice();
    next[index] = { ...messages[index]!, ...patch };
    return next;
  }
  const userIndex = messages.findIndex(
    (message) => message.role === "user" && message.runId === runId,
  );
  if (userIndex < 0) return messages;
  const user = messages[userIndex]!;
  const assistant: ChatLine = {
    role: "assistant",
    content: "",
    runId,
    turnId: user.turnId,
    clientRequestId: user.clientRequestId,
    ...patch,
  };
  const next = messages.slice();
  next.splice(userIndex + 1, 0, assistant);
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
    const userIndex = messages.findIndex(
      (message) => message.role === "user" && message.runId === runId,
    );
    if (userIndex < 0) return messages;
    const next = messages.slice();
    next.splice(userIndex + 1, 0, {
      role: "system",
      content: "本次回答已取消。",
      runId,
    });
    return next;
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
  if (event.payload.kind !== "failed") {
    return "本次运行未能完成。";
  }
  return userVisibleRunFailureMessage(
    event.payload.code,
    event.payload.message,
    Boolean(run.webSearched),
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
