import { useCallback, type Dispatch, type SetStateAction } from "react";

import { mergeContextPackets } from "@/lib/ai/merge-context-packets";
import { invokeErrorMessage } from "@/lib/credentials";
import { agentTaskResume, harnessResume } from "@/lib/ipc";
import { mapChatToolCallsForUi } from "@/lib/map-chat-tool-calls";
import { accumulateTokenUsage } from "@/lib/token-usage";
import type {
  AiSendMessageResult,
  ContextPacket,
  TokenUsage,
} from "@/types/ai";

import type { ChatLine } from "../AiMessageList";

interface UseAssistantHarnessResumeParams {
  ensureAssistantStreamSlot: () => void;
  harnessRequestId: string | null;
  pausedTaskId: string | null;
  setActivityHint: Dispatch<SetStateAction<string | null>>;
  setLastError: Dispatch<SetStateAction<string | null>>;
  setMessages: Dispatch<SetStateAction<ChatLine[]>>;
  setPackets: Dispatch<SetStateAction<ContextPacket[]>>;
  setPausedTaskId: Dispatch<SetStateAction<string | null>>;
  setSessionTokenUsage: Dispatch<SetStateAction<TokenUsage | null>>;
  setStreaming: Dispatch<SetStateAction<boolean>>;
}

export function useAssistantHarnessResume({
  ensureAssistantStreamSlot,
  harnessRequestId,
  pausedTaskId,
  setActivityHint,
  setLastError,
  setMessages,
  setPackets,
  setPausedTaskId,
  setSessionTokenUsage,
  setStreaming,
}: UseAssistantHarnessResumeParams) {
  return useCallback(async () => {
    if (!pausedTaskId && !harnessRequestId) return;
    setLastError(null);
    setStreaming(true);
    setActivityHint(
      pausedTaskId ? "正在继续暂停任务…" : "正在从 checkpoint 恢复 Agent…",
    );
    ensureAssistantStreamSlot();
    try {
      const result = pausedTaskId
        ? await agentTaskResume(pausedTaskId)
        : ((await harnessResume(harnessRequestId!)) as AiSendMessageResult);
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
      setPausedTaskId(
        result.status === "paused_budget"
          ? (result.task_id ?? pausedTaskId)
          : null,
      );
    } catch (error) {
      setLastError(invokeErrorMessage(error));
    } finally {
      setStreaming(false);
      setActivityHint(null);
    }
  }, [
    ensureAssistantStreamSlot,
    harnessRequestId,
    pausedTaskId,
    setActivityHint,
    setLastError,
    setMessages,
    setPackets,
    setPausedTaskId,
    setSessionTokenUsage,
    setStreaming,
  ]);
}
