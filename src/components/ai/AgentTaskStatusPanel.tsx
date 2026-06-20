import {
  ChevronDown,
  ClipboardList,
  Play,
  ShieldAlert,
  StopCircle,
} from "lucide-react";
import { useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
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
  onAbort: () => void;
  onOpenAudit: () => void;
  onResume: () => void;
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

export function AgentTaskStatusPanel({
  task,
  steps,
  events,
  onAbort,
  onOpenAudit,
  onResume,
}: AgentTaskStatusPanelProps) {
  const [summaryOpen, setSummaryOpen] = useState(false);

  if (!task || task.kind !== "complex") return null;

  const waitingForPermission = permissionWaiting(task, events);

  return (
    <div className="ai-task-surface px-3 pt-3" data-testid="agent-task-panel">
      <Card className="border-border/60">
        <CardHeader className="space-y-2 pb-2">
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="flex min-w-0 items-center gap-2 text-sm font-medium">
              <ClipboardList className="h-4 w-4 shrink-0" />
              <span className="truncate">复杂任务</span>
            </CardTitle>
            <Badge variant="outline" className="shrink-0 text-[10px]">
              {STATUS_LABELS[task.status]}
            </Badge>
          </div>
          <p className="line-clamp-2 text-xs text-muted-foreground">
            {task.user_goal_summary}
          </p>
        </CardHeader>
        <CardContent className="space-y-3">
          {waitingForPermission ? (
            <div className="flex items-center gap-2 rounded-md border border-amber-300/60 bg-amber-50 px-2 py-1.5 text-xs text-amber-800">
              <ShieldAlert className="h-3.5 w-3.5 shrink-0" />
              <span>权限等待</span>
            </div>
          ) : null}

          <div className="flex flex-wrap items-center gap-1.5">
            {canResume(task.status) ? (
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
            {canAbort(task.status) ? (
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
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="h-7 gap-1 px-2 text-xs"
              onClick={() => setSummaryOpen((open) => !open)}
            >
              <ChevronDown className="h-3.5 w-3.5" />
              {summaryOpen ? "隐藏进度摘要" : "查看进度摘要"}
            </Button>
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

          {summaryOpen ? (
            <div className="space-y-2 border-t border-border/60 pt-2">
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
            </div>
          ) : null}
        </CardContent>
      </Card>
    </div>
  );
}
