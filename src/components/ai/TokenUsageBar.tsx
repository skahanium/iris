import { ChevronDown, ChevronUp } from "lucide-react";
import { useState } from "react";

import { cn } from "@/lib/utils";
import type { TokenUsage } from "@/types/ai";

interface TokenUsageBarProps {
  turnUsage: TokenUsage | null;
  sessionTotal: TokenUsage | null;
  className?: string;
}

function cacheHitPercent(usage: TokenUsage | null): number | null {
  if (!usage) return null;
  const hit = usage.prompt_cache_hit_tokens ?? 0;
  const miss = usage.prompt_cache_miss_tokens ?? 0;
  const denom = hit + miss;
  if (denom === 0) return null;
  return Math.round((hit / denom) * 100);
}

export function TokenUsageBar({
  turnUsage,
  sessionTotal,
  className,
}: TokenUsageBarProps) {
  const [expanded, setExpanded] = useState(false);
  if (!turnUsage && !sessionTotal) return null;

  const turnTotal = turnUsage?.total_tokens ?? 0;
  const sessionSum = sessionTotal?.total_tokens ?? 0;
  const cachePct = cacheHitPercent(turnUsage);

  return (
    <div
      className={cn(
        "border-t border-border/60 bg-surface-inset/30 px-3 py-1.5 text-[10px] text-muted-foreground",
        className,
      )}
      data-testid="token-usage-bar"
    >
      <button
        type="button"
        className="flex w-full items-center gap-2"
        onClick={() => setExpanded((v) => !v)}
      >
        <span>
          本轮 {turnTotal.toLocaleString()} tokens
          {cachePct !== null ? ` | Cache 命中 ${cachePct}%` : ""}
          {sessionSum > 0 ? ` | 累计 ${sessionSum.toLocaleString()}` : ""}
        </span>
        <span className="flex-1" />
        {expanded ? (
          <ChevronDown className="h-3 w-3" />
        ) : (
          <ChevronUp className="h-3 w-3" />
        )}
      </button>
      {expanded && turnUsage ? (
        <div className="mt-2 grid grid-cols-2 gap-x-4 gap-y-1 border-t border-border/40 pt-2">
          <span>Prompt</span>
          <span className="text-right">{turnUsage.prompt_tokens}</span>
          <span>Completion</span>
          <span className="text-right">{turnUsage.completion_tokens}</span>
          <span>Cache Hit</span>
          <span className="text-right">
            {turnUsage.prompt_cache_hit_tokens ?? 0}
          </span>
          <span>Cache Miss</span>
          <span className="text-right">
            {turnUsage.prompt_cache_miss_tokens ?? 0}
          </span>
        </div>
      ) : null}
    </div>
  );
}
