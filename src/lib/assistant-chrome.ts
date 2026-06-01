import type { ChatLine } from "@/components/ai/AiMessageList";
import type { ContextPacket } from "@/types/ai";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";

import { toolDisplayName } from "./tool-display-names";

export function cacheHitPercentFromUsage(
  usage: { prompt_cache_hit_tokens?: number; prompt_cache_miss_tokens?: number } | null,
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
}): AssistantChromeSnapshot {
  const webPacketCount = countWebPackets(options.packets);
  return {
    sessionTokenUsage: options.sessionTokenUsage,
    toolActivityLabel: resolveToolActivityLabel({
      activityHint: options.activityHint,
      streaming: options.streaming,
      messages: options.messages,
      harnessPhaseLabel: options.harnessPhaseLabel,
    }),
    evidenceCount: options.packets.length,
    webPacketCount,
  };
}
