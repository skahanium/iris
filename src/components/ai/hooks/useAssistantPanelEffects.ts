import {
  useEffect,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";

import { buildAssistantChromeSnapshot } from "@/lib/assistant-chrome";
import { listenAiRequestStarted } from "@/lib/ipc";
import type {
  AssistantActionState,
  ContextPacket,
  TokenUsage,
} from "@/types/ai";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";

import type { ChatLine } from "../AiMessageList";
import type { AssistantSelectionQuote } from "../types";
import { buildActionState } from "../unified-assistant-panel-utils";

interface UseAssistantPanelEffectsParams {
  activityHint: string | null;
  harnessRequestId: string | null;
  messages: ChatLine[];
  onChromeChange?: (snapshot: AssistantChromeSnapshot) => void;
  packets: ContextPacket[];
  prefillMessage?: string | null;
  requestIdRef: MutableRefObject<string | null>;
  selectionQuote?: AssistantSelectionQuote | null;
  sessionTokenUsage: TokenUsage | null;
  setActionState: Dispatch<SetStateAction<AssistantActionState>>;
  setAgentTaskId: Dispatch<SetStateAction<string | null>>;
  setHarnessRequestId: Dispatch<SetStateAction<string | null>>;
  setInput: Dispatch<SetStateAction<string>>;
  setSessionId: Dispatch<SetStateAction<number | null>>;
  streaming: boolean;
}

export function useAssistantPanelEffects({
  activityHint,
  harnessRequestId,
  messages,
  onChromeChange,
  packets,
  prefillMessage,
  requestIdRef,
  selectionQuote,
  sessionTokenUsage,
  setActionState,
  setAgentTaskId,
  setHarnessRequestId,
  setInput,
  setSessionId,
  streaming,
}: UseAssistantPanelEffectsParams) {
  useEffect(() => {
    onChromeChange?.(
      buildAssistantChromeSnapshot({
        sessionTokenUsage,
        activityHint,
        streaming,
        messages,
        harnessPhaseLabel: null,
        packets,
        harnessRequestId,
      }),
    );
  }, [
    activityHint,
    harnessRequestId,
    messages,
    onChromeChange,
    packets,
    sessionTokenUsage,
    streaming,
  ]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listenAiRequestStarted((payload) => {
      if (cancelled) return;
      requestIdRef.current = payload.request_id;
      setHarnessRequestId(payload.request_id);
      if (typeof payload.session_id === "number") {
        setSessionId(payload.session_id);
      }
      if (payload.task_id) {
        setAgentTaskId(payload.task_id);
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [requestIdRef, setAgentTaskId, setHarnessRequestId, setSessionId]);

  useEffect(() => {
    if (!selectionQuote?.text) return;
    setActionState(buildActionState("writing", "idle"));
  }, [selectionQuote?.filePath, selectionQuote?.text, setActionState]);

  useEffect(() => {
    if (!prefillMessage?.trim()) return;
    setInput(prefillMessage.trim());
  }, [prefillMessage, setInput]);
}
