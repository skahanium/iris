import { useMemo } from "react";

import type { HarnessTraceEvent } from "@/types/ipc";

import { useHarnessActivity } from "./useHarnessActivity";

export type ActivityTimelineEntry = {
  id: string;
  label: string;
  status: string;
  phase: string;
  toolName: string;
  collapsed: boolean;
};

function shouldCollapseEntry(status: string): boolean {
  return status === "ok" || status === "completed" || status === "running";
}

export function useAssistantActivity(
  requestId: string | null,
  active: boolean,
) {
  const base = useHarnessActivity(requestId, active);

  const timeline: ActivityTimelineEntry[] = useMemo(() => {
    return base.traceEvents.map((ev: HarnessTraceEvent, index: number) => ({
      id: `${ev.request_id}-${ev.round}-${index}`,
      label: formatTraceLabel(ev),
      status: ev.status,
      phase: String(ev.phase),
      toolName: ev.tool_name,
      collapsed: shouldCollapseEntry(ev.status),
    }));
  }, [base.traceEvents]);

  return {
    ...base,
    timeline,
    phaseLabel: base.latestPhaseLabel,
  };
}

function formatTraceLabel(ev: HarnessTraceEvent): string {
  const name = ev.tool_name || ev.phase;
  if (ev.status === "pending") return `等待确认：${name}`;
  if (ev.status === "error") return `失败：${name}`;
  if (ev.phase === "final_stream") return "正在生成最终回答";
  if (ev.phase === "tool_start") return `正在执行：${name}`;
  if (ev.phase === "tool_complete") return `已完成：${name}`;
  return `${name} (${ev.status})`;
}
