import { Brain, ChevronDown, ChevronRight, Workflow } from "lucide-react";
import { useState } from "react";

import type { HarnessActivityState } from "@/hooks/useHarnessActivity";
import type { HarnessTraceEvent } from "@/types/ipc";

interface HarnessActivityStripProps {
  activity: HarnessActivityState;
  statusHint?: string | null;
}

function traceStatusClass(status: string): string {
  if (status === "ok" || status === "completed") {
    return "text-foreground/70";
  }
  if (status === "error" || status === "failed") {
    return "text-destructive";
  }
  return "text-muted-foreground";
}

function TraceRow({ event }: { event: HarnessTraceEvent }) {
  const label = event.tool_name || event.phase || "step";
  return (
    <li className="flex items-start gap-2 text-[10px] leading-snug">
      <span className="shrink-0 tabular-nums text-muted-foreground/80">
        R{event.round}
      </span>
      <span className="min-w-0 flex-1">
        <span className="font-medium">{label}</span>
        <span className={`ml-1.5 ${traceStatusClass(event.status)}`}>
          {event.status}
        </span>
        {event.output_preview ? (
          <p className="mt-0.5 line-clamp-2 text-muted-foreground">
            {event.output_preview}
          </p>
        ) : null}
      </span>
    </li>
  );
}

export function HarnessActivityStrip({
  activity,
  statusHint,
}: HarnessActivityStripProps) {
  const [expanded, setExpanded] = useState(false);
  const hasThinking = activity.thinkingSnippets.length > 0;
  const hasTraces = activity.traceEvents.length > 0;
  const headline =
    activity.latestPhaseLabel ?? statusHint ?? "Agent 运行中…";

  if (!hasThinking && !hasTraces && !statusHint) {
    return null;
  }

  return (
    <div
      className="mx-2 mb-2 rounded-lg border border-border/70 bg-surface-elevated/80 text-xs shadow-sm"
      data-testid="harness-activity-strip"
    >
      <button
        type="button"
        className="flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-muted/40"
        onClick={() => setExpanded((v) => !v)}
        aria-expanded={expanded}
      >
        <Workflow className="h-3.5 w-3.5 shrink-0 text-primary/80" />
        <span className="min-w-0 flex-1 truncate text-foreground/90">
          {headline}
        </span>
        {expanded ? (
          <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
        ) : (
          <ChevronRight className="h-3.5 w-3.5 text-muted-foreground" />
        )}
      </button>

      {expanded ? (
        <div className="space-y-3 border-t border-border/60 px-3 py-2">
          {hasTraces ? (
            <div>
              <p className="mb-1 flex items-center gap-1 text-[10px] font-medium text-muted-foreground">
                <Workflow className="h-3 w-3" />
                Harness 进度
              </p>
              <ul className="max-h-32 space-y-1 overflow-y-auto">
                {activity.traceEvents.map((ev, i) => (
                  <TraceRow key={`${ev.round}-${ev.tool_name}-${i}`} event={ev} />
                ))}
              </ul>
            </div>
          ) : null}

          {hasThinking ? (
            <div>
              <p className="mb-1 flex items-center gap-1 text-[10px] font-medium text-muted-foreground">
                <Brain className="h-3 w-3" />
                推理片段
              </p>
              <div className="max-h-28 space-y-1.5 overflow-y-auto">
                {activity.thinkingSnippets.map((snippet, i) => (
                  <p
                    key={`think-${i}`}
                    className="rounded bg-muted/40 px-2 py-1 text-[10px] text-muted-foreground"
                  >
                    {snippet}
                  </p>
                ))}
              </div>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
