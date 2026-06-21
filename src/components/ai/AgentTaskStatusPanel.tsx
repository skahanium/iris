import { ChevronDown, Play, ShieldAlert, StopCircle } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { artifactPassesValueGate } from "@/lib/assistant-artifact-tabs";
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

function canResume(status: AgentTaskStatus): boolean {
  return status === "paused_budget" || status === "paused_recoverable";
}

function canAbort(status: AgentTaskStatus): boolean {
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
    events.some((event) => event.event_type.includes("permission"))
  );
}

function failedVerificationItems(task: AgentTaskDto): string[] {
  return (
    task.verification_summary?.items
      .filter((item) => item.status === "failed")
      .map((item) => item.description) ?? []
  );
}

function hasTaskDetails(task: AgentTaskDto | null): boolean {
  return Boolean(
    task?.deliberation_state ||
    task?.verification_summary ||
    (task && task.status !== "completed"),
  );
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
  const hasDetails =
    hasTaskDetails(task) ||
    steps.length > 0 ||
    events.length > 0 ||
    Boolean(
      intentDetection ||
      permissionPreflightSummary ||
      researchState ||
      runPlanSummary,
    );

  if (!hasDetails || (task && task.kind !== "complex")) return null;

  const waitingForPermission = task ? permissionWaiting(task, events) : false;
  const canResumeTask = task ? canResume(task.status) : false;
  const canAbortTask = task ? canAbort(task.status) : false;
  const deliberation = task?.deliberation_state ?? null;
  const failedItems = task ? failedVerificationItems(task) : [];
  const sourceRequestId =
    task?.request_id ?? runPlanSummary?.requestId ?? "process";
  const processArtifact: AssistantArtifactDraft = {
    kind: "task_process",
    title: "过程详情",
    sourceRequestId,
    payload: {
      task,
      steps,
      events,
      intentDetection,
      permissionPreflightSummary,
      researchState,
      runPlanSummary,
      plan: deliberation?.plan_outline ?? runPlanSummary?.contextSummary ?? [],
      evidenceGaps: deliberation?.evidence_gaps ?? [],
      verificationFailures: failedItems,
    },
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
            <Badge variant="outline" className="text-[10px]">
              {STATUS_LABELS[task.status]}
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
