import {
  useEffect,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";

import { buildAssistantChromeSnapshot } from "@/lib/assistant-chrome";
import { syncActiveAiScene } from "@/lib/assistant-scene";
import { OPEN_AUDIT_TRAIL_EVENT } from "@/lib/audit-trail-events";
import { listenAiRequestStarted } from "@/lib/ipc";
import type {
  AssistantActionState,
  ContextPacket,
  TokenUsage,
} from "@/types/ai";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";

import type { ChatLine } from "../AiMessageList";
import type { AssistantSelectionQuote } from "../UnifiedAssistantPanel.impl";
import { buildActionState } from "../unified-assistant-panel-utils";

interface UseAssistantPanelEffectsParams {
  actionState: AssistantActionState;
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
  setAuditDrawerOpen: Dispatch<SetStateAction<boolean>>;
  setHarnessRequestId: Dispatch<SetStateAction<string | null>>;
  setInput: Dispatch<SetStateAction<string>>;
  streaming: boolean;
}

export function useAssistantPanelEffects({
  actionState,
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
  setAuditDrawerOpen,
  setHarnessRequestId,
  setInput,
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
    const openAudit = () => setAuditDrawerOpen(true);
    window.addEventListener(OPEN_AUDIT_TRAIL_EVENT, openAudit);
    return () => window.removeEventListener(OPEN_AUDIT_TRAIL_EVENT, openAudit);
  }, [setAuditDrawerOpen]);

  useEffect(() => {
    if (!streaming) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listenAiRequestStarted((payload) => {
      if (cancelled) return;
      requestIdRef.current = payload.request_id;
      setHarnessRequestId(payload.request_id);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [requestIdRef, setHarnessRequestId, streaming]);

  useEffect(() => {
    if (!selectionQuote?.text) return;
    setActionState(buildActionState("writing", "idle"));
  }, [selectionQuote?.filePath, selectionQuote?.text, setActionState]);

  useEffect(() => {
    if (!prefillMessage?.trim()) return;
    setInput(prefillMessage.trim());
  }, [prefillMessage, setInput]);

  useEffect(() => {
    syncActiveAiScene(actionState.intent);
  }, [actionState.intent]);
}
