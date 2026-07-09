import type { ChatLine } from "@/components/ai/AiMessageList";
import type { ContextPacket } from "@/types/ai";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";

import { toolDisplayName } from "./tool-display-names";

export function cacheHitPercentFromUsage(
  usage: {
    prompt_cache_hit_tokens?: number;
    prompt_cache_miss_tokens?: number;
  } | null,
): number | null {
  if (!usage) return null;
  const hit = usage.prompt_cache_hit_tokens ?? 0;
  const miss = usage.prompt_cache_miss_tokens ?? 0;
  const denom = hit + miss;
  if (denom === 0) return null;
  return Math.round((hit / denom) * 100);
}

export function countWebPackets(packets: ContextPacket[]): number {
  return packets.filter((p) => p.source_type === "web").length;
}

export function countWebSearchPackets(packets: ContextPacket[]): number {
  return packets.filter(
    (p) => p.source_type === "web" && p.retrieval_reason !== "web_page_fetch",
  ).length;
}

export function countWebPageFetchPackets(packets: ContextPacket[]): number {
  return packets.filter(
    (p) => p.source_type === "web" && p.retrieval_reason === "web_page_fetch",
  ).length;
}

export function resolveToolActivityLabel(options: {
  activityHint: string | null;
  streaming: boolean;
  messages: ChatLine[];
  harnessPhaseLabel: string | null;
}): string | null {
  const { activityHint, streaming, messages, harnessPhaseLabel } = options;
  if (activityHint?.trim()) return activityHint.trim();
  if (!streaming) return null;
  if (harnessPhaseLabel?.trim()) return harnessPhaseLabel.trim();

  for (let i = messages.length - 1; i >= 0; i--) {
    const line = messages[i];
    if (line?.role !== "assistant" || !line.toolCalls?.length) continue;
    const pending = [...line.toolCalls]
      .reverse()
      .find((tc) => tc.status === "pending");
    if (pending) return toolDisplayName(pending.name);
    const last = line.toolCalls[line.toolCalls.length - 1];
    if (last) return toolDisplayName(last.name);
  }
  return null;
}

export function buildAssistantChromeSnapshot(options: {
  sessionTokenUsage: AssistantChromeSnapshot["sessionTokenUsage"];
  activityHint: string | null;
  streaming: boolean;
  messages: ChatLine[];
  harnessPhaseLabel: string | null;
  packets: ContextPacket[];
  harnessRequestId?: string | null;
}): AssistantChromeSnapshot {
  const webPacketCount = countWebPackets(options.packets);
  return {
    sessionTokenUsage: options.sessionTokenUsage,
    toolActivityLabel: null,
    evidenceCount: options.packets.length,
    webPacketCount,
    harnessRequestId: options.harnessRequestId ?? null,
  };
}

export function assistantChromeSnapshotsEqual(
  left: AssistantChromeSnapshot,
  right: AssistantChromeSnapshot,
): boolean {
  return (
    left.toolActivityLabel === right.toolActivityLabel &&
    left.evidenceCount === right.evidenceCount &&
    left.webPacketCount === right.webPacketCount &&
    left.harnessRequestId === right.harnessRequestId &&
    tokenUsageEqual(left.sessionTokenUsage, right.sessionTokenUsage)
  );
}

function tokenUsageEqual(
  left: AssistantChromeSnapshot["sessionTokenUsage"],
  right: AssistantChromeSnapshot["sessionTokenUsage"],
): boolean {
  if (left === right) return true;
  if (!left || !right) return false;
  return (
    left.prompt_tokens === right.prompt_tokens &&
    left.completion_tokens === right.completion_tokens &&
    left.total_tokens === right.total_tokens &&
    left.prompt_cache_hit_tokens === right.prompt_cache_hit_tokens &&
    left.prompt_cache_miss_tokens === right.prompt_cache_miss_tokens
  );
}
