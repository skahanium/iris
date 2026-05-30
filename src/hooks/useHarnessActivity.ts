import { useCallback, useEffect, useState } from "react";

import {
  listenAiThinking,
  listenHarnessTrace,
  type AiThinkingEvent,
} from "@/lib/ipc";
import type { HarnessTraceEvent } from "@/types/ipc";

const MAX_THINKING_SNIPPETS = 8;
const MAX_TRACE_EVENTS = 24;

export interface HarnessActivityState {
  thinkingSnippets: string[];
  traceEvents: HarnessTraceEvent[];
  latestPhaseLabel: string | null;
  clear: () => void;
}

function phaseLabel(event: HarnessTraceEvent): string {
  const tool = event.tool_name;
  switch (event.phase) {
    case "tool_start":
      return `工具 ${tool} 开始`;
    case "tool_complete":
      return `工具 ${tool} ${event.status === "ok" ? "完成" : "结束"}`;
    case "subagent_spawn":
      return "启动子任务 Agent";
    case "subagent_complete":
      return `子任务 ${event.status === "ok" ? "完成" : "失败"}`;
    case "reflection":
      return "反思与证据评估";
    case "final_stream":
      return "生成最终回答";
    case "thinking":
      return "推理中";
    default:
      return tool ? `${tool} · ${event.status}` : event.status;
  }
}

export function useHarnessActivity(
  requestId: string | null,
  active: boolean,
): HarnessActivityState {
  const [thinkingSnippets, setThinkingSnippets] = useState<string[]>([]);
  const [traceEvents, setTraceEvents] = useState<HarnessTraceEvent[]>([]);
  const [latestPhaseLabel, setLatestPhaseLabel] = useState<string | null>(null);

  const clear = useCallback(() => {
    setThinkingSnippets([]);
    setTraceEvents([]);
    setLatestPhaseLabel(null);
  }, []);

  useEffect(() => {
    if (!active || !requestId) {
      return;
    }
    let unlistenThinking: (() => void) | undefined;
    let unlistenTrace: (() => void) | undefined;
    let cancelled = false;

    const setup = async () => {
      unlistenThinking = await listenAiThinking((payload: AiThinkingEvent) => {
        if (payload.request_id !== requestId || !payload.content.trim()) {
          return;
        }
        setThinkingSnippets((prev) => {
          const next = [...prev, payload.content.trim()];
          return next.slice(-MAX_THINKING_SNIPPETS);
        });
      });
      unlistenTrace = await listenHarnessTrace((payload) => {
        if (payload.request_id !== requestId) return;
        setTraceEvents((prev) => {
          const next = [...prev, payload];
          return next.slice(-MAX_TRACE_EVENTS);
        });
        setLatestPhaseLabel(phaseLabel(payload));
      });
      if (cancelled) {
        unlistenThinking?.();
        unlistenTrace?.();
      }
    };

    void setup();
    return () => {
      cancelled = true;
      unlistenThinking?.();
      unlistenTrace?.();
    };
  }, [active, requestId]);

  useEffect(() => {
    if (!active) {
      clear();
    }
  }, [active, clear]);

  return {
    thinkingSnippets,
    traceEvents,
    latestPhaseLabel,
    clear,
  };
}
