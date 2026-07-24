import { Activity, Globe, Lock } from "lucide-react";

import { Button } from "@/components/ui/button";
import type { AssistantRunState } from "@/hooks/useAssistantRun";

interface AgentStatusBadgeProps {
  runState: AssistantRunState;
  webSearchEnabled?: boolean;
  webSearchProviderName?: string | null;
  disabled?: boolean;
}

function runStateLabel(runState: AssistantRunState): string {
  switch (runState) {
    case "accepted":
      return "已接收";
    case "preparing":
      return "正在准备";
    case "awaiting_confirmation":
      return "等待确认";
    case "running":
      return "正在回答";
    case "paused":
      return "已暂停";
    case "verifying":
      return "正在验证";
    case "completed":
      return "已完成";
    case "failed":
      return "未完成";
    case "cancelled":
      return "已取消";
    default:
      return "准备就绪";
  }
}
/** Compact, scene-free status affordance for the single Run lifecycle. */
export function AgentStatusBadge({
  runState,
  webSearchEnabled = false,
  webSearchProviderName,
  disabled,
}: AgentStatusBadgeProps) {
  const webLabel = webSearchEnabled
    ? `联网：${webSearchProviderName?.trim() || "已启用"}`
    : "联网：已关闭";

  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      className="h-7 shrink-0 gap-1 px-2 text-caption"
      title={`${runStateLabel(runState)}；${webLabel}`}
      disabled={disabled}
      data-testid="agent-status-trigger"
    >
      <Activity className="h-3.5 w-3.5" />
      {runStateLabel(runState)}
      {webSearchEnabled ? (
        <Globe className="h-3 w-3 text-primary" aria-label={webLabel} />
      ) : (
        <Lock className="h-3 w-3 text-muted-foreground" aria-label={webLabel} />
      )}
    </Button>
  );
}
