import { useEffect, type Dispatch, type MutableRefObject, type SetStateAction } from "react";

import {
  listenLlmDone,
  listenLlmError,
  listenLlmToken,
} from "@/lib/ipc";
import type { LlmTokenEvent } from "@/types/ipc";

import type { ChatLine } from "@/components/ai/AiMessageList";

/**
 * 注册侧栏 LLM 流事件。处理 StrictMode 下异步 listen 竞态，避免重复订阅导致逐字翻倍。
 */
export function useAiPanelLlmStream(options: {
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

  useEffect(() => {
    let disposed = false;
    let unlistenToken: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;

    void listenLlmToken((ev: LlmTokenEvent) => {
      if (disposed || !panelSendActiveRef.current) return;
      if (!requestIdRef.current) {
        requestIdRef.current = ev.request_id;
      } else if (ev.request_id !== requestIdRef.current) {
        return;
      }
      streamBufRef.current += ev.token;
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
