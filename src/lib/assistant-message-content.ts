import type { ToolCallInfo } from "@/types/ai";

const EMPTY_ASSISTANT_FALLBACK = "模型未返回正文，请重试或检查网络与模型配置。";

export type AssistantReconcileMutation = "noop" | "replace";

export type AssistantReconcileReason =
  | "empty_fallback"
  | "equivalent_noop"
  | "server_authoritative"
  | "server_fills_empty_stream"
  | "stream_buffer_fallback"
  | "tool_summary_fallback";

export interface AssistantReconcileInput {
  currentContent: string;
  serverContent: string;
  streamBuffer: string;
  toolCalls: ToolCallInfo[] | undefined;
}

export interface AssistantReconcileResult {
  content: string;
  mutation: AssistantReconcileMutation;
  reason: AssistantReconcileReason;
}

/** 从工具调用摘要拼出可见正文（当模型 content 为空时） */
export function toolCallsFallbackContent(
  toolCalls: ToolCallInfo[] | undefined,
): string {
  if (!toolCalls?.length) return "";
  const parts = toolCalls
    .map((tc) => tc.result_summary?.trim())
    .filter((s): s is string => Boolean(s));
  return parts.join("\n\n");
}

/** 解析助手消息最终展示正文 */
export function resolveAssistantDisplayContent(
  serverContent: string,
  streamBuffer: string,
  toolCalls: ToolCallInfo[] | undefined,
): string {
  const merged = (serverContent.trim() || streamBuffer.trim()).trim();
  if (merged) return merged;
  const fromTools = toolCallsFallbackContent(toolCalls);
  if (fromTools) return fromTools;
  return EMPTY_ASSISTANT_FALLBACK;
}

/** Resolve the authoritative final assistant content and whether it must mutate the visible slot. */
export function resolveAssistantReconcileContent({
  currentContent,
  serverContent,
  streamBuffer,
  toolCalls,
}: AssistantReconcileInput): AssistantReconcileResult {
  const current = currentContent.trim();
  const server = serverContent.trim();
  const stream = streamBuffer.trim();

  if (server && server === current) {
    return {
      content: currentContent,
      mutation: "noop",
      reason: "equivalent_noop",
    };
  }
  if (server && server === stream && current === stream) {
    return {
      content: currentContent,
      mutation: "noop",
      reason: "equivalent_noop",
    };
  }
  if (server) {
    return {
      content: server,
      mutation: "replace",
      reason: stream ? "server_authoritative" : "server_fills_empty_stream",
    };
  }
  if (stream) {
    return {
      content: stream,
      mutation: stream === current ? "noop" : "replace",
      reason: stream === current ? "equivalent_noop" : "stream_buffer_fallback",
    };
  }

  const fromTools = toolCallsFallbackContent(toolCalls);
  if (fromTools) {
    return {
      content: fromTools,
      mutation: fromTools.trim() === current ? "noop" : "replace",
      reason:
        fromTools.trim() === current
          ? "equivalent_noop"
          : "tool_summary_fallback",
    };
  }

  return {
    content: EMPTY_ASSISTANT_FALLBACK,
    mutation: EMPTY_ASSISTANT_FALLBACK === current ? "noop" : "replace",
    reason:
      EMPTY_ASSISTANT_FALLBACK === current
        ? "equivalent_noop"
        : "empty_fallback",
  };
}
