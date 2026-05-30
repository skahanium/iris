import type { ToolCallInfo } from "@/types/ai";

const EMPTY_ASSISTANT_FALLBACK = "模型未返回正文，请重试或检查网络与模型配置。";

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
