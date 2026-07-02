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
  listenHarnessTrace,
  listenLlmDone,
  listenLlmError,
  listenLlmReset,
  listenLlmToken,
} from "@/lib/ipc";
import type {
  HarnessTraceEvent,
  LlmDoneEvent,
  LlmTokenEvent,
  StreamSurface,
} from "@/types/ipc";

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

function formatDuration(durationMs: number | null | undefined): string | null {
  if (typeof durationMs !== "number" || !Number.isFinite(durationMs)) {
    return null;
  }
  if (durationMs < 1000) return `${Math.max(0, Math.round(durationMs))} 毫秒`;
  return `${(durationMs / 1000).toFixed(1)} 秒`;
}

function harnessToolLabel(toolName: string): string {
  switch (toolName) {
    case "web_search":
      return "联网检索";
    case "search_hybrid":
    case "search_semantic":
    case "search_keyword":
      return "笔记检索";
    case "fetch_web_page":
      return "网页正文抽取";
    case "reflection":
      return "证据检查";
    case "final":
      return "最终回答";
    case "spawn_subagent":
      return "子任务";
    default:
      return toolName.replaceAll("_", " ");
  }
}

function harnessTraceHint(ev: HarnessTraceEvent): string | null {
  const label = harnessToolLabel(ev.tool_name);
  const duration = formatDuration(ev.duration_ms);
  switch (ev.phase) {
    case "tool_start":
      return ev.status === "pending" ? `${label}等待确认…` : `${label}中…`;
    case "tool_complete":
      return duration ? `${label}完成，用时 ${duration}。` : `${label}完成。`;
    case "subagent_spawn":
      return "正在启动子任务…";
    case "subagent_complete":
      return duration ? `子任务完成，用时 ${duration}。` : "子任务完成。";
    case "reflection":
      return "正在检查证据充分性…";
    case "final_stream":
      return "正在流式输出最终回答…";
    case "thinking":
      return "正在思考…";
    default:
      return null;
  }
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
    let unlistenHarnessTrace: (() => void) | undefined;

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
      recordAiLifecycleEvent(lifecycleRecorder, {
        event: "llm_error",
        phase: "frontend_stream",
        requestId: ev.request_id ?? requestIdRef.current,
        source: "llm:error",
      });
      if (ev.final === false) {
        setActivityHint("连接中断，正在重试流式响应…");
        return;
      }

      panelSendActiveRef.current = false;
      setStreaming(false);
      requestIdRef.current = null;
      cancelScheduledFlush();
      setMessages((prev) => {
        const last = prev[prev.length - 1];
        const hasVisiblePartial =
          last?.role === "assistant" && last.content.trim().length > 0;
        if (!hasVisiblePartial) {
          streamBufRef.current = "";
        }
        return [
          ...prev,
          {
            role: "system",
            content: hasVisiblePartial
              ? `错误: ${ev.error ?? "未知错误"}（已保留部分输出）`
              : `错误: ${ev.error ?? "未知错误"}`,
          },
        ];
      });
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

    void listenHarnessTrace((ev) => {
      if (disposed || !panelSendActiveRef.current) return;
      if (
        requestIdRef.current &&
        ev.request_id &&
        ev.request_id !== requestIdRef.current
      ) {
        return;
      }
      const hint = harnessTraceHint(ev);
      if (hint) setActivityHint(hint);
      recordAiLifecycleEvent(lifecycleRecorder, {
        event: "harness_trace",
        phase: "frontend_stream",
        requestId: ev.request_id,
        source: "ai:harness_trace",
      });
    }).then((fn) => {
      if (disposed) fn();
      else unlistenHarnessTrace = fn;
    });

    return () => {
      disposed = true;
      cancelScheduledFlush();
      unlistenToken?.();
      unlistenDone?.();
      unlistenError?.();
      unlistenReset?.();
      unlistenRetryStatus?.();
      unlistenHarnessTrace?.();
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
