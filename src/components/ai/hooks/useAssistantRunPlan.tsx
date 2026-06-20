import { useMemo, useState } from "react";

import type {
  AgentRunPlanSummary,
  IntentDetectionResult,
  PermissionPreflightSummary,
} from "@/types/ai";

export function useAssistantRunPlan() {
  const [intentDetection, setIntentDetection] =
    useState<IntentDetectionResult | null>(null);
  const [runPlanSummary, setRunPlanSummary] =
    useState<AgentRunPlanSummary | null>(null);
  const [permissionPreflightSummary, setPermissionPreflightSummary] =
    useState<PermissionPreflightSummary | null>(null);
  const layer = useMemo(() => {
    if (!intentDetection && !runPlanSummary && !permissionPreflightSummary) {
      return null;
    }
    const blockedCount =
      (runPlanSummary?.blockedReasons.length ?? 0) +
      (permissionPreflightSummary?.blockedCapabilities.length ?? 0) +
      (permissionPreflightSummary?.missingUserGrants.length ?? 0);
    const confirmationCount =
      permissionPreflightSummary?.requiredConfirmations.length ?? 0;

    return (
      <div
        className="mx-3 mt-3 rounded-md border border-border/60 bg-surface-inset px-3 py-2 text-xs"
        data-testid="assistant-run-plan"
      >
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
          <span className="font-medium">
            {intentDetection?.detectedIntent ??
              runPlanSummary?.detectedIntent ??
              "agent"}
          </span>
          <span className="text-muted-foreground">
            {runPlanSummary?.progressState ?? "规划已记录"}
          </span>
          {confirmationCount > 0 ? (
            <span className="text-amber-700">待确认 {confirmationCount}</span>
          ) : null}
          {blockedCount > 0 ? (
            <span className="text-destructive">阻塞 {blockedCount}</span>
          ) : null}
        </div>
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
    );
  }, [intentDetection, permissionPreflightSummary, runPlanSummary]);

  return {
    layer,
    setIntentDetection,
    setPermissionPreflightSummary,
    setRunPlanSummary,
  };
}
