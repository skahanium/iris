import {
  startTransition,
  useEffect,
  useRef,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";

import {
  listenAiRetryStatus,
  listenLlmDone,
  listenLlmError,
  listenLlmReset,
  listenLlmToken,
} from "@/lib/ipc";
import type { LlmDoneEvent, LlmTokenEvent, StreamSurface } from "@/types/ipc";

import type { ChatLine } from "@/components/ai/AiMessageList";
import type { AiDomain } from "@/lib/ai-domain";
import {
  recordAiLifecycleEvent,
  summarizeLifecycleContent,
  type AiLifecycleRecorder,
} from "@/lib/ai-lifecycle-trace";

function isVisibleAnswerSurface(surface: StreamSurface | undefined | null) {
  return (
    surface === undefined || surface === null || surface === "visible_answer"
  );
}

/**
 * 统一助手 LLM 流式事件监听（RAF 节流 + request_id 过滤）。
 */
export function useAssistantLlmStream(options: {
  panelSendActiveRef: MutableRefObject<boolean>;
  requestIdRef: MutableRefObject<string | null>;
  streamBufRef: MutableRefObject<string>;
  setActivityHint: Dispatch<SetStateAction<string | null>>;
  setMessages: Dispatch<SetStateAction<ChatLine[]>>;
  setStreaming: Dispatch<SetStateAction<boolean>>;
  domain?: AiDomain;
  lifecycleRecorder?: AiLifecycleRecorder;
}) {
  const {
    panelSendActiveRef,
    requestIdRef,
    streamBufRef,
    setActivityHint,
    setMessages,
    setStreaming,
    domain,
    lifecycleRecorder,
  } = options;

  const domainRef = useRef(domain);
  domainRef.current = domain;

  const rafRef = useRef<number | undefined>(undefined);

  useEffect(() => {
    let disposed = false;
    let unlistenToken: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;
    let unlistenReset: (() => void) | undefined;
    let unlistenRetryStatus: (() => void) | undefined;

    function setMessagesFromBuf(source: string) {
      const snapshot = streamBufRef.current;
      setMessages((prev) => {
        const last = prev[prev.length - 1];
        if (last?.role === "assistant") {
          if (last.content === snapshot) {
            recordAiLifecycleEvent(lifecycleRecorder, {
              event: "message_mutation",
              mutation: "noop",
              nextSummary: summarizeLifecycleContent(snapshot),
              phase: "frontend_stream",
              previousSummary: summarizeLifecycleContent(last.content),
              requestId: requestIdRef.current,
              source,
            });
            return prev;
          }
          const copy = [...prev];
          copy[copy.length - 1] = { ...last, content: snapshot };
          recordAiLifecycleEvent(lifecycleRecorder, {
            event: "message_mutation",
            mutation: "replace_assistant",
            nextSummary: summarizeLifecycleContent(snapshot),
            phase: "frontend_stream",
            previousSummary: summarizeLifecycleContent(last.content),
            requestId: requestIdRef.current,
            source,
          });
          return copy;
        }
        const copy = [...prev];
        copy.push({ role: "assistant", content: snapshot });
        recordAiLifecycleEvent(lifecycleRecorder, {
          event: "message_mutation",
          mutation: "push_assistant",
          nextSummary: summarizeLifecycleContent(snapshot),
          phase: "frontend_stream",
          requestId: requestIdRef.current,
          source,
        });
        return copy;
      });
    }

    function clearAssistantSlot(reasonKind?: string | null) {
      setMessages((prev) => {
        const last = prev[prev.length - 1];
        if (last?.role === "assistant") {
          if (last.content === "") {
            recordAiLifecycleEvent(lifecycleRecorder, {
              event: "message_mutation",
              mutation: "noop",
              nextSummary: summarizeLifecycleContent(""),
              phase: "frontend_stream",
              previousSummary: summarizeLifecycleContent(last.content),
              reasonKind,
              requestId: requestIdRef.current,
              source: "llm_reset",
            });
            return prev;
          }
          const copy = [...prev];
          copy[copy.length - 1] = { ...last, content: "" };
          recordAiLifecycleEvent(lifecycleRecorder, {
            event: "message_mutation",
            mutation: "clear_assistant",
            nextSummary: summarizeLifecycleContent(""),
            phase: "frontend_stream",
            previousSummary: summarizeLifecycleContent(last.content),
            reasonKind,
            requestId: requestIdRef.current,
            source: "llm_reset",
          });
          return copy;
        }
        const copy = [...prev];
        copy.push({ role: "assistant", content: "" });
        recordAiLifecycleEvent(lifecycleRecorder, {
          event: "message_mutation",
          mutation: "push_empty_assistant",
          nextSummary: summarizeLifecycleContent(""),
          phase: "frontend_stream",
          reasonKind,
          requestId: requestIdRef.current,
          source: "llm_reset",
        });
        return copy;
      });
    }

    /** rAF 回调中的中间流式更新，用 startTransition 降低优先级。 */
    function flushSnapshot() {
      startTransition(() => {
        setMessagesFromBuf("llm_token_raf");
      });
    }

    function cancelScheduledFlush() {
      if (rafRef.current !== undefined) {
        window.cancelAnimationFrame(rafRef.current);
        rafRef.current = undefined;
      }
    }

    void listenLlmToken((ev: LlmTokenEvent) => {
      if (disposed || !panelSendActiveRef.current) return;
      if (!requestIdRef.current) {
        requestIdRef.current = ev.request_id;
      } else if (ev.request_id !== requestIdRef.current) {
        return;
      }
      // Ignore late classified tokens after leaving classified domain
      if (domainRef.current !== "classified" && ev.classified) {
        return;
      }
      recordAiLifecycleEvent(lifecycleRecorder, {
        candidateKind: ev.candidate_kind,
        contentSummary: summarizeLifecycleContent(ev.token),
        event: "llm_token",
        phase: "frontend_stream",
        requestId: ev.request_id,
        source: "llm:token",
        surface: ev.surface ?? "visible_answer",
      });
      if (!isVisibleAnswerSurface(ev.surface)) {
        return;
      }
      streamBufRef.current += ev.token;

      if (rafRef.current === undefined) {
        rafRef.current = window.requestAnimationFrame(() => {
          rafRef.current = undefined;
          if (disposed) return;
          flushSnapshot();
        });
      }
    }).then((fn) => {
      if (disposed) fn();
      else unlistenToken = fn;
    });

    void listenLlmDone((ev: LlmDoneEvent) => {
      if (disposed || !panelSendActiveRef.current) return;
      if (
        requestIdRef.current &&
        ev.request_id &&
        ev.request_id !== requestIdRef.current
      ) {
        return;
      }
      if (domainRef.current !== "classified" && ev.classified) {
        return;
      }
      recordAiLifecycleEvent(lifecycleRecorder, {
        candidateKind: ev.candidate_kind,
        contentSummary: summarizeLifecycleContent(streamBufRef.current),
        event: "llm_done",
        phase: "frontend_stream",
        requestId: ev.request_id ?? requestIdRef.current,
        source: "llm:done",
        surface: ev.surface ?? "visible_answer",
      });
      if (!isVisibleAnswerSurface(ev.surface)) {
        return;
      }
      cancelScheduledFlush();
      setMessagesFromBuf("llm_done");
      // NOTE: streaming state is owned by the task runner's finally block.
      // The harness may emit multiple llm:done events across rounds; ending
      // streaming here would suppress tokens from subsequent rounds.
    }).then((fn) => {
      if (disposed) fn();
      else unlistenDone = fn;
    });

    void listenLlmReset((ev) => {
      if (disposed || !panelSendActiveRef.current) return;
      if (
        requestIdRef.current &&
        ev.request_id &&
        ev.request_id !== requestIdRef.current
      ) {
        return;
      }
      recordAiLifecycleEvent(lifecycleRecorder, {
        candidateKind: ev.candidate_kind,
        contentSummary: summarizeLifecycleContent(streamBufRef.current),
        event: "llm_reset",
        phase: "frontend_stream",
        reasonKind: ev.reason_kind ?? null,
        requestId: ev.request_id ?? requestIdRef.current,
        source: "llm:reset",
        surface: ev.surface ?? "visible_answer",
      });
      if (!isVisibleAnswerSurface(ev.surface)) {
        if (ev.reason_kind === "tool_round") {
          setActivityHint("正在处理工具结果…");
        } else if (ev.reason_kind === "need_more_evidence") {
          setActivityHint("证据不足，正在补充检索…");
        } else if (ev.reason_kind === "parse_retry") {
          setActivityHint("模型工具参数异常，正在重试…");
        }
        return;
      }
      cancelScheduledFlush();
      streamBufRef.current = "";
      clearAssistantSlot(ev.reason_kind ?? null);
    }).then((fn) => {
      if (disposed) fn();
      else unlistenReset = fn;
    });

    void listenLlmError((ev) => {
      if (disposed || !panelSendActiveRef.current) return;
      if (
        requestIdRef.current &&
        ev.request_id &&
        ev.request_id !== requestIdRef.current
      ) {
        return;
      }
      if (domainRef.current !== "classified" && ev.classified) {
        return;
      }
      panelSendActiveRef.current = false;
      setStreaming(false);
      recordAiLifecycleEvent(lifecycleRecorder, {
        event: "llm_error",
        phase: "frontend_stream",
        requestId: ev.request_id ?? requestIdRef.current,
        source: "llm:error",
      });
      streamBufRef.current = "";
      requestIdRef.current = null;
      cancelScheduledFlush();
      setMessages((prev) => [
        ...prev,
        {
          role: "system",
          content: `错误: ${ev.error ?? "未知错误"}`,
        },
      ]);
    }).then((fn) => {
      if (disposed) fn();
      else unlistenError = fn;
    });

    void listenAiRetryStatus((ev) => {
      if (disposed || !panelSendActiveRef.current) return;
      if (
        requestIdRef.current &&
        ev.request_id &&
        ev.request_id !== requestIdRef.current
      ) {
        return;
      }
      setActivityHint(
        `重试中（${ev.attempt}/${ev.max_attempts}），约 ${Math.ceil(
          ev.delay_ms / 1000,
        )} 秒后继续。`,
      );
      recordAiLifecycleEvent(lifecycleRecorder, {
        event: "retry_status",
        phase: "frontend_stream",
        reasonKind: ev.reason_kind ?? null,
        requestId: ev.request_id,
        source: "ai:retry_status",
      });
    }).then((fn) => {
      if (disposed) fn();
      else unlistenRetryStatus = fn;
    });

    return () => {
      disposed = true;
      cancelScheduledFlush();
      unlistenToken?.();
      unlistenDone?.();
      unlistenError?.();
      unlistenReset?.();
      unlistenRetryStatus?.();
    };
  }, [
    panelSendActiveRef,
    requestIdRef,
    streamBufRef,
    setActivityHint,
    setMessages,
    setStreaming,
    lifecycleRecorder,
  ]);
}
