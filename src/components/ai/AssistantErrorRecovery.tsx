import { AlertTriangle } from "lucide-react";

import { Button } from "@/components/ui/button";

interface AssistantErrorRecoveryProps {
  disabled: boolean;
  harnessRequestId: string | null;
  lastError: string | null;
  pausedTaskId?: string | null;
  onResume: () => void;
}

export function AssistantErrorRecovery({
  disabled,
  harnessRequestId,
  lastError,
  pausedTaskId,
  onResume,
}: AssistantErrorRecoveryProps) {
  if (!lastError && !pausedTaskId) return null;

  return (
    <div className="space-y-2 px-3 pt-3">
      {lastError ? (
        <div className="flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          <span>{lastError}</span>
        </div>
      ) : null}
      {pausedTaskId || harnessRequestId ? (
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="h-7 text-xs"
          disabled={disabled}
          onClick={onResume}
        >
          {pausedTaskId ? "继续任务" : "从 checkpoint 恢复 Agent"}
        </Button>
      ) : null}
    </div>
  );
}
