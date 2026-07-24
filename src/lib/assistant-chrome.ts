import type { ChatLine } from "@/components/ai/AiMessageList";
import type { EvidenceRef } from "@/types/ai";
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
  const denominator = hit + miss;
  return denominator === 0 ? null : Math.round((hit / denominator) * 100);
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

  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const line = messages[index];
    if (line?.role !== "assistant" || !line.toolCalls?.length) continue;
    const pending = [...line.toolCalls]
      .reverse()
      .find((toolCall) => toolCall.status === "pending");
    if (pending) return toolDisplayName(pending.name);
    const last = line.toolCalls.at(-1);
    if (last) return toolDisplayName(last.name);
  }
  return null;
}

/** Builds chrome state from safe EvidenceRef metadata and live activity. */
export function buildAssistantChromeSnapshot(options: {
  sessionTokenUsage: AssistantChromeSnapshot["sessionTokenUsage"];
  evidence: EvidenceRef[];
  activityHint?: string | null;
  streaming?: boolean;
  messages?: ChatLine[];
  harnessPhaseLabel?: string | null;
}): AssistantChromeSnapshot {
  return {
    sessionTokenUsage: options.sessionTokenUsage,
    toolActivityLabel: resolveToolActivityLabel({
      activityHint: options.activityHint ?? null,
      streaming: options.streaming ?? false,
      messages: options.messages ?? [],
      harnessPhaseLabel: options.harnessPhaseLabel ?? null,
    }),
    evidenceCount: options.evidence.length,
    webEvidenceCount: options.evidence.filter(
      (evidence) => evidence.sourceKind === "web",
    ).length,
  };
}

export function assistantChromeSnapshotsEqual(
  left: AssistantChromeSnapshot,
  right: AssistantChromeSnapshot,
): boolean {
  return (
    left.toolActivityLabel === right.toolActivityLabel &&
    left.evidenceCount === right.evidenceCount &&
    left.webEvidenceCount === right.webEvidenceCount &&
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
