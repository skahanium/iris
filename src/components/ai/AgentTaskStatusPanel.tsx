import {
  ChevronDown,
  ListChecks,
  Play,
  ShieldAlert,
  StopCircle,
} from "lucide-react";
import { useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
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

import { ResearchStatePanel } from "./assistant/ResearchStatePanel";

interface AgentTaskStatusPanelProps {
  task: AgentTaskDto | null;
  steps: AgentTaskStepDto[];
  events: AgentTaskEventDto[];
  intentDetection?: IntentDetectionResult | null;
  onAbort: () => void;
  onOpenAudit: () => void;
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
  onOpenAudit,
  onResume,
  permissionPreflightSummary,
  researchState,
  runPlanSummary,
}: AgentTaskStatusPanelProps) {
  const [summaryOpen, setSummaryOpen] = useState(false);

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
  const deliberation = task?.deliberation_state ?? null;
  const failedItems = task ? failedVerificationItems(task) : [];

  return (
    <div className="ai-task-surface px-3 pt-2" data-testid="agent-task-panel">
      <div className="rounded-md border border-border/60 bg-surface-inset px-3 py-2 text-xs">
        <div className="flex flex-wrap items-center gap-1.5">
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="h-7 gap-1 px-2 text-xs"
            onClick={() => setSummaryOpen((open) => !open)}
          >
            <ChevronDown className="h-3.5 w-3.5" />
            {summaryOpen ? "隐藏过程详情" : "过程详情"}
          </Button>
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
          {task && canResume(task.status) ? (
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
          {task && canAbort(task.status) ? (
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

        {summaryOpen ? (
          <div className="mt-2 space-y-2 border-t border-border/60 pt-2">
            {runPlanSummary || intentDetection || permissionPreflightSummary ? (
              <div className="rounded-md border border-border/60 px-2 py-1.5">
                <p className="font-medium">运行计划</p>
                <p className="mt-1 text-muted-foreground">
                  {runPlanSummary?.progressState ??
                    intentDetection?.detectedIntent ??
                    "规划已记录"}
                </p>
                {runPlanSummary?.contextSummary.length ? (
                  <p className="mt-1 line-clamp-2 text-muted-foreground">
                    {runPlanSummary.contextSummary.join(" / ")}
                  </p>
                ) : permissionPreflightSummary?.summary ? (
                  <p className="mt-1 line-clamp-2 text-muted-foreground">
                    {permissionPreflightSummary.summary}
                  </p>
                ) : null}
              </div>
            ) : null}
            {deliberation ? (
              <div
                className="rounded-md border border-border/60 px-2 py-1.5 text-xs"
                data-testid="agent-task-deliberation"
              >
                <div className="mb-1 flex items-center gap-1.5 font-medium">
                  <ListChecks className="h-3.5 w-3.5" />
                  计划
                </div>
                {deliberation.plan_outline.slice(0, 3).map((item) => (
                  <p key={item} className="text-muted-foreground">
                    {item}
                  </p>
                ))}
                {deliberation.evidence_gaps.length > 0 ? (
                  <div className="mt-2">
                    <p className="font-medium">证据缺口</p>
                    {deliberation.evidence_gaps.slice(0, 3).map((gap) => (
                      <p key={gap} className="text-muted-foreground">
                        {gap}
                      </p>
                    ))}
                  </div>
                ) : null}
                {failedItems.length > 0 ? (
                  <div className="mt-2">
                    <p className="font-medium text-amber-700">验证未通过</p>
                    {failedItems.slice(0, 3).map((item) => (
                      <p key={item} className="text-muted-foreground">
                        {item}
                      </p>
                    ))}
                  </div>
                ) : null}
              </div>
            ) : null}
            {researchState ? (
              <ResearchStatePanel state={researchState} />
            ) : null}
            {steps.length > 0 ? (
              steps.map((step) => (
                <div
                  key={step.id}
                  className="rounded-md border border-border/60 px-2 py-1.5 text-xs"
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-medium">{step.kind}</span>
                    <span className="text-muted-foreground">
                      引用 {step.evidence_packet_ids.length}
                    </span>
                  </div>
                  <p className="mt-1 text-muted-foreground">
                    {step.output_summary || step.input_summary}
                  </p>
                </div>
              ))
            ) : (
              <p className="text-xs text-muted-foreground">暂无步骤摘要</p>
            )}
            {events
              .filter((event) => event.message.trim().length > 0)
              .map((event) => (
                <p key={event.id} className="text-xs text-muted-foreground">
                  {event.message}
                </p>
              ))}
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="h-7 px-2 text-xs"
              onClick={onOpenAudit}
            >
              查看审计
            </Button>
          </div>
        ) : null}
      </div>
    </div>
  );
}
