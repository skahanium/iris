import { useCallback, type Dispatch, type SetStateAction } from "react";

import { mergeContextPackets } from "@/lib/ai/merge-context-packets";
import { invokeErrorMessage } from "@/lib/credentials";
import { harnessResume } from "@/lib/ipc";
import { mapChatToolCallsForUi } from "@/lib/map-chat-tool-calls";
import { accumulateTokenUsage } from "@/lib/token-usage";
import type { ContextPacket, TokenUsage } from "@/types/ai";

import type { ChatLine } from "../AiMessageList";

interface UseAssistantHarnessResumeParams {
  ensureAssistantStreamSlot: () => void;
  harnessRequestId: string | null;
  setActivityHint: Dispatch<SetStateAction<string | null>>;
  setLastError: Dispatch<SetStateAction<string | null>>;
  setMessages: Dispatch<SetStateAction<ChatLine[]>>;
  setPackets: Dispatch<SetStateAction<ContextPacket[]>>;
  setSessionTokenUsage: Dispatch<SetStateAction<TokenUsage | null>>;
  setStreaming: Dispatch<SetStateAction<boolean>>;
}

export function useAssistantHarnessResume({
  ensureAssistantStreamSlot,
  harnessRequestId,
  setActivityHint,
  setLastError,
  setMessages,
  setPackets,
  setSessionTokenUsage,
  setStreaming,
}: UseAssistantHarnessResumeParams) {
  return useCallback(async () => {
    if (!harnessRequestId) return;
    setLastError(null);
    setStreaming(true);
    setActivityHint("正在从 checkpoint 恢复 Agent…");
    ensureAssistantStreamSlot();
    try {
      const raw = await harnessResume(harnessRequestId);
      const result = raw as {
        content?: string;
        tool_calls?: Parameters<typeof mapChatToolCallsForUi>[0];
        tool_results?: Parameters<typeof mapChatToolCallsForUi>[1];
        evidence_packets?: ContextPacket[];
        usage?: TokenUsage;
      };
      const toolCalls = mapChatToolCallsForUi(
        result.tool_calls,
        result.tool_results,
      );
      const content = result.content?.trim() ?? "";
      if (result.evidence_packets?.length) {
        setPackets((prev) =>
          mergeContextPackets(prev, result.evidence_packets ?? []),
        );
      }
      if (result.usage) {
        setSessionTokenUsage((prev) =>
          accumulateTokenUsage(prev, result.usage!),
        );
      }
      setMessages((prev) => {
        const next = [...prev];
        const last = next[next.length - 1];
        if (last?.role === "assistant") {
          next[next.length - 1] = { ...last, content, toolCalls };
        } else {
          next.push({ role: "assistant", content, toolCalls });
        }
        return next;
      });
    } catch (error) {
      setLastError(invokeErrorMessage(error));
    } finally {
      setStreaming(false);
      setActivityHint(null);
    }
  }, [
    ensureAssistantStreamSlot,
    harnessRequestId,
    setActivityHint,
    setLastError,
    setMessages,
    setPackets,
    setSessionTokenUsage,
    setStreaming,
  ]);
}
