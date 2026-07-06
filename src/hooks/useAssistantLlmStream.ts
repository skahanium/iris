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
import { AssistantStreamBuffer } from "@/lib/assistant-stream-buffer";
import type { AiDomain } from "@/lib/ai-domain";
import { appendSystemMessageAfterDroppingEmptyAssistant } from "@/lib/assistant-transcript";
import { restoreChatLineContent } from "@/lib/ai-payload-store";
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
  if (durationMs < 1000)
    return `${Math.max(0, Math.round(durationMs))} \u6beb\u79d2`;
  return `${(durationMs / 1000).toFixed(1)} \u79d2`;
}

function harnessToolLabel(toolName: string): string {
  switch (toolName) {
    case "web_search":
      return "\u8054\u7f51\u68c0\u7d22";
    case "search_hybrid":
    case "search_semantic":
    case "search_keyword":
      return "\u7b14\u8bb0\u68c0\u7d22";
    case "fetch_web_page":
      return "\u7f51\u9875\u6b63\u6587\u6293\u53d6";
    case "reflection":
      return "\u8bc1\u636e\u68c0\u67e5";
    case "final":
      return "\u6700\u7ec8\u56de\u7b54";
    case "spawn_subagent":
      return "\u5b50\u4efb\u52a1";
    default:
      return toolName.replaceAll("_", " ");
  }
}

function harnessTraceHint(ev: HarnessTraceEvent): string | null {
  const label = harnessToolLabel(ev.tool_name);
  const duration = formatDuration(ev.duration_ms);
  switch (ev.phase) {
    case "tool_start":
      return ev.status === "pending"
        ? `${label}\u7b49\u5f85\u786e\u8ba4...`
        : `${label}\u4e2d...`;
    case "tool_complete":
      return duration
        ? `${label}\u5b8c\u6210\uff0c\u7528\u65f6 ${duration}\u3002`
        : `${label}\u5b8c\u6210\u3002`;
    case "subagent_spawn":
      return "\u6b63\u5728\u542f\u52a8\u5b50\u4efb\u52a1...";
    case "subagent_complete":
      return duration
        ? `\u5b50\u4efb\u52a1\u5b8c\u6210\uff0c\u7528\u65f6 ${duration}\u3002`
        : "\u5b50\u4efb\u52a1\u5b8c\u6210\u3002";
    case "reflection":
      return "\u6b63\u5728\u68c0\u67e5\u8bc1\u636e\u5145\u5206\u6027...";
    case "final_stream":
      return "\u6b63\u5728\u6d41\u5f0f\u8f93\u51fa\u6700\u7ec8\u56de\u7b54...";
    case "thinking":
      return "\u6b63\u5728\u601d\u8003...";
    default:
      return null;
  }
}
/**
 * 缂備胶鍠嶇粩鎾礉閳哄倸顤?LLM 婵炵繝绀佺槐鈩冪鐎ｂ晜顐介柣鈺傚灥閹鏁嶉崷顪嘑 闁煎搫鍊圭粊?+ request_id 閺夆晛娲﹂幎銈夋晬婢跺牃鍋? */
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
  const streamBufferRef = useRef(new AssistantStreamBuffer());
  const streamBufferRequestIdRef = useRef<string | null>(null);

  useEffect(() => {
    let disposed = false;
    let unlistenToken: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;
    let unlistenReset: (() => void) | undefined;
    let unlistenRetryStatus: (() => void) | undefined;
    let unlistenHarnessTrace: (() => void) | undefined;

    function currentStreamSnapshot(): string {
      if (
        streamBufferRef.current.length > 0 ||
        streamBufRef.current.length === 0
      ) {
        return streamBufferRef.current.toString();
      }
      return streamBufRef.current;
    }

    function setMessagesFromBuf(source: string) {
      const snapshot = currentStreamSnapshot();
      streamBufRef.current = snapshot;
      setMessages((prev) => {
        const last = prev[prev.length - 1];
        if (last?.role === "assistant") {
          const previousContent = restoreChatLineContent(last);
          if (previousContent === snapshot) {
            recordAiLifecycleEvent(lifecycleRecorder, {
              event: "message_mutation",
              mutation: "noop",
              nextSummary: summarizeLifecycleContent(snapshot),
              phase: "frontend_stream",
              previousSummary: summarizeLifecycleContent(previousContent),
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
            previousSummary: summarizeLifecycleContent(previousContent),
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
          const previousContent = restoreChatLineContent(last);
          if (previousContent === "") {
            recordAiLifecycleEvent(lifecycleRecorder, {
              event: "message_mutation",
              mutation: "noop",
              nextSummary: summarizeLifecycleContent(""),
              phase: "frontend_stream",
              previousSummary: summarizeLifecycleContent(previousContent),
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
            previousSummary: summarizeLifecycleContent(previousContent),
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

    /** Intermediate streaming updates flushed inside rAF with lower priority. */
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
      if (streamBufferRequestIdRef.current !== ev.request_id) {
        streamBufferRef.current.clear();
        streamBufferRequestIdRef.current = ev.request_id;
        streamBufRef.current = "";
      }
      streamBufferRef.current.append(ev.token);

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
        contentSummary: summarizeLifecycleContent(currentStreamSnapshot()),
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
        contentSummary: summarizeLifecycleContent(currentStreamSnapshot()),
        event: "llm_reset",
        phase: "frontend_stream",
        reasonKind: ev.reason_kind ?? null,
        requestId: ev.request_id ?? requestIdRef.current,
        source: "llm:reset",
        surface: ev.surface ?? "visible_answer",
      });
      if (!isVisibleAnswerSurface(ev.surface)) {
        if (ev.reason_kind === "tool_round") {
          setActivityHint(
            "\u6b63\u5728\u5904\u7406\u5de5\u5177\u7ed3\u679c...",
          );
        } else if (ev.reason_kind === "need_more_evidence") {
          setActivityHint(
            "\u8bc1\u636e\u4e0d\u8db3\uff0c\u6b63\u5728\u8865\u5145\u68c0\u7d22...",
          );
        } else if (ev.reason_kind === "parse_retry") {
          setActivityHint(
            "\u6a21\u578b\u5de5\u5177\u53c2\u6570\u5f02\u5e38\uff0c\u6b63\u5728\u91cd\u8bd5...",
          );
        }
        return;
      }
      cancelScheduledFlush();
      streamBufferRef.current.clear();
      streamBufferRequestIdRef.current = null;
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
        setActivityHint(
          "\u8fde\u63a5\u4e2d\u65ad\uff0c\u6b63\u5728\u91cd\u8bd5\u6d41\u5f0f\u54cd\u5e94\u2026",
        );
        return;
      }

      panelSendActiveRef.current = false;
      setStreaming(false);
      requestIdRef.current = null;
      cancelScheduledFlush();
      setMessages((prev) => {
        const last = prev[prev.length - 1];
        const hasVisiblePartial =
          last?.role === "assistant" &&
          restoreChatLineContent(last).trim().length > 0;
        if (!hasVisiblePartial) {
          streamBufferRef.current.clear();
          streamBufferRequestIdRef.current = null;
          streamBufRef.current = "";
        }
        const baseError = `\u9519\u8bef: ${ev.error ?? "\u672a\u77e5\u9519\u8bef"}`;
        const errorContent = hasVisiblePartial
          ? `${baseError}\uff08\u5df2\u4fdd\u7559\u90e8\u5206\u8f93\u51fa\uff09`
          : baseError;
        return appendSystemMessageAfterDroppingEmptyAssistant(
          prev,
          errorContent,
        );
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
