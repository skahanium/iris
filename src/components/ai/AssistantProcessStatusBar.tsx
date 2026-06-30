import { Loader2, StopCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { AgentTaskDto } from "@/types/ipc";

export interface ResearchProgressData {
  request_id: string;
  topic: string;
  state: string;
  current_round: number;
  max_rounds: number;
  queries_executed: string[];
  new_evidence_count: number;
  total_evidence_count: number;
  tokens_used: number;
  token_budget: number;
  progress_pct: number;
  round_terminated_early: boolean;
}

interface AssistantProcessStatusBarProps {
  activityHint: string | null;
  agentTask: AgentTaskDto | null;
  hasError?: boolean;
  researchProgress: ResearchProgressData | null;
  researchRunning: boolean;
  onAbort: () => void;
  streaming?: boolean;
}

function baseStatusLabel(
  task: AgentTaskDto | null,
  activityHint: string | null,
  progress: ResearchProgressData | null,
  researchRunning: boolean,
): string {
  if (task?.status === "awaiting_confirmation") return "等待确认";
  if (
    task?.status === "paused_budget" ||
    task?.status === "paused_recoverable"
  ) {
    return "已暂停";
  }
  if (task?.status === "failed_safe") return "处理遇到问题";
  if (researchRunning && progress) {
    return `正在研究 · 第 ${progress.current_round}/${progress.max_rounds} 轮 · 已收集 ${progress.total_evidence_count} 条证据`;
  }
  if (activityHint?.includes("重试中")) return activityHint;
  if (activityHint?.includes("检索")) return "正在检索证据";
  if (activityHint?.includes("最终") || activityHint?.includes("生成")) {
    return "正在生成回答";
  }
  if (activityHint?.includes("分析") || activityHint?.includes("处理")) {
    return "正在分析";
  }
  return "正在理解";
}

export function AssistantProcessStatusBar({
  activityHint,
  agentTask,
  hasError = false,
  researchProgress,
  researchRunning,
  onAbort,
  streaming = false,
}: AssistantProcessStatusBarProps) {
  const terminalError = hasError || agentTask?.status === "failed_safe";
  const retrying = streaming && activityHint?.includes("重试中");
  const active = researchRunning || terminalError || retrying;

  if (!active) return null;

  const label = terminalError
    ? "处理遇到问题"
    : baseStatusLabel(
        agentTask,
        activityHint,
        researchProgress,
        researchRunning,
      );
  const canAbort = researchRunning;

  return (
    <div className="px-4 pb-4 pt-2" data-testid="assistant-process-status">
      <div
        className="flex w-fit max-w-[calc(100%-1rem)] items-center justify-between gap-3 border-l-2 border-primary/45 bg-transparent py-1 pl-2.5 pr-1 text-[11px]"
        data-testid="assistant-process-status-strip"
        role="status"
      >
        <div className="flex min-w-0 items-center gap-2">
          {terminalError ? null : (
            <Loader2 className="h-3 w-3 shrink-0 animate-spin text-muted-foreground" />
          )}
          <span className="truncate text-muted-foreground">{label}</span>
        </div>
        {canAbort ? (
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="h-6 shrink-0 gap-1 px-2 text-[11px]"
            onClick={onAbort}
          >
            <StopCircle className="h-3.5 w-3.5" />
            中止
          </Button>
        ) : null}
      </div>
    </div>
  );
}
