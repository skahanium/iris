import { toolDisplayName } from "@/lib/tool-display-names";
import type { AssistantRunEvent } from "@/types/ai";

/** A safe, presentation-only item rendered inside one assistant message. */
export interface AssistantProcessItem {
  id: string;
  kind: "stage" | "reasoning_summary" | "tool";
  label: string;
  status: "running" | "completed" | "failed";
  createdAt: number;
  durationMs?: number;
}

/**
 * Project persisted or live Run events into user-visible process items.
 * Final answer deltas, tool arguments, raw outputs, and provider internals are
 * deliberately excluded from this boundary.
 */
export function projectAssistantProcessEvents(
  events: readonly AssistantRunEvent[],
  liveReasoningSummaries: readonly { summaryId: string; text: string }[] = [],
): AssistantProcessItem[] {
  const items: AssistantProcessItem[] = [];
  const toolIndexes = new Map<string, number>();

  for (const event of events) {
    const createdAt = timestampMs(event.timestamp);
    switch (event.payload.kind) {
      case "stage_changed":
        if (isInternalPreparingStage(event.payload.stage)) {
          break;
        }
        items.push({
          id: `stage:${event.seq}`,
          kind: "stage",
          label: event.payload.stage,
          status: "completed",
          createdAt,
        });
        break;
      case "reasoning_summary":
        items.push({
          id: `reasoning:${event.payload.summaryId}`,
          kind: "reasoning_summary",
          label: event.payload.text,
          status: "completed",
          createdAt,
        });
        break;
      case "tool_started": {
        const id = `tool:${event.payload.toolCallId}`;
        toolIndexes.set(event.payload.toolCallId, items.length);
        items.push({
          id,
          kind: "tool",
          label: displayCapability(event.payload.capability),
          status: "running",
          createdAt,
        });
        break;
      }
      case "tool_completed": {
        const index = toolIndexes.get(event.payload.toolCallId);
        const current = index === undefined ? undefined : items[index];
        if (index !== undefined && current) {
          items[index] = {
            ...current,
            status: event.payload.success === false ? "failed" : "completed",
            ...(typeof event.payload.durationMs === "number"
              ? { durationMs: event.payload.durationMs }
              : createdAt >= current.createdAt
                ? { durationMs: createdAt - current.createdAt }
                : {}),
          };
          break;
        }
        items.push({
          id: `tool:${event.payload.toolCallId}`,
          kind: "tool",
          label: displayCapability(event.payload.capability),
          status: "completed",
          createdAt,
        });
        break;
      }
      default:
        break;
    }
  }

  const knownSummaryIds = new Set(
    items
      .filter((item) => item.kind === "reasoning_summary")
      .map((item) => item.id.replace(/^reasoning:/, "")),
  );
  const fallbackCreatedAt = items.at(-1)?.createdAt ?? 0;
  for (const summary of liveReasoningSummaries) {
    if (knownSummaryIds.has(summary.summaryId)) continue;
    items.push({
      id: `reasoning:${summary.summaryId}`,
      kind: "reasoning_summary",
      label: summary.text,
      status: "completed",
      createdAt: fallbackCreatedAt,
    });
  }

  return items;
}

/** Pure internal prep labels stay out of the user-visible process timeline. */
function isInternalPreparingStage(stage: string): boolean {
  const trimmed = stage.trim();
  return (
    trimmed === "正在准备" ||
    trimmed === "正在准备工具执行" ||
    trimmed === "正在恢复运行状态"
  );
}

function displayCapability(capability: string): string {
  const direct = toolDisplayName(capability);
  if (direct !== capability) return direct;
  return toolDisplayName(capability.replaceAll(".", "_"));
}

function timestampMs(timestamp: string): number {
  const parsed = Date.parse(timestamp);
  return Number.isFinite(parsed) ? parsed : 0;
}
