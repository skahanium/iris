import {
  useEffect,
  useRef,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";

import { listenLlmDone, listenLlmError, listenLlmToken } from "@/lib/ipc";
import type { LlmTokenEvent } from "@/types/ipc";

import type { ChatLine } from "@/components/ai/AiMessageList";

/**
 * 统一助手 LLM 流式事件监听（RAF 节流 + request_id 过滤）。
 */
export function useAssistantLlmStream(options: {
  panelSendActiveRef: MutableRefObject<boolean>;
  requestIdRef: MutableRefObject<string | null>;
  streamBufRef: MutableRefObject<string>;
  setMessages: Dispatch<SetStateAction<ChatLine[]>>;
  setStreaming: Dispatch<SetStateAction<boolean>>;
}) {
  const {
    panelSendActiveRef,
    requestIdRef,
    streamBufRef,
    setMessages,
    setStreaming,
  } = options;

  const rafRef = useRef<number | undefined>(undefined);
  const lastFlushRef = useRef<number>(0);

  useEffect(() => {
    let disposed = false;
    let unlistenToken: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;

    function flushSnapshot() {
      const snapshot = streamBufRef.current;
      setMessages((prev) => {
        const copy = [...prev];
        const last = copy[copy.length - 1];
        if (last?.role === "assistant") {
          copy[copy.length - 1] = { ...last, content: snapshot };
        } else {
          copy.push({ role: "assistant", content: snapshot });
        }
        return copy;
      });
    }

    void listenLlmToken((ev: LlmTokenEvent) => {
      if (disposed || !panelSendActiveRef.current) return;
      if (!requestIdRef.current) {
        requestIdRef.current = ev.request_id;
      } else if (ev.request_id !== requestIdRef.current) {
        return;
      }
      streamBufRef.current += ev.token;

      if (rafRef.current === undefined) {
        const elapsed = performance.now() - lastFlushRef.current;
        const delay = elapsed < 50 ? 50 - elapsed : 0;
        rafRef.current = window.setTimeout(() => {
          rafRef.current = undefined;
          if (disposed) return;
          lastFlushRef.current = performance.now();
          flushSnapshot();
        }, delay) as unknown as number;
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
      if (rafRef.current !== undefined) {
        clearTimeout(rafRef.current);
        rafRef.current = undefined;
        flushSnapshot();
      }
      setStreaming(false);
    }).then((fn) => {
      if (disposed) fn();
      else unlistenDone = fn;
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
      panelSendActiveRef.current = false;
      setStreaming(false);
      streamBufRef.current = "";
      requestIdRef.current = null;
      if (rafRef.current !== undefined) {
        clearTimeout(rafRef.current);
        rafRef.current = undefined;
      }
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

    return () => {
      disposed = true;
      if (rafRef.current !== undefined) {
        clearTimeout(rafRef.current);
        rafRef.current = undefined;
      }
      unlistenToken?.();
      unlistenDone?.();
      unlistenError?.();
    };
  }, [
    panelSendActiveRef,
    requestIdRef,
    streamBufRef,
    setMessages,
    setStreaming,
  ]);
}
