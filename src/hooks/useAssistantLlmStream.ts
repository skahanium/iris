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
import type { LlmTokenEvent } from "@/types/ipc";

import type { ChatLine } from "@/components/ai/AiMessageList";
import type { AiDomain } from "@/lib/ai-domain";

/**
 * 统一助手 LLM 流式事件监听（RAF 节流 + request_id 过滤）。
 */
export function useAssistantLlmStream(options: {
  panelSendActiveRef: MutableRefObject<boolean>;
  requestIdRef: MutableRefObject<string | null>;
  streamBufRef: MutableRefObject<string>;
  setMessages: Dispatch<SetStateAction<ChatLine[]>>;
  setStreaming: Dispatch<SetStateAction<boolean>>;
  domain?: AiDomain;
}) {
  const {
    panelSendActiveRef,
    requestIdRef,
    streamBufRef,
    setMessages,
    setStreaming,
    domain,
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

    function setMessagesFromBuf() {
      const snapshot = streamBufRef.current;
      setMessages((prev) => {
        const last = prev[prev.length - 1];
        if (last?.role === "assistant") {
          if (last.content === snapshot) return prev;
          const copy = [...prev];
          copy[copy.length - 1] = { ...last, content: snapshot };
          return copy;
        }
        const copy = [...prev];
        copy.push({ role: "assistant", content: snapshot });
        return copy;
      });
    }

    function clearAssistantSlot() {
      setMessages((prev) => {
        const last = prev[prev.length - 1];
        if (last?.role === "assistant") {
          if (last.content === "") return prev;
          const copy = [...prev];
          copy[copy.length - 1] = { ...last, content: "" };
          return copy;
        }
        const copy = [...prev];
        copy.push({ role: "assistant", content: "" });
        return copy;
      });
    }

    /** rAF 回调中的中间流式更新，用 startTransition 降低优先级。 */
    function flushSnapshot() {
      startTransition(() => {
        setMessagesFromBuf();
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

    void listenLlmDone((ev) => {
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
      cancelScheduledFlush();
      setMessagesFromBuf();
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
      // A non-terminal round (tool-call round or inconclusive reflection)
      // produced tokens that should not be shown as the final answer. Drop
      // the buffered content and empty the assistant slot so the next round
      // streams into a clean surface.
      cancelScheduledFlush();
      streamBufRef.current = "";
      clearAssistantSlot();
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
      setMessages((prev) => [
        ...prev,
        {
          role: "system",
          content: `重试中（${ev.attempt}/${ev.max_attempts}），约 ${Math.ceil(
            ev.delay_ms / 1000,
          )} 秒后继续。`,
        },
      ]);
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
    setMessages,
    setStreaming,
  ]);
}
