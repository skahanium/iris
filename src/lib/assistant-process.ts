import { toolDisplayName } from "@/lib/tool-display-names";
import {
  ANSWER_COMPLETE_PROCESS_ID,
  ANSWER_COMPLETE_PROCESS_LABEL,
} from "@/lib/assistant-presentation";
import { stageTextForPayload } from "@/lib/assistant-run-events";
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

const INTERNAL_RUNTIME_TOOLS = new Set([
  "system_time_now",
  "app_context_read",
  "capabilities_read",
]);

/**
 * Runtime-context reads are for the model, not the user-facing process
 * timeline. Capability may arrive as snake_case or dotted MCP-style names.
 */
export function isInternalRuntimeTool(capability: string): boolean {
  const normalized = capability.trim().replaceAll(".", "_");
  return INTERNAL_RUNTIME_TOOLS.has(normalized);
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
  let answerTerminalAt: number | null = null;
  let answerTerminalLabel: string | null = null;

  for (const event of events) {
    const createdAt = timestampMs(event.timestamp);
    switch (event.payload.kind) {
      case "stage_changed": {
        const stage = stageTextForPayload(event.payload);
        if (isInternalPreparingStage(stage)) {
          break;
        }
        items.push({
          id: `stage:${event.seq}`,
          kind: "stage",
          label: stage,
          status: "completed",
          createdAt,
        });
        break;
      }
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
        if (isInternalRuntimeTool(event.payload.capability)) {
          break;
        }
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
        if (isInternalRuntimeTool(event.payload.capability)) {
          break;
        }
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
      case "completed":
        answerTerminalAt = createdAt;
        answerTerminalLabel = ANSWER_COMPLETE_PROCESS_LABEL;
        break;
      case "failed":
        answerTerminalAt = createdAt;
        answerTerminalLabel = "答复失败";
        break;
      case "cancelled":
        answerTerminalAt = createdAt;
        answerTerminalLabel = "已取消";
        break;
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

  if (
    answerTerminalLabel &&
    !items.some((item) => item.id === ANSWER_COMPLETE_PROCESS_ID)
  ) {
    items.push({
      id: ANSWER_COMPLETE_PROCESS_ID,
      kind: "stage",
      label: answerTerminalLabel,
      status: "completed",
      createdAt: answerTerminalAt ?? fallbackCreatedAt,
    });
  }

  return collapseRepeatedWebSearchProcessItems(items);
}

/**
 * Collapse repeated Web searches into one compact process item for a single
 * answer. This is presentation-only: the durable tool audit and evidence
 * ledger retain every individual operation.
 */
export function collapseRepeatedWebSearchProcessItems(
  items: readonly AssistantProcessItem[],
): AssistantProcessItem[] {
  const collapsed: AssistantProcessItem[] = [];
  let firstWebSearchIndex: number | null = null;

  for (const item of items) {
    if (!isWebSearchProcessItem(item)) {
      collapsed.push(item);
      continue;
    }
    if (firstWebSearchIndex === null) {
      firstWebSearchIndex = collapsed.length;
      collapsed.push(item);
      continue;
    }
    const first = collapsed[firstWebSearchIndex];
    if (!first) continue;
    collapsed[firstWebSearchIndex] = {
      ...first,
      status: mergeProcessStatus(first.status, item.status),
      ...mergeProcessDuration(first.durationMs, item.durationMs),
    };
  }

  return collapsed;
}

function isWebSearchProcessItem(item: AssistantProcessItem): boolean {
  return item.kind === "tool" && item.label === displayCapability("web_search");
}

function mergeProcessStatus(
  left: AssistantProcessItem["status"],
  right: AssistantProcessItem["status"],
): AssistantProcessItem["status"] {
  if (left === "running" || right === "running") return "running";
  if (left === "completed" || right === "completed") return "completed";
  return "failed";
}

function mergeProcessDuration(
  left: number | undefined,
  right: number | undefined,
): { durationMs?: number } {
  const durations = [left, right].filter(
    (duration): duration is number =>
      typeof duration === "number" && Number.isFinite(duration),
  );
  return durations.length > 0
    ? { durationMs: durations.reduce((total, duration) => total + duration, 0) }
    : {};
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
