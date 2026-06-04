import { useEffect, useRef, useState } from "react";

import { cacheHitPercentFromUsage } from "@/lib/assistant-chrome";
import { cn } from "@/lib/utils";
import type { TokenUsage } from "@/types/ai";

interface StatusBarTokenUsageProps {
  sessionUsage: TokenUsage | null;
}

export function StatusBarTokenUsage({
  sessionUsage,
}: StatusBarTokenUsageProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    window.addEventListener("pointerdown", onPointerDown);
    return () => window.removeEventListener("pointerdown", onPointerDown);
  }, [open]);

  if (!sessionUsage || (sessionUsage.total_tokens ?? 0) <= 0) {
    return null;
  }

  const total = sessionUsage.total_tokens ?? 0;
  const cachePct = cacheHitPercentFromUsage(sessionUsage);
  const summary =
    cachePct !== null
      ? `累计 ${total.toLocaleString()} · Cache ${cachePct}%`
      : `累计 ${total.toLocaleString()}`;

  return (
    <div ref={rootRef} className="relative shrink-0">
      <button
        type="button"
        className="max-w-[11rem] truncate rounded-sm px-1 tabular-nums text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-1 focus-visible:ring-offset-panel"
        aria-expanded={open}
        aria-haspopup="dialog"
        title={summary}
        data-testid="status-bar-token-usage"
        onClick={() => setOpen((v) => !v)}
      >
        {summary}
      </button>
      {open ? (
        <div
          role="dialog"
          aria-label="Token 用量明细"
          className={cn(
            "absolute bottom-full right-0 z-50 mb-1 w-44 rounded-md border border-border bg-popover p-2 text-[10px] text-muted-foreground shadow-md",
          )}
        >
          <div className="grid grid-cols-2 gap-x-3 gap-y-1">
            <span>Prompt</span>
            <span className="text-right tabular-nums">
              {sessionUsage.prompt_tokens.toLocaleString()}
            </span>
            <span>Completion</span>
            <span className="text-right tabular-nums">
              {sessionUsage.completion_tokens.toLocaleString()}
            </span>
            <span>Cache Hit</span>
            <span className="text-right tabular-nums">
              {(sessionUsage.prompt_cache_hit_tokens ?? 0).toLocaleString()}
            </span>
            <span>Cache Miss</span>
            <span className="text-right tabular-nums">
              {(sessionUsage.prompt_cache_miss_tokens ?? 0).toLocaleString()}
            </span>
          </div>
        </div>
      ) : null}
    </div>
  );
}
