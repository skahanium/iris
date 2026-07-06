import { ChevronDown, Play, ShieldAlert, StopCircle } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { artifactPassesValueGate } from "@/lib/assistant-artifact-tabs";
import {
  getAiPayloadStore,
  sanitizePayloadForUi,
} from "@/lib/ai-payload-store";
import type { AssistantArtifactDraft } from "@/types/assistant-artifact";
import type {
  AgentRunPlanSummary,
  IntentDetectionResult,
  PermissionPreflightSummary,
  ResearchState,
} from "@/types/ai";
import type {
  AgentTaskDto,
  AgentTaskEventDto,
  AgentTaskStatus,
  AgentTaskStepDto,
} from "@/types/ipc";

interface AgentTaskStatusPanelProps {
  task: AgentTaskDto | null;
  steps: AgentTaskStepDto[];
  events: AgentTaskEventDto[];
  intentDetection?: IntentDetectionResult | null;
  onAbort: () => void;
  onOpenArtifact: (draft: AssistantArtifactDraft) => void;
  onResume: () => void;
  permissionPreflightSummary?: PermissionPreflightSummary | null;
  researchState?: ResearchState | null;
  runPlanSummary?: AgentRunPlanSummary | null;
}

const STATUS_LABELS: Record<AgentTaskStatus, string> = {
  queued: "等待中",
  running: "运行中",
  awaiting_confirmation: "等待确认",
  paused_budget: "预算暂停",
  paused_recoverable: "可恢复暂停",
  completed: "已完成",
  failed_safe: "安全失败",
  aborted: "已中止",
};

const PROCESS_EVENT_LIMIT = 40;
const PROCESS_STEP_LIMIT = 30;

function isKnownStatus(status: unknown): status is AgentTaskStatus {
  return typeof status === "string" && status in STATUS_LABELS;
}

function statusLabel(status: unknown): string {
  return isKnownStatus(status) ? STATUS_LABELS[status] : "状态异常";
}

function canResume(status: unknown): boolean {
  return status === "paused_budget" || status === "paused_recoverable";
}

function canAbort(status: unknown): boolean {
  return (
    status === "queued" ||
    status === "running" ||
    status === "awaiting_confirmation" ||
    status === "paused_budget" ||
    status === "paused_recoverable"
  );
}

function permissionWaiting(task: AgentTaskDto, events: AgentTaskEventDto[]) {
  return (
    task.status === "awaiting_confirmation" ||
    events.some(
      (event) =>
        typeof event.event_type === "string" &&
        event.event_type.includes("permission"),
    )
  );
}

function failedVerificationItems(task: AgentTaskDto): string[] {
  const items = task.verification_summary?.items;
  if (!Array.isArray(items)) return [];

  return items
    .filter(
      (item) =>
        item &&
        item.status === "failed" &&
        typeof item.description === "string",
    )
    .map((item) => item.description);
}

function hasTaskDetails(task: AgentTaskDto | null): boolean {
  return Boolean(
    task?.deliberation_state ||
    task?.verification_summary ||
    (task && task.status !== "completed"),
  );
}

function normalizeSteps(steps: AgentTaskStepDto[]): AgentTaskStepDto[] {
  return (Array.isArray(steps) ? steps : []).slice(-PROCESS_STEP_LIMIT);
}

function normalizeEvents(events: AgentTaskEventDto[]): AgentTaskEventDto[] {
  const safeEvents = Array.isArray(events) ? events : [];
  const deduped: AgentTaskEventDto[] = [];
  let previousKey = "";
  for (const event of safeEvents) {
    const eventType =
      typeof event.event_type === "string" ? event.event_type : "unknown";
    const message = typeof event.message === "string" ? event.message : "";
    const key = `${eventType}:${message}`;
    if (key === previousKey) continue;
    previousKey = key;
    deduped.push({ ...event, event_type: eventType, message });
  }
  return deduped.slice(-PROCESS_EVENT_LIMIT);
}

export function AgentTaskStatusPanel({
  task,
  steps,
  events,
  intentDetection,
  onAbort,
  onOpenArtifact,
  onResume,
  permissionPreflightSummary,
  researchState,
  runPlanSummary,
}: AgentTaskStatusPanelProps) {
  const safeSteps = normalizeSteps(steps);
  const safeEvents = normalizeEvents(events);
  const hasDetails =
    hasTaskDetails(task) ||
    safeSteps.length > 0 ||
    safeEvents.length > 0 ||
    Boolean(
      intentDetection ||
      permissionPreflightSummary ||
      researchState ||
      runPlanSummary,
    );

  if (!hasDetails || (task && task.kind !== "complex")) return null;

  const waitingForPermission = task
    ? permissionWaiting(task, safeEvents)
    : false;
  const canResumeTask = task ? canResume(task.status) : false;
  const canAbortTask = task ? canAbort(task.status) : false;
  const deliberation = task ? (task.deliberation_state ?? null) : null;
  const failedItems = task ? failedVerificationItems(task) : [];
  const sourceRequestId =
    task?.request_id ?? runPlanSummary?.requestId ?? "process";
  const payload = sanitizePayloadForUi(getAiPayloadStore(), {
    task,
    steps: safeSteps,
    events: safeEvents,
    intentDetection,
    permissionPreflightSummary,
    researchState,
    runPlanSummary,
    plan: deliberation?.plan_outline ?? runPlanSummary?.contextSummary ?? [],
    evidenceGaps: deliberation?.evidence_gaps ?? [],
    verificationFailures: failedItems,
  });
  const processArtifact: AssistantArtifactDraft = {
    kind: "task_process",
    title: "过程详情",
    sourceRequestId,
    payload,
  };
  const canOpenProcessArtifact = artifactPassesValueGate(processArtifact);

  if (
    !canOpenProcessArtifact &&
    !waitingForPermission &&
    !canResumeTask &&
    !canAbortTask
  ) {
    return null;
  }

  return (
    <div className="ai-task-surface px-3 pt-2" data-testid="agent-task-panel">
      <div className="rounded-md border border-border/60 bg-surface-inset px-3 py-2 text-xs">
        {deliberation ? (
          <span className="sr-only" data-testid="agent-task-deliberation" />
        ) : null}
        <div className="flex flex-wrap items-center gap-1.5">
          {canOpenProcessArtifact ? (
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="h-7 gap-1 px-2 text-xs"
              onClick={() => onOpenArtifact(processArtifact)}
            >
              <ChevronDown className="h-3.5 w-3.5" />
              在工作区打开过程详情
            </Button>
          ) : null}
          {task ? (
            <Badge
              variant="outline"
              className="text-[10px]"
              data-testid="agent-task-status-badge"
            >
              {statusLabel(task.status)}
            </Badge>
          ) : null}
          {waitingForPermission ? (
            <Badge
              variant="outline"
              className="gap-1 text-[10px] text-amber-700"
            >
              <ShieldAlert className="h-3 w-3" />
              权限等待
            </Badge>
          ) : null}
          {task && canResumeTask ? (
            <Button
              type="button"
              size="sm"
              className="h-7 gap-1 px-2 text-xs"
              onClick={onResume}
            >
              <Play className="h-3.5 w-3.5" />
              继续
            </Button>
          ) : null}
          {task && canAbortTask ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="h-7 gap-1 px-2 text-xs"
              onClick={onAbort}
            >
              <StopCircle className="h-3.5 w-3.5" />
              中止
            </Button>
          ) : null}
        </div>
      </div>
    </div>
  );
}
